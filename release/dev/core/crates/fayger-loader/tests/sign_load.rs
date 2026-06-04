//! Feature: fayger-m4, Property 1 & 5: sign-load round-trip & enforce mode.
//!
//! Property 1：seal_and_sign(o, k) → serialize → load_signed(含对应公钥的 TrustStore) 成功且 Valid。
//! Property 5：未签名 BuF 在 enforce=true→MissingSignature，enforce=false→通过。
//!
//! Validates: Requirements 6.1, 6.2, 3.1, 3.2, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::SemVer;
use fayger_buf::serialize;
use fayger_buf::testing::arb_buf_object;
use fayger_loader::{
    ed25519_public_key, load_signed, seal, seal_and_sign, LoadPolicy, TrustStore, TrustedRoot,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

fn permissive_policy() -> LoadPolicy {
    let max = SemVer {
        major: u32::MAX,
        minor: u32::MAX,
        patch: u32::MAX,
        pre: None,
    };
    let min = SemVer {
        major: 0,
        minor: 0,
        patch: 0,
        pre: None,
    };
    LoadPolicy::new((min, max.clone()), BTreeSet::new(), max)
}

fn arb_seed() -> impl Strategy<Value = [u8; 32]> {
    proptest::array::uniform32(any::<u8>())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 1：sign → serialize → load_signed 成功。
    #[test]
    fn sign_then_load_signed_succeeds(mut obj in arb_buf_object(), seed in arb_seed()) {
        seal_and_sign(&mut obj, &seed, "k1").expect("seal_and_sign");
        let bytes = serialize(&obj).expect("serialize");
        let pk = ed25519_public_key(&seed);
        // 强制模式也应通过（有有效签名）。
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], true);
        let loaded = load_signed(&bytes, &permissive_policy(), &ts).expect("load_signed");
        prop_assert_eq!(loaded.sections.len(), obj.sections.len());
    }

    /// Property 5：未签名 BuF 的强制模式行为。
    #[test]
    fn enforce_mode_governs_unsigned(mut obj in arb_buf_object(), enforce in any::<bool>()) {
        seal(&mut obj).expect("seal");  // 仅封装，不签名
        let bytes = serialize(&obj).expect("serialize");
        let ts = TrustStore::new(vec![], enforce);
        let result = load_signed(&bytes, &permissive_policy(), &ts);
        if enforce {
            let err = result.expect_err("enforce rejects unsigned");
            prop_assert_eq!(err.code, fayger_error::ErrorCode::LdrSignatureFail);
        } else {
            prop_assert!(result.is_ok());
        }
    }
}
