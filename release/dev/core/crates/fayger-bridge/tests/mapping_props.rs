//! Feature: fayger-m10, Property 1: 指令翻译确定性与字段保持。
//!
//! 对任意运行层指令与映射：to_adapter_instruction 产出的适配层指令 operands 等于输入，
//! capabilities_required 等于映射声明，category/opcode 遵守覆盖规则；同输入两次调用相等。
//!
//! Validates: Requirements 1.1, 1.2, 1.3, 1.4, 1.5, 1.6

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_adapter::{Capability, InstructionCategory};
use fayger_bridge::{to_adapter_instruction, InstructionMapping};
use fayger_runtime::UniversalInstruction as RtInstruction;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

fn arb_category() -> impl Strategy<Value = InstructionCategory> {
    prop_oneof![
        Just(InstructionCategory::Io),
        Just(InstructionCategory::Net),
        Just(InstructionCategory::Ui),
        Just(InstructionCategory::Time),
        Just(InstructionCategory::Random),
        Just(InstructionCategory::Crypto),
        Just(InstructionCategory::Proc),
        Just(InstructionCategory::Host),
    ]
}

fn arb_caps() -> impl Strategy<Value = BTreeSet<Capability>> {
    proptest::collection::btree_set(
        prop_oneof![
            Just("fs.read".to_owned()),
            Just("net.http".to_owned()),
            Just("proc.spawn".to_owned()),
            Just("host.call".to_owned()),
        ],
        0..=4,
    )
    .prop_map(|s| s.into_iter().map(Capability::new).collect())
}

fn arb_mapping() -> impl Strategy<Value = InstructionMapping> {
    (
        proptest::option::of(arb_category()),
        proptest::option::of(any::<u16>()),
        arb_caps(),
    )
        .prop_map(|(category, opcode, caps)| InstructionMapping {
            category,
            opcode,
            capabilities_required: caps,
        })
}

fn arb_rt_instruction() -> impl Strategy<Value = RtInstruction> {
    (
        any::<u8>(),
        any::<u16>(),
        proptest::collection::vec(any::<u8>(), 0..=16),
    )
        .prop_map(|(c, o, ops)| RtInstruction::new(c, o, ops))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 字段保持 + 覆盖规则 + 确定性。
    #[test]
    fn translation_preserves_fields(instr in arb_rt_instruction(), mapping in arb_mapping()) {
        let out = to_adapter_instruction(&instr, &mapping);

        // operands 原样
        prop_assert_eq!(&out.operands, &instr.operands);
        // capabilities_required 等于映射声明
        prop_assert_eq!(&out.capabilities_required, &mapping.capabilities_required);
        // category 覆盖规则
        let expected_cat = mapping
            .category
            .unwrap_or_else(|| InstructionCategory::from_raw(instr.category));
        prop_assert_eq!(out.category, expected_cat);
        // opcode 覆盖规则
        let expected_op = mapping.opcode.unwrap_or(instr.opcode);
        prop_assert_eq!(out.opcode, expected_op);

        // 确定性
        let out2 = to_adapter_instruction(&instr, &mapping);
        prop_assert_eq!(out, out2);
    }
}
