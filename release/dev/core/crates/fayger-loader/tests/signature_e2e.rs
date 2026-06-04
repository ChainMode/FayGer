//! M4 端到端：密钥对 → seal_and_sign → load_signed（含/不含公钥）。
//!
//! Validates: Requirements 8.2

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_error::ErrorCode;
use fayger_loader::{
    ed25519_public_key, load_signed, seal, seal_and_sign, LoadPolicy, TrustStore, TrustedRoot,
};
use std::collections::{BTreeMap, BTreeSet};

fn v(s: &str) -> SemVer {
    SemVer::parse(s).expect("parse")
}

fn descriptor(id: &str, kind: SectionKind, length: u64) -> SectionDescriptor {
    SectionDescriptor {
        id: id.into(),
        kind,
        digest: Digest::placeholder(),
        length,
        visibility: SectionVisibility::Required,
        profile_constraints: ProfileConstraints::default(),
    }
}

fn two_section_object() -> BuFObject {
    let manifest = BuFManifest {
        schema_version: v("1.0.0"),
        runtime_interface_min: v("1.0.0"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![
            descriptor("code", SectionKind::Code, 4),
            descriptor("asset", SectionKind::Asset, 3),
        ],
        signature: None,
        extensions: BTreeMap::new(),
    };
    BuFObject {
        manifest,
        sections: vec![
            Section::new("code", vec![0xde, 0xad, 0xbe, 0xef]),
            Section::new("asset", vec![0x01, 0x02, 0x03]),
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
    LoadPolicy::new((v("1.0.0"), v("1.9.9")), BTreeSet::new(), v("1.0.0"))
}

#[test]
fn signed_buf_loads_with_matching_root_and_fails_without() {
    let seed = [42u8; 32];
    let public_key = ed25519_public_key(&seed);

    // 生产侧：封装 + 签名。
    let mut obj = two_section_object();
    seal_and_sign(&mut obj, &seed, "release-key-1").expect("seal_and_sign");
    let bytes = serialize(&obj).expect("serialize");

    // 含对应公钥的 TrustStore（强制模式）→ 成功。
    let trusting = TrustStore::new(
        vec![TrustedRoot::new("release-key-1", public_key.to_vec())],
        true,
    );
    let loaded = load_signed(&bytes, &policy(), &trusting).expect("trusted load ok");
    assert_eq!(loaded.sections.len(), 2);
    assert_eq!(loaded.sections[0].payload, vec![0xde, 0xad, 0xbe, 0xef]);

    // 不含该公钥的 TrustStore → UnknownKey。
    let untrusting = TrustStore::new(vec![], false);
    let err = load_signed(&bytes, &policy(), &untrusting).expect_err("unknown key");
    assert_eq!(err.code, ErrorCode::LdrSignatureFail);

    // 含错误 key_ref 的根 → 仍 UnknownKey（key_ref 不匹配）。
    let wrong_ref = TrustStore::new(
        vec![TrustedRoot::new("other-key", public_key.to_vec())],
        false,
    );
    assert!(load_signed(&bytes, &policy(), &wrong_ref).is_err());
}

#[test]
fn enforce_mode_rejects_unsigned_buf() {
    // 仅封装（不签名）。
    let mut obj = two_section_object();
    seal(&mut obj).expect("seal");
    let bytes = serialize(&obj).expect("serialize");

    // 强制模式 → MissingSignature。
    let enforcing = TrustStore::new(vec![], true);
    let err = load_signed(&bytes, &policy(), &enforcing).expect_err("missing signature");
    assert_eq!(err.code, ErrorCode::LdrSignatureFail);

    // 非强制模式 → 通过（Absent）。
    let lenient = TrustStore::new(vec![], false);
    assert!(load_signed(&bytes, &policy(), &lenient).is_ok());
}
