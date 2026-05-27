// Feature: fayger-m0
// Property 5: Section_Index inconsistency between index and bounds is detected
//
// 蓝图 Properties §"Section_Index 自相矛盾被检测" 与 M0 需求文档 Requirement 3.6 / 7.1。
//
// 这个 PBT 用反向构造：从 BuFObject 派生一个 manifest，把第一个 section
// 的 length 谎报为远超 payload 实际长度，再手工组装字节流；parse 必须返回
// SectionOutOfBounds 或 SectionMismatch。

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::default_trait_access,
    clippy::items_after_statements,
    clippy::missing_const_for_fn
)]

use fayger_buf::canonical::canonical_encode;
use fayger_buf::format::{Header, Trailer, FORMAT_VERSION_M0, HEADER_SIZE, TRAILER_SIZE};
use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SemVer};
use fayger_buf::parse;
use fayger_buf::testing::arb_buf_object;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 200,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("regressions"),
        )),
        ..ProptestConfig::default()
    })]

    /// 当 Section_Index 中 offset+length 超过 sections region 实际长度时，
    /// parse 必须报 SectionOutOfBounds 或 SectionMismatch。
    #[test]
    fn oversized_section_length_is_detected(
        seed in arb_buf_object(),
        bogus_extra in 1u64..=u64::from(u32::MAX),
    ) {
        // 至少需要一个 section；空 sections 跳过。
        if seed.sections.is_empty() {
            return Ok(());
        }

        // 拼一个新 manifest，第一个 descriptor 的 length 谎报，但 payload 不变。
        let mut descriptors = seed.manifest.sections.clone();
        descriptors[0].length = descriptors[0].length.saturating_add(bogus_extra);

        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: Default::default(),
            quotas: None,
            sections: descriptors.clone(),
            signature: None,
            extensions: Default::default(),
        };
        let manifest_bytes = canonical_encode(&manifest).expect("encode");

        // Section_Index 与 manifest 一致（同样谎报 length）
        #[derive(serde::Serialize)]
        struct Entry {
            section_id: String,
            kind: SectionKind,
            offset: u64,
            length: u64,
            digest: Digest,
            visibility: SectionVisibility,
            profile_constraints: ProfileConstraints,
        }
        let entries: Vec<Entry> = {
            let mut current_offset: u64 = 0;
            let mut out = Vec::new();
            for (d, s) in descriptors.iter().zip(seed.sections.iter()) {
                out.push(Entry {
                    section_id: s.id.clone(),
                    kind: d.kind.clone(),
                    offset: current_offset,
                    length: d.length,
                    digest: d.digest.clone(),
                    visibility: d.visibility,
                    profile_constraints: d.profile_constraints.clone(),
                });
                // payload 实际长度 vs 谎报 length 不一致；这里仍按谎报累加，
                // 否则后续条目 offset 也不对——但反正第一个就会失败。
                current_offset = current_offset.saturating_add(d.length);
            }
            out
        };
        let index_bytes = canonical_encode(&entries).expect("encode entries");

        // 实际 payload 区域比 manifest 声称的小：把 sections 的真实 payload 拼进去
        let mut sections_region: Vec<u8> = Vec::new();
        for s in &seed.sections {
            sections_region.extend_from_slice(&s.payload);
        }

        let manifest_offset = HEADER_SIZE as u64;
        let manifest_length = manifest_bytes.len() as u64;
        let section_index_offset = manifest_offset + manifest_length;
        let section_index_length = index_bytes.len() as u64;
        let sections_offset = section_index_offset + section_index_length;
        let total_length =
            sections_offset + sections_region.len() as u64 + TRAILER_SIZE as u64;

        let header = Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset,
            manifest_length,
            section_index_offset,
            section_index_length,
        };
        let trailer = Trailer {
            total_length,
            manifest_digest: [0u8; 32],
            crc32: 0,
        };

        let mut bytes = Vec::new();
        let mut hb = std::io::Cursor::new(Vec::new());
        binrw::BinWrite::write(&header, &mut hb).expect("write header");
        bytes.extend_from_slice(&hb.into_inner());
        bytes.extend_from_slice(&manifest_bytes);
        bytes.extend_from_slice(&index_bytes);
        bytes.extend_from_slice(&sections_region);
        let mut tb = std::io::Cursor::new(Vec::new());
        binrw::BinWrite::write(&trailer, &mut tb).expect("write trailer");
        bytes.extend_from_slice(&tb.into_inner());

        let err = parse(&bytes).expect_err("oversized length must fail");
        prop_assert_eq!(err.code, fayger_error::ErrorCode::LdrParseFail);
        let reason = err.context.get("reason");
        prop_assert!(
            matches!(
                reason,
                Some(fayger_error::ContextValue::String(s))
                    if s == "SectionOutOfBounds" || s == "SectionMismatch"
            ),
            "expected SectionOutOfBounds or SectionMismatch, got {reason:?}"
        );
    }
}
