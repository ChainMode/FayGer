//! BuF 内存对象 [`BuFObject`] 与 [`Section`]。
//!
//! 蓝图 §3.5 与设计文档 §`object` 模块定义。
//!
//! `BuFObject` 持有：
//!
//! - `manifest`：[`BuFManifest`]。
//! - `sections`：与 `manifest.sections` 一一对应的 payload。
//! - `raw_header` / `raw_trailer`：解析时保留的原始字节，序列化时原样写出。
//!
//! M0 阶段不引入 `BuF_Source`、`LoadProfile` 与 Lazy 加载等概念；这些是 M5/M6
//! 的工作。因此 `BuFObject` 在 M0 一定持有全部 sections。

use crate::format::{Header, Trailer};
use crate::manifest::{ldr_missing_field, BuFManifest};
use fayger_error::ContextValue;

/// 一个 Section 的运行时形态。
///
/// `id` 与 [`crate::manifest::SectionDescriptor::id`] 一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Section 标识，与 manifest.sections 中的对应项相同。
    pub id: String,
    /// Section 的二进制内容。
    pub payload: Vec<u8>,
}

impl Section {
    /// 便利构造。
    pub fn new(id: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            id: id.into(),
            payload: payload.into(),
        }
    }
}

/// 内存中的 BuF 对象。
///
/// 序列化时 `raw_header` 与 `raw_trailer` 中的占位字段（M0 阶段的
/// `manifest_digest` / `crc32`）会原样写回；这保证 round-trip 字节级一致。
///
/// # Eq 与 extensions
///
/// `BuFObject` 包含 [`BuFManifest`]，后者只实现 `PartialEq`（因 ciborium::Value 不实现 Eq），
/// 所以 `BuFObject` 同样只实现 `PartialEq`。
#[derive(Debug, Clone, PartialEq)]
pub struct BuFObject {
    /// 制品的元数据清单。
    pub manifest: BuFManifest,
    /// 按 `manifest.sections` 顺序保存的 section 内容。
    /// 在 [`BuFObject::check_consistency`] 通过后，`sections.len() == manifest.sections.len()`，
    /// 且 `sections[i].id == manifest.sections[i].id`，且 `sections[i].payload.len() == manifest.sections[i].length`。
    pub sections: Vec<Section>,
    /// 原始 Header 字节布局。Lazy 加载或不可见字段保留时使用。
    pub raw_header: Header,
    /// 原始 Trailer 字节布局（M0 阶段 `manifest_digest`/`crc32` 通常为零）。
    pub raw_trailer: Trailer,
}

