//! source 加载链编排：[`load_from_source`]。
//!
//! Eager：读全字节 → 复用 M1 `parse + verify_digest`。
//! Lazy：读 Header → 骨架 → Trailer → `parse_skeleton` → 校验 manifest_digest → 构造 SectionLoader。
//!
//! 所有"load 调用栈内"的错误标注 `phase=eager`；仅 SectionLoader.fetch 内的错误标 `phase=lazy`。

use crate::digest::sha256;
use crate::source::{read_exact, BuFSource};
use crate::strategy::{LoadStrategy, LoadedArtifact, Phase};
use crate::verify::verify_digest;
use fayger_buf::format::{Header, HEADER_SIZE, TRAILER_SIZE};
use fayger_buf::object::BuFObject;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 从 BuF_Source 按指定策略加载。
///
/// # Errors
/// - 结构错误 `LDR_PARSE_FAIL`；
/// - 摘要错误 `LDR_DIGEST_MISMATCH`（含 `phase=eager`）；
/// - 源读取错误 `LDR_SOURCE_READ_FAILED`（`phase=eager`）。
///
/// Lazy 模式下 Section 段体的读取/校验错误在 [`LoadedArtifact::section`] 访问时才产生。
pub fn load_from_source(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
) -> Result<LoadedArtifact, FaygerError> {
    match strategy {
        LoadStrategy::Eager => load_eager(source.as_ref()),
        LoadStrategy::Lazy => load_lazy(source),
    }
}

/// 读取总长度并映射失败（load 调用栈内 → phase=eager）。
fn read_length(source: &dyn BuFSource) -> Result<u64, FaygerError> {
    source.length().map_err(|e| {
        FaygerError::new(
            ErrorCode::LdrSourceReadFailed,
            Layer::Loader,
            "failed to query BuF source length",
        )
        .with_context("phase", ContextValue::string("eager"))
        .with_context("detail", ContextValue::string(e.message))
    })
}

fn load_eager(source: &dyn BuFSource) -> Result<LoadedArtifact, FaygerError> {
    let total = read_length(source)?;
    let bytes = read_exact(source, 0, total, Phase::Eager)?;
    // 复用 M1：结构解析 + 完整 digest 校验（manifest_digest + section digest + crc32）。
    let obj = fayger_buf::parse(&bytes)?;
    verify_digest(&bytes, &obj).map_err(with_phase_eager)?;
    Ok(LoadedArtifact::eager(
        obj.manifest,
        obj.raw_header,
        obj.raw_trailer,
        obj.sections,
    ))
}

fn load_lazy(source: Box<dyn BuFSource>) -> Result<LoadedArtifact, FaygerError> {
    let total = read_length(source.as_ref())?;

    // 1. 读 Header 拿布局。
    let header_bytes = read_exact(
        source.as_ref(),
        0,
        u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX),
        Phase::Eager,
    )?;
    let header = read_header(&header_bytes)?;
    let sections_region_offset = header
        .sections_offset()
        .ok_or_else(|| parse_fail_eager("sections offset overflow in header"))?;

    // 2. 读可信骨架 [0, sections_region_offset)。
    let skeleton = read_exact(source.as_ref(), 0, sections_region_offset, Phase::Eager)?;

    // 3. 读 Trailer（末 TRAILER_SIZE 字节）。
    let trailer_offset = total
        .checked_sub(u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX))
        .ok_or_else(|| parse_fail_eager("total length too small for trailer"))?;
    let trailer_bytes = read_exact(
        source.as_ref(),
        trailer_offset,
        u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX),
        Phase::Eager,
    )?;

    // 4. 骨架解析（结构 + manifest 必填 + Section_Index 一致）。
    let obj: BuFObject = fayger_buf::parse_skeleton(&skeleton, &trailer_bytes, total)?;

    // 5. manifest_digest 校验：sha256(skeleton[0, sections_region_offset)) == trailer.manifest_digest。
    let end = usize::try_from(sections_region_offset)
        .map_err(|_| parse_fail_eager("sections offset exceeds addressable range"))?;
    let region = skeleton
        .get(..end)
        .ok_or_else(|| parse_fail_eager("manifest digest region out of bounds"))?;
    let actual = sha256(region);
    if actual != obj.raw_trailer.manifest_digest {
        return Err(FaygerError::new(
            ErrorCode::LdrDigestMismatch,
            Layer::Loader,
            "manifest digest does not match (lazy skeleton)",
        )
        .with_context("phase", ContextValue::string("eager"))
        .with_context("reason", ContextValue::string("ManifestDigest")));
    }

    Ok(LoadedArtifact::lazy(
        obj.manifest,
        obj.raw_header,
        obj.raw_trailer,
        source,
        sections_region_offset,
    ))
}

