//! Feature: fayger-m7, Property 3 + 4: 未超限/无配额不触发 / 非 Running 不自动迁移。
//!
//! Property 3：Running 实例无配额时任意用量都不改变状态、不设置 quota_exceeded；有配额但本次
//! 未超限时状态不变且 quota_exceeded 为 None。
//!
//! Property 4：非 Running（Initialized / Suspended / Terminated / Failed）实例注入超限用量不
//! 因超限自动迁移（保持原状态），但 last_usage 被更新。
//!
//! Validates: Requirements 2.3, 2.4, 4.4, 4.5, 8.6, 7.1

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
    first_exceeded, LifecycleState, MockAdapter, ResourceUsage, Runtime, RuntimeInterface,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

fn object_with(quotas: Option<ResourceQuota>) -> BuFObject {
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
            quotas,
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

fn arb_usage() -> impl Strategy<Value = ResourceUsage> {
    (0u64..=u64::MAX, 0u64..=u64::MAX, 0u64..=u64::MAX)
        .prop_map(|(c, m, i)| ResourceUsage::new(c, m, i))
}

/// 生成"未超限于给定配额"的用量（每维 <= 对应上限，未声明维度任取）。
fn arb_within(quota: &ResourceQuota) -> impl Strategy<Value = ResourceUsage> {
    let cpu_max = quota.cpu_ms_per_sec.map_or(u64::MAX, u64::from);
    let mem_max = quota.memory_bytes.unwrap_or(u64::MAX);
    let io_max = quota.io_bytes_per_sec.unwrap_or(u64::MAX);
    (0u64..=cpu_max, 0u64..=mem_max, 0u64..=io_max)
        .prop_map(|(c, m, i)| ResourceUsage::new(c, m, i))
}

/// 非 None 的配额（至少一维声明）。
fn arb_some_quota() -> impl Strategy<Value = ResourceQuota> {
    (
        proptest::option::of(1u32..=100),
        proptest::option::of(1u64..=100),
        proptest::option::of(1u64..=100),
    )
        .prop_filter("at least one dimension", |(c, m, i)| {
            c.is_some() || m.is_some() || i.is_some()
        })
        .prop_map(|(cpu, mem, io)| ResourceQuota {
            cpu_ms_per_sec: cpu,
            memory_bytes: mem,
            io_bytes_per_sec: io,
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 3a：无配额时任意用量都不改变状态、不设置 quota_exceeded。
    #[test]
    fn no_quota_never_suspends(usages in proptest::collection::vec(arb_usage(), 1..=8)) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(None)).expect("load");
        r.init(id).expect("init");
        r.start(id).expect("start");
        for u in usages {
            r.observe_usage(id, u).expect("observe");
            prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Running);
            prop_assert_eq!(r.quota_exceeded(id).expect("qe"), None);
            prop_assert_eq!(r.usage(id).expect("usage"), Some(u));
        }
    }

    /// Property 3b：有配额但未超限时状态不变、quota_exceeded 为 None。
    #[test]
    fn within_quota_keeps_running(
        quota in arb_some_quota(),
        seed in any::<u64>(),
    ) {
        // 用 seed 派生一个"必然未超限"的用量。
        let mut runner_usage = {
            // 直接构造未超限用量（取每维上限本身，等于上限不算越界）。
            let cpu = quota.cpu_ms_per_sec.map_or(seed % 50, u64::from);
            let mem = quota.memory_bytes.unwrap_or(seed % 50);
            let io = quota.io_bytes_per_sec.unwrap_or(seed % 50);
            ResourceUsage::new(cpu, mem, io)
        };
        // 确保确实未超限（保险起见，超限则夹到上限）。
        if first_exceeded(&quota, &runner_usage).is_some() {
            runner_usage = ResourceUsage::new(
                quota.cpu_ms_per_sec.map_or(0, u64::from),
                quota.memory_bytes.unwrap_or(0),
                quota.io_bytes_per_sec.unwrap_or(0),
            );
        }
        prop_assert!(first_exceeded(&quota, &runner_usage).is_none());

        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(Some(quota))).expect("load");
        r.init(id).expect("init");
        r.start(id).expect("start");
        r.observe_usage(id, runner_usage).expect("observe");
        prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Running);
        prop_assert_eq!(r.quota_exceeded(id).expect("qe"), None);
    }

    /// Property 3c（生成器版）：未超限用量恒不暂停。
    #[test]
    fn generated_within_quota_keeps_running(
        (quota, usage) in arb_some_quota().prop_flat_map(|q| {
            let qc = q.clone();
            arb_within(&qc).prop_map(move |u| (q.clone(), u))
        }),
    ) {
        prop_assert!(first_exceeded(&quota, &usage).is_none());
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(Some(quota))).expect("load");
        r.init(id).expect("init");
        r.start(id).expect("start");
        r.observe_usage(id, usage).expect("observe");
        prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Running);
        prop_assert_eq!(r.quota_exceeded(id).expect("qe"), None);
    }

    /// Property 4：非 Running 状态注入超限用量不迁移，但更新 last_usage。
    #[test]
    fn non_running_does_not_migrate_on_exceed(
        target in 0u8..=3,
        big in 101u64..=10_000,
    ) {
        // 配额：memory <= 100，注入 memory = big 必然超限。
        let quota = ResourceQuota {
            cpu_ms_per_sec: None,
            memory_bytes: Some(100),
            io_bytes_per_sec: None,
        };
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(Some(quota))).expect("load");

        // 把实例驱动到一个非 Running 目标状态。
        let expected = match target {
            0 => {
                // Loaded（不做任何迁移）
                LifecycleState::Loaded
            }
            1 => {
                r.init(id).expect("init");
                LifecycleState::Initialized
            }
            2 => {
                r.init(id).expect("init");
                r.start(id).expect("start");
                r.suspend(id).expect("suspend");
                LifecycleState::Suspended
            }
            _ => {
                r.terminate(id).expect("terminate");
                LifecycleState::Terminated
            }
        };
        prop_assert_eq!(r.state(id).expect("state"), expected);

        let usage = ResourceUsage::new(0, big, 0);
        r.observe_usage(id, usage).expect("observe");

        // 状态不变（不因超限自动迁移）。
        prop_assert_eq!(r.state(id).expect("state"), expected);
        // 非 Running 超限不设置 quota_exceeded。
        prop_assert_eq!(r.quota_exceeded(id).expect("qe"), None);
        // last_usage 被更新。
        prop_assert_eq!(r.usage(id).expect("usage"), Some(usage));
    }
}
