//! Feature: fayger-m3, Property 2: capability negotiation set algebra.
//!
//! 对任意 (requested, available, policy)：granted == requested ∩ available ∩ policy；
//! denied == requested \ granted；granted ∩ denied == ∅；granted ∪ denied == requested。
//!
//! Validates: Requirements 5.1, 5.2, 5.3, 5.4, 10.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone,
    clippy::unnecessary_lazy_evaluations
)]

use fayger_adapter::{negotiate, Capability, HostPolicy};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

/// 从一个固定能力字典中取任意子集。
fn arb_cap_set() -> impl Strategy<Value = BTreeSet<Capability>> {
    let dict = [
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
    proptest::collection::vec(any::<bool>(), dict.len()).prop_map(move |mask| {
        dict.iter()
            .zip(mask)
            .filter(|(_, keep)| *keep)
            .map(|(c, _)| Capability::new(*c))
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn negotiate_satisfies_set_algebra(
        requested in arb_cap_set(),
        available in arb_cap_set(),
        allowed in arb_cap_set(),
    ) {
        let policy = HostPolicy::new(allowed.clone());
        let r = negotiate(&requested, &available, &policy);

        // granted == requested ∩ available ∩ allowed
        let expected_granted: BTreeSet<Capability> = requested
            .iter()
            .filter(|c| available.contains(*c) && allowed.contains(*c))
            .cloned()
            .collect();
        prop_assert_eq!(&r.granted, &expected_granted);

        // denied == requested \ granted
        let expected_denied: BTreeSet<Capability> =
            requested.difference(&r.granted).cloned().collect();
        prop_assert_eq!(&r.denied, &expected_denied);

        // granted ∩ denied == ∅
        prop_assert!(r.granted.is_disjoint(&r.denied));

        // granted ∪ denied == requested
        let union: BTreeSet<Capability> = r.granted.union(&r.denied).cloned().collect();
        prop_assert_eq!(union, requested.clone());

        // granted ⊆ requested / available / allowed
        prop_assert!(r.granted.is_subset(&requested));
        prop_assert!(r.granted.is_subset(&available));
        prop_assert!(r.granted.is_subset(&allowed));
    }
}
