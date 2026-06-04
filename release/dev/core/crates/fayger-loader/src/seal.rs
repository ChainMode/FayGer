//! 封装（Sealing）：对内存对象计算并填入全部摘要与 crc32。
//!
//! [`seal`] 让一个 BuFObject 在再次 `serialize` 后能够通过 [`crate::verify::verify_digest`]。
//! BuF 生产者用它产出合法制品。
//!
//! ## 为什么需要两次 serialize
//!
//! `manifest_digest` 覆盖 `[0, sections_offset)`，而 `sections_offset` 取决于 Manifest 与
//! Section_Index 的 canonical 编码长度——只有 `serialize` 之后才精确已知。且写入 section
//! digest 会改变 Manifest 内容，进而改变其编码长度。流程因此是：
//!
//! 1. 校验对象（与 `serialize` 同样的 `validate` + `check_consistency`）。
//! 2. 对每段计算 SHA-256，写入对应 `SectionDescriptor.digest`。
//! 3. 占位（`manifest_digest`/`crc32` 置零）后 `serialize` 一次（`bytes0`），借此拿到精确布局，
//!    对 `bytes0[0, sections_offset)` 计算 `manifest_digest` 并写回。
//! 4. 再 `serialize` 一次（`bytes1`，此时 `manifest_digest` 已是真值、`crc32` 仍为零），
//!    对 `bytes1[0, total-4)` 计算 `crc32` 并写回。
//!
//! 由于 `manifest_digest` 字段位于 `sections_offset` 之后、`crc32` 字段位于文件末 4 字节，
//! 用户后续真正 `serialize(sealed_obj)` 得到的字节流在 `[0, total-4)` 上与 `bytes1` 完全一致，
//! 在 `[0, sections_offset)` 上与 `bytes0`/`bytes1` 完全一致，故两个摘要对最终字节流都有效。

use crate::digest::{crc32, sha256};
use fayger_buf::manifest::types::Digest;
use fayger_buf::object::BuFObject;
use fayger_buf::{parse, serialize};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 对 `obj` 计算并填入全部 Section 摘要、`manifest_digest` 与 `crc32`。
///
/// 成功后，`serialize(obj)` 产出的字节流可通过 [`crate::verify::verify_digest`]。
///
/// # Errors
///
/// 若 `obj` 未通过 [`fayger_buf::manifest::BuFManifest::validate`] 或
/// [`BuFObject::check_consistency`]，返回与 M0 `serialize` 一致的错误码
/// （[`fayger_error::ErrorCode::LdrMissingRequiredField`]）。内部 `serialize`/`parse`
/// 的异常以 [`fayger_error::ErrorCode::LdrParseFail`] 透出（正常输入不会触发）。
pub fn seal(obj: &mut BuFObject) -> Result<(), FaygerError> {
    // 1. 与 serialize 一致的前置校验，让 seal 的失败语义与 serialize 对齐。
    obj.manifest.validate()?;
    obj.check_consistency()?;

    // 2. 计算每段 payload 的 SHA-256 并写入 descriptor.digest。
    for (descriptor, section) in obj.manifest.sections.iter_mut().zip(obj.sections.iter()) {
        descriptor.digest = Digest {
            algorithm: "sha256".to_owned(),
            value: sha256(&section.payload).to_vec(),
        };
    }

    // 3. 占位后序列化一次，拿到精确布局并计算 manifest_digest。
    obj.raw_trailer.manifest_digest = [0u8; 32];
    obj.raw_trailer.crc32 = 0;
    let bytes0 = serialize(obj)?;
    let layout = parse(&bytes0)?;
    let sections_offset = layout
        .raw_header
        .sections_offset()
        .ok_or_else(|| seal_internal("sections offset overflow during seal"))?;
    let end = usize::try_from(sections_offset)
        .map_err(|_| seal_internal("sections offset exceeds addressable range"))?;
    let region = bytes0
        .get(..end)
        .ok_or_else(|| seal_internal("manifest digest region out of bounds during seal"))?;
    obj.raw_trailer.manifest_digest = sha256(region);

    // 4. 把真值 manifest_digest 落字节后，再序列化算 crc32。
    let bytes1 = serialize(obj)?;
    let crc_end = bytes1
        .len()
        .checked_sub(4)
        .ok_or_else(|| seal_internal("serialized stream too short for crc32 during seal"))?;
    obj.raw_trailer.crc32 = crc32(&bytes1[..crc_end]);

    Ok(())
}

/// seal 内部不应发生的失败（正常输入不会触发）。归类为 `LDR_PARSE_FAIL`。
fn seal_internal(message: &'static str) -> FaygerError {
    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, message)
        .with_context("reason", ContextValue::string("SealInternal"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::seal;
    use crate::verify::verify_digest;
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::{parse, serialize};
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

    #[test]
    fn sealed_object_verifies_after_serialize_and_parse() {
        let mut obj = sample_object();
        seal(&mut obj).expect("seal ok");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        verify_digest(&bytes, &parsed).expect("sealed object must verify");
    }

    #[test]
    fn seal_writes_real_section_digests() {
        let mut obj = sample_object();
        seal(&mut obj).expect("seal ok");
        // 每段 digest 应当不再是占位的全零。
        for d in &obj.manifest.sections {
            assert_eq!(d.digest.algorithm, "sha256");
            assert_ne!(d.digest.value, vec![0u8; 32], "digest should be computed");
        }
    }

    #[test]
    fn seal_writes_nonzero_trailer_fields() {
        let mut obj = sample_object();
        seal(&mut obj).expect("seal ok");
        assert_ne!(obj.raw_trailer.manifest_digest, [0u8; 32]);
        // crc32 极小概率恰为 0，但本样例不为 0。
        assert_ne!(obj.raw_trailer.crc32, 0);
    }

    #[test]
    fn seal_is_idempotent() {
        // seal 两次应得到相同的摘要（确定性）。
        let mut a = sample_object();
        seal(&mut a).expect("seal a");
        let mut b = a.clone();
        seal(&mut b).expect("seal b");
        assert_eq!(a.manifest.sections, b.manifest.sections);
        assert_eq!(a.raw_trailer.manifest_digest, b.raw_trailer.manifest_digest);
        assert_eq!(a.raw_trailer.crc32, b.raw_trailer.crc32);
    }

    #[test]
    fn seal_with_zero_sections_verifies() {
        let mut obj = sample_object();
        obj.manifest.sections.clear();
        obj.sections.clear();
        seal(&mut obj).expect("seal ok");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        verify_digest(&bytes, &parsed).expect("empty verify");
    }

    #[test]
    fn seal_rejects_inconsistent_object() {
        let mut obj = sample_object();
        obj.sections.pop(); // 数量与 manifest.sections 不一致
        let err = seal(&mut obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrMissingRequiredField);
    }

    #[test]
    fn seal_rejects_missing_required_field() {
        let mut obj = sample_object();
        obj.manifest.entry = EntryPoint::new("");
        let err = seal(&mut obj).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrMissingRequiredField);
    }
}
