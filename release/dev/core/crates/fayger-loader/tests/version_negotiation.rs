//! Feature: fayger-m1, Property 4: version negotiation decision table.
//!
//! 对任意 `(schema_version, runtime_interface_min, policy)` 组合，`negotiate_version`
//! 的结果必须满足设计文档决策表，且 schema 错误优先于 runtime 错误。
//!
//! Validates: Requirements 7.1, 7.2, 7.3, 7.4, 8.1, 8.2, 8.3, 9.3, 11.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone
)]

use fayger_buf::manifest::{BuFManifest, EntryPoint, SemVer};
use fayger_buf::testing::arb_semver;
use fayger_error::ErrorCode;
use fayger_loader::{negotiate_version, LoadPolicy};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

fn manifest_with(schema: SemVer, rt_min: SemVer) -> BuFManifest {
    BuFManifest {
        schema_version: schema,
        runtime_interface_min: rt_min,
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![],
        signature: None,
        extensions: BTreeMap::new(),
    }
}

/// 生成一个策略：随机区间 [low, high]（保证 low <= high）、随机弃用集合、随机 provided。
fn arb_policy() -> impl Strategy<Value = LoadPolicy> {
    (
        arb_semver(),
        arb_semver(),
        arb_semver(),
        prop::collection::vec(arb_semver(), 0..=4),
    )
        .prop_map(|(a, b, provided, deprecated)| {
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            LoadPolicy::new(
                (low, high),
                deprecated.into_iter().collect::<BTreeSet<_>>(),
                provided,
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 决策表：对任意组合，negotiate_version 的结果与设计文档一致。
    #[test]
    fn negotiation_matches_decision_table(
        schema in arb_semver(),
        rt_min in arb_semver(),
        policy in arb_policy(),
    ) {
        let m = manifest_with(schema.clone(), rt_min.clone());
        let (low, high) = &policy.supported_schema_range;

        let schema_in_range = schema >= *low && schema <= *high;
        let runtime_ok = rt_min <= policy.runtime_interface_provided;
        let deprecated = policy.deprecated_schemas.contains(&schema);

        let result = negotiate_version(&m, &policy);

        if !schema_in_range {
            // schema 超界优先于一切
            let err = result.expect_err("schema out of range must error");
            prop_assert_eq!(err.code, ErrorCode::LdrSchemaUnsupported);
        } else if !runtime_ok {
            let err = result.expect_err("runtime too high must error");
            prop_assert_eq!(err.code, ErrorCode::LdrRuntimeVersionTooHigh);
        } else if deprecated {
            let notice = result.expect("should be Ok with notice");
            prop_assert!(notice.is_some(), "deprecated schema must yield a notice");
        } else {
            let notice = result.expect("should be Ok");
            prop_assert_eq!(notice, None);
        }
    }

    /// schema 错误严格优先于 runtime 错误：当二者都不满足时，必返回 schema 错误。
    ///
    /// 为避免随机生成"双重失败"组合的低命中率（`prop_assume` 拒绝过多），这里直接
    /// **构造**保证双重失败的输入：schema 取严格大于区间上界的版本（major+1），
    /// runtime_min 取严格大于 provided 的版本（major+1）。
    #[test]
    fn schema_error_strictly_precedes_runtime_error(policy in arb_policy()) {
        let (_low, high) = &policy.supported_schema_range;
        // schema 严格超过上界：保证 schema 超界。
        let schema = SemVer {
            major: high.major.saturating_add(1),
            minor: 0,
            patch: 0,
            pre: None,
        };
        // runtime_min 严格高于 provided：保证 runtime 过高。
        let provided = &policy.runtime_interface_provided;
        let rt_min = SemVer {
            major: provided.major.saturating_add(1),
            minor: 0,
            patch: 0,
            pre: None,
        };
        // saturating_add 在 u32::MAX 时不增长；跳过这种无法构造严格更大版本的边界样本。
        prop_assume!(schema > *high && rt_min > *provided);

        let m = manifest_with(schema, rt_min);
        let err = negotiate_version(&m, &policy).expect_err("both fail");
        prop_assert_eq!(err.code, ErrorCode::LdrSchemaUnsupported);
    }
}
