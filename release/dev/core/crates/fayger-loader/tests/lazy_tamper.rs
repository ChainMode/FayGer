//! Feature: fayger-m5, Property 4: lazy section digest tamper detection.
//!
//! 对 sealed 字节流的某段 payload 任一字节翻转后承载于 MemorySource：Lazy 加载本身成功
//! （骨架未变），但对被篡改 Section 的首次 fetch 返回 LDR_LAZY_DIGEST_MISMATCH（phase=lazy），
//! 且不缓存。
//!
//! Validates: Requirements 3.5, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_buf::format::TRAILER_SIZE;
use fayger_buf::serialize;
use fayger_buf::testing::arb_buf_object;
use fayger_error::{ContextValue, ErrorCode};
use fayger_loader::{load_from_source, seal, LoadStrategy, MemorySource};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

/// 生成 (sealed_bytes, sections_region_offset)；要求至少一段非空 payload。
fn sealed_with_nonempty_section() -> impl Strategy<Value = Vec<u8>> {
    arb_buf_object()
        .prop_filter("has non-empty payload", |o| {
            o.sections.iter().any(|s| !s.payload.is_empty())
        })
        .prop_map(|mut obj| {
            seal(&mut obj).expect("seal");
            serialize(&obj).expect("serialize")
        })
}

fn phase_of(err: &fayger_error::FaygerError) -> Option<String> {
    match err.context.get("phase") {
        Some(ContextValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn tampered_section_payload_fails_lazy_fetch(bytes in sealed_with_nonempty_section(), pick in any::<u64>()) {
        // 定位 Sections 区 [sections_region_offset, total - TRAILER_SIZE)。
        let parsed = fayger_buf::parse(&bytes).expect("parse");
        let sro = parsed.raw_header.sections_offset().expect("sro") as usize;
        let total = bytes.len();
        let payload_region_end = total - TRAILER_SIZE;
        prop_assume!(payload_region_end > sro);

        // 翻转 sections 区内一个字节。
        let region_len = payload_region_end - sro;
        let pos = sro + (pick as usize % region_len);
        let mut tampered = bytes.clone();
        tampered[pos] ^= 0xff;
        prop_assume!(tampered != bytes);

        // Lazy 加载：骨架未变，应成功。
        let mut art = load_from_source(Box::new(MemorySource::new(tampered.clone())), LoadStrategy::Lazy)
            .expect("lazy skeleton ok");

        // 找出被篡改字节落在哪个 section（按 manifest 顺序累加长度）。
        let rel = pos - sro;
        let mut acc = 0usize;
        let mut tampered_id: Option<String> = None;
        for sd in &art.manifest.sections {
            let len = sd.length as usize;
            if rel >= acc && rel < acc + len {
                tampered_id = Some(sd.id.clone());
                break;
            }
            acc += len;
        }
        let id = tampered_id.expect("byte falls within some section");

        // 对被篡改段 fetch 应失败（LDR_LAZY_DIGEST_MISMATCH，phase=lazy），且不缓存。
        let err = art.section(&id).expect_err("digest mismatch");
        prop_assert_eq!(err.code, ErrorCode::LdrLazyDigestMismatch);
        let phase = phase_of(&err);
        prop_assert_eq!(phase.as_deref(), Some("lazy"));
        prop_assert!(!art.loader().expect("loader").is_loaded(&id), "tampered section must not be cached");
    }
}
