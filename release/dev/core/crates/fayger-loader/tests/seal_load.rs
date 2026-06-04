//! Feature: fayger-m1, Property 1: seal-load round-trip.
//!
//! 对任意合法 BuFObject `o`，`seal(&mut o)` 后 `bytes = serialize(o)`，则
//! `load(bytes, policy)` 必须成功，且其完整性校验全通过；进一步
//! `serialize(load(bytes, policy)) == bytes`（继承 M0 字节级强往返）。
//!
//! Validates: Requirements 5.4, 9.4, 11.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::SemVer;
use fayger_buf::testing::arb_buf_object;
use fayger_buf::{parse, serialize};
use fayger_loader::{load, seal, verify_digest, LoadPolicy};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

/// 构造一个对任意 manifest 都接受的宽松策略：
/// schema 区间覆盖 [0.0.0, u32::MAX.MAX.MAX]，无弃用，runtime 提供版本取最高。
fn permissive_policy() -> LoadPolicy {
    LoadPolicy::new(
        (
            SemVer {
                major: 0,
                minor: 0,
                patch: 0,
                pre: None,
            },
            SemVer {
                major: u32::MAX,
                minor: u32::MAX,
                patch: u32::MAX,
                pre: None,
            },
        ),
        BTreeSet::new(),
        SemVer {
            major: u32::MAX,
            minor: u32::MAX,
            patch: u32::MAX,
            pre: None,
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// seal -> serialize -> load 成功，且 verify_digest 全通过。
    #[test]
    fn sealed_object_loads_and_verifies(mut obj in arb_buf_object()) {
        seal(&mut obj).expect("seal should succeed for a well-formed object");
        let bytes = serialize(&obj).expect("serialize should succeed");

        // 独立地确认 verify_digest 通过（load 内部也会做，这里显式断言一次）。
        let parsed = parse(&bytes).expect("parse should succeed");
        verify_digest(&bytes, &parsed).expect("sealed object must pass digest verification");

        let loaded = load(&bytes, &permissive_policy()).expect("load should succeed");
        prop_assert_eq!(loaded.sections.len(), obj.sections.len());
        // 宽松策略无弃用，notice 应为 None。
        prop_assert_eq!(loaded.manifest.deprecation_notice.clone(), None);
    }

    /// 字节级强往返：load 后 serialize 应与原字节相等（继承 M0 Property 2）。
    #[test]
    fn seal_load_serialize_is_byte_identical(mut obj in arb_buf_object()) {
        seal(&mut obj).expect("seal");
        let bytes = serialize(&obj).expect("serialize");
        let loaded = load(&bytes, &permissive_policy()).expect("load");
        let re = serialize(&loaded).expect("re-serialize");
        prop_assert_eq!(bytes, re);
    }
}
