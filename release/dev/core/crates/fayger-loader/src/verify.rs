//! Verify(Digest)：加载链的完整性校验阶段。
//!
//! 对一段已成功 `parse` 的 BuF 执行三类校验（任一失败立即返回）：
//!
//! 1. **Manifest 摘要**：`sha256([0, sections_offset))` 与 `raw_trailer.manifest_digest` 比较。
//! 2. **Section 段体摘要**：逐段按声明算法（M1 仅支持 `sha256`）计算并与 `descriptor.digest` 比较。
//! 3. **crc32**：`crc32([0, total_length - 4))` 与 `raw_trailer.crc32` 比较。
//!
//! 三类失败都返回 [`fayger_error::ErrorCode::LdrDigestMismatch`]，由 `context.reason`
//! 区分：`ManifestDigest` / `UnsupportedDigestAlgorithm` / `SectionDigest` / `Crc32`。
//!
//! 设计动机与覆盖区间见 `MILESTONES/M1-load-chain/design.md`。

use crate::digest::{crc32, sha256};
use fayger_buf::object::BuFObject;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// M1 支持的唯一段体摘要算法标识。
const SUPPORTED_DIGEST_ALGORITHM: &str = "sha256";

/// 对一段已成功 `parse` 的 BuF 执行 Verify(Digest)。
///
/// `bytes` 是原始字节流（用于区间摘要与 crc32），`obj` 是 `parse(bytes)` 的产物
/// （用于按 section 取 payload 与声明摘要）。两者必须来自同一次加载。
///
/// # Errors
///
/// 返回 [`fayger_error::ErrorCode::LdrDigestMismatch`]，`context.reason` 标明具体失败：
///
/// - `ManifestDigest`：`[0, sections_offset)` 区间摘要与 `manifest_digest` 不一致。
/// - `UnsupportedDigestAlgorithm`：某段声明了非 `sha256` 算法。
/// - `SectionDigest`：某段 payload 摘要与声明值不一致。
/// - `Crc32`：整文件 crc32 不一致。
pub fn verify_digest(bytes: &[u8], obj: &BuFObject) -> Result<(), FaygerError> {
    verify_manifest_digest(bytes, obj)?;
    verify_section_digests(obj)?;
    verify_crc32(bytes, obj)?;
    Ok(())
}

/// 构造 `LDR_DIGEST_MISMATCH` 错误并附带 reason。
fn digest_mismatch(reason: &'static str, message: impl Into<String>) -> FaygerError {
    FaygerError::new(ErrorCode::LdrDigestMismatch, Layer::Loader, message)
        .with_context("reason", ContextValue::string(reason))
}