impl BuFObject {
    /// 一致性自检：确保 `sections` 与 `manifest.sections` 一一对应。
    ///
    /// 检查项：
    ///
    /// 1. 长度一致（数量相同）。
    /// 2. 每个 `sections[i].id` 等于 `manifest.sections[i].id`。
    /// 3. 每个 `sections[i].payload.len()` 等于 `manifest.sections[i].length`。
    ///
    /// 失败时返回 [`fayger_error::ErrorCode::LdrMissingRequiredField`] +
    /// `context.reason = "SectionInconsistency"`，并携带具体差异信息。
    pub fn check_consistency(&self) -> Result<(), fayger_error::FaygerError> {
        let manifest_len = self.manifest.sections.len();
        let payload_len = self.sections.len();

        if manifest_len != payload_len {
            return Err(ldr_missing_field("BuFObject sections count mismatch")
                .with_context("reason", ContextValue::string("SectionInconsistency"))
                .with_context(
                    "manifest_count",
                    ContextValue::U64(u64::try_from(manifest_len).unwrap_or(u64::MAX)),
                )
                .with_context(
                    "payload_count",
                    ContextValue::U64(u64::try_from(payload_len).unwrap_or(u64::MAX)),
                ));
        }

        for (i, (descriptor, section)) in self
            .manifest
            .sections
            .iter()
            .zip(self.sections.iter())
            .enumerate()
        {
            if descriptor.id != section.id {
                return Err(ldr_missing_field("section id mismatch")
                    .with_context("reason", ContextValue::string("SectionInconsistency"))
                    .with_context(
                        "index",
                        ContextValue::U64(u64::try_from(i).unwrap_or(u64::MAX)),
                    )
                    .with_context("expected_id", ContextValue::string(descriptor.id.clone()))
                    .with_context("actual_id", ContextValue::string(section.id.clone())));
            }
            let actual_len = u64::try_from(section.payload.len()).unwrap_or(u64::MAX);
            if descriptor.length != actual_len {
                return Err(ldr_missing_field("section payload length mismatch")
                    .with_context("reason", ContextValue::string("SectionInconsistency"))
                    .with_context("section_id", ContextValue::string(section.id.clone()))
                    .with_context("expected_length", ContextValue::U64(descriptor.length))
                    .with_context("actual_length", ContextValue::U64(actual_len)));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::format::{Trailer, FORMAT_VERSION_M0, HEADER_SIZE, MANIFEST_DIGEST_SIZE};
    use crate::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
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
        let sections = vec![
            Section::new("code", vec![1, 2, 3, 4]),
            Section::new("data", vec![5, 6]),
        ];
        let raw_header = Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset: HEADER_SIZE as u64,
            manifest_length: 0,
            section_index_offset: HEADER_SIZE as u64,
            section_index_length: 0,
        };
        let raw_trailer = Trailer {
            total_length: 0,
            manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
            crc32: 0,
        };
        BuFObject {
            manifest,
            sections,
            raw_header,
            raw_trailer,
        }
    }

    #[test]
    fn consistent_object_passes_check() {
        sample_object()
            .check_consistency()
            .expect("sample should be consistent");
    }

    #[test]
    fn empty_sections_object_passes() {
        let mut obj = sample_object();
        obj.manifest.sections.clear();
        obj.sections.clear();
        obj.check_consistency().expect("empty is consistent");
    }

    #[test]
    fn count_mismatch_returns_section_inconsistency() {
        let mut obj = sample_object();
        // 把 payload 删掉一个，使数量不一致
        obj.sections.pop();
        let err = obj.check_consistency().expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "SectionInconsistency"
        ));
        assert!(matches!(
            err.context.get("manifest_count"),
            Some(ContextValue::U64(2))
        ));
        assert!(matches!(
            err.context.get("payload_count"),
            Some(ContextValue::U64(1))
        ));
    }

    #[test]
    fn id_mismatch_at_index_is_reported() {
        let mut obj = sample_object();
        obj.sections[1].id = "wrong".into();
        let err = obj.check_consistency().expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        assert!(matches!(
            err.context.get("index"),
            Some(ContextValue::U64(1))
        ));
        assert!(matches!(
            err.context.get("expected_id"),
            Some(ContextValue::String(s)) if s == "data"
        ));
        assert!(matches!(
            err.context.get("actual_id"),
            Some(ContextValue::String(s)) if s == "wrong"
        ));
    }

    #[test]
    fn length_mismatch_is_reported() {
        let mut obj = sample_object();
        // 把第二段 payload 添一个字节，长度变成 3 而 manifest 仍说 2
        obj.sections[1].payload.push(99);
        let err = obj.check_consistency().expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        assert!(matches!(
            err.context.get("section_id"),
            Some(ContextValue::String(s)) if s == "data"
        ));
        assert!(matches!(
            err.context.get("expected_length"),
            Some(ContextValue::U64(2))
        ));
        assert!(matches!(
            err.context.get("actual_length"),
            Some(ContextValue::U64(3))
        ));
    }

    #[test]
    fn first_failure_short_circuits() {
        // 多个失败同时存在时，只报告第一个：count 一致、id[0] 错误时不应继续检查 id[1]。
        let mut obj = sample_object();
        obj.sections[0].id = "wrong".into();
        obj.sections[1].id = "also-wrong".into();
        let err = obj.check_consistency().expect_err("should fail");
        assert!(matches!(
            err.context.get("index"),
            Some(ContextValue::U64(0)),
        ));
    }
}
