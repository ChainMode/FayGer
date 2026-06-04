//! Feature: fayger-m6, Property 1 & 5: section selection decision & Required/Optional handling.
//!
//! Property 1：select_sections 的 selected 恰为 fits 的 id（manifest 顺序）；被跳过 Optional 的
//! reasons 非空且等于 unmet；确定性；存在 fits-假的 Required → LDR_PROFILE_REQUIRED_SECTION_MISSING。
//! Property 5：Required 缺失拒绝 / Optional 跳过降级。
//!
//! Validates: Requirements 2.x, 3.x, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_error::ErrorCode;
use fayger_loader::{select_sections, LoadProfile, SkipReason};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

/// 一个生成的"带约束 section 规格"。
#[derive(Debug, Clone)]
struct SectionSpec {
    id: String,
    length: u64,
    required_features: BTreeSet<String>,
    max_size: Option<u64>,
    optional: bool,
}

fn feature_pool() -> Vec<&'static str> {
    vec!["gpu", "realtime", "secure", "audio"]
}

fn arb_feature_set() -> impl Strategy<Value = BTreeSet<String>> {
    let pool = feature_pool();
    proptest::collection::vec(any::<bool>(), pool.len()).prop_map(move |mask| {
        pool.iter()
            .zip(mask)
            .filter(|(_, keep)| *keep)
            .map(|(f, _)| (*f).to_string())
            .collect()
    })
}

fn arb_section_spec(idx: usize) -> impl Strategy<Value = SectionSpec> {
    (
        1u64..=64,
        arb_feature_set(),
        prop::option::of(1u64..=64),
        any::<bool>(),
    )
        .prop_map(
            move |(length, required_features, max_size, optional)| SectionSpec {
                id: format!("s{idx}"),
                length,
                required_features,
                max_size,
                optional,
            },
        )
}

fn arb_specs() -> impl Strategy<Value = Vec<SectionSpec>> {
    (0usize..=6).prop_flat_map(|n| {
        let parts: Vec<_> = (0..n).map(arb_section_spec).collect();
        parts
    })
}

fn arb_profile() -> impl Strategy<Value = LoadProfile> {
    (arb_feature_set(), prop::option::of(1u64..=64))
        .prop_map(|(features, max)| LoadProfile::new("test", BTreeSet::new(), max, features))
}

fn build_manifest(specs: &[SectionSpec]) -> BuFManifest {
    let sections = specs
        .iter()
        .map(|s| SectionDescriptor {
            id: s.id.clone(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: s.length,
            visibility: if s.optional {
                SectionVisibility::Optional
            } else {
                SectionVisibility::Required
            },
            profile_constraints: ProfileConstraints {
                required_capabilities: BTreeSet::new(),
                max_size: s.max_size,
                required_features: s.required_features.clone(),
            },
        })
        .collect();
    BuFManifest {
        schema_version: SemVer::parse("1.0.0").expect("ok"),
        runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections,
        signature: None,
        extensions: std::collections::BTreeMap::new(),
    }
}

/// 参考实现：某 spec 在 profile 下是否 fits（与库内 unmet_reasons 等价的独立计算）。
fn spec_fits(s: &SectionSpec, p: &LoadProfile) -> bool {
    let feat_ok = s.required_features.is_subset(&p.features);
    let size_ok = s.max_size.is_none_or(|m| s.length <= m)
        && p.max_section_bytes.is_none_or(|q| s.length <= q);
    feat_ok && size_ok
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn selection_matches_predicate(specs in arb_specs(), profile in arb_profile()) {
        let manifest = build_manifest(&specs);
        let result = select_sections(&manifest, &profile);

        // 参考：是否存在 fits-假的 Required。
        let first_required_unfit = specs.iter().find(|s| !s.optional && !spec_fits(s, &profile));

        if let Some(req) = first_required_unfit {
            let err = result.expect_err("required unmet");
            prop_assert_eq!(err.code, ErrorCode::LdrProfileRequiredSectionMissing);
            // 报告的是 manifest 顺序第一个 fits-假的 Required
            match err.context.get("section_id") {
                Some(fayger_error::ContextValue::String(id)) => prop_assert_eq!(id, &req.id),
                other => prop_assert!(false, "expected section_id, got {:?}", other),
            }
        } else {
            let sel = result.expect("ok");
            // selected 恰为 fits 的 id（manifest 顺序）
            let expected_selected: Vec<String> = specs
                .iter()
                .filter(|s| spec_fits(s, &profile))
                .map(|s| s.id.clone())
                .collect();
            prop_assert_eq!(&sel.selected, &expected_selected);
            // skipped 恰为 Optional 且 fits-假，reasons 非空
            let expected_skipped: Vec<String> = specs
                .iter()
                .filter(|s| s.optional && !spec_fits(s, &profile))
                .map(|s| s.id.clone())
                .collect();
            let got_skipped: Vec<String> =
                sel.skipped.iter().map(|r| r.section_id.clone()).collect();
            prop_assert_eq!(got_skipped, expected_skipped);
            for rec in &sel.skipped {
                prop_assert!(!rec.reasons.is_empty());
            }

            // 确定性
            let again = select_sections(&manifest, &profile).expect("ok2");
            prop_assert_eq!(sel, again);
        }
    }

    /// 单个 Optional 不满足特征 → 跳过且 reason 含 FeatureMissing。
    #[test]
    fn optional_feature_missing_is_skipped(len in 1u64..=64) {
        let specs = vec![SectionSpec {
            id: "gpu".into(),
            length: len,
            required_features: std::iter::once("gpu".to_string()).collect(),
            max_size: None,
            optional: true,
        }];
        let manifest = build_manifest(&specs);
        let profile = LoadProfile::new("t", BTreeSet::new(), None, BTreeSet::new());
        let sel = select_sections(&manifest, &profile).expect("ok");
        prop_assert!(sel.selected.is_empty());
        prop_assert_eq!(sel.skipped.len(), 1);
        prop_assert!(sel.skipped[0].reasons.contains(&SkipReason::FeatureMissing));
    }
}
