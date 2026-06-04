//! Verify(Signature) 阶段：ed25519 验证基元、签名作用域计算与签名验证决策。
//!
//! 签名作用域为 `Header || canonical(Manifest_without_signature) || Section_Index`，
//! 与顶层 spec design.md §"签名作用范围"一致——不含任何 Section 段体，也不含 Trailer。
//!
//! 决策见 [`verify_signature`]：区分 `Absent` / `Valid` 与四类失败原因
//! （`MissingSignature` / `UnsupportedAlgorithm` / `UnknownKey` / `InvalidSignature`）。

use crate::truststore::TrustStore;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use fayger_buf::canonical::canonical_encode;
use fayger_buf::format::HEADER_SIZE;
use fayger_buf::object::BuFObject;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// ed25519 公钥长度（字节）。
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;
/// ed25519 签名长度（字节）。
pub const ED25519_SIGNATURE_LEN: usize = 64;
/// M4 支持的唯一签名算法标识。
pub const SIG_ALG_ED25519: &str = "ed25519";

/// 签名验证结果（成功路径）。失败走 `Err(LDR_SIGNATURE_FAIL)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// 有签名且验证通过。
    Valid,
    /// 无签名（且非强制模式）。
    Absent,
}

/// 用 ed25519 验证签名。
///
/// 返回 `Ok(true)` 验证通过、`Ok(false)` 验证不通过、`Err` 表示公钥/签名长度非法等结构错误。
///
/// # Errors
/// 公钥非 32 字节、签名非 64 字节、或公钥不是合法 ed25519 点时返回 `LDR_SIGNATURE_FAIL`。
pub fn ed25519_verify(
    public_key: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<bool, FaygerError> {
    let pk_bytes: [u8; ED25519_PUBLIC_KEY_LEN] = public_key.try_into().map_err(|_| {
        sig_fail("InvalidSignature")
            .with_context("detail", ContextValue::string("public key length != 32"))
    })?;
    let sig_bytes: [u8; ED25519_SIGNATURE_LEN] = signature.try_into().map_err(|_| {
        sig_fail("InvalidSignature")
            .with_context("detail", ContextValue::string("signature length != 64"))
    })?;
    let verifying_key = VerifyingKey::from_bytes(&pk_bytes).map_err(|e| {
        sig_fail("InvalidSignature").with_context("detail", ContextValue::string(format!("{e}")))
    })?;
    let sig = Signature::from_bytes(&sig_bytes);
    Ok(verifying_key.verify(message, &sig).is_ok())
}

/// 计算签名作用域：`Header_normalized || canonical(Manifest_without_signature) || Section_Index`。
///
/// **Header 归一化**：直接切文件头会引入"签名大小依赖"——`signed_digest: Vec<u8>` 在 CBOR 中
/// 编码为整数数组（非定长字节串），占位签名（全零小整数）与真实签名（含 >23 的值）编码长度不同，
/// 导致 `manifest_length` 字段在占位/真实两版字节流中不一致。为使作用域在 sign 与 verify 两侧
/// 字节级一致，本函数用 `manifest_without_sig` 的长度**重建**一个归一化 Header（magic +
/// format_version + 由去签名 manifest 长度推导的 offset/length），它与 `signature` 内容无关。
///
/// Manifest 部分用清空 signature 后的 canonical 重编码；Section_Index 取自原始字节切片。
///
/// # Errors
/// 区间越界或长度溢出时返回 `LDR_SIGNATURE_FAIL`。
pub(crate) fn signed_scope(bytes: &[u8], obj: &BuFObject) -> Result<Vec<u8>, FaygerError> {
    // Manifest_without_signature：clone manifest、清空 signature、canonical 编码。
    let mut manifest_no_sig = obj.manifest.clone();
    manifest_no_sig.signature = None;
    let manifest_bytes = canonical_encode(&manifest_no_sig)?;

    // Section_Index：用 header 中声明的 offset/length 从原始字节切出。
    let sidx_off = usize::try_from(obj.raw_header.section_index_offset).map_err(|_| {
        sig_fail("InvalidSignature").with_context(
            "detail",
            ContextValue::string("section_index_offset overflow"),
        )
    })?;
    let sidx_len = usize::try_from(obj.raw_header.section_index_length).map_err(|_| {
        sig_fail("InvalidSignature").with_context(
            "detail",
            ContextValue::string("section_index_length overflow"),
        )
    })?;
    let sidx_end = sidx_off.checked_add(sidx_len).ok_or_else(|| {
        sig_fail("InvalidSignature").with_context(
            "detail",
            ContextValue::string("section index range overflow"),
        )
    })?;
    let sidx = bytes.get(sidx_off..sidx_end).ok_or_else(|| {
        sig_fail("InvalidSignature").with_context(
            "detail",
            ContextValue::string("section index out of bounds"),
        )
    })?;

    // 归一化 Header：由去签名 manifest 长度推导，签名无关。
    let manifest_len_u64 = manifest_bytes.len() as u64;
    let header_size_u64 = HEADER_SIZE as u64;
    let sidx_len_u64 = sidx.len() as u64;
    let mut scope = Vec::with_capacity(HEADER_SIZE + manifest_bytes.len() + sidx.len());
    scope.extend_from_slice(b"BUF\0");
    scope.extend_from_slice(&obj.raw_header.format_version.to_le_bytes());
    scope.extend_from_slice(&header_size_u64.to_le_bytes()); // manifest_offset = HEADER_SIZE
    scope.extend_from_slice(&manifest_len_u64.to_le_bytes()); // manifest_length（去签名）
    scope.extend_from_slice(&(header_size_u64 + manifest_len_u64).to_le_bytes()); // section_index_offset
    scope.extend_from_slice(&sidx_len_u64.to_le_bytes()); // section_index_length

    scope.extend_from_slice(&manifest_bytes);
    scope.extend_from_slice(sidx);
    Ok(scope)
}

/// Verify(Signature) 阶段。
///
/// # Errors
/// 见决策表：`MissingSignature` / `UnsupportedAlgorithm` / `UnknownKey` / `InvalidSignature`
/// 均以 `LDR_SIGNATURE_FAIL` 返回，由 `context.reason` 区分。
pub fn verify_signature(
    bytes: &[u8],
    obj: &BuFObject,
    truststore: &TrustStore,
) -> Result<VerifyOutcome, FaygerError> {
    let Some(block) = obj.manifest.signature.as_ref() else {
        // 无签名。
        return if truststore.enforce_signature() {
            Err(sig_fail("MissingSignature"))
        } else {
            Ok(VerifyOutcome::Absent)
        };
    };

    // 算法白名单（先于命中检查）。
    if block.algorithm != SIG_ALG_ED25519 {
        return Err(sig_fail("UnsupportedAlgorithm")
            .with_context("algorithm", ContextValue::string(block.algorithm.clone())));
    }

    // 命中可信根。
    let Some(root) = truststore.find(&block.public_key_ref) else {
        return Err(sig_fail("UnknownKey").with_context(
            "public_key_ref",
            ContextValue::string(block.public_key_ref.clone()),
        ));
    };

    // 验证签名作用域。
    let scope = signed_scope(bytes, obj)?;
    match ed25519_verify(&root.public_key, &scope, &block.signed_digest) {
        Ok(true) => Ok(VerifyOutcome::Valid),
        Ok(false) => Err(sig_fail("InvalidSignature")),
        Err(e) => Err(e),
    }
}

/// 构造 `LDR_SIGNATURE_FAIL` 错误并附 reason。
fn sig_fail(reason: &'static str) -> FaygerError {
    FaygerError::new(
        ErrorCode::LdrSignatureFail,
        Layer::Loader,
        "signature verification failed",
    )
    .with_context("reason", ContextValue::string(reason))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone
)]
mod tests {
    use super::{ed25519_verify, signed_scope, verify_signature, VerifyOutcome, SIG_ALG_ED25519};
    use crate::sign::{ed25519_public_key, ed25519_sign, seal_and_sign};
    use crate::truststore::{TrustStore, TrustedRoot};
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility, SignatureBlock},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::{parse, serialize};
    use fayger_error::{ContextValue, ErrorCode};
    use std::collections::{BTreeMap, BTreeSet};

    const SEED: [u8; 32] = [7u8; 32];

    fn sample_object() -> BuFObject {
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
        BuFObject {
            manifest,
            sections: vec![Section::new("code", vec![1, 2, 3, 4])],
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

    fn reason_of(err: &fayger_error::FaygerError) -> String {
        match err.context.get("reason") {
            Some(ContextValue::String(s)) => s.clone(),
            other => panic!("expected reason, got {other:?}"),
        }
    }

    fn store_with_seed(enforce: bool) -> TrustStore {
        let pk = ed25519_public_key(&SEED);
        TrustStore::new(
            vec![TrustedRoot {
                key_ref: "k1".into(),
                public_key: pk.to_vec(),
            }],
            enforce,
        )
    }

    #[test]
    fn ed25519_verify_accepts_valid_triple() {
        let msg = b"hello fayger";
        let sig = ed25519_sign(&SEED, msg);
        let pk = ed25519_public_key(&SEED);
        assert!(ed25519_verify(&pk, msg, &sig).expect("ok"));
    }

    #[test]
    fn ed25519_verify_rejects_tampered_message() {
        let sig = ed25519_sign(&SEED, b"hello");
        let pk = ed25519_public_key(&SEED);
        assert!(!ed25519_verify(&pk, b"hellp", &sig).expect("ok"));
    }

    #[test]
    fn ed25519_verify_rejects_bad_lengths() {
        assert!(ed25519_verify(&[0u8; 10], b"x", &[0u8; 64]).is_err());
        assert!(ed25519_verify(&[0u8; 32], b"x", &[0u8; 10]).is_err());
    }

    #[test]
    fn signed_scope_is_deterministic_and_sig_independent() {
        let mut obj = sample_object();
        seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let s1 = signed_scope(&bytes, &parsed).expect("scope1");
        let s2 = signed_scope(&bytes, &parsed).expect("scope2");
        assert_eq!(s1, s2);
        // 改 signed_digest 内容不改变 scope（scope 用 manifest_without_sig）。
        let mut obj2 = parsed.clone();
        if let Some(sig) = obj2.manifest.signature.as_mut() {
            sig.signed_digest = vec![9u8; 64];
        }
        let s3 = signed_scope(&bytes, &obj2).expect("scope3");
        assert_eq!(s1, s3);
    }

    #[test]
    fn decision_absent_non_enforce() {
        let mut obj = sample_object();
        // 仅 seal（无签名）。用 M1 seal 让 digest 就位。
        crate::seal(&mut obj).expect("seal");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let r = verify_signature(&bytes, &parsed, &store_with_seed(false)).expect("absent ok");
        assert_eq!(r, VerifyOutcome::Absent);
    }

    #[test]
    fn decision_missing_signature_enforce() {
        let mut obj = sample_object();
        crate::seal(&mut obj).expect("seal");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let err = verify_signature(&bytes, &parsed, &store_with_seed(true)).expect_err("missing");
        assert_eq!(err.code, ErrorCode::LdrSignatureFail);
        assert_eq!(reason_of(&err), "MissingSignature");
    }

    #[test]
    fn decision_valid() {
        let mut obj = sample_object();
        seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let r = verify_signature(&bytes, &parsed, &store_with_seed(false)).expect("valid");
        assert_eq!(r, VerifyOutcome::Valid);
    }

    #[test]
    fn scope_at_sign_equals_scope_at_verify() {
        // 复刻 seal_and_sign 的占位阶段，比较占位作用域与最终作用域是否字节相等。
        let mut obj = sample_object();
        // section 摘要
        for (d, s) in obj.manifest.sections.iter_mut().zip(obj.sections.iter()) {
            d.digest = Digest {
                algorithm: "sha256".into(),
                value: crate::digest::sha256(&s.payload).to_vec(),
            };
        }
        // 占位签名
        obj.manifest.signature = Some(SignatureBlock {
            algorithm: "ed25519".into(),
            public_key_ref: "k1".into(),
            signed_digest: vec![0u8; 64],
        });
        let bytes_p = serialize(&obj).expect("serialize placeholder");
        let parsed_p = parse(&bytes_p).expect("parse placeholder");
        let scope_p = signed_scope(&bytes_p, &parsed_p).expect("scope p");

        // 完整 seal_and_sign
        let mut obj2 = sample_object();
        seal_and_sign(&mut obj2, &SEED, "k1").expect("sign");
        let bytes_f = serialize(&obj2).expect("serialize final");
        let parsed_f = parse(&bytes_f).expect("parse final");
        let scope_f = signed_scope(&bytes_f, &parsed_f).expect("scope f");

        assert_eq!(
            scope_p, scope_f,
            "sign-time scope must equal verify-time scope"
        );
    }

    #[test]
    fn decision_unknown_key() {
        let mut obj = sample_object();
        seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        // TrustStore 不含 k1。
        let empty = TrustStore::new(vec![], false);
        let err = verify_signature(&bytes, &parsed, &empty).expect_err("unknown");
        assert_eq!(reason_of(&err), "UnknownKey");
    }

    #[test]
    fn decision_invalid_signature() {
        let mut obj = sample_object();
        seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
        // 篡改 signed_digest。
        if let Some(sig) = obj.manifest.signature.as_mut() {
            sig.signed_digest[0] ^= 0xff;
        }
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let err = verify_signature(&bytes, &parsed, &store_with_seed(false)).expect_err("invalid");
        assert_eq!(reason_of(&err), "InvalidSignature");
    }

    #[test]
    fn decision_unsupported_algorithm() {
        let mut obj = sample_object();
        crate::seal(&mut obj).expect("seal");
        obj.manifest.signature = Some(SignatureBlock {
            algorithm: "rsa-pss".into(),
            public_key_ref: "k1".into(),
            signed_digest: vec![0u8; 64],
        });
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");
        let err =
            verify_signature(&bytes, &parsed, &store_with_seed(false)).expect_err("unsupported");
        assert_eq!(reason_of(&err), "UnsupportedAlgorithm");
        let _ = SIG_ALG_ED25519;
    }
}