/// 把字节切片转成小写十六进制，便于放进 context 做诊断。
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use core::fmt::Write as _;
        // 写入固定宽度十六进制；对 String 写入不会失败。
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// 校验 `manifest_digest` 覆盖区间 `[0, sections_offset)`。
fn verify_manifest_digest(bytes: &[u8], obj: &BuFObject) -> Result<(), FaygerError> {
    let sections_offset = obj.raw_header.sections_offset().ok_or_else(|| {
        digest_mismatch(
            "ManifestDigest",
            "sections offset overflow while verifying digest",
        )
    })?;
    let end = usize::try_from(sections_offset).map_err(|_| {
        digest_mismatch(
            "ManifestDigest",
            "sections offset exceeds addressable range",
        )
    })?;
    let region = bytes.get(..end).ok_or_else(|| {
        digest_mismatch(
            "ManifestDigest",
            "manifest digest region extends beyond byte stream",
        )
        .with_context("region_end", ContextValue::U64(sections_offset))
        .with_context(
            "actual_length",
            ContextValue::U64(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        )
    })?;
    let actual = sha256(region);
    if actual != obj.raw_trailer.manifest_digest {
        return Err(digest_mismatch(
            "ManifestDigest",
            "manifest digest does not match Header||Manifest||Section_Index",
        )
        .with_context(
            "expected",
            ContextValue::string(hex(&obj.raw_trailer.manifest_digest)),
        )
        .with_context("actual", ContextValue::string(hex(&actual))));
    }
    Ok(())
}

/// 逐段校验 Section payload 摘要。
fn verify_section_digests(obj: &BuFObject) -> Result<(), FaygerError> {
    for (descriptor, section) in obj.manifest.sections.iter().zip(obj.sections.iter()) {
        if descriptor.digest.algorithm != SUPPORTED_DIGEST_ALGORITHM {
            return Err(digest_mismatch(
                "UnsupportedDigestAlgorithm",
                "section declares an unsupported digest algorithm",
            )
            .with_context("section_id", ContextValue::string(section.id.clone()))
            .with_context(
                "algorithm",
                ContextValue::string(descriptor.digest.algorithm.clone()),
            ));
        }
        let actual = sha256(&section.payload);
        if actual.as_slice() != descriptor.digest.value.as_slice() {
            return Err(digest_mismatch(
                "SectionDigest",
                "section payload digest does not match declared digest",
            )
            .with_context("section_id", ContextValue::string(section.id.clone()))
            .with_context(
                "expected",
                ContextValue::string(hex(&descriptor.digest.value)),
            )
            .with_context("actual", ContextValue::string(hex(&actual))));
        }
    }
    Ok(())
}

/// 校验整文件 crc32 覆盖区间 `[0, total_length - 4)`。
fn verify_crc32(bytes: &[u8], obj: &BuFObject) -> Result<(), FaygerError> {
    let total = bytes.len();
    let end = total
        .checked_sub(4)
        .ok_or_else(|| digest_mismatch("Crc32", "byte stream too short to contain crc32 field"))?;
    // 安全：end <= total <= bytes.len()
    let region = &bytes[..end];
    let actual = crc32(region);
    if actual != obj.raw_trailer.crc32 {
        return Err(
            digest_mismatch("Crc32", "file crc32 does not match trailer")
                .with_context(
                    "expected",
                    ContextValue::U64(u64::from(obj.raw_trailer.crc32)),
                )
                .with_context("actual", ContextValue::U64(u64::from(actual))),
        );
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::verify_digest;
    use crate::seal::seal;
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::{parse, serialize};
    use fayger_error::{ContextValue, ErrorCode};
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
        BuFObject {
            manifest,
            sections: vec![
                Section::new("code", vec![1, 2, 3, 4]),
                Section::new("data", vec![5, 6]),
            ],
            raw_header: fayger_buf::format::Header {
                format_version: fayger_buf::format::FORMAT_VERSION_M0,
                manifest_offset: 0,
                manifest_length: 0,
                section_index_offset: 0,
                section_index_length: 0,
            },
            raw_trailer: fayger_buf::format::Trailer::placeholder(0),
        }
    }

    /// 封装并序列化一个样例对象，返回 (bytes, parsed_obj)。
    fn sealed_bytes_and_obj() -> (Vec<u8>, BuFObject) {
        let mut obj = sample_object();
        seal(&mut obj).expect("seal");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        (bytes, parsed)
    }

    fn reason_of(err: &fayger_error::FaygerError) -> String {
        match err.context.get("reason") {
            Some(ContextValue::String(s)) => s.clone(),
            other => panic!("expected reason string, got {other:?}"),
        }
    }

    #[test]
    fn sealed_object_passes_verification() {
        let (bytes, obj) = sealed_bytes_and_obj();
        verify_digest(&bytes, &obj).expect("sealed object should verify");
    }

    #[test]
    fn empty_sections_object_verifies() {
        let mut obj = sample_object();
        obj.manifest.sections.clear();
        obj.sections.clear();
        seal(&mut obj).expect("seal");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        verify_digest(&bytes, &parsed).expect("empty sections verify");
    }

    #[test]
    fn tampered_section_payload_fails_with_section_digest() {
        let (bytes, mut obj) = sealed_bytes_and_obj();
        // 篡改解析后对象的某段 payload，但不更新 descriptor.digest。
        obj.sections[0].payload[0] ^= 0xff;
        let err = verify_digest(&bytes, &obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_of(&err), "SectionDigest");
        assert!(matches!(
            err.context.get("section_id"),
            Some(ContextValue::String(s)) if s == "code"
        ));
    }

    #[test]
    fn unsupported_algorithm_fails() {
        let (bytes, mut obj) = sealed_bytes_and_obj();
        obj.manifest.sections[0].digest.algorithm = "sha512".into();
        let err = verify_digest(&bytes, &obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_of(&err), "UnsupportedDigestAlgorithm");
        assert!(matches!(
            err.context.get("algorithm"),
            Some(ContextValue::String(s)) if s == "sha512"
        ));
    }

    #[test]
    fn tampered_manifest_region_fails_with_manifest_digest() {
        let (mut bytes, obj) = sealed_bytes_and_obj();
        // 翻转 Manifest 区间内的一个字节（HEADER_SIZE 处即 Manifest 起点）。
        bytes[fayger_buf::format::HEADER_SIZE] ^= 0x01;
        // 注意：obj 仍是未篡改字节的解析结果，其 trailer.manifest_digest 是旧值。
        let err = verify_digest(&bytes, &obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_of(&err), "ManifestDigest");
    }

    #[test]
    fn tampered_crc32_field_fails_with_crc32() {
        let (mut bytes, _obj) = sealed_bytes_and_obj();
        // 翻转末 4 字节 crc32 中的一个字节。crc32 字段位于 [n-4, n)，落在 crc32 计算
        // 区间 [0, n-4) 之外，因此必须按真实 load 流程重新 parse，让 obj.raw_trailer.crc32
        // 取到被篡改的值，才会与区间计算结果失配。
        let n = bytes.len();
        bytes[n - 1] ^= 0xff;
        let obj = parse(&bytes).expect("parse still succeeds (M0 parse is digest-agnostic)");
        let err = verify_digest(&bytes, &obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_of(&err), "Crc32");
    }

    #[test]
    fn tampered_manifest_digest_field_fails_via_crc32() {
        // 只改 Trailer 的 manifest_digest 字段字节（不改 obj），crc32 应当先发现……
        // 但 verify_digest 顺序是 manifest_digest 先于 crc32。这里 obj 来自原始 bytes，
        // 其 manifest_digest 与 region 一致，所以 manifest 校验通过；改动落在 bytes 的
        // trailer 区域不影响 region([0,sections_offset))，因此由 crc32 兜底捕获。
        let (mut bytes, obj) = sealed_bytes_and_obj();
        let n = bytes.len();
        // manifest_digest 字段位于 [n-36, n-4)
        bytes[n - 36] ^= 0xff;
        let err = verify_digest(&bytes, &obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_of(&err), "Crc32");
    }
}
