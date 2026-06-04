//! 加载链编排：[`load`]。
//!
//! 把字节流加载为可信、版本兼容的 [`BuFObject`]，按固定顺序串联三阶段：
//!
//! 1. **Parse + Verify(Structural)**：复用 M0 [`fayger_buf::parse`]。
//! 2. **Verify(Digest)**：[`crate::verify::verify_digest`]。
//! 3. **Negotiate(Version)**：[`crate::version::negotiate_version`]。
//!
//! 阶段顺序即错误优先级：结构错误（`LDR_PARSE_FAIL`）优先于摘要错误
//! （`LDR_DIGEST_MISMATCH`），摘要错误优先于版本错误（`LDR_SCHEMA_UNSUPPORTED` /
//! `LDR_RUNTIME_VERSION_TOO_HIGH`）。`?` 的短路天然保证最先失败阶段的错误被返回。

use crate::verify::verify_digest;
use crate::version::{negotiate_version, LoadPolicy};
use fayger_buf::object::BuFObject;
use fayger_error::FaygerError;
/// 加载链入口：把 `bytes` 加载为版本兼容、完整性已校验的 [`BuFObject`]。
///
/// 成功时返回的对象，其 `manifest.deprecation_notice` 反映 Negotiate(Version) 的结果：
/// 命中弃用 schema 时为 `Some(notice)`，否则为 `None`。
///
/// 加载链不执行任何 Section payload 的内容——它全程只做字节解析与摘要/版本比较。
///
/// # Errors
///
/// 按阶段顺序返回最先失败阶段的错误：
///
/// - [`fayger_error::ErrorCode::LdrParseFail`]：字节流结构不合法（Parse 阶段）。
/// - [`fayger_error::ErrorCode::LdrDigestMismatch`]：完整性校验失败（Verify(Digest) 阶段）。
/// - [`fayger_error::ErrorCode::LdrSchemaUnsupported`] /
///   [`fayger_error::ErrorCode::LdrRuntimeVersionTooHigh`]：版本协商失败（Negotiate 阶段）。
pub fn load(bytes: &[u8], policy: &LoadPolicy) -> Result<BuFObject, FaygerError> {
    // 阶段 1：Parse + Verify(Structural)
    let mut obj = fayger_buf::parse(bytes)?;
    // 阶段 2：Verify(Digest)
    verify_digest(bytes, &obj)?;
    // 阶段 3：Negotiate(Version)
    let notice = negotiate_version(&obj.manifest, policy)?;
    obj.manifest.deprecation_notice = notice;
    Ok(obj)
}

