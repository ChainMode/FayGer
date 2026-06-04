//! BuF 字节流解析器。
//!
//! 顶层 API：[`parse`]。流程见设计文档 §`parser` 模块。
//!
//! 解析顺序：
//!
//! 1. **Read**：从字节流抽取 Header / Manifest / Section_Index / Trailer。
//! 2. **Verify Structural**：检查 magic、Header 字段、Section_Index 边界。
//! 3. **Verify Digest**：M0 阶段不计算实际 sha-256，但仍校验 Section_Index
//!    与 manifest.sections 数量与字段一致；Trailer.total_length 与文件总长一致。
//! 4. **HandOff**：构造 [`BuFObject`] 返回。
//!
//! 所有失败统一返回 [`FaygerError`]，code = [`fayger_error::ErrorCode::LdrParseFail`]，
//! `context.reason` 区分具体原因（参考 `requirements.md` Requirement 3.3-3.8）。

use crate::canonical::canonical_decode;
use crate::error::cbor_decode_fail;
use crate::format::{Header, Trailer, BUF_MAGIC, HEADER_SIZE, TRAILER_SIZE};
use crate::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use crate::manifest::BuFManifest;
use crate::object::{BuFObject, Section};
use binrw::BinRead;
use fayger_error::{ContextValue, FaygerError};
use serde::{Deserialize, Serialize};

/// Section_Index 段的内部条目。
///
/// 这是 BuF 文件中实际写在 Section_Index CBOR 数组中的对象；与
/// [`crate::manifest::SectionDescriptor`] 在字段集上几乎等价，差别在于
/// `offset` 字段：Section_Index 中的 offset 是相对于 **Sections 段开头** 的偏移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SectionIndexEntry {
    pub(crate) section_id: String,
    pub(crate) kind: SectionKind,
    /// 相对于 Sections 段开头（不是文件开头）的字节偏移。
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) digest: Digest,
    pub(crate) visibility: SectionVisibility,
    pub(crate) profile_constraints: ProfileConstraints,
}

