//! Feature: fayger-m2, Property 5: suspend stops dispatch / resume continues.
//!
//! 对走完 load→init→start 的实例与任意 step/suspend/resume 操作序列：Suspended 期间每次
//! step 都不增加 Adapter 收到的指令数；整个过程中 Adapter 收到的指令序列恰为 mock 程序
//! 的一个前缀，且顺序与 program 一致。
//!
//! Validates: Requirements 6.1-6.4, 9.1, 9.2

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
use fayger_runtime::{LifecycleState, MockAdapter, Runtime, RuntimeInterface};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

/// 三种被测操作。
#[derive(Debug, Clone, Copy)]
enum Op {
    Step,
    Suspend,
    Resume,
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![Just(Op::Step), Just(Op::Suspend), Just(Op::Resume)]
}

fn object_with(payload_lens: &[usize]) -> BuFObject {
    let descriptors: Vec<SectionDescriptor> = payload_lens
        .iter()
        .enumerate()
        .map(|(i, &len)| SectionDescriptor {
            id: format!("s{i}"),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: len as u64,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let sections: Vec<Section> = payload_lens
        .iter()
        .enumerate()
        .map(|(i, &len)| Section::new(format!("s{i}"), vec![0u8; len]))
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

    /// Suspended 期间 step 不增加派发计数；Adapter 收到的恒为 program 前缀。
    #[test]
    fn suspend_halts_dispatch_and_received_is_prefix(
        payload_lens in prop::collection::vec(0usize..=8, 0..=6),
        ops in prop::collection::vec(arb_op(), 0..=24),
    ) {
        let mut r = Runtime::new(MockAdapter::new());
        let obj = object_with(&payload_lens);
        let id = r.load(obj).expect("load");
        r.init(id).expect("init");
        r.start(id).expect("start");

        let program_len = payload_lens.len();
        let mut expected_dispatched = 0usize;

        for op in ops {
            let state_before = r.state(id).expect("state");
            let count_before = r.adapter().received.len();

            match op {
                Op::Step => {
                    r.step(id).expect("step");
                    if state_before == LifecycleState::Running && expected_dispatched < program_len {
                        expected_dispatched += 1;
                    }
                    // Suspended 时 step 不得增加计数。
                    if state_before == LifecycleState::Suspended {
                        prop_assert_eq!(r.adapter().received.len(), count_before);
                    }
                }
                Op::Suspend => { let _ = r.suspend(id); }
                Op::Resume => { let _ = r.resume(id); }
            }

            // 收到的指令数恒等于期望派发数。
            prop_assert_eq!(r.adapter().received.len(), expected_dispatched);
        }

        // 收到的指令序列恰为 program 的前缀：opcode 依次为 0,1,2,...
        for (i, instr) in r.adapter().received.iter().enumerate() {
            prop_assert_eq!(instr.opcode as usize, i);
            prop_assert_eq!(instr.category, 0u8);
            // operands = 第 i 段 payload 长度的小端 u32
            let expected_len = payload_lens[i] as u32;
            prop_assert_eq!(instr.operands.clone(), expected_len.to_le_bytes().to_vec());
        }
    }
}