/// 含签名校验的加载链入口（M4）。
///
/// 按固定顺序执行 Parse → Verify(Digest) → **Verify(Signature)** → Negotiate(Version)。
/// 签名验证以 `truststore` 为信任锚，并据其强制签名开关处理无签名 BuF。
///
/// 错误优先级（`?` 短路）：结构（`LDR_PARSE_FAIL`）→ 摘要（`LDR_DIGEST_MISMATCH`）→
/// 签名（`LDR_SIGNATURE_FAIL`）→ 版本（`LDR_SCHEMA_UNSUPPORTED` /
/// `LDR_RUNTIME_VERSION_TOO_HIGH`）。
///
/// M1 的 [`load`] 保持签名无关，不受本入口影响。
///
/// # Errors
/// 任一阶段失败时返回对应阶段的 [`FaygerError`]。
pub fn load_signed(
    bytes: &[u8],
    policy: &LoadPolicy,
    truststore: &crate::truststore::TrustStore,
) -> Result<BuFObject, FaygerError> {
    let mut obj = fayger_buf::parse(bytes)?; // 阶段 1：Parse + Verify(Structural)
    verify_digest(bytes, &obj)?; // 阶段 2：Verify(Digest)
    crate::sig::verify_signature(bytes, &obj, truststore)?; // 阶段 3：Verify(Signature)
    let notice = negotiate_version(&obj.manifest, policy)?; // 阶段 4：Negotiate(Version)
    obj.manifest.deprecation_notice = notice;
    Ok(obj)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{load, load_signed};
    use crate::seal::seal;
    use crate::version::LoadPolicy;
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::serialize;
    use fayger_error::ErrorCode;
    use std::collections::{BTreeMap, BTreeSet};

    fn v(s: &str) -> SemVer {
        SemVer::parse(s).expect("parse")
    }

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

    fn object_with(schema: &str, rt_min: &str) -> BuFObject {
        let manifest = BuFManifest {
            schema_version: v(schema),
            runtime_interface_min: v(rt_min),
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

    fn policy() -> LoadPolicy {
        LoadPolicy::new(
            (v("1.0.0"), v("1.9.9")),
            std::iter::once(v("1.0.0")).collect(),
            v("1.2.0"),
        )
    }

    fn sealed_bytes(schema: &str, rt_min: &str) -> Vec<u8> {
        let mut obj = object_with(schema, rt_min);
        seal(&mut obj).expect("seal");
        serialize(&obj).expect("serialize")
    }

    #[test]
    fn happy_path_loads_without_notice() {
        let bytes = sealed_bytes("1.5.0", "1.0.0");
        let obj = load(&bytes, &policy()).expect("load ok");
        assert_eq!(obj.manifest.deprecation_notice, None);
        assert_eq!(obj.sections.len(), 2);
    }

    #[test]
    fn deprecated_schema_sets_notice() {
        let bytes = sealed_bytes("1.0.0", "1.0.0");
        let obj = load(&bytes, &policy()).expect("load ok");
        assert!(obj.manifest.deprecation_notice.is_some());
    }

    #[test]
    fn byte_level_round_trip_after_load() {
        // 继承 M0 字节级强往返：load 后 serialize 应等于原字节。
        let bytes = sealed_bytes("1.5.0", "1.0.0");
        let obj = load(&bytes, &policy()).expect("load ok");
        let re = serialize(&obj).expect("re-serialize");
        // 注意：load 写入了 deprecation_notice=None（本例无弃用），与原始一致。
        assert_eq!(bytes, re);
    }

    #[test]
    fn structural_error_takes_priority() {
        // 截断到不足 Header+Trailer：parse 阶段先失败。
        let bytes = vec![0u8; 8];
        let err = load(&bytes, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrParseFail);
    }

    #[test]
    fn digest_error_takes_priority_over_version() {
        // 构造一个 schema 超界（版本会失败）的合法封装制品，再篡改一个 payload 字节
        // （摘要会失败）。digest 阶段在 version 阶段之前，应返回 digest 错误。
        let mut bytes = sealed_bytes("2.0.0", "1.0.0"); // schema 2.0.0 超界
                                                        // 篡改最后一个 section payload 字节区域。两段 payload 紧邻在 Section_Index 之后。
                                                        // 简单起见翻转 crc32 之前、trailer 之前的一个字节即落在 sections 或 index 区。
        let n = bytes.len();
        // trailer 占末 44 字节；其前一个字节属于 sections 段（本样例最后一段 payload）。
        bytes[n - 45] ^= 0xff;
        let err = load(&bytes, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);
    }

    #[test]
    fn version_error_when_digest_ok() {
        // 合法封装但 schema 超界：digest 通过，version 失败。
        let bytes = sealed_bytes("2.0.0", "1.0.0");
        let err = load(&bytes, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrSchemaUnsupported);
    }

    #[test]
    fn runtime_too_high_is_reported() {
        let bytes = sealed_bytes("1.5.0", "1.5.0"); // provided 1.2.0
        let err = load(&bytes, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrRuntimeVersionTooHigh);
    }

    // ----- M4: load_signed 集成 -----

    fn signed_bytes(schema: &str, rt_min: &str, seed: &[u8; 32], key_ref: &str) -> Vec<u8> {
        let mut obj = object_with(schema, rt_min);
        crate::sign::seal_and_sign(&mut obj, seed, key_ref).expect("seal_and_sign");
        serialize(&obj).expect("serialize")
    }

    #[test]
    fn load_signed_happy_path() {
        let seed = [9u8; 32];
        let bytes = signed_bytes("1.5.0", "1.0.0", &seed, "k1");
        let pk = crate::sign::ed25519_public_key(&seed);
        let ts = crate::truststore::TrustStore::new(
            vec![crate::truststore::TrustedRoot::new("k1", pk.to_vec())],
            true, // 强制模式也应通过（有有效签名）
        );
        let obj = load_signed(&bytes, &policy(), &ts).expect("load_signed ok");
        assert_eq!(obj.sections.len(), 2);
    }

    #[test]
    fn load_signed_unknown_key_fails() {
        let seed = [9u8; 32];
        let bytes = signed_bytes("1.5.0", "1.0.0", &seed, "k1");
        let ts = crate::truststore::TrustStore::new(vec![], false);
        let err = load_signed(&bytes, &policy(), &ts).expect_err("unknown key");
        assert_eq!(err.code, ErrorCode::LdrSignatureFail);
    }

    #[test]
    fn load_signed_signature_priority_over_version() {
        // schema 超界（版本会失败）+ 未知 key（签名会失败）：签名阶段在版本之前。
        let seed = [9u8; 32];
        let bytes = signed_bytes("2.0.0", "1.0.0", &seed, "k1");
        let ts = crate::truststore::TrustStore::new(vec![], false);
        let err = load_signed(&bytes, &policy(), &ts).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrSignatureFail);
    }

    #[test]
    fn load_signed_enforce_rejects_unsigned() {
        // 未签名（仅 seal）+ 强制模式 → MissingSignature。
        let bytes = sealed_bytes("1.5.0", "1.0.0");
        let ts = crate::truststore::TrustStore::new(vec![], true);
        let err = load_signed(&bytes, &policy(), &ts).expect_err("missing sig");
        assert_eq!(err.code, ErrorCode::LdrSignatureFail);
    }

    #[test]
    fn m1_load_remains_signature_agnostic() {
        // M1 load 对已签名制品仍按签名无关方式加载成功（不校验签名）。
        let seed = [9u8; 32];
        let bytes = signed_bytes("1.5.0", "1.0.0", &seed, "k1");
        let obj = load(&bytes, &policy()).expect("M1 load ignores signature");
        assert!(obj.manifest.signature.is_some());
    }
}