/// 顶层入口：把字节流解析成 [`BuFObject`]。
///
/// 失败原因（`context.reason`）：
///
/// - `Truncated`：字节流不足以容纳 Header + Trailer。
/// - `MagicMismatch`：前 4 字节不是 `"BUF\0"`。
/// - `OffsetOutOfBounds`：Header 中的 offset/length 越过字节流边界。
/// - `ManifestInvalid`：Manifest CBOR 解码失败或不是 canonical。
/// - `ManifestMissingField`：Manifest 缺必填字段（由 BuFManifest 反序列化报）。
/// - `SectionIndexInvalid`：Section_Index CBOR 解码失败或不是 canonical。
/// - `SectionOutOfBounds`：Section_Index 条目的 offset+length 超过 Sections 段长度。
/// - `SectionMismatch`：Section_Index 与 manifest.sections 不一致（数量/id/length）。
/// - `TrailerLengthMismatch`：Trailer.total_length 与字节流总长不等。
#[allow(clippy::too_many_lines)] // parse 是顺序流水线，拆分会割裂可读性
pub fn parse(bytes: &[u8]) -> Result<BuFObject, FaygerError> {
    // ----- Read & Verify Structural -----

    if bytes.len() < HEADER_SIZE + TRAILER_SIZE {
        return Err(
            parse_fail("byte stream too short to contain header + trailer")
                .with_context("reason", ContextValue::string("Truncated"))
                .with_context(
                    "expected_min_length",
                    ContextValue::U64(
                        u64::try_from(HEADER_SIZE + TRAILER_SIZE).unwrap_or(u64::MAX),
                    ),
                )
                .with_context(
                    "actual_length",
                    ContextValue::U64(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                ),
        );
    }

    // 直接读前 4 字节，给出更精准的 MagicMismatch 错误。
    if bytes[..4] != BUF_MAGIC {
        return Err(parse_fail("magic does not match BUF\\0")
            .with_context("reason", ContextValue::string("MagicMismatch"))
            .with_context("magic", ContextValue::Bytes(bytes[..4].to_vec())));
    }

    let header = read_header(bytes)?;

    // 验证 Header 中的 offset 字段不越界
    let total_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    check_offset_in_bounds(
        "manifest",
        header.manifest_offset,
        header.manifest_length,
        total_len,
    )?;
    check_offset_in_bounds(
        "section_index",
        header.section_index_offset,
        header.section_index_length,
        total_len,
    )?;

    // Manifest 段
    let manifest_bytes = slice_segment(bytes, header.manifest_offset, header.manifest_length)?;
    let manifest: BuFManifest = canonical_decode(manifest_bytes).map_err(|e| {
        cbor_decode_fail("manifest CBOR decode failed")
            .with_context("reason", manifest_failure_reason(&e))
            .with_context("detail", ContextValue::string(format!("{e}")))
    })?;

    // Section_Index 段
    let index_bytes = slice_segment(
        bytes,
        header.section_index_offset,
        header.section_index_length,
    )?;
    let section_index: Vec<SectionIndexEntry> = canonical_decode(index_bytes).map_err(|e| {
        cbor_decode_fail("section index CBOR decode failed")
            .with_context("reason", ContextValue::string("SectionIndexInvalid"))
            .with_context("detail", ContextValue::string(format!("{e}")))
    })?;

    // Sections 段：从 section_index_offset + section_index_length 开始，
    // 直到 trailer_offset = bytes.len() - TRAILER_SIZE。
    let trailer_offset = total_len
        .checked_sub(u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX))
        .ok_or_else(|| {
            parse_fail("trailer position underflow")
                .with_context("reason", ContextValue::string("Truncated"))
        })?;
    let sections_offset = header
        .section_index_offset
        .checked_add(header.section_index_length)
        .ok_or_else(|| {
            parse_fail("sections offset overflow")
                .with_context("reason", ContextValue::string("OffsetOutOfBounds"))
                .with_context(
                    "field",
                    ContextValue::string("section_index_offset+section_index_length"),
                )
        })?;
    if sections_offset > trailer_offset {
        return Err(parse_fail("sections region overlaps trailer")
            .with_context("reason", ContextValue::string("OffsetOutOfBounds"))
            .with_context("sections_offset", ContextValue::U64(sections_offset))
            .with_context("trailer_offset", ContextValue::U64(trailer_offset)));
    }
    let sections_region_len = trailer_offset - sections_offset;
    let sections_region = slice_segment(bytes, sections_offset, sections_region_len)?;

    // 用 Section_Index 切出每段 payload
    let payloads = read_section_payloads(&section_index, sections_region)?;

    // Trailer
    let trailer = read_trailer(bytes, trailer_offset)?;
    if trailer.total_length != total_len {
        return Err(
            parse_fail("trailer total_length disagrees with stream length")
                .with_context("reason", ContextValue::string("TrailerLengthMismatch"))
                .with_context(
                    "trailer_total_length",
                    ContextValue::U64(trailer.total_length),
                )
                .with_context("actual_length", ContextValue::U64(total_len)),
        );
    }

    // ----- Section_Index 与 manifest.sections 一致性检查 -----
    cross_check_index_and_manifest(&section_index, &manifest)?;

    let sections: Vec<Section> = section_index
        .iter()
        .zip(payloads)
        .map(|(entry, payload)| Section {
            id: entry.section_id.clone(),
            payload,
        })
        .collect();

    Ok(BuFObject {
        manifest,
        sections,
        raw_header: header,
        raw_trailer: trailer,
    })
}

