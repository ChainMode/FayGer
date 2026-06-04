//! Feature: fayger-m10, Property 3 + 4 + 5: 能力裁剪 / 不支持 / 跨层错误链。
//!
//! Property 3：映射声明了宿主未授予能力 → dispatch_one 返回 ADP_CAPABILITY_DENIED。
//! Property 4：映射到所选适配器不支持的指令种类 → ADP_UNSUPPORTED_INSTRUCTION。
//! Property 5：wrap_dispatch_failure 产出的链 layer_chain == [Runtime, Adapter]、root_cause
//! 为该 ADP_*、被包装错误未改写、depth + 1。
//!
//! Validates: Requirements 3.1, 3.3, 3.4, 4.1, 4.2, 4.3, 4.4

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_adapter::builtin::CAP_HOST_CALL;
use fayger_adapter::{
    AdapterRegistry, Capability, HostEnvironment, HostPolicy, InstructionCategory,
    NativeDesktopAdapter,
};
use fayger_bridge::{dispatch_one, wrap_dispatch_failure, InstructionMapping};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use fayger_runtime::UniversalInstruction as RtInstruction;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

/// 只授予 host.call 的 native-desktop 注册表（用于能力裁剪测试）。
fn limited_desktop_registry() -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(Box::new(NativeDesktopAdapter::new()));
    let host = HostEnvironment::new(
        "native-desktop",
        "linux",
        "x86_64",
        "none",
        std::iter::once(Capability::new(CAP_HOST_CALL)).collect(),
    );
    r.select(&host, &HostPolicy::allow_all([CAP_HOST_CALL]))
        .expect("select");
    r
}

/// 未授予的能力（host 只给了 host.call）。
fn arb_ungranted_cap() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("fs.write".to_owned()),
        Just("net.http".to_owned()),
        Just("proc.spawn".to_owned()),
        Just("ui.dom".to_owned()),
    ]
}

fn arb_adp_error() -> impl Strategy<Value = FaygerError> {
    prop_oneof![
        Just(ErrorCode::AdpCapabilityDenied),
        Just(ErrorCode::AdpUnsupportedInstruction),
        Just(ErrorCode::AdpNoMatchingPlatform),
        Just(ErrorCode::AdpHostCallFailed),
    ]
    .prop_map(|code| {
        FaygerError::new(code, Layer::Adapter, "adapter failure")
            .with_context("k", ContextValue::string("v"))
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 3：越权能力被拒。
    #[test]
    fn ungranted_capability_is_denied(cap in arb_ungranted_cap(), operands in proptest::collection::vec(any::<u8>(), 0..=8)) {
        let registry = limited_desktop_registry();
        let mapping = InstructionMapping::canonical_io().requiring([cap.clone()]);
        let outcome = dispatch_one(&registry, &mapping, &RtInstruction::new(0, 0, operands));
        let err = outcome.expect_err("should be denied");
        prop_assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
        // missing 非空且包含该能力。
        match err.context.get("missing") {
            Some(ContextValue::List(items)) => {
                prop_assert!(!items.is_empty());
                let has = items.iter().any(|v| matches!(v, ContextValue::String(s) if s == &cap));
                prop_assert!(has, "missing should contain the ungranted cap");
            }
            other => prop_assert!(false, "expected missing list, got {:?}", other),
        }
    }

    /// Property 4：不支持的指令种类被拒。内置适配器每个类别只在某个特定 opcode 被支持
    /// （Io 仅 opcode 0）。故 category=Io、opcode>=1 必不被支持。无能力要求以避免先被能力拦截。
    #[test]
    fn unsupported_instruction_is_rejected(opcode in 1u16..=u16::MAX) {
        let registry = limited_desktop_registry();
        let mapping = InstructionMapping {
            category: Some(InstructionCategory::Io),
            opcode: Some(opcode),
            capabilities_required: BTreeSet::new(),
        };
        let outcome = dispatch_one(&registry, &mapping, &RtInstruction::new(0, 0, vec![]));
        let err = outcome.expect_err("should be unsupported");
        prop_assert_eq!(err.code, ErrorCode::AdpUnsupportedInstruction);
    }

    /// Property 5：跨层错误链顺序保持。
    #[test]
    fn wrap_preserves_cross_layer_chain(adp in arb_adp_error()) {
        let adp_code = adp.code;
        let base_depth = adp.depth();
        let wrapped = wrap_dispatch_failure(adp);
        prop_assert_eq!(wrapped.code, ErrorCode::RtInstanceFailed);
        prop_assert_eq!(wrapped.layer_chain(), vec![Layer::Runtime, Layer::Adapter]);
        prop_assert_eq!(wrapped.root_cause().code, adp_code);
        prop_assert_eq!(wrapped.root_cause().source_layer, Layer::Adapter);
        prop_assert!(wrapped.root_cause().context.contains_key("k"));
        prop_assert_eq!(wrapped.depth(), base_depth + 1);
    }
}
