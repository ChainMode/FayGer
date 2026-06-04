//! Feature: fayger-m3, Property 5: cross-adapter behavior consistency.
//!
//! 两个声明相同 supported_capabilities 与相同 supported_instructions 的适配器，对同一指令序列
//! 做 translate 的接受/拒绝判定逐条一致；对受支持指令，HostSyscall 的 name/args 一致
//! （仅 platform_id 可不同）。
//!
//! Validates: Requirements 8.1, 8.2, 10.1

#![allow(missing_docs, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_adapter::adapter::{translate_via_descriptor, AdapterDescriptor};
use fayger_adapter::{Capability, InstructionCategory, InstructionKey, UniversalInstruction};
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

fn arb_instruction() -> impl Strategy<Value = UniversalInstruction> {
    (
        arb_category(),
        0u16..=12,
        proptest::collection::vec(any::<u8>(), 0..=8),
    )
        .prop_map(|(cat, op, operands)| {
            UniversalInstruction::new(cat, op, operands, BTreeSet::<Capability>::new())
        })
}

/// 两个 descriptor，supported_instructions / capabilities 相同，仅 platform_id 不同。
fn twin_descriptors() -> (AdapterDescriptor, AdapterDescriptor) {
    let supported_instructions: BTreeSet<InstructionKey> = [
        InstructionKey::new(InstructionCategory::Io, 0),
        InstructionKey::new(InstructionCategory::Net, 1),
        InstructionKey::new(InstructionCategory::Time, 3),
        InstructionKey::new(InstructionCategory::Host, 7),
    ]
    .into_iter()
    .collect();
    let supported_capabilities = BTreeSet::new();
    let a = AdapterDescriptor {
        platform_id: "twin-a".into(),
        supported_capabilities: supported_capabilities.clone(),
        supported_instructions: supported_instructions.clone(),
    };
    let b = AdapterDescriptor {
        platform_id: "twin-b".into(),
        supported_capabilities,
        supported_instructions,
    };
    (a, b)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn twins_agree_on_accept_reject_and_payload(
        instrs in proptest::collection::vec(arb_instruction(), 0..=16),
    ) {
        let (da, db) = twin_descriptors();
        for instr in &instrs {
            let ra = translate_via_descriptor(&da, instr);
            let rb = translate_via_descriptor(&db, instr);

            match (ra, rb) {
                (Ok(sa), Ok(sb)) => {
                    // 受支持：name/args 一致，platform_id 可不同。
                    prop_assert_eq!(&sa.name, &sb.name);
                    prop_assert_eq!(&sa.args, &sb.args);
                    prop_assert_eq!(&sa.platform_id, "twin-a");
                    prop_assert_eq!(&sb.platform_id, "twin-b");
                }
                (Err(ea), Err(eb)) => {
                    // 同样拒绝且同错误码。
                    prop_assert_eq!(ea.code, eb.code);
                }
                _ => prop_assert!(false, "twins disagreed on accept/reject"),
            }
        }
    }
}
