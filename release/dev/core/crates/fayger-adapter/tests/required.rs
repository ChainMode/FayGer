//! Feature: fayger-m3, Property 4: required-capability rejection.
//!
//! 对可服务 registry 与必需能力集合 required：check_required 成功 ⟺ required ⊆ granted；
//! 失败时 missing == required \ granted。dispatch 对指令 capabilities_required 同理。
//!
//! Validates: Requirements 6.1, 6.2, 6.3, 7.1, 7.2, 10.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone,
    clippy::unnecessary_lazy_evaluations
)]

use fayger_adapter::{
    AdapterRegistry, Capability, HostEnvironment, HostPolicy, NativeDesktopAdapter,
};
use fayger_error::ErrorCode;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

const DICT: [&str; 9] = [
    "fs.read",
    "fs.write",
    "net.http",
    "ui.dom",
    "time.now",
    "random.bytes",
    "crypto.hash",
    "proc.spawn",
    "host.call",
];

fn arb_cap_set() -> impl Strategy<Value = BTreeSet<Capability>> {
    proptest::collection::vec(any::<bool>(), DICT.len()).prop_map(|mask| {
        DICT.iter()
            .zip(mask)
            .filter(|(_, keep)| *keep)
            .map(|(c, _)| Capability::new(*c))
            .collect()
    })
}

/// 构造一个选中 NativeDesktop（支持全能力）的可服务 registry，
/// granted 由 host.available ∩ policy.allowed 决定（NativeDesktop supported 是全集）。
fn serviceable(
    available: &BTreeSet<Capability>,
    allowed: &BTreeSet<Capability>,
) -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(Box::new(NativeDesktopAdapter::new()));
    let host = HostEnvironment::new(
        "native-desktop",
        "linux",
        "x86_64",
        "none",
        available.clone(),
    );
    r.select(&host, &HostPolicy::new(allowed.clone()))
        .expect("select native-desktop");
    r
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn check_required_iff_subset(
        available in arb_cap_set(),
        allowed in arb_cap_set(),
        required in arb_cap_set(),
    ) {
        let r = serviceable(&available, &allowed);
        let granted = r.granted().clone();
        let result = r.check_required(&required);

        if required.is_subset(&granted) {
            prop_assert!(result.is_ok());
        } else {
            let err = result.expect_err("missing caps");
            prop_assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
            // missing == required \ granted
            let expected_missing: BTreeSet<String> = required
                .difference(&granted)
                .map(|c| c.as_str().to_string())
                .collect();
            let got_missing: BTreeSet<String> = match err.context.get("missing") {
                Some(fayger_error::ContextValue::List(l)) => l
                    .iter()
                    .filter_map(|v| match v {
                        fayger_error::ContextValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => BTreeSet::new(),
            };
            prop_assert_eq!(got_missing, expected_missing);
        }
    }
}