/// 仅从"骨架字节"解析 [`BuFObject`]，不读取 Section 段体、不校验 crc32 与 section digest。
///
/// 供 Lazy 加载（M5）使用：调用方先从 BuF_Source 读取 `skeleton`（`[0, sections_region_offset)`，
/// 即 Header + Manifest + Section_Index）与末 [`TRAILER_SIZE`] 字节 `trailer`，并提供制品总长度
/// `total_len`。本函数完成结构性校验（magic、offset 边界、Manifest CBOR 与必填字段、
/// Section_Index 与 manifest.sections 一致），但：
///
/// - 返回的 [`BuFObject::sections`] 为空（Lazy 模式按需填充）；
/// - 不校验 crc32（覆盖 Section 段体，Lazy 未读全）；
/// - 不校验任何 Section 段体 digest（推迟到首次访问）；
/// - manifest_digest 的校验由调用方（加载层）基于 `skeleton` 完成。
///
/// `skeleton` 的长度必须恰好等于 `sections_region_offset`（由 Header 推导）。
///
/// # Errors
/// 结构不合法（magic / offset / CBOR / 必填 / Section_Index 不一致 / 长度自洽性）时返回
/// `LDR_PARSE_FAIL`。
pub fn parse_skeleton(
    skeleton: &[u8],
    trailer: &[u8],
    total_len: u64,
) -> Result<BuFObject, FaygerError> {
    // Header：骨架前 HEADER_SIZE 字节。
    if skeleton.len() < HEADER_SIZE {
        return Err(parse_fail("skeleton shorter than header")
            .with_context("reason", ContextValue::string("Truncated")));
    }
    if skeleton[..4] != BUF_MAGIC {
        return Err(parse_fail("magic does not match BUF\\0")
            .with_context("reason", ContextValue::string("MagicMismatch"))
            .with_context("magic", ContextValue::Bytes(skeleton[..4].to_vec())));
    }
    let header = read_header(skeleton)?;

    // sections_region_offset = section_index_offset + section_index_length。
    let sections_region_offset = header
        .section_index_offset
        .checked_add(header.section_index_length)
        .ok_or_else(|| {
            out_of_bounds(
                "section_index_offset+section_index_length",
                header.section_index_offset,
                header.section_index_length,
            )
        })?;

    // 骨架长度必须恰好等于 sections_region_offset。
    if u64::try_from(skeleton.len()).unwrap_or(u64::MAX) != sections_region_offset {
        return Err(parse_fail("skeleton length does not match header layout")
            .with_context("reason", ContextValue::string("OffsetOutOfBounds"))
            .with_context(
                "sections_region_offset",
                ContextValue::U64(sections_region_offset),
            )
            .with_context(
                "skeleton_length",
                ContextValue::U64(u64::try_from(skeleton.len()).unwrap_or(u64::MAX)),
            ));
    }

    // Manifest 与 Section_Index 偏移必须落在骨架内（[HEADER_SIZE, sections_region_offset]）。
    check_skeleton_segment(
        "manifest",
        header.manifest_offset,
        header.manifest_length,
        sections_region_offset,
    )?;
    check_skeleton_segment(
        "section_index",
        header.section_index_offset,
        header.section_index_length,
        sections_region_offset,
    )?;

    let manifest_bytes = slice_segment(skeleton, header.manifest_offset, header.manifest_length)?;
    let manifest: BuFManifest = canonical_decode(manifest_bytes).map_err(|e| {
        cbor_decode_fail("manifest CBOR decode failed")
            .with_context("reason", manifest_failure_reason(&e))
            .with_context("detail", ContextValue::string(format!("{e}")))
    })?;

    let index_bytes = slice_segment(
        skeleton,
        header.section_index_offset,
        header.section_index_length,
    )?;
    let section_index: Vec<SectionIndexEntry> = canonical_decode(index_bytes).map_err(|e| {
        cbor_decode_fail("section index CBOR decode failed")
            .with_context("reason", ContextValue::string("SectionIndexInvalid"))
            .with_context("detail", ContextValue::string(format!("{e}")))
    })?;

    cross_check_index_and_manifest(&section_index, &manifest)?;

    // Trailer：从给定的末段字节解析（offset 0，因为调用方已切出末 TRAILER_SIZE 字节）。
    if trailer.len() != TRAILER_SIZE {
        return Err(parse_fail("trailer slice has wrong size")
            .with_context("reason", ContextValue::string("Truncated"))
            .with_context(
                "expected",
                ContextValue::U64(u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX)),
            )
            .with_context(
                "actual",
                ContextValue::U64(u64::try_from(trailer.len()).unwrap_or(u64::MAX)),
            ));
    }
    let trailer_obj = read_trailer(trailer, 0)?;

    // total_length 自洽：Trailer 声明的总长应等于调用方提供的 total_len，
    // 且 total_len 至少容纳 sections_region_offset + TRAILER_SIZE。
    let min_total = sections_region_offset
        .checked_add(u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX))
        .ok_or_else(|| parse_fail("total length overflow"))?;
    if total_len < min_total {
        return Err(parse_fail("total length too small for skeleton + trailer")
            .with_context("reason", ContextValue::string("Truncated"))
            .with_context("total_len", ContextValue::U64(total_len))
            .with_context("min_total", ContextValue::U64(min_total)));
    }
    if trailer_obj.total_length != total_len {
        return Err(
            parse_fail("trailer total_length disagrees with provided total length")
                .with_context("reason", ContextValue::string("TrailerLengthMismatch"))
                .with_context(
                    "trailer_total_length",
                    ContextValue::U64(trailer_obj.total_length),
                )
                .with_context("total_len", ContextValue::U64(total_len)),
        );
    }

    Ok(BuFObject {
        manifest,
        sections: Vec::new(), // Lazy：按需填充
        raw_header: header,
        raw_trailer: trailer_obj,
    })
}

