//! Feature: fayger-m2, Property 2 & 3: registration completeness & routing.
//!
//! Property 2：`register(d)` 成功 ⟺ REQUIRED ⊆ d.supported_operations；
//! 失败时 missing_methods == REQUIRED \ supported_operations。
//! Property 3：resolve 满足策略决策表、确定性、且返回的实现恒兼容。
//!
//! Validates: Requirements 2.3, 2.4, 3.2-3.6, 9.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::types::{RuntimeRequirement, SelectionStrategy};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SemVer};
use fayger_error::ErrorCode;
use fayger_runtime::{RuntimeDescriptor, RuntimeOperation, RuntimeRegistry};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

fn all_ops() -> Vec<RuntimeOperation> {
    RuntimeOperation::required().into_iter().collect()
}

/// 生成必需操作集合的任意子集。
fn arb_op_subset() -> impl Strategy<Value = BTreeSet<RuntimeOperation>> {
    let ops = all_ops();
    proptest::collection::vec(any::<bool>(), ops.len()).prop_map(move |mask| {
        ops.iter()
            .zip(mask)
            .filter_map(|(op, keep)| keep.then_some(*op))
            .collect()
    })
}

fn arb_semver() -> impl Strategy<Value = SemVer> {
    (0u32..=5, 0u32..=5, 0u32..=5).prop_map(|(a, b, c)| SemVer {
        major: a,
        minor: b,
        patch: c,
        pre: None,
    })
}

fn manifest(rt_min: SemVer, runtime: Option<RuntimeRequirement>) -> BuFManifest {
    BuFManifest {
        schema_version: SemVer::parse("1.0.0").expect("ok"),
        runtime_interface_min: rt_min,
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![],
        signature: None,
        extensions: BTreeMap::new(),
    }
}

fn arb_strategy() -> impl Strategy<Value = SelectionStrategy> {
    prop_oneof![
        Just(SelectionStrategy::Strict),
        Just(SelectionStrategy::PreferThenAny),
        Just(SelectionStrategy::Any),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 2：注册完备性集合代数。
    #[test]
    fn register_iff_superset(ops in arb_op_subset(), ver in arb_semver()) {
        let mut reg = RuntimeRegistry::new();
        let desc = RuntimeDescriptor {
            impl_id: "x".into(),
            rt_version: ver,
            supported_operations: ops.clone(),
        };
        let required = RuntimeOperation::required();
        let is_superset = required.is_subset(&ops);
        let result = reg.register(desc);

        if is_superset {
            prop_assert!(result.is_ok());
        } else {
            let err = result.expect_err("incomplete must fail");
            prop_assert_eq!(err.code, ErrorCode::RtImplMissingMethods);
            // missing_methods == REQUIRED \ ops
            let expected_missing: BTreeSet<String> = required
                .difference(&ops)
                .map(|op| format!("{op:?}"))
                .collect();
            let got_missing: BTreeSet<String> = match err.context.get("missing_methods") {
                Some(fayger_error::ContextValue::List(l)) => l
                    .iter()
                    .filter_map(|v| match v {
                        fayger_error::ContextValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => BTreeSet::new(),
            };
            prop_assert_eq!(got_missing, expected_missing);
        }
    }

    /// Property 3：路由决策、确定性、兼容性。
    #[test]
    fn resolve_decision_and_determinism(
        versions in prop::collection::vec(arb_semver(), 1..=5),
        rt_min in arb_semver(),
        strategy in arb_strategy(),
        preferred_idx in prop::option::of(0usize..5),
    ) {
        let mut reg = RuntimeRegistry::new();
        for (i, ver) in versions.iter().enumerate() {
            reg.register(RuntimeDescriptor::complete(format!("impl{i}"), ver.clone()))
                .expect("complete register");
        }

        // preferred 指向某个已注册实现（若 idx 在范围内）。
        let preferred = preferred_idx
            .filter(|&i| i < versions.len())
            .map(|i| format!("impl{i}"));
        let m = manifest(
            rt_min.clone(),
            Some(RuntimeRequirement {
                preferred_impl: preferred,
                selection_strategy: strategy,
            }),
        );

        let result = reg.resolve(&m);

        // 计算期望：候选 = 兼容（rt_version >= rt_min）的注册项，按注册顺序。
        let compatible: Vec<usize> = versions
            .iter()
            .enumerate()
            .filter(|(_, v)| **v >= rt_min)
            .map(|(i, _)| i)
            .collect();
        // preferred 命中：preferred_idx 指向的实现存在且兼容。
        let preferred_compatible = preferred_idx
            .filter(|&i| i < versions.len() && versions[i] >= rt_min);

        match strategy {
            SelectionStrategy::Strict => {
                if let Some(pi) = preferred_compatible {
                    prop_assert_eq!(result.expect("strict hit").0, pi);
                } else {
                    prop_assert_eq!(result.expect_err("strict miss").code, ErrorCode::RtImplNotFound);
                }
            }
            SelectionStrategy::PreferThenAny => {
                if let Some(pi) = preferred_compatible {
                    prop_assert_eq!(result.expect("prefer hit").0, pi);
                } else if let Some(&first) = compatible.first() {
                    prop_assert_eq!(result.expect("prefer fallback").0, first);
                } else {
                    prop_assert_eq!(result.expect_err("none").code, ErrorCode::RtImplNotFound);
                }
            }
            SelectionStrategy::Any => {
                if let Some(&first) = compatible.first() {
                    prop_assert_eq!(result.expect("any hit").0, first);
                } else {
                    prop_assert_eq!(result.expect_err("none").code, ErrorCode::RtImplNotFound);
                }
            }
        }

        // 确定性 + 兼容性：再次 resolve 相同；命中者恒兼容。
        let again = reg.resolve(&m);
        match (reg.resolve(&m), again) {
            (Ok(a), Ok(b)) => {
                prop_assert_eq!(a, b);
                prop_assert!(versions[a.0] >= rt_min);
            }
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "resolve not deterministic"),
        }
    }
}
