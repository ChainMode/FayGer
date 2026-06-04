//! 生产侧：ed25519 签名基元与"封装+签名"操作 [`seal_and_sign`]。
//!
//! [`seal_and_sign`] 把 M1 的 seal（section 摘要 + manifest_digest + crc32）与 M4 的签名
//! 合成一个正确顺序的流程，使最终制品**同时**通过 Verify(Digest) 与 Verify(Signature)。

use crate::digest::{crc32, sha256};
use crate::sig::{signed_scope, SIG_ALG_ED25519};
use ed25519_dalek::{Signer, SigningKey};
use fayger_buf::manifest::types::{Digest, SignatureBlock};
use fayger_buf::object::BuFObject;
use fayger_buf::{parse, serialize};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 从 ed25519 私钥种子（32 字节）派生公钥（32 字节）。
#[must_use]
pub fn ed25519_public_key(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// 用 ed25519 私钥种子对消息签名，返回 64 字节签名（确定性，无需 CSPRNG）。
#[must_use]
pub fn ed25519_sign(seed: &[u8; 32], message: &[u8]) -> [u8; 64] {
    SigningKey::from_bytes(seed).sign(message).to_bytes()
}

/// 对 `obj` 封装并签名：使其再序列化后同时通过 Verify(Digest) 与 Verify(Signature)。
///
/// 流程（顺序敏感）：
/// 1. 校验 + 一致性。
/// 2. 计算每段 sha256 → `SectionDescriptor.digest`。
/// 3. 插入占位签名（signed_digest = `[0; 64]`），trailer 置零。
/// 4. 序列化取签名作用域 → ed25519 签名 → 同长度替换 signed_digest。
/// 5. 重算 manifest_digest 写回。
/// 6. 重算 crc32 写回。
///
/// # Errors
/// 校验/一致性失败时返回与 M1 序列化一致的错误码；内部序列化异常以 `LDR_PARSE_FAIL` 透出。
pub fn seal_and_sign(
    obj: &mut BuFObject,
    seed: &[u8; 32],
    key_ref: &str,
) -> Result<(), FaygerError> {
    // 1. 前置校验（与 serialize 一致）。
    obj.manifest.validate()?;
    obj.check_consistency()?;

    // 2. section 摘要。
    for (descriptor, section) in obj.manifest.sections.iter_mut().zip(obj.sections.iter()) {
        descriptor.digest = Digest {
            algorithm: "sha256".to_owned(),
            value: sha256(&section.payload).to_vec(),
        };
    }

    // 3. 占位签名 + trailer 清零。
    obj.manifest.signature = Some(SignatureBlock {
        algorithm: SIG_ALG_ED25519.to_owned(),
        public_key_ref: key_ref.to_owned(),
        signed_digest: vec![0u8; 64],
    });
    obj.raw_trailer.manifest_digest = [0u8; 32];
    obj.raw_trailer.crc32 = 0;

    // 4. 序列化占位版 → 计算签名作用域 → 签名 → 替换。
    let bytes_placeholder = serialize(obj)?;
    let parsed_placeholder = parse(&bytes_placeholder)?;
    let scope = signed_scope(&bytes_placeholder, &parsed_placeholder)?;
    let signature = ed25519_sign(seed, &scope);
    if let Some(block) = obj.manifest.signature.as_mut() {
        block.signed_digest = signature.to_vec();
    }

    // 5. 真实签名版 → 重算 manifest_digest。
    let bytes_signed = serialize(obj)?;
    let layout = parse(&bytes_signed)?;
    let sections_offset = layout
        .raw_header
        .sections_offset()
        .ok_or_else(|| sign_internal("sections offset overflow during sign"))?;
    let end = usize::try_from(sections_offset)
        .map_err(|_| sign_internal("sections offset exceeds addressable range"))?;
    let region = bytes_signed
        .get(..end)
        .ok_or_else(|| sign_internal("manifest digest region out of bounds during sign"))?;
    obj.raw_trailer.manifest_digest = sha256(region);

    // 6. 写入 manifest_digest 后 → 重算 crc32。
    let bytes_final = serialize(obj)?;
    let crc_end = bytes_final
        .len()
        .checked_sub(4)
        .ok_or_else(|| sign_internal("serialized stream too short for crc32 during sign"))?;
    obj.raw_trailer.crc32 = crc32(&bytes_final[..crc_end]);

    Ok(())
}

/// sign 内部不应发生的失败（正常输入不触发）。归类为 `LDR_PARSE_FAIL`。
fn sign_internal(message: &'static str) -> FaygerError {
    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, message)
        .with_context("reason", ContextValue::string("SignInternal"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{ed25519_public_key, ed25519_sign, seal_and_sign};
    use crate::sig::{ed25519_verify, verify_signature, VerifyOutcome};
    use crate::truststore::{TrustStore, TrustedRoot};
    use crate::verify::verify_digest;
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::{parse, serialize};
    use std::collections::{BTreeMap, BTreeSet};

    const SEED: [u8; 32] = [3u8; 32];

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

    #[test]
    fn sign_verify_round_trip_primitive() {
        let msg = b"abc";
        let sig = ed25519_sign(&SEED, msg);
        let pk = ed25519_public_key(&SEED);
        assert!(ed25519_verify(&pk, msg, &sig).expect("ok"));
    }

    #[test]
    fn public_key_derivation_is_stable() {
        assert_eq!(ed25519_public_key(&SEED), ed25519_public_key(&SEED));
    }

    #[test]
    fn seal_and_sign_sets_signature_block() {
        let mut obj = sample_object();
        seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
        let block = obj.manifest.signature.as_ref().expect("signature present");
        assert_eq!(block.algorithm, "ed25519");
        assert_eq!(block.public_key_ref, "k1");
        assert_eq!(block.signed_digest.len(), 64);
    }

    #[test]
    fn seal_and_sign_passes_digest_and_signature() {
        let mut obj = sample_object();
        seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let parsed = parse(&bytes).expect("parse");

        // digest 通过
        verify_digest(&bytes, &parsed).expect("digest ok");

        // 签名通过（含对应公钥的 TrustStore）
        let pk = ed25519_public_key(&SEED);
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], false);
        assert_eq!(
            verify_signature(&bytes, &parsed, &ts).expect("sig ok"),
            VerifyOutcome::Valid
        );
    }
}
