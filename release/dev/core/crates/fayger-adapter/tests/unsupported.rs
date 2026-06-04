//! Feature: fayger-m3, Property 3: unsupported instruction handling.
//!
//! 对任意适配器与一条 InstructionKey 不在其 supported_instructions 中的指令：translate 必返回
//! ADP_UNSUPPORTED_INSTRUCTION（含 category/opcode），且不产生 HostSyscall；key 在集合内时返回 Ok。
//!
//! Validates: Requirements 2.5, 7.3, 7.4, 10.1

#![allow(missing_docs, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_adapter::{
    InstructionCategory, NativeDesktopAdapter, PlatformAdapter, ServerAdapter, UniversalInstruction,
};
use fayger_error::ErrorCode;
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

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 对 Server 适配器（其 supported_instructions 已知）任取一条指令：
    /// 支持 ⟺ translate Ok；不支持 ⟺ ADP_UNSUPPORTED_INSTRUCTION。
    #[test]
    fn translate_matches_supported_set(
        category in arb_category(),
        opcode in any::<u16>(),
    ) {
        let adapter = ServerAdapter::new();
        let supported = adapter.descriptor().supported_instructions;
        let instr = UniversalInstruction::new(category, opcode, vec![1, 2], BTreeSet::new());
        let key = instr.key();

        let result = adapter.translate(&instr);
        if supported.contains(&key) {
            let sc = result.expect("supported key must translate");
            prop_assert_eq!(sc.args, vec![1, 2]);
        } else {
            let err = result.expect_err("unsupported key must error");
            prop_assert_eq!(err.code, ErrorCode::AdpUnsupportedInstruction);
            prop_assert!(err.context.contains_key("category"));
            prop_assert!(err.context.contains_key("opcode"));
        }
    }

    /// NativeDesktop 支持全部 8 类的代表 opcode（keys_for 中 opcode = 类别序号）。
    /// 对一条已知支持的指令，translate 必成功且 name 形如 "{cat}.op{opcode}"。
    #[test]
    fn native_desktop_supported_translates(category in arb_category()) {
        let adapter = NativeDesktopAdapter::new();
        let key = adapter
            .descriptor()
            .supported_instructions
            .into_iter()
            .find(|k| k.category == category)
            .expect("native desktop supports every category");
        let instr =
            UniversalInstruction::new(key.category, key.opcode, vec![7], BTreeSet::new());
        let sc = adapter.translate(&instr).expect("supported");
        prop_assert_eq!(sc.name, format!("{}.op{}", category.as_str(), key.opcode));
    }
}
