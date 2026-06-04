//! Feature: fayger-m4, Property 3: tampering within signed scope is detected.
//!
//! 对已 seal_and_sign 的字节流，对签名作用域内（Header / Manifest / Section_Index 区间）任一字节
//! 翻转（不重新签名），load_signed 必返回错误（LDR_SIGNATURE_FAIL 或更早的 DIGEST/PARSE 错误）。
//!
//! Validates: Requirements 2.1, 2.3, 3.5, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_buf::format::TRAILER_SIZE;
use fayger_buf::manifest::SemVer;
use fayger_buf::testing::arb_buf_object;
use fayger_buf::{parse, serialize};
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

    /// 对 [0, total-TRAILER_SIZE)（Header+Manifest+Section_Index+Sections，含签名作用域）
    /// 任一字节翻转后加载必失败。签名作用域内的修改至少被 signature 或 digest 检出。
    #[test]
    fn tamper_in_signed_region_is_detected(
        mut obj in arb_buf_object(),
        seed in arb_seed(),
        bit_seed in any::<u64>(),
    ) {
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let pk = ed25519_public_key(&seed);
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], false);

        let total = bytes.len();
        // 受保护前缀：[0, total - TRAILER_SIZE)（Header || Manifest || Section_Index || Sections）。
        // 该区间被 manifest_digest（前缀）、section digest（payload）、crc32（整体）、
        // 以及签名（Header/Manifest/Section_Index）覆盖；任一翻转都至少被一关检出。
        let region_len = total - TRAILER_SIZE;
        prop_assume!(region_len > 0);
        let bit = bit_seed % (region_len as u64 * 8);
        let byte_idx = (bit / 8) as usize;
        let bit_in_byte = (bit % 8) as u8;

        let mut tampered = bytes.clone();
        tampered[byte_idx] ^= 1u8 << bit_in_byte;
        prop_assume!(tampered != bytes);

        let err = load_signed(&tampered, &permissive_policy(), &ts)
            .expect_err("tamper must be detected");
        prop_assert!(
            matches!(
                err.code,
                ErrorCode::LdrSignatureFail
                    | ErrorCode::LdrDigestMismatch
                    | ErrorCode::LdrParseFail
            ),
            "unexpected code: {:?}",
            err.code
        );
    }

    /// 重新解析受篡改的 Manifest 区域可能改变结构，但 load_signed 永不成功。
    #[test]
    fn tampered_never_loads_ok(
        mut obj in arb_buf_object(),
        seed in arb_seed(),
        idx_seed in any::<u64>(),
    ) {
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let pk = ed25519_public_key(&seed);
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], false);

        // 先确认未篡改可加载。
        prop_assert!(load_signed(&bytes, &permissive_policy(), &ts).is_ok());

        // 翻转 Manifest 区间内一个字节（[HEADER_SIZE, section_index_offset)）。
        let parsed = parse(&bytes).expect("parse");
        let m_start = fayger_buf::format::HEADER_SIZE;
        let m_end = parsed.raw_header.section_index_offset as usize;
        prop_assume!(m_end > m_start);
        let pos = m_start + (idx_seed as usize % (m_end - m_start));
        let mut tampered = bytes.clone();
        tampered[pos] ^= 0xff;
        prop_assume!(tampered != bytes);

        prop_assert!(load_signed(&tampered, &permissive_policy(), &ts).is_err());
    }
}
