//! Feature: fayger-m6, Property 4: signature consistency under partial loading.
//!
//! 同一已签名 BuF 用两个不同 profile（可能选中不同子集）部分加载，签名校验结果相同（均通过）；
//! 篡改被跳过段不改签名结果；篡改 Section_Index 改签名结果。
//!
//! Validates: Requirements 5.2, 5.3, 5.4, 5.5, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation,
    clippy::redundant_clone
)]

use fayger_buf::format::{HEADER_SIZE, TRAILER_SIZE};
use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_loader::{
    ed25519_public_key, load_with_profile_signed, seal_and_sign, LoadProfile, LoadStrategy,
    MemorySource, TrustStore, TrustedRoot,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

const SEED: [u8; 32] = [11u8; 32];

/// 构造并签名一个 BuF：code(Required,无约束) + gpu(Optional,features={gpu}) + audio(Optional,features={audio})。
fn signed_three() -> Vec<u8> {
    fn sd(id: &str, len: u64, vis: SectionVisibility, feat: Option<&str>) -> SectionDescriptor {
        SectionDescriptor {
            id: id.into(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: len,
            visibility: vis,
            profile_constraints: ProfileConstraints {
                required_capabilities: BTreeSet::new(),
                max_size: None,
                required_features: feat.into_iter().map(str::to_string).collect(),
            },
        }
    }
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").unwrap(),
        runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![
            sd("code", 4, SectionVisibility::Required, None),
            sd("gpu", 3, SectionVisibility::Optional, Some("gpu")),
            sd("audio", 2, SectionVisibility::Optional, Some("audio")),
        ],
        signature: None,
        extensions: BTreeMap::new(),
    };
    let mut obj = BuFObject {
        manifest,
        sections: vec![
            Section::new("code", vec![1, 2, 3, 4]),
            Section::new("gpu", vec![7, 8, 9]),
            Section::new("audio", vec![5, 6]),
        ],
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    };
    seal_and_sign(&mut obj, &SEED, "k1").expect("sign");
    serialize(&obj).expect("serialize")
}

fn profile(features: &[&str]) -> LoadProfile {
    LoadProfile::new(
        "t",
        BTreeSet::new(),
        None,
        features.iter().map(|s| (*s).to_string()).collect(),
    )
}

fn trusting() -> TrustStore {
    let pk = ed25519_public_key(&SEED);
    TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], true)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 任意两组特征（选中不同子集）下，签名校验都通过。
    #[test]
    fn different_profiles_agree_on_valid_signature(
        f1 in prop::collection::vec(prop_oneof![Just("gpu"), Just("audio")], 0..=2),
        f2 in prop::collection::vec(prop_oneof![Just("gpu"), Just("audio")], 0..=2),
        strategy in prop_oneof![Just(LoadStrategy::Eager), Just(LoadStrategy::Lazy)],
    ) {
        let bytes = signed_three();
        let p1 = profile(&f1);
        let p2 = profile(&f2);

        let r1 = load_with_profile_signed(
            Box::new(MemorySource::new(bytes.clone())), strategy, &p1, &trusting());
        let r2 = load_with_profile_signed(
            Box::new(MemorySource::new(bytes.clone())), strategy, &p2, &trusting());

        // 两个 profile 都应加载成功（签名通过；code Required 无约束恒选中）。
        prop_assert!(r1.is_ok(), "p1 load failed: {:?}", r1.err());
        prop_assert!(r2.is_ok(), "p2 load failed: {:?}", r2.err());
    }
}

#[test]
fn tampering_skipped_section_does_not_break_signature() {
    let mut bytes = signed_three();
    // 找到 gpu 段（Sections 区第二段）的字节位置并篡改。
    let parsed = fayger_buf::parse(&bytes).expect("parse");
    let sro = parsed.raw_header.sections_offset().expect("sro") as usize;
    // code(4) 之后是 gpu(3)：gpu 首字节在 sro+4。
    let gpu_first = sro + 4;
    assert!(gpu_first < bytes.len() - TRAILER_SIZE);
    bytes[gpu_first] ^= 0xff;

    // 用一个不含 gpu 特征的 profile（gpu 被跳过）：签名仍应通过（被跳过段不在签名作用域）。
    let art = load_with_profile_signed(
        Box::new(MemorySource::new(bytes)),
        LoadStrategy::Eager,
        &profile(&[]), // 跳过 gpu 与 audio
        &trusting(),
    );
    assert!(
        art.is_ok(),
        "tampering skipped section must not break signature: {:?}",
        art.err()
    );
}

#[test]
fn tampering_section_index_breaks_signature() {
    let mut bytes = signed_three();
    let parsed = fayger_buf::parse(&bytes).expect("parse");
    // Section_Index 区间 [section_index_offset, section_index_offset + section_index_length)。
    let idx_off = parsed.raw_header.section_index_offset as usize;
    let idx_len = parsed.raw_header.section_index_length as usize;
    assert!(idx_len > 0);
    // 翻转 Section_Index 中部一个字节（避开可能导致 CBOR 结构破坏从而 parse 失败的极端情形，
    // 但即便 parse 失败也满足"签名校验结果改变"——这里断言 load 不再成功）。
    let pos = idx_off + idx_len / 2;
    let _ = HEADER_SIZE; // 区间常量来源
    bytes[pos] ^= 0xff;

    let art = load_with_profile_signed(
        Box::new(MemorySource::new(bytes)),
        LoadStrategy::Eager,
        &profile(&["gpu", "audio"]),
        &trusting(),
    );
    // Section_Index 在签名作用域内 + manifest_digest 覆盖范围内：必然加载失败。
    assert!(art.is_err(), "tampering section index must break load");
}
