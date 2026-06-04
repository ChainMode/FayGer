//! Feature: fayger-m4, Property 2: signature verification decision table.
//!
//! 对 (has_sig, enforce, algorithm, key_in_store, sig_valid) 组合，verify_signature 满足决策表。
//!
//! Validates: Requirements 3.1–3.6, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::types::SignatureBlock;
use fayger_buf::object::BuFObject;
use fayger_buf::testing::arb_buf_object;
use fayger_buf::{parse, serialize};
use fayger_error::{ContextValue, ErrorCode};
use fayger_loader::{
    ed25519_public_key, seal, seal_and_sign, verify_signature, TrustStore, TrustedRoot,
    VerifyOutcome,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn arb_seed() -> impl Strategy<Value = [u8; 32]> {
    proptest::array::uniform32(any::<u8>())
}

fn reason_of(err: &fayger_error::FaygerError) -> String {
    match err.context.get("reason") {
        Some(ContextValue::String(s)) => s.clone(),
        other => panic!("expected reason, got {other:?}"),
    }
}

/// 把一个对象 seal（不签名），序列化并解析。
fn sealed(mut obj: BuFObject) -> (Vec<u8>, BuFObject) {
    seal(&mut obj).expect("seal");
    let bytes = serialize(&obj).expect("serialize");
    let parsed = parse(&bytes).expect("parse");
    (bytes, parsed)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 无签名 × enforce 开关。
    #[test]
    fn no_signature_branch(obj in arb_buf_object(), enforce in any::<bool>()) {
        let (bytes, parsed) = sealed(obj);
        let ts = TrustStore::new(vec![], enforce);
        let result = verify_signature(&bytes, &parsed, &ts);
        if enforce {
            let err = result.expect_err("missing");
            prop_assert_eq!(reason_of(&err), "MissingSignature");
        } else {
            prop_assert_eq!(result.expect("absent"), VerifyOutcome::Absent);
        }
    }

    /// 有签名 × (key 命中 / 不命中)。
    #[test]
    fn signed_branch_key_presence(mut obj in arb_buf_object(), seed in arb_seed(), key_present in any::<bool>()) {
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let pk = ed25519_public_key(&seed);
        let ts = if key_present {
            TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], false)
        } else {
            TrustStore::new(vec![], false)
        };
        let result = verify_signature(&bytes, &parsed, &ts);
        if key_present {
            prop_assert_eq!(result.expect("valid"), VerifyOutcome::Valid);
        } else {
            let err = result.expect_err("unknown");
            prop_assert_eq!(err.code, ErrorCode::LdrSignatureFail);
            prop_assert_eq!(reason_of(&err), "UnknownKey");
        }
    }

    /// 有签名 × 篡改 signed_digest → InvalidSignature（key 命中）。
    #[test]
    fn tampered_signature_is_invalid(mut obj in arb_buf_object(), seed in arb_seed()) {
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        if let Some(sig) = obj.manifest.signature.as_mut() {
            sig.signed_digest[0] ^= 0xff;
        }
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let pk = ed25519_public_key(&seed);
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], false);
        let err = verify_signature(&bytes, &parsed, &ts).expect_err("invalid");
        prop_assert_eq!(reason_of(&err), "InvalidSignature");
    }

    /// 有签名 × 非 ed25519 算法 → UnsupportedAlgorithm。
    #[test]
    fn unsupported_algorithm_branch(obj in arb_buf_object(), alg in "[a-z]{3,8}") {
        prop_assume!(alg != "ed25519");
        let mut o = obj;
        seal(&mut o).expect("seal");
        o.manifest.signature = Some(SignatureBlock {
            algorithm: alg,
            public_key_ref: "k1".into(),
            signed_digest: vec![0u8; 64],
        });
        let bytes = serialize(&o).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", vec![0u8; 32])], false);
        let err = verify_signature(&bytes, &parsed, &ts).expect_err("unsupported");
        prop_assert_eq!(reason_of(&err), "UnsupportedAlgorithm");
    }
}
