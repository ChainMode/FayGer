//! Feature: fayger-m6, Property 2 & 3: selected content consistency & skipped inaccessibility.
//!
//! Property 2：被选中 Section 内容与 M5 全选（load_from_source）在相同 Section 上逐字节一致。
//! Property 3：被跳过 id 访问报 SectionSkipped；Eager 不含被跳过段、Lazy 不读被跳过段。
//!
//! Validates: Requirements 4.2, 4.3, 4.4, 4.5, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation,
    clippy::redundant_clone
)]

use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_error::ErrorCode;
use fayger_loader::{
    load_from_source, load_with_profile, seal, LoadProfile, LoadStrategy, MemorySource,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

/// 生成的 section 规格（含内容、是否可选、所需特征）。
#[derive(Debug, Clone)]
struct Spec {
    id: String,
    payload: Vec<u8>,
    optional: bool,
    required_feature: Option<String>,
}

fn arb_spec(idx: usize) -> impl Strategy<Value = Spec> {
    (
        proptest::collection::vec(any::<u8>(), 0..=32),
        any::<bool>(),
        prop::option::of(prop_oneof![Just("gpu"), Just("audio")]),
    )
        .prop_map(move |(payload, optional, feat)| {
            // 第 0 段强制 Required 且无约束，保证整体加载在空 profile 下不因 Required 失败。
            let is_optional = if idx == 0 { false } else { optional };
            // required_feature 只挂在 Optional 段上：Required 段保持无约束，
            // 这样空 profile 下不会因 Required 缺失约束而整体失败（该路径由 selection.rs 与 e2e 专门测）。
            let required_feature = if is_optional {
                feat.map(str::to_string)
            } else {
                None
            };
            Spec {
                id: format!("s{idx}"),
                payload,
                optional: is_optional,
                required_feature,
            }
        })
}

fn arb_specs() -> impl Strategy<Value = Vec<Spec>> {
    (1usize..=5).prop_flat_map(|n| (0..n).map(arb_spec).collect::<Vec<_>>())
}

/// 把 specs 构造为 sealed BuF 字节。
fn build_sealed(specs: &[Spec]) -> Vec<u8> {
    let descriptors = specs
        .iter()
        .map(|s| SectionDescriptor {
            id: s.id.clone(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: s.payload.len() as u64,
            visibility: if s.optional {
                SectionVisibility::Optional
            } else {
                SectionVisibility::Required
            },
            profile_constraints: ProfileConstraints {
                required_capabilities: BTreeSet::new(),
                max_size: None,
                required_features: s.required_feature.iter().cloned().collect(),
            },
        })
        .collect();
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").expect("ok"),
        runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: descriptors,
        signature: None,
        extensions: BTreeMap::new(),
    };
    let sections = specs
        .iter()
        .map(|s| Section::new(s.id.clone(), s.payload.clone()))
        .collect();
    let mut obj = BuFObject {
        manifest,
        sections,
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    };
    seal(&mut obj).expect("seal");
    serialize(&obj).expect("serialize")
}

/// 空 profile（无特征）：所有带 required_feature 的 Optional 都会被跳过。
fn empty_profile() -> LoadProfile {
    LoadProfile::new("t", BTreeSet::new(), None, BTreeSet::new())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 被选中段内容与 M5 全选一致；被跳过段访问报 SectionSkipped。
    #[test]
    fn selected_content_matches_full_load(specs in arb_specs(), strategy in prop_oneof![Just(LoadStrategy::Eager), Just(LoadStrategy::Lazy)]) {
        let bytes = build_sealed(&specs);

        // M5 全选基线（用同策略）
        let mut full = load_from_source(Box::new(MemorySource::new(bytes.clone())), strategy)
            .expect("full load");

        // M6 部分加载（空 profile）
        let mut partial = load_with_profile(
            Box::new(MemorySource::new(bytes.clone())),
            strategy,
            &empty_profile(),
        )
        .expect("partial load");

        // 被选中段内容逐字节等于全选。
        for id in partial.selected_ids().to_vec() {
            let p = partial.section(&id).expect("partial section").to_vec();
            let f = full.section(&id).expect("full section").to_vec();
            prop_assert_eq!(p, f, "selected section {} differs", id);
        }

        // 被跳过段访问报 SectionSkipped。
        for rec in partial.skipped().to_vec() {
            let err = partial.section(&rec.section_id).expect_err("skipped must error");
            prop_assert_eq!(err.code, ErrorCode::LdrParseFail);
            match err.context.get("reason") {
                Some(fayger_error::ContextValue::String(s)) => prop_assert_eq!(s.as_str(), "SectionSkipped"),
                other => prop_assert!(false, "expected SectionSkipped, got {:?}", other),
            }
        }

        // selected ∩ skipped == ∅（互斥完整）
        let skipped_ids: BTreeSet<String> =
            partial.skipped().iter().map(|r| r.section_id.clone()).collect();
        for id in partial.selected_ids() {
            prop_assert!(!skipped_ids.contains(id));
        }
    }
}
