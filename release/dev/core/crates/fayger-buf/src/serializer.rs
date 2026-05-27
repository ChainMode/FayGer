//! BuF 字节流序列化器。
//!
//! 顶层 API：[`serialize`]。流程见设计文档 §`serializer` 模块。
//!
//! 输出顺序：Header（写出最后填充） → Manifest（canonical CBOR）→
//! Section_Index（canonical CBOR）→ Sections payload → Trailer。
//!
//! M0 阶段 `manifest_digest` / `crc32` 占位写零；如果 `BuFObject.raw_trailer`
//! 已有现成值则原样保留（保证 round-trip 字节级一致）。

use crate::canonical::canonical_encode;
use crate::format::{Header, Trailer, FORMAT_VERSION_M0, HEADER_SIZE, TRAILER_SIZE};
use crate::object::BuFObject;
use crate::parser::SectionIndexEntry;
use binrw::BinWrite;
use fayger_error::{ContextValue, FaygerError};

/// 把 [`BuFObject`] 写出为字节流。
///
/// 流程：
///
/// 1. `obj.manifest.validate()` —— 必填字段、SemVer 合法性、section id 唯一性
/// 2. `obj.check_consistency()` —— sections 与 manifest.sections 一一对应
/// 3. canonical 编码 Manifest 与 Section_Index
/// 4. 写 Header / Manifest / Section_Index / payloads / Trailer
///
/// `raw_header` / `raw_trailer` 中除偏移与长度外的占位字段（`format_version`、
/// `manifest_digest`、`crc32`）从入参原样保留；偏移、长度、`total_length` 由
/// 序列化器根据实际 payload 重新计算填回。
///
/// 失败时返回 [`fayger_error::FaygerError`]，code 视情况为：
/// - [`fayger_error::ErrorCode::LdrMissingRequiredField`]：validate / check_consistency 失败。
/// - [`fayger_error::ErrorCode::LdrParseFail`]：CBOR 编码内部错误（理论上不应发生）。
#[allow(clippy::too_many_lines)] // serialize 是顺序流水线，拆分会割裂可读性
pub fn serialize(obj: &BuFObject) -> Result<Vec<u8>, FaygerError> {
    // 1. 必填字段与一致性
    obj.manifest.validate()?;
    obj.check_consistency()?;

    // 2. canonical CBOR 编码 Manifest
    let manifest_bytes = canonical_encode(&obj.manifest)?;

    // 3. 由 sections 反推 Section_Index：offset 按 sections region 累加
    let mut entries: Vec<SectionIndexEntry> = Vec::with_capacity(obj.sections.len());
    let mut current_offset: u64 = 0;
    for (descriptor, section) in obj.manifest.sections.iter().zip(obj.sections.iter()) {
        let payload_len = u64::try_from(section.payload.len()).map_err(|_| {
            FaygerError::new(
                fayger_error::ErrorCode::LdrParseFail,
                fayger_error::Layer::Loader,
                "section payload length overflows u64",
            )
            .with_context("section_id", ContextValue::string(section.id.clone()))
        })?;
        entries.push(SectionIndexEntry {
            section_id: section.id.clone(),
            kind: descriptor.kind.clone(),
            offset: current_offset,
            length: payload_len,
            digest: descriptor.digest.clone(),
            visibility: descriptor.visibility,
            profile_constraints: descriptor.profile_constraints.clone(),
        });
        current_offset = current_offset.checked_add(payload_len).ok_or_else(|| {
            FaygerError::new(
                fayger_error::ErrorCode::LdrParseFail,
                fayger_error::Layer::Loader,
                "cumulative section offset overflow",
            )
        })?;
    }
    let index_bytes = canonical_encode(&entries)?;

    // 4. 计算各偏移
    let header_size_u64 = u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX);
    let trailer_size_u64 = u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX);
    let manifest_offset = header_size_u64;
    let manifest_length = u64::try_from(manifest_bytes.len()).map_err(|_| {
        FaygerError::new(
            fayger_error::ErrorCode::LdrParseFail,
            fayger_error::Layer::Loader,
            "manifest length overflows u64",
        )
    })?;
    let section_index_offset = manifest_offset
        .checked_add(manifest_length)
        .ok_or_else(|| length_overflow("section_index_offset"))?;
    let section_index_length = u64::try_from(index_bytes.len()).map_err(|_| {
        FaygerError::new(
            fayger_error::ErrorCode::LdrParseFail,
            fayger_error::Layer::Loader,
            "section index length overflows u64",
        )
    })?;
    let sections_offset = section_index_offset
        .checked_add(section_index_length)
        .ok_or_else(|| length_overflow("sections_offset"))?;
    let total_length = sections_offset
        .checked_add(current_offset)
        .and_then(|v| v.checked_add(trailer_size_u64))
        .ok_or_else(|| length_overflow("total_length"))?;

    // 5. 构造最终 Header / Trailer。
    //
    // raw_header 中除 format_version 之外的字段都被偏移/长度计算覆盖。
    // 我们尊重 raw_header 中的 format_version（允许将来支持多版本写出），
    // 但目前 M0 强制写 FORMAT_VERSION_M0，以避免 raw_header 被外部篡改后影响产出。
    let header = Header {
        format_version: if obj.raw_header.format_version == 0 {
            FORMAT_VERSION_M0
        } else {
            obj.raw_header.format_version
        },
        manifest_offset,
        manifest_length,
        section_index_offset,
        section_index_length,
    };
    let trailer = Trailer {
        total_length,
        manifest_digest: obj.raw_trailer.manifest_digest,
        crc32: obj.raw_trailer.crc32,
    };

    // 6. 拼接字节流
    let total_usize = usize::try_from(total_length).map_err(|_| length_overflow("total_length"))?;
    let mut out = Vec::with_capacity(total_usize);
    let mut header_buf = std::io::Cursor::new(Vec::with_capacity(HEADER_SIZE));
    header.write(&mut header_buf).map_err(|e| {
        FaygerError::new(
            fayger_error::ErrorCode::LdrParseFail,
            fayger_error::Layer::Loader,
            "failed to write header",
        )
        .with_context("detail", ContextValue::string(format!("{e}")))
    })?;
    out.extend_from_slice(&header_buf.into_inner());
    out.extend_from_slice(&manifest_bytes);
    out.extend_from_slice(&index_bytes);
    for section in &obj.sections {
        out.extend_from_slice(&section.payload);
    }
    let mut trailer_buf = std::io::Cursor::new(Vec::with_capacity(TRAILER_SIZE));
    trailer.write(&mut trailer_buf).map_err(|e| {
        FaygerError::new(
            fayger_error::ErrorCode::LdrParseFail,
            fayger_error::Layer::Loader,
            "failed to write trailer",
        )
        .with_context("detail", ContextValue::string(format!("{e}")))
    })?;
    out.extend_from_slice(&trailer_buf.into_inner());

    debug_assert_eq!(
        out.len() as u64,
        total_length,
        "serializer: produced byte length disagrees with computed total_length"
    );
    Ok(out)
}

