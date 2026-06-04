//! Feature: fayger-m2, Property 4: failure preservation.
//!
//! 对任意非终态实例，inject_fault 后：state == Failed；failure_info 为 Some，其
//! error.code == RT_INSTANCE_FAILED，last_instruction 等于失败前最后一次 step 派发的指令；
//! 非 Failed 实例的 failure_info 为 None。
//!
//! Validates: Requirements 7.1-7.4, 9.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_error::ErrorCode;
use fayger_runtime::{InstanceId, LifecycleState, MockAdapter, Runtime, RuntimeInterface};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

fn object_with(n_sections: usize) -> BuFObject {
    let descriptors: Vec<SectionDescriptor> = (0..n_sections)
        .map(|i| SectionDescriptor {
            id: format!("s{i}"),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: 1,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let sections: Vec<Section> = (0..n_sections)
        .map(|i| Section::new(format!("s{i}"), vec![i as u8]))
        .collect();
    BuFObject {
        manifest: BuFManifest {
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
        },
        sections,
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

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// inject_fault 后进入 Failed，FailureInfo 完整且 last_instruction 与最后派发一致。
    #[test]
    fn fault_after_n_steps_preserves_last_instruction(
        n_sections in 1usize..=6,
        steps in 0usize..=6,
    ) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(n_sections)).expect("load");
        r.init(id).expect("init");
        r.start(id).expect("start");

        // 推进 steps 步（不超过程序长度时才会真正派发）。
        let mut dispatched = 0usize;
        for _ in 0..steps {
            if r.step(id).expect("step") {
                dispatched += 1;
            }
        }

        // 在 Running 状态注入未处理错误。
        r.inject_fault(id, "boom").expect("inject");
        prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Failed);

        let fi = r.failure_info(id).expect("query").expect("some failure");
        prop_assert_eq!(fi.error.code, ErrorCode::RtInstanceFailed);

        if dispatched == 0 {
            prop_assert!(fi.last_instruction.is_none());
        } else {
            // 最后派发的指令应是 program[dispatched-1]：opcode == dispatched-1。
            let last = fi.last_instruction.as_ref().expect("has last");
            prop_assert_eq!(last.opcode as usize, dispatched - 1);
        }
    }

    /// 未失败实例的 failure_info 恒为 None。
    #[test]
    fn non_failed_has_no_failure_info(n_sections in 0usize..=4) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(n_sections)).expect("load");
        prop_assert!(r.failure_info(id).expect("query").is_none());
        r.init(id).expect("init");
        prop_assert!(r.failure_info(id).expect("query").is_none());
    }

    /// 终态（Terminated）拒绝 inject_fault。
    #[test]
    fn terminated_rejects_fault(_unit in Just(())) {
        let mut r = Runtime::new(MockAdapter::new());
        let id: InstanceId = r.load(object_with(0)).expect("load");
        r.terminate(id).expect("terminate");
        prop_assert!(r.inject_fault(id, "boom").is_err());
    }
}
