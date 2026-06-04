//! Feature: fayger-m1, Property 2 & 3: digest tampering is detected.
//!
//! Property 2：对经封装的合法字节流，翻转 `[0, total-4)` 区间内任一比特（不重新封装），
//! 加载必须被检测（结构错误或摘要错误）；若落在 Section payload 区，则必为
//! `LDR_DIGEST_MISMATCH`。
//!
//! Property 3：仅修改末 4 字节 crc32 为不同值，加载必返回 `LDR_DIGEST_MISMATCH`
//! 且 `reason == "Crc32"`。
//!
//! Validates: Requirements 2.2, 3.2, 4.2, 11.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone,
    clippy::cast_possible_truncation
)]

use fayger_buf::format::TRAILER_SIZE;
use fayger_buf::testing::arb_buf_object;
use fayger_buf::{parse, serialize};
use fayger_error::{ContextValue, ErrorCode};
use fayger_loader::{load, seal, LoadPolicy};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeSet;

fn permissive_policy() -> LoadPolicy {
    use fayger_buf::manifest::SemVer;
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

/// 生成一段经封装的合法 BuF 字节流。
fn sealed_bytes_strategy() -> impl Strategy<Value = Vec<u8>> {
    arb_buf_object().prop_map(|mut obj| {
        seal(&mut obj).expect("seal should succeed for well-formed object");
        serialize(&obj).expect("serialize should succeed")
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 翻转 `[0, total-4)` 区间内任一比特，加载必须被检测（返回 Err）。
    ///
    /// 原理：crc32 覆盖整个 `[0, total-4)`，而其存储值在末 4 字节未被触碰，
    /// 因此区间内任一改动都会破坏 crc32（除非更早的 parse / manifest / section
    /// 校验已先行捕获）。无论哪一关，结果都是 Err。
    #[test]
    fn any_single_bit_flip_in_protected_region_is_detected(
        bytes in sealed_bytes_strategy(),
        bit_seed in any::<u64>(),
    ) {
        let total = bytes.len();
        // 受保护区间 [0, total-4)。封装后的最小制品也远大于 4 字节。
        let region_len = total - 4;
        let region_bits = (region_len as u64) * 8;
        prop_assume!(region_bits > 0);
        let bit = bit_seed % region_bits;
        let byte_idx = (bit / 8) as usize;
        let bit_in_byte = (bit % 8) as u8;

        let mut tampered = bytes.clone();
        tampered[byte_idx] ^= 1u8 << bit_in_byte;

        // 单点翻转后，原字节流与篡改流必不同。
        prop_assume!(tampered != bytes);

        let err = load(&tampered, &permissive_policy())
            .expect_err("tampered byte stream must be rejected");
        // 检测可能发生在结构阶段或摘要阶段；两者都算"被检测"。
        prop_assert!(
            err.code == ErrorCode::LdrParseFail || err.code == ErrorCode::LdrDigestMismatch,
            "unexpected error code: {:?}",
            err.code
        );
    }

    /// 翻转 Section payload 区内任一比特，加载必返回 `LDR_DIGEST_MISMATCH`。
    ///
    /// payload 字节不影响 Header / Manifest / Section_Index 的结构解析，所以 parse
    /// 仍成功；随后 section digest（或 crc32）必然失配。
    #[test]
    fn payload_corruption_yields_digest_mismatch(
        bytes in sealed_bytes_strategy(),
        bit_seed in any::<u64>(),
    ) {
        // 解析出 Header 以定位 Sections 区间 [sections_offset, total-TRAILER_SIZE)。
        let obj = parse(&bytes).expect("sealed bytes parse");
        let sections_offset = obj
            .raw_header
            .sections_offset()
            .expect("sections_offset") as usize;
        let total = bytes.len();
        let payload_region_end = total - TRAILER_SIZE;
        prop_assume!(payload_region_end > sections_offset); // 至少有一段非空 payload

        let region_len = payload_region_end - sections_offset;
        let region_bits = (region_len as u64) * 8;
        let bit = bit_seed % region_bits;
        let byte_idx = sections_offset + (bit / 8) as usize;
        let bit_in_byte = (bit % 8) as u8;

        let mut tampered = bytes.clone();
        tampered[byte_idx] ^= 1u8 << bit_in_byte;

        let err = load(&tampered, &permissive_policy())
            .expect_err("payload corruption must be rejected");
        prop_assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
    }

    /// 仅改动末 4 字节 crc32，加载必返回 `LDR_DIGEST_MISMATCH` 且 reason == "Crc32"。
    #[test]
    fn crc32_field_only_corruption_reports_crc32(
        bytes in sealed_bytes_strategy(),
        xor_mask in 1u32..=u32::MAX,
    ) {
        let total = bytes.len();
        let mut tampered = bytes.clone();
        // crc32 字段位于 [total-4, total)，小端。
        let stored = u32::from_le_bytes([
            tampered[total - 4],
            tampered[total - 3],
            tampered[total - 2],
            tampered[total - 1],
        ]);
        let new_val = stored ^ xor_mask; // xor_mask != 0 保证值不同
        tampered[total - 4..total].copy_from_slice(&new_val.to_le_bytes());

        let err = load(&tampered, &permissive_policy())
            .expect_err("crc32 field corruption must be rejected");
        prop_assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        match err.context.get("reason") {
            Some(ContextValue::String(s)) => prop_assert_eq!(s.as_str(), "Crc32"),
            other => prop_assert!(false, "expected reason=Crc32, got {:?}", other),
        }
    }
}
