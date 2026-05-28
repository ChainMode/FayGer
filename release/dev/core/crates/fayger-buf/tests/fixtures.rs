//! 样例字节流的快照测试。
//!
//! 在 `tests/fixtures/` 下存放几份手工构造好的合法 BuF 字节流。这些样例:
//!
//! - 让外部工具（CLI、查看器、跨语言实现）可以以已知字节流为基准回归。
//! - 检查 fayger-buf 自身在版本演进时不会无意间改变线上格式。
//!
//! 测试逻辑:
//!
//! 1. 用代码构造若干 `expected: BuFObject`。
//! 2. 调 [`fayger_buf::serialize`] 得到字节流，断言与磁盘上的 fixture 字节相等。
//! 3. 反过来调 [`fayger_buf::parse`] 解析 fixture 字节流，断言与 `expected`
//!    在「manifest + sections」上等价。
//!
//! 当格式有意发生变更（例如 schema_version 抬版）时，运行被忽略的
//! `regenerate_fixtures` 测试可以一键刷新字节流，再人工 review 字节差异
//! 并提交进 git。

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::format::{Header, Trailer, FORMAT_VERSION_M0, MANIFEST_DIGEST_SIZE};
use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::{parse, serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fresh_header() -> Header {
    Header {
        format_version: FORMAT_VERSION_M0,
        manifest_offset: 0,
        manifest_length: 0,
        section_index_offset: 0,
        section_index_length: 0,
    }
}

fn fresh_trailer() -> Trailer {
    Trailer {
        total_length: 0,
        manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
        crc32: 0,
    }
}

fn descriptor(id: &str, length: u64, kind: SectionKind) -> SectionDescriptor {
    SectionDescriptor {
        id: id.into(),
        kind,
        digest: Digest::placeholder(),
        length,
        visibility: SectionVisibility::Required,
        profile_constraints: ProfileConstraints::default(),
    }
}

/// 最小合法 BuF（无 sections）的预期对象。
fn expected_minimal() -> BuFObject {
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").expect("ok"),
        runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: Vec::new(),
        signature: None,
        extensions: BTreeMap::new(),
    };
    BuFObject {
        manifest,
        sections: Vec::new(),
        raw_header: fresh_header(),
        raw_trailer: fresh_trailer(),
    }
}

/// 含两个 sections（code + asset）的合法 BuF 预期对象。
fn expected_two_sections() -> BuFObject {
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").expect("ok"),
        runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![
            descriptor("code", 8, SectionKind::Code),
            descriptor("asset", 4, SectionKind::Asset),
        ],
        signature: None,
        extensions: BTreeMap::new(),
    };
    BuFObject {
        manifest,
        sections: vec![
            Section::new("code", vec![0xfa, 0xce, 0xfe, 0xed, 0xde, 0xad, 0xbe, 0xef]),
            Section::new("asset", vec![1u8, 2, 3, 4]),
        ],
        raw_header: fresh_header(),
        raw_trailer: fresh_trailer(),
    }
}

#[test]
fn minimal_fixture_round_trips_byte_equivalent() {
    let path = fixtures_dir().join("minimal.buf");
    assert!(
        path.exists(),
        "fixture not found: {}\n运行 `cargo test --test fixtures -- --ignored regenerate_fixtures` 重新生成",
        path.display()
    );
    let on_disk = std::fs::read(&path).expect("read fixture");

    let expected = expected_minimal();
    let serialized = serialize(&expected).expect("serialize");
    assert_eq!(
        serialized, on_disk,
        "minimal.buf 字节漂移了；如确实有意变更，运行 `cargo test --test fixtures -- --ignored regenerate_fixtures`"
    );

    let parsed = parse(&on_disk).expect("parse");
    assert_eq!(parsed.manifest, expected.manifest);
    assert_eq!(parsed.sections, expected.sections);
}

#[test]
fn two_sections_fixture_round_trips_byte_equivalent() {
    let path = fixtures_dir().join("two_sections.buf");
    assert!(
        path.exists(),
        "fixture not found: {}\n运行 `cargo test --test fixtures -- --ignored regenerate_fixtures` 重新生成",
        path.display()
    );
    let on_disk = std::fs::read(&path).expect("read fixture");

    let expected = expected_two_sections();
    let serialized = serialize(&expected).expect("serialize");
    assert_eq!(
        serialized, on_disk,
        "two_sections.buf 字节漂移了；如确实有意变更，运行 `cargo test --test fixtures -- --ignored regenerate_fixtures`"
    );

    let parsed = parse(&on_disk).expect("parse");
    assert_eq!(parsed.manifest, expected.manifest);
    assert_eq!(parsed.sections, expected.sections);
}

/// 一键刷新 fixtures。默认被 `#[ignore]`，需要时显式跑：
///
/// ```text
/// cargo test --test fixtures -- --ignored regenerate_fixtures
/// ```
///
/// 刷新后**必须人工 review** `git diff` 确认字节差异符合预期，再 commit 进 git。
#[test]
#[ignore = "regeneration is opt-in: run with --ignored"]
fn regenerate_fixtures() {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("create fixtures dir");

    let minimal = serialize(&expected_minimal()).expect("serialize minimal");
    std::fs::write(dir.join("minimal.buf"), &minimal).expect("write minimal.buf");

    let two = serialize(&expected_two_sections()).expect("serialize two_sections");
    std::fs::write(dir.join("two_sections.buf"), &two).expect("write two_sections.buf");

    println!(
        "regenerated fixtures:\n  - {} ({} bytes)\n  - {} ({} bytes)",
        dir.join("minimal.buf").display(),
        minimal.len(),
        dir.join("two_sections.buf").display(),
        two.len(),
    );
}