/// 读 Header（复用 [`fayger_buf::format::Header::read_from_prefix`]）；失败映射为 phase=eager 的结构错误。
fn read_header(bytes: &[u8]) -> Result<Header, FaygerError> {
    if bytes.len() < HEADER_SIZE {
        return Err(parse_fail_eager("header bytes too short"));
    }
    Header::read_from_prefix(bytes).map_err(|e| {
        parse_fail_eager("failed to parse header")
            .with_context("detail", ContextValue::string(format!("{e}")))
    })
}

fn parse_fail_eager(message: &'static str) -> FaygerError {
    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, message)
        .with_context("phase", ContextValue::string("eager"))
}

/// 给 M1 verify_digest 的错误补标 phase=eager（不改 M1）。
fn with_phase_eager(err: FaygerError) -> FaygerError {
    err.with_context("phase", ContextValue::string("eager"))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::load_from_source;
    use crate::seal::seal;
    use crate::source::{BuFSource, MemorySource, SourceError};
    use crate::strategy::LoadStrategy;
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::serialize;
    use fayger_error::ErrorCode;
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

    fn sealed_bytes() -> Vec<u8> {
        let mut obj = sample_object();
        seal(&mut obj).expect("seal");
        serialize(&obj).expect("serialize")
    }

    fn reason_phase(err: &fayger_error::FaygerError) -> Option<String> {
        match err.context.get("phase") {
            Some(fayger_error::ContextValue::String(s)) => Some(s.clone()),
            _ => None,
        }
    }

    #[test]
    fn eager_loads_all_sections() {
        let src = Box::new(MemorySource::new(sealed_bytes()));
        let mut art = load_from_source(src, LoadStrategy::Eager).expect("eager");
        assert!(art.is_eager());
        assert_eq!(art.section("code").expect("code"), &[1, 2, 3, 4]);
        assert_eq!(art.section("data").expect("data"), &[5, 6]);
    }

    #[test]
    fn lazy_loads_skeleton_only() {
        let src = Box::new(MemorySource::new(sealed_bytes()));
        let mut art = load_from_source(src, LoadStrategy::Lazy).expect("lazy");
        assert!(!art.is_eager());
        // 骨架就位：manifest 完整
        assert_eq!(art.manifest.sections.len(), 2);
        // loader 尚未加载任何段
        assert!(art.loader().expect("loader").loaded_ids().is_empty());
        // 首次访问后填充
        assert_eq!(art.section("code").expect("code"), &[1, 2, 3, 4]);
    }

    #[test]
    fn eager_section_digest_mismatch_phase_eager() {
        let mut bytes = sealed_bytes();
        // 篡改第一段 payload（落在 sections 区，crc32/section digest 会失败）。
        // sections 区起点 = sections_region_offset；用 parse 找。
        let obj = fayger_buf::parse(&bytes).expect("parse");
        let sro = obj.raw_header.sections_offset().expect("sro") as usize;
        bytes[sro] ^= 0xff;
        let src = Box::new(MemorySource::new(bytes));
        let err = load_from_source(src, LoadStrategy::Eager).expect_err("digest");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_phase(&err).as_deref(), Some("eager"));
    }

    #[test]
    fn eager_source_failure_phase_eager() {
        struct FailLength;
        impl BuFSource for FailLength {
            fn read_at(&self, _: u64, _: u64) -> Result<Vec<u8>, SourceError> {
                Ok(vec![])
            }
            fn length(&self) -> Result<u64, SourceError> {
                Err(SourceError::new("no length"))
            }
        }
        let err = load_from_source(Box::new(FailLength), LoadStrategy::Eager).expect_err("fail");
        assert_eq!(err.code, ErrorCode::LdrSourceReadFailed);
        assert_eq!(reason_phase(&err).as_deref(), Some("eager"));
    }

    #[test]
    fn lazy_skeleton_digest_mismatch_phase_eager() {
        let mut bytes = sealed_bytes();
        // 篡改 Trailer 中的 manifest_digest 字段（不破坏骨架结构）：骨架解析成功，
        // 但 sha256(skeleton) 与被篡改的 manifest_digest 不符 → ManifestDigest，phase=eager。
        // manifest_digest 字段位于 [total-36, total-4)。
        let n = bytes.len();
        bytes[n - 36] ^= 0xff;
        let src = Box::new(MemorySource::new(bytes));
        let err = load_from_source(src, LoadStrategy::Lazy).expect_err("digest");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
        assert_eq!(reason_phase(&err).as_deref(), Some("eager"));
    }
}