/// 检查 (offset, length) 落在 [HEADER_SIZE, region_end] 内（骨架版边界检查）。
fn check_skeleton_segment(
    field: &str,
    offset: u64,
    length: u64,
    region_end: u64,
) -> Result<(), FaygerError> {
    let header_end = u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX);
    let end = offset
        .checked_add(length)
        .ok_or_else(|| out_of_bounds(field, offset, length))?;
    if offset < header_end || end > region_end {
        return Err(out_of_bounds(field, offset, length)
            .with_context("region_end", ContextValue::U64(region_end)));
    }
    Ok(())
}

/// 构造 LDR_PARSE_FAIL 错误。
fn parse_fail(message: impl Into<String>) -> FaygerError {
    FaygerError::new(
        fayger_error::ErrorCode::LdrParseFail,
        fayger_error::Layer::Loader,
        message,
    )
}

/// 用 binrw 把 Header 读出来。
fn read_header(bytes: &[u8]) -> Result<Header, FaygerError> {
    let mut cursor = std::io::Cursor::new(&bytes[..HEADER_SIZE]);
    Header::read(&mut cursor).map_err(|e| {
        // 我们已经手工检查过 magic；如果 binrw 仍然失败，多半是字段值无法读全。
        parse_fail("failed to parse header")
            .with_context("reason", ContextValue::string("Truncated"))
            .with_context("detail", ContextValue::string(format!("{e}")))
    })
}

/// 从 trailer_offset 处用 binrw 把 Trailer 读出来。
fn read_trailer(bytes: &[u8], trailer_offset: u64) -> Result<Trailer, FaygerError> {
    let trailer_offset_usize =
        usize::try_from(trailer_offset).map_err(|_| parse_fail("trailer offset overflow"))?;
    let trailer_bytes = bytes
        .get(trailer_offset_usize..)
        .ok_or_else(|| parse_fail("trailer slice out of bounds"))?;
    let mut cursor = std::io::Cursor::new(trailer_bytes);
    Trailer::read(&mut cursor).map_err(|e| {
        parse_fail("failed to parse trailer")
            .with_context("reason", ContextValue::string("Truncated"))
            .with_context("detail", ContextValue::string(format!("{e}")))
    })
}

/// 检查 (offset, length) 这一段在 [HEADER_SIZE, total_len - TRAILER_SIZE] 范围内。
fn check_offset_in_bounds(
    field: &str,
    offset: u64,
    length: u64,
    total_len: u64,
) -> Result<(), FaygerError> {
    let header_end = u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX);
    let trailer_start = total_len
        .checked_sub(u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX))
        .ok_or_else(|| {
            parse_fail("trailer position underflow")
                .with_context("reason", ContextValue::string("Truncated"))
        })?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| out_of_bounds(field, offset, length))?;
    if offset < header_end || end > trailer_start {
        return Err(out_of_bounds(field, offset, length)
            .with_context("trailer_start", ContextValue::U64(trailer_start)));
    }
    Ok(())
}

fn out_of_bounds(field: &str, offset: u64, length: u64) -> FaygerError {
    parse_fail("offset out of bounds")
        .with_context("reason", ContextValue::string("OffsetOutOfBounds"))
        .with_context("field", ContextValue::string(field.to_owned()))
        .with_context("offset", ContextValue::U64(offset))
        .with_context("length", ContextValue::U64(length))
}

/// 把字节流的 [offset, offset+length) 切成 slice。
fn slice_segment(bytes: &[u8], offset: u64, length: u64) -> Result<&[u8], FaygerError> {
    let start = usize::try_from(offset).map_err(|_| out_of_bounds("offset", offset, length))?;
    let end = offset
        .checked_add(length)
        .and_then(|e| usize::try_from(e).ok())
        .ok_or_else(|| out_of_bounds("offset+length", offset, length))?;
    bytes
        .get(start..end)
        .ok_or_else(|| out_of_bounds("offset+length", offset, length))
}

/// 用 Section_Index 把 sections_region 切成各段 payload。
///
/// 同时检查每条索引项的 `offset+length` 是否越过 sections_region 的长度。
fn read_section_payloads(
    index: &[SectionIndexEntry],
    sections_region: &[u8],
) -> Result<Vec<Vec<u8>>, FaygerError> {
    let region_len = u64::try_from(sections_region.len()).unwrap_or(u64::MAX);
    let mut out = Vec::with_capacity(index.len());
    for entry in index {
        let end = entry.offset.checked_add(entry.length).ok_or_else(|| {
            section_out_of_bounds(&entry.section_id, entry.offset, entry.length, region_len)
        })?;
        if end > region_len {
            return Err(section_out_of_bounds(
                &entry.section_id,
                entry.offset,
                entry.length,
                region_len,
            ));
        }
        let start = usize::try_from(entry.offset).map_err(|_| {
            section_out_of_bounds(&entry.section_id, entry.offset, entry.length, region_len)
        })?;
        let end_usize = usize::try_from(end).map_err(|_| {
            section_out_of_bounds(&entry.section_id, entry.offset, entry.length, region_len)
        })?;
        out.push(sections_region[start..end_usize].to_vec());
    }
    Ok(out)
}