fn length_overflow(field: &str) -> FaygerError {
    FaygerError::new(
        fayger_error::ErrorCode::LdrParseFail,
        fayger_error::Layer::Loader,
        "byte stream layout overflow",
    )
    .with_context("field", ContextValue::string(field.to_owned()))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;
    use crate::format::{Header, FORMAT_VERSION_M0, MANIFEST_DIGEST_SIZE};
    use crate::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use crate::object::Section;
    use crate::parser::parse;
    use std::collections::{BTreeMap, BTreeSet};

    fn descriptor(id: &str, length: u64) -> SectionDescriptor {
        SectionDescriptor {
            id: id.into(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        }
    }

    fn fresh_header() -> Header {
        Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        }
    }

    fn fresh_trailer() -> Trailer {
        Trailer {
            total_length: 0,
            manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
            crc32: 0,
        }
    }

    fn sample_object() -> BuFObject {
        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![descriptor("code", 4), descriptor("data", 2)],
            signature: None,
            extensions: BTreeMap::new(),
        };
        BuFObject {
            manifest,
            sections: vec![
                Section::new("code", vec![1, 2, 3, 4]),
                Section::new("data", vec![5, 6]),
            ],
            raw_header: fresh_header(),
            raw_trailer: fresh_trailer(),
        }
    }

    #[test]
    fn serialize_then_parse_round_trips() {
        let obj = sample_object();
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.manifest, obj.manifest);
        assert_eq!(parsed.sections, obj.sections);
    }

    #[test]
    fn serialize_is_deterministic() {
        let obj = sample_object();
        let a = serialize(&obj).expect("serialize 1");
        let b = serialize(&obj).expect("serialize 2");
        assert_eq!(
            a, b,
            "two consecutive serializes must produce identical bytes"
        );
    }

    #[test]
    fn parse_then_serialize_byte_round_trip() {
        // 端到端：先 serialize，再 parse，再 serialize；后两次 serialize 必须字节相等
        let obj = sample_object();
        let bytes1 = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes1).expect("parse");
        let bytes2 = serialize(&parsed).expect("re-serialize");
        assert_eq!(
            bytes1, bytes2,
            "serialize ∘ parse ∘ serialize must give the same bytes"
        );
    }

    #[test]
    fn serialize_with_zero_sections() {
        let mut obj = sample_object();
        obj.manifest.sections.clear();
        obj.sections.clear();
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        assert!(parsed.sections.is_empty());
    }

    #[test]
    fn serialize_rejects_missing_required_field() {
        let mut obj = sample_object();
        obj.manifest.entry = EntryPoint::new(""); // empty entry
        let err = serialize(&obj).expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        let mf = err.context.get("missing_fields").expect("missing_fields");
        let list = match mf {
            ContextValue::List(l) => l,
            other => panic!("expected list, got {other:?}"),
        };
        assert!(list
            .iter()
            .any(|v| matches!(v, ContextValue::String(s) if s == "entry"),));
    }

    #[test]
    fn serialize_rejects_section_inconsistency() {
        let mut obj = sample_object();
        // 删掉一段 payload，让数量与 manifest.sections 不匹配
        obj.sections.pop();
        let err = serialize(&obj).expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "SectionInconsistency"
        ));
    }

    #[test]
    fn serializer_preserves_extensions_through_round_trip() {
        use ciborium::value::Value;
        let mut obj = sample_object();
        obj.manifest
            .extensions
            .insert("x_foo".to_owned(), Value::Text("bar".into()));
        obj.manifest
            .extensions
            .insert("x_num".to_owned(), Value::Integer(42_i64.into()));

        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.manifest.extensions, obj.manifest.extensions);
    }

    #[test]
    fn serializer_sets_format_version_when_raw_is_zero() {
        let mut obj = sample_object();
        obj.raw_header.format_version = 0; // 表示「未指定」
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.raw_header.format_version, FORMAT_VERSION_M0);
    }

    #[test]
    fn serializer_preserves_nonzero_raw_format_version() {
        let mut obj = sample_object();
        obj.raw_header.format_version = 7; // 故意写一个非默认值
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.raw_header.format_version, 7);
    }

    #[test]
    fn serializer_total_length_matches_actual_byte_count() {
        let obj = sample_object();
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.raw_trailer.total_length, bytes.len() as u64);
    }

    #[test]
    fn round_trip_preserves_canonical_byte_form() {
        // Property M0.RT.2（字节级强往返）的最小确认：
        // parse 后 serialize 应当与原字节流相等。
        let obj = sample_object();
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let bytes2 = serialize(&parsed).expect("re-serialize");
        assert_eq!(bytes, bytes2);
    }
}
