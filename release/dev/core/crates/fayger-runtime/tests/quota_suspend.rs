//! Feature: fayger-m7, Property 2: 配额超限触发暂停且维度确定。
//!
//! 对处于 Running 的实例、其配额 q 与用量序列 [u0, u1, ...]：设 k 为首个使
//! first_exceeded(q, uk) = Some(_) 的下标。则注入 uk 后实例 Suspended，且
//! quota_exceeded.resource == first_exceeded(q, uk)、.usage == uk；uk 之前保持 Running。
//!
//! Validates: Requirements 3.2, 3.3, 4.1, 4.2, 4.3, 7.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, ResourceQuota, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_runtime::{
    first_exceeded, LifecycleState, MockAdapter, ResourceKind, ResourceUsage, Runtime,
    RuntimeInterface,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

fn object_with_quota(quota: ResourceQuota) -> BuFObject {
    let descriptors = vec![SectionDescriptor {
        id: "s0".to_owned(),
        kind: SectionKind::Code,
        digest: Digest::placeholder(),
        length: 1,
        visibility: SectionVisibility::Required,
        profile_constraints: ProfileConstraints::default(),
    }];
    BuFObject {
        manifest: BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: Some(quota),
            sections: descriptors,
            signature: None,
            extensions: BTreeMap::new(),
        },
        sections: vec![Section::new("s0", vec![0u8])],
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    }
}

fn arb_quota() -> impl Strategy<Value = ResourceQuota> {
    (
        proptest::option::of(1u32..=100),
        proptest::option::of(1u64..=100),
        proptest::option::of(1u64..=100),
    )
        .prop_map(|(cpu, mem, io)| ResourceQuota {
            cpu_ms_per_sec: cpu,
            memory_bytes: mem,
            io_bytes_per_sec: io,
        })
}

fn arb_usage() -> impl Strategy<Value = ResourceUsage> {
    (0u64..=200, 0u64..=200, 0u64..=200).prop_map(|(c, m, i)| ResourceUsage::new(c, m, i))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 用量序列中首个超限采样使实例 Suspended，维度与用量与 first_exceeded 一致；之前保持 Running。
    #[test]
    fn first_exceeding_sample_suspends_with_correct_dimension(
        quota in arb_quota(),
        usages in proptest::collection::vec(arb_usage(), 1..=10),
    ) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with_quota(quota.clone())).expect("load");
        r.init(id).expect("init");
        r.start(id).expect("start");

        // 计算预期的首个超限下标。
        let first_idx = usages
            .iter()
            .position(|u| first_exceeded(&quota, u).is_some());

        let mut suspended_at: Option<usize> = None;
        for (i, u) in usages.iter().enumerate() {
            let state_before = r.state(id).expect("state");
            // 一旦已暂停（非 Running），后续注入不应再迁移。
            r.observe_usage(id, *u).expect("observe");
            let state_after = r.state(id).expect("state");

            // last_usage 总是被记录。
            prop_assert_eq!(r.usage(id).expect("usage"), Some(*u));

            if suspended_at.is_none() {
                // 暂停发生前一直保持 Running。
                if state_before == LifecycleState::Running
                    && state_after == LifecycleState::Suspended
                {
                    suspended_at = Some(i);
                    let expected_kind: ResourceKind =
                        first_exceeded(&quota, u).expect("must exceed at suspend");
                    let qe = r.quota_exceeded(id).expect("qe").expect("some");
                    prop_assert_eq!(qe.resource, expected_kind);
                    prop_assert_eq!(qe.usage, *u);
                } else {
                    // 尚未暂停：必然仍是 Running 且本次未超限。
                    prop_assert_eq!(state_after, LifecycleState::Running);
                    prop_assert!(first_exceeded(&quota, u).is_none());
                }
            }
        }

        prop_assert_eq!(suspended_at, first_idx);
    }
}