fn section_out_of_bounds(
    section_id: &str,
    offset: u64,
    length: u64,
    region_len: u64,
) -> FaygerError {
    parse_fail("section out of bounds")
        .with_context("reason", ContextValue::string("SectionOutOfBounds"))
        .with_context("section_id", ContextValue::string(section_id.to_owned()))
        .with_context("offset", ContextValue::U64(offset))
        .with_context("length", ContextValue::U64(length))
        .with_context("region_length", ContextValue::U64(region_len))
}

/// 交叉校验 Section_Index 与 manifest.sections。
///
/// 两者数量必须相等，且按顺序匹配 id / kind / digest / length / visibility / profile_constraints。
fn cross_check_index_and_manifest(
    index: &[SectionIndexEntry],
    manifest: &BuFManifest,
) -> Result<(), FaygerError> {
    if index.len() != manifest.sections.len() {
        return Err(parse_fail("section index count mismatch")
            .with_context("reason", ContextValue::string("SectionMismatch"))
            .with_context(
                "index_count",
                ContextValue::U64(u64::try_from(index.len()).unwrap_or(u64::MAX)),
            )
            .with_context(
                "manifest_count",
                ContextValue::U64(u64::try_from(manifest.sections.len()).unwrap_or(u64::MAX)),
            ));
    }
    for (i, (entry, descriptor)) in index.iter().zip(manifest.sections.iter()).enumerate() {
        if entry.section_id != descriptor.id {
            return Err(parse_fail("section id mismatch between index and manifest")
                .with_context("reason", ContextValue::string("SectionMismatch"))
                .with_context(
                    "index",
                    ContextValue::U64(u64::try_from(i).unwrap_or(u64::MAX)),
                )
                .with_context("index_id", ContextValue::string(entry.section_id.clone()))
                .with_context("manifest_id", ContextValue::string(descriptor.id.clone())));
        }
        if entry.length != descriptor.length {
            return Err(
                parse_fail("section length mismatch between index and manifest")
                    .with_context("reason", ContextValue::string("SectionMismatch"))
                    .with_context("section_id", ContextValue::string(descriptor.id.clone()))
                    .with_context("index_length", ContextValue::U64(entry.length))
                    .with_context("manifest_length", ContextValue::U64(descriptor.length)),
            );
        }
        if entry.kind != descriptor.kind
            || entry.digest != descriptor.digest
            || entry.visibility != descriptor.visibility
            || entry.profile_constraints != descriptor.profile_constraints
        {
            return Err(
                parse_fail("section descriptor disagreement between index and manifest")
                    .with_context("reason", ContextValue::string("SectionMismatch"))
                    .with_context("section_id", ContextValue::string(descriptor.id.clone())),
            );
        }
    }
    Ok(())
}

/// 根据 manifest 解码出错的内部错误推断 `reason`。
fn manifest_failure_reason(err: &FaygerError) -> ContextValue {
    let msg = format!("{err}");
    if msg.contains("missing required field") {
        ContextValue::string("ManifestMissingField")
    } else if msg.contains("NonCanonicalCbor") || msg.contains("not canonical") {
        ContextValue::string("NonCanonicalCbor")
    } else {
        ContextValue::string("ManifestInvalid")
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value
)]
mod tests {
    use super::*;
    use crate::canonical::canonical_encode;
    use crate::format::{Header, FORMAT_VERSION_M0, MANIFEST_DIGEST_SIZE};
    use crate::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use binrw::BinWrite;
    use std::collections::{BTreeMap, BTreeSet};

    /// 测试辅助：构造一个最小但合法的 BuF 字节流。
    ///
    /// 返回 (bytes, expected_manifest, expected_payloads)。
    fn build_buf_bytes(
        sections: Vec<(String, SectionKind, Vec<u8>)>,
    ) -> (Vec<u8>, BuFManifest, Vec<Vec<u8>>) {
        let descriptors: Vec<SectionDescriptor> = sections
            .iter()
            .map(|(id, kind, payload)| SectionDescriptor {
                id: id.clone(),
                kind: kind.clone(),
                digest: Digest::placeholder(),
                length: payload.len() as u64,
                visibility: SectionVisibility::Required,
                profile_constraints: ProfileConstraints::default(),
            })
            .collect();

        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: descriptors.clone(),
            signature: None,
            extensions: BTreeMap::new(),
        };

