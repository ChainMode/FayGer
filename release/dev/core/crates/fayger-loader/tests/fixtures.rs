//! M1 快照测试：经封装的样例 BuF 字节流。
//!
//! `tests/fixtures/sealed.buf` 是一个经 [`fayger_loader::seal`] 的两段 BuF（Code + Asset），
//! 提交进 git。测试断言：
//!
//! 1. `load(sealed.buf, policy)` 成功，且各字段符合预期。
//! 2. 篡改任一字节后 `load` 失败。
//!
//! 若 BuF 格式或封装逻辑有意演进，用下面的 `#[ignore]` 重生成器刷新 fixture：
//!
//! ```bash
//! cargo test -p fayger-loader --test fixtures -- --ignored regenerate_sealed_fixture
//! ```
//!
//! 跑完后人工 review `git diff`，确认变化是预期的演进而非回归。

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_error::ErrorCode;
use fayger_loader::{load, seal, LoadPolicy};
use std::collections::{BTreeMap, BTreeSet};

const SEALED_FIXTURE: &[u8] = include_bytes!("fixtures/sealed.buf");

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

/// 构造 fixture 的源对象（未封装）。封装在 `seal` 内完成。
fn fixture_source_object() -> BuFObject {
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

fn fixture_policy() -> LoadPolicy {
    LoadPolicy::new((v("1.0.0"), v("1.9.9")), BTreeSet::new(), v("1.0.0"))
}

#[test]
fn sealed_fixture_loads_successfully() {
    let obj = load(SEALED_FIXTURE, &fixture_policy()).expect("fixture should load");
    assert_eq!(obj.sections.len(), 2);
    assert_eq!(obj.sections[0].id, "code");
    assert_eq!(obj.sections[0].payload, vec![0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(obj.sections[1].id, "asset");
    assert_eq!(obj.sections[1].payload, vec![0x01, 0x02, 0x03]);
    assert_eq!(obj.manifest.deprecation_notice, None);
}

#[test]
fn sealed_fixture_matches_freshly_sealed_bytes() {
    // fixture 必须与当前 seal+serialize 的产物字节级一致。
    let mut obj = fixture_source_object();
    seal(&mut obj).expect("seal");
    let bytes = serialize(&obj).expect("serialize");
    assert_eq!(
        bytes, SEALED_FIXTURE,
        "fixture drift: regenerate with `--ignored regenerate_sealed_fixture` and review the diff"
    );
}

#[test]
fn tampering_any_byte_breaks_load() {
    // 对每个字节翻转最低位，断言 load 都失败。
    for i in 0..SEALED_FIXTURE.len() {
        let mut tampered = SEALED_FIXTURE.to_vec();
        tampered[i] ^= 0x01;
        if tampered == SEALED_FIXTURE {
            continue;
        }
        let res = load(&tampered, &fixture_policy());
        assert!(
            res.is_err(),
            "flipping byte {i} should break load but it succeeded"
        );
        let code = res.unwrap_err().code;
        assert!(
            code == ErrorCode::LdrParseFail || code == ErrorCode::LdrDigestMismatch,
            "byte {i}: unexpected error code {code:?}"
        );
    }
}

/// 重生成 fixture。默认 `#[ignore]`，仅在格式演进时手动运行。
#[test]
#[ignore = "regenerator: run manually when the BuF format or seal logic changes"]
fn regenerate_sealed_fixture() {
    let mut obj = fixture_source_object();
    seal(&mut obj).expect("seal");
    let bytes = serialize(&obj).expect("serialize");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sealed.buf");
    std::fs::write(path, &bytes).expect("write fixture");
    eprintln!("regenerated {} ({} bytes)", path, bytes.len());
}
