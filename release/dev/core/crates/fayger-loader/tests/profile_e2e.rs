//! M6 端到端：含 Required + Optional + 带约束 Section 的 BuF，用满足/不满足的 profile 加载。
//!
//! Validates: Requirements 8.2

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_error::ErrorCode;
use fayger_loader::{
    ed25519_public_key, load_with_profile, load_with_profile_signed, seal, seal_and_sign,
    LoadProfile, LoadStrategy, MemorySource, SkipReason, TrustStore, TrustedRoot,
};
use std::collections::{BTreeMap, BTreeSet};

fn caps(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn sd(
    id: &str,
    len: u64,
    vis: SectionVisibility,
    constraints: ProfileConstraints,
) -> SectionDescriptor {
    SectionDescriptor {
        id: id.into(),
        kind: SectionKind::Code,
        digest: Digest::placeholder(),
        length: len,
        visibility: vis,
        profile_constraints: constraints,
    }
}

/// code(Required,无约束) + gpu(Optional, features={gpu}) + huge(Optional, max_size=10, length=20)。
fn build_obj() -> BuFObject {
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").unwrap(),
        runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![
            sd(
                "code",
                4,
                SectionVisibility::Required,
                ProfileConstraints::default(),
            ),
            sd(
                "gpu",
                3,
                SectionVisibility::Optional,
                ProfileConstraints {
                    required_capabilities: BTreeSet::new(),
                    max_size: None,
                    required_features: caps(&["gpu"]),
                },
            ),
            sd(
                "huge",
                20,
                SectionVisibility::Optional,
                ProfileConstraints {
                    required_capabilities: BTreeSet::new(),
                    max_size: Some(10),
                    required_features: BTreeSet::new(),
                },
            ),
        ],
        signature: None,
        extensions: BTreeMap::new(),
    };
    BuFObject {
        manifest,
        sections: vec![
            Section::new("code", vec![1, 2, 3, 4]),
            Section::new("gpu", vec![7, 8, 9]),
            Section::new("huge", vec![0u8; 20]),
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

fn sealed() -> Vec<u8> {
    let mut obj = build_obj();
    seal(&mut obj).expect("seal");
    serialize(&obj).expect("serialize")
}

#[test]
fn profile_a_selects_code_and_gpu_skips_huge() {
    // features={gpu}, 无 max → 选中 code+gpu，跳过 huge（OverMaxSize，length 20 > max_size 10）。
    let profile = LoadProfile::new("desktop", BTreeSet::new(), None, caps(&["gpu"]));
    let art = load_with_profile(
        Box::new(MemorySource::new(sealed())),
        LoadStrategy::Eager,
        &profile,
    )
    .expect("load");
    assert_eq!(art.selected_ids(), &["code".to_string(), "gpu".to_string()]);
    assert_eq!(art.skipped().len(), 1);
    assert_eq!(art.skipped()[0].section_id, "huge");
    assert!(art.skipped()[0].reasons.contains(&SkipReason::OverMaxSize));
}

#[test]
fn profile_b_selects_code_skips_gpu_and_huge() {
    // features={} → 选中 code，跳过 gpu(FeatureMissing)+huge(OverMaxSize)。
    let profile = LoadProfile::new("camera", BTreeSet::new(), None, BTreeSet::new());
    let mut art = load_with_profile(
        Box::new(MemorySource::new(sealed())),
        LoadStrategy::Lazy,
        &profile,
    )
    .expect("load");
    assert_eq!(art.selected_ids(), &["code".to_string()]);
    let skipped: BTreeSet<String> = art.skipped().iter().map(|r| r.section_id.clone()).collect();
    assert_eq!(skipped, caps(&["gpu", "huge"]));
    // 被选中段可读
    assert_eq!(art.section("code").unwrap(), &[1, 2, 3, 4]);
    // 被跳过段不可读
    assert_eq!(
        art.section("gpu").unwrap_err().code,
        ErrorCode::LdrParseFail
    );
}

#[test]
fn required_constraint_unmet_fails() {
    // 构造 core(Required, features={secure})；profile 不含 secure → 加载失败。
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").unwrap(),
        runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![sd(
            "core",
            4,
            SectionVisibility::Required,
            ProfileConstraints {
                required_capabilities: BTreeSet::new(),
                max_size: None,
                required_features: caps(&["secure"]),
            },
        )],
        signature: None,
        extensions: BTreeMap::new(),
    };
    let mut obj = BuFObject {
        manifest,
        sections: vec![Section::new("core", vec![1, 2, 3, 4])],
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    };
    seal(&mut obj).expect("seal");
    let bytes = serialize(&obj).expect("serialize");
    let profile = LoadProfile::new("t", BTreeSet::new(), None, BTreeSet::new());
    let err = load_with_profile(
        Box::new(MemorySource::new(bytes)),
        LoadStrategy::Eager,
        &profile,
    )
    .expect_err("required unmet");
    assert_eq!(err.code, ErrorCode::LdrProfileRequiredSectionMissing);
}

#[test]
fn signed_partial_load_passes_under_both_profiles() {
    let seed = [21u8; 32];
    let mut obj = build_obj();
    seal_and_sign(&mut obj, &seed, "k1").expect("sign");
    let bytes = serialize(&obj).expect("serialize");
    let pk = ed25519_public_key(&seed);
    let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], true);

    // profile A（gpu）与 profile B（空）选中不同子集，但签名都通过。
    let a = load_with_profile_signed(
        Box::new(MemorySource::new(bytes.clone())),
        LoadStrategy::Eager,
        &LoadProfile::new("a", BTreeSet::new(), None, caps(&["gpu"])),
        &ts,
    );
    let b = load_with_profile_signed(
        Box::new(MemorySource::new(bytes)),
        LoadStrategy::Eager,
        &LoadProfile::new("b", BTreeSet::new(), None, BTreeSet::new()),
        &ts,
    );
    assert!(a.is_ok(), "profile A signed load failed: {:?}", a.err());
    assert!(b.is_ok(), "profile B signed load failed: {:?}", b.err());
}