        let manifest_bytes = canonical_encode(&manifest).expect("encode manifest");

        // Section_Index：按 sections 顺序累加 offset
        let mut entries: Vec<SectionIndexEntry> = Vec::with_capacity(sections.len());
        let mut current_offset: u64 = 0;
        let mut payloads: Vec<Vec<u8>> = Vec::with_capacity(sections.len());
        for ((id, kind, payload), descriptor) in sections.iter().zip(descriptors.iter()) {
            entries.push(SectionIndexEntry {
                section_id: id.clone(),
                kind: kind.clone(),
                offset: current_offset,
                length: descriptor.length,
                digest: descriptor.digest.clone(),
                visibility: descriptor.visibility,
                profile_constraints: descriptor.profile_constraints.clone(),
            });
            current_offset += descriptor.length;
            payloads.push(payload.clone());
        }
        let index_bytes = canonical_encode(&entries).expect("encode index");

        let manifest_offset = HEADER_SIZE as u64;
        let manifest_length = manifest_bytes.len() as u64;
        let section_index_offset = manifest_offset + manifest_length;
        let section_index_length = index_bytes.len() as u64;
        let sections_offset = section_index_offset + section_index_length;
        let sections_total_len: u64 = payloads.iter().map(|p| p.len() as u64).sum();
        let total_length = sections_offset + sections_total_len + TRAILER_SIZE as u64;

