//! Feature: fayger-m3, Property 1: adapter selection is decisive & deterministic.
//!
//! 对任意 HostEnvironment 与已注册适配器集合：无 matches→ADP_NO_MATCHING_PLATFORM 且不可服务；
//! 有 matches→按注册顺序选中首个匹配者且进入可服务；相同输入多次 select 结果相同。
//!
//! Validates: Requirements 4.2, 4.3, 4.4, 10.1

#![allow(missing_docs, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_adapter::{
    AdapterRegistry, BrowserAdapter, Capability, HostEnvironment, HostPolicy, InAppAdapter,
    NativeDesktopAdapter, ServerAdapter,
};
use fayger_error::ErrorCode;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

/// 五种 platform_id（含一个不匹配任何内置适配器的）。
fn arb_platform_id() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("native-desktop".to_string()),
        Just("server".to_string()),
        Just("browser".to_string()),
        Just("inapp".to_string()),
        Just("nonexistent".to_string()),
    ]
}

fn fresh_registry() -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(Box::new(NativeDesktopAdapter::new()));
    r.register(Box::new(ServerAdapter::new()));
    r.register(Box::new(BrowserAdapter::new()));
    r.register(Box::new(InAppAdapter::new()));
    r
}

/// 已注册适配器的 platform_id，按注册顺序。
const REGISTERED: [&str; 4] = ["native-desktop", "server", "browser", "inapp"];

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// select 的决策性：无匹配报错，有匹配确定性选中。
    #[test]
    fn select_is_decisive(platform_id in arb_platform_id()) {
        let mut r = fresh_registry();
        let host = HostEnvironment::new(
            platform_id.clone(),
            "x", "x", "none",
            BTreeSet::<Capability>::new(),
        );
        let result = r.select(&host, &HostPolicy::default());

        if REGISTERED.contains(&platform_id.as_str()) {
            prop_assert!(result.is_ok());
            prop_assert!(r.is_serviceable());
        } else {
            let err = result.expect_err("no match");
            prop_assert_eq!(err.code, ErrorCode::AdpNoMatchingPlatform);
            prop_assert!(!r.is_serviceable());
        }
    }

    /// 确定性：相同输入两个独立 registry 选出的 granted 一致。
    #[test]
    fn select_is_deterministic(platform_id in arb_platform_id()) {
        let host = HostEnvironment::new(
            platform_id, "x", "x", "none",
            BTreeSet::<Capability>::new(),
        );
        let policy = HostPolicy::default();
        let mut a = fresh_registry();
        let mut b = fresh_registry();
        let ra = a.select(&host, &policy);
        let rb = b.select(&host, &policy);
        prop_assert_eq!(ra.is_ok(), rb.is_ok());
        if ra.is_ok() {
            prop_assert_eq!(a.granted(), b.granted());
        }
    }
}
