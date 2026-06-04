//! Feature: fayger-m4, Property 4: trusted-root update visibility.
//!
//! 每次 load_signed 用其之前最后一次 update_roots 的根集合验证：移除对应根→UnknownKey，
//! 加回→Valid。
//!
//! Validates: Requirements 4.3, 4.4, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::redundant_clone
)]

use fayger_buf::manifest::SemVer;
use fayger_buf::serialize;
use fayger_buf::testing::arb_buf_object;
use fayger_error::ErrorCode;
use fayger_loader::{
    ed25519_public_key, load_signed, seal_and_sign, LoadPolicy, TrustStore, TrustedRoot,
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

    /// 移除根 → UnknownKey；update_roots 加回 → Valid。
    #[test]
    fn update_roots_visibility(mut obj in arb_buf_object(), seed in arb_seed()) {
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let pk = ed25519_public_key(&seed);
        let root = TrustedRoot::new("k1", pk.to_vec());

        // 初始：空根集 → UnknownKey
        let mut ts = TrustStore::new(vec![], false);
        let err = load_signed(&bytes, &permissive_policy(), &ts).expect_err("unknown");
        prop_assert_eq!(err.code, ErrorCode::LdrSignatureFail);

        // 更新加入对应根 → 通过
        ts.update_roots(vec![root.clone()]);
        prop_assert!(load_signed(&bytes, &permissive_policy(), &ts).is_ok());

        // 再次移除 → 又 UnknownKey
        ts.update_roots(vec![]);
        prop_assert!(load_signed(&bytes, &permissive_policy(), &ts).is_err());
    }

    /// 含错误公钥的根 → InvalidSignature；换成正确公钥 → Valid。
    #[test]
    fn wrong_then_right_key(mut obj in arb_buf_object(), seed in arb_seed()) {
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let pk = ed25519_public_key(&seed);
        // 错误公钥（全零，合法 ed25519 点但非签名者）
        let mut ts = TrustStore::new(vec![TrustedRoot::new("k1", vec![0u8; 32])], false);
        prop_assert!(load_signed(&bytes, &permissive_policy(), &ts).is_err());
        // 换成正确公钥
        ts.update_roots(vec![TrustedRoot::new("k1", pk.to_vec())]);
        prop_assert!(load_signed(&bytes, &permissive_policy(), &ts).is_ok());
    }
}