        let header = Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset,
            manifest_length,
            section_index_offset,
            section_index_length,
        };
        let trailer = Trailer {
            total_length,
            manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
            crc32: 0,
        };

        let mut bytes = Vec::with_capacity(total_length as usize);
        // Header
        let mut hb = std::io::Cursor::new(Vec::new());
        header.write(&mut hb).expect("write header");
        bytes.extend_from_slice(&hb.into_inner());
        // Manifest
        bytes.extend_from_slice(&manifest_bytes);
        // Section_Index
        bytes.extend_from_slice(&index_bytes);
        // Sections
        for p in &payloads {
            bytes.extend_from_slice(p);
        }
        // Trailer
        let mut tb = std::io::Cursor::new(Vec::new());
        trailer.write(&mut tb).expect("write trailer");
        bytes.extend_from_slice(&tb.into_inner());

        assert_eq!(bytes.len() as u64, total_length, "total_length sanity");
        (bytes, manifest, payloads)
    }

    fn sample_two_sections() -> (Vec<u8>, BuFManifest, Vec<Vec<u8>>) {
        build_buf_bytes(vec![
            ("code".into(), SectionKind::Code, vec![1, 2, 3, 4]),
            ("data".into(), SectionKind::Data, vec![5, 6]),
        ])
    }

    #[test]
    fn parses_minimal_two_section_buf() {
        let (bytes, manifest, payloads) = sample_two_sections();
        let obj = parse(&bytes).expect("parse ok");
        assert_eq!(obj.manifest, manifest);
        assert_eq!(obj.sections.len(), 2);
        assert_eq!(obj.sections[0].id, "code");
        assert_eq!(obj.sections[0].payload, payloads[0]);
        assert_eq!(obj.sections[1].id, "data");
        assert_eq!(obj.sections[1].payload, payloads[1]);
        // 一致性自检也应该通过
        obj.check_consistency().expect("consistent");
    }

    #[test]
    fn parses_buf_with_zero_sections() {
        let (bytes, _manifest, _) = build_buf_bytes(vec![]);
        let obj = parse(&bytes).expect("parse ok");
        assert!(obj.sections.is_empty());
        assert!(obj.manifest.sections.is_empty());
    }

    // ---- 错误条件覆盖 ----

    #[test]
    fn rejects_truncated_below_header_plus_trailer() {
        let bytes = vec![0u8; HEADER_SIZE + TRAILER_SIZE - 1];
        let err = parse(&bytes).expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrParseFail);
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "Truncated"
        ));
    }

    #[test]
    fn rejects_wrong_magic() {
        let (mut bytes, _, _) = sample_two_sections();
        bytes[0] = b'X'; // 把 'B' 改成 'X'
        let err = parse(&bytes).expect_err("should fail");
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "MagicMismatch"
        ));
        // magic 字段应当包含实际读到的字节
        assert!(matches!(
            err.context.get("magic"),
            Some(ContextValue::Bytes(_))
        ));
    }

    #[test]
    fn rejects_offset_out_of_bounds() {
        let (mut bytes, _, _) = sample_two_sections();
        // 把 manifest_offset 改成超过 bytes.len()。Header 的 manifest_offset 在偏移 8（magic 4 + ver 4）。
        let bogus: u64 = 999_999_999;
        bytes[8..16].copy_from_slice(&bogus.to_le_bytes());
        let err = parse(&bytes).expect_err("should fail");
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "OffsetOutOfBounds"
        ));
        assert!(matches!(
            err.context.get("field"),
            Some(ContextValue::String(s)) if s == "manifest"
        ));
    }

    #[test]
    fn rejects_invalid_manifest_cbor() {
        let (mut bytes, _, _) = sample_two_sections();
        // 把 Manifest 段的第一个字节改成 0xff（非合法 CBOR header）。
        // Manifest 起点是 HEADER_SIZE = 40。
        bytes[HEADER_SIZE] = 0xff;
        let err = parse(&bytes).expect_err("should fail");
        let reason = match err.context.get("reason") {
            Some(ContextValue::String(s)) => s.clone(),
            other => panic!("expected reason, got {other:?}"),
        };
        assert!(
            reason == "ManifestInvalid"
                || reason == "ManifestMissingField"
                || reason == "NonCanonicalCbor",
            "unexpected reason: {reason}"
        );
    }

    #[test]
    fn rejects_trailer_length_mismatch() {
        let (mut bytes, _, _) = sample_two_sections();
        // 改 trailer.total_length 为错误值。Trailer 起点 = bytes.len() - TRAILER_SIZE，
        // total_length 是 trailer 前 8 字节。
        let trailer_offset = bytes.len() - TRAILER_SIZE;
        let bogus: u64 = 42;
        bytes[trailer_offset..trailer_offset + 8].copy_from_slice(&bogus.to_le_bytes());
        let err = parse(&bytes).expect_err("should fail");
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "TrailerLengthMismatch"
        ));
    }

    #[test]
    fn rejects_section_out_of_bounds_in_index() {
        // 构造 BuF：manifest 与 index 中 length 都说 100，
        // 但实际 sections region 比这小。这需要手工拼字节。
        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![SectionDescriptor {
                id: "code".into(),
                kind: SectionKind::Code,
                digest: Digest::placeholder(),
                length: 100, // 谎报
                visibility: SectionVisibility::Required,
                profile_constraints: ProfileConstraints::default(),
            }],
            signature: None,
            extensions: BTreeMap::new(),
        };
        let manifest_bytes = canonical_encode(&manifest).expect("encode");
        let entries = vec![SectionIndexEntry {
            section_id: "code".into(),
            kind: SectionKind::Code,
            offset: 0,
            length: 100,
            digest: Digest::placeholder(),
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        }];
        let index_bytes = canonical_encode(&entries).expect("encode");

        let manifest_offset = HEADER_SIZE as u64;
        let manifest_length = manifest_bytes.len() as u64;
        let section_index_offset = manifest_offset + manifest_length;
        let section_index_length = index_bytes.len() as u64;
        // 实际只放 4 个字节而不是 100
        let actual_payload = vec![1u8, 2, 3, 4];
        let total_length = section_index_offset
            + section_index_length
            + actual_payload.len() as u64
            + TRAILER_SIZE as u64;

        let header = Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset,
            manifest_length,
            section_index_offset,
            section_index_length,
        };
        let trailer = Trailer {
            total_length,
            manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
            crc32: 0,
        };

        let mut bytes = Vec::new();
        let mut hb = std::io::Cursor::new(Vec::new());
        header.write(&mut hb).expect("write header");
        bytes.extend_from_slice(&hb.into_inner());
        bytes.extend_from_slice(&manifest_bytes);
        bytes.extend_from_slice(&index_bytes);
        bytes.extend_from_slice(&actual_payload);
        let mut tb = std::io::Cursor::new(Vec::new());
        trailer.write(&mut tb).expect("write trailer");
        bytes.extend_from_slice(&tb.into_inner());

        let err = parse(&bytes).expect_err("should fail");
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "SectionOutOfBounds"
        ));
    }

    #[test]
    fn rejects_section_mismatch_between_index_and_manifest() {
        // manifest 与 index 都合法但 id 不一致：手工构造。
        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![SectionDescriptor {
                id: "code".into(),
                kind: SectionKind::Code,
                digest: Digest::placeholder(),
                length: 4,
                visibility: SectionVisibility::Required,
                profile_constraints: ProfileConstraints::default(),
            }],
            signature: None,
            extensions: BTreeMap::new(),
        };
        let manifest_bytes = canonical_encode(&manifest).expect("encode");
        // Index 中 id 故意写成 "data"
        let entries = vec![SectionIndexEntry {
            section_id: "data".into(),
            kind: SectionKind::Code,
            offset: 0,
            length: 4,
            digest: Digest::placeholder(),
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        }];
        let index_bytes = canonical_encode(&entries).expect("encode");

        let manifest_offset = HEADER_SIZE as u64;
        let manifest_length = manifest_bytes.len() as u64;
        let section_index_offset = manifest_offset + manifest_length;
        let section_index_length = index_bytes.len() as u64;
        let payload = vec![1u8, 2, 3, 4];
        let total_length = section_index_offset
            + section_index_length
            + payload.len() as u64
            + TRAILER_SIZE as u64;

        let header = Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset,
            manifest_length,
            section_index_offset,
            section_index_length,
        };
        let trailer = Trailer {
            total_length,
            manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
            crc32: 0,
        };
        let mut bytes = Vec::new();
        let mut hb = std::io::Cursor::new(Vec::new());
        header.write(&mut hb).expect("write header");
        bytes.extend_from_slice(&hb.into_inner());
        bytes.extend_from_slice(&manifest_bytes);
        bytes.extend_from_slice(&index_bytes);
        bytes.extend_from_slice(&payload);
        let mut tb = std::io::Cursor::new(Vec::new());
        trailer.write(&mut tb).expect("write trailer");
        bytes.extend_from_slice(&tb.into_inner());

        let err = parse(&bytes).expect_err("should fail");
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "SectionMismatch"
        ));
    }

    #[test]
    fn parser_does_not_execute_section_payloads() {
        // 这是一个语义验证：parser 是纯字节操作；payload 可以是任意字节包括看起来像
        // 可执行码的 0xff 0xfe。这里我们就用 [0xff; 16]，断言 parse 仍成功且 payload 内容相等。
        let payload = vec![0xffu8; 16];
        let (bytes, _manifest, payloads) =
            build_buf_bytes(vec![("code".into(), SectionKind::Code, payload.clone())]);
        let obj = parse(&bytes).expect("parse ok");
        assert_eq!(obj.sections[0].payload, payload);
        assert_eq!(payloads[0], payload);
    }

    // ----- parse_skeleton (M5) -----

    #[test]
    fn parse_skeleton_yields_full_manifest_and_empty_sections() {
        let (bytes, manifest, _payloads) = sample_two_sections();
        // 完整 parse 得到 sections_region_offset。
        let full = parse(&bytes).expect("parse");
        let sro = full.raw_header.sections_offset().expect("sro") as usize;
        let skeleton = &bytes[..sro];
        let trailer = &bytes[bytes.len() - super::TRAILER_SIZE..];
        let total = bytes.len() as u64;

        let obj = parse_skeleton(skeleton, trailer, total).expect("parse_skeleton");
        // manifest 与完整解析一致
        assert_eq!(obj.manifest, manifest);
        // sections 为空（Lazy 按需填充）
        assert!(obj.sections.is_empty());
        // header/trailer 与完整解析一致
        assert_eq!(obj.raw_header, full.raw_header);
        assert_eq!(obj.raw_trailer, full.raw_trailer);
    }

    #[test]
    fn parse_skeleton_rejects_wrong_skeleton_length() {
        let (bytes, _m, _p) = sample_two_sections();
        let full = parse(&bytes).expect("parse");
        let sro = full.raw_header.sections_offset().expect("sro") as usize;
        let trailer = &bytes[bytes.len() - super::TRAILER_SIZE..];
        // 故意多给一个字节（骨架长度 != sections_region_offset）
        let bad_skeleton = &bytes[..=sro];
        let err =
            parse_skeleton(bad_skeleton, trailer, bytes.len() as u64).expect_err("should fail");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrParseFail);
    }

    #[test]
    fn parse_skeleton_rejects_total_length_mismatch() {
        let (bytes, _m, _p) = sample_two_sections();
        let full = parse(&bytes).expect("parse");
        let sro = full.raw_header.sections_offset().expect("sro") as usize;
        let skeleton = &bytes[..sro];
        let trailer = &bytes[bytes.len() - super::TRAILER_SIZE..];
        // 提供错误的 total_len（与 trailer.total_length 不符）
        let err =
            parse_skeleton(skeleton, trailer, bytes.len() as u64 + 100).expect_err("should fail");
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "TrailerLengthMismatch"
        ));
    }
}
