//! M5 端到端：MemorySource → Eager/Lazy 等价；失败 Source → Lazy 阶段标注。
//!
//! Validates: Requirements 8.2

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::redundant_clone
)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_error::ErrorCode;
use fayger_loader::{load_from_source, seal, BuFSource, LoadStrategy, MemorySource, SourceError};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

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

fn two_section_bytes() -> Vec<u8> {
    let manifest = BuFManifest {
        schema_version: SemVer::parse("1.0.0").unwrap(),
        runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
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
    let mut obj = BuFObject {
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
    };
    seal(&mut obj).expect("seal");
    serialize(&obj).expect("serialize")
}

#[test]
fn eager_and_lazy_equivalent_via_memory_source() {
    let bytes = two_section_bytes();

    let mut eager = load_from_source(
        Box::new(MemorySource::new(bytes.clone())),
        LoadStrategy::Eager,
    )
    .expect("eager");
    let mut lazy = load_from_source(
        Box::new(MemorySource::new(bytes.clone())),
        LoadStrategy::Lazy,
    )
    .expect("lazy");

    assert!(eager.is_eager());
    assert!(!lazy.is_eager());
    assert_eq!(eager.manifest, lazy.manifest);
    assert_eq!(eager.raw_header, lazy.raw_header);
    assert_eq!(eager.raw_trailer, lazy.raw_trailer);

    assert_eq!(eager.section("code").unwrap(), &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(lazy.section("code").unwrap(), &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(eager.section("asset").unwrap(), &[0x01, 0x02, 0x03]);
    assert_eq!(lazy.section("asset").unwrap(), &[0x01, 0x02, 0x03]);
}

#[test]
fn always_failing_source_blocks_lazy_skeleton() {
    struct AlwaysFail;
    impl BuFSource for AlwaysFail {
        fn read_at(&self, _: u64, _: u64) -> Result<Vec<u8>, SourceError> {
            Err(SourceError::new("down"))
        }
        fn length(&self) -> Result<u64, SourceError> {
            Ok(1000)
        }
    }
    // 骨架读取阶段（load 栈内）失败 → SourceReadFailed/eager。
    let err = load_from_source(Box::new(AlwaysFail), LoadStrategy::Lazy).expect_err("fail");
    assert_eq!(err.code, ErrorCode::LdrSourceReadFailed);
    match err.context.get("phase") {
        Some(fayger_error::ContextValue::String(s)) => assert_eq!(s, "eager"),
        other => panic!("expected phase, got {other:?}"),
    }
}

#[test]
fn skeleton_readable_section_unavailable_yields_lazy_error() {
    // 骨架可读、但 Section 段体读取失败的 source：Lazy 加载骨架成功、fetch 失败。
    struct SkeletonOnlySource {
        bytes: Vec<u8>,
        sections_region_offset: u64,
        calls: Cell<usize>,
    }
    impl BuFSource for SkeletonOnlySource {
        fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError> {
            self.calls.set(self.calls.get() + 1);
            // 落在 Sections 区（offset >= sections_region_offset 且不是末尾 Trailer 读取）
            // 的读取一律失败，模拟段体不可用。
            let trailer_offset = self.bytes.len() as u64 - fayger_buf::format::TRAILER_SIZE as u64;
            if offset >= self.sections_region_offset && offset < trailer_offset {
                return Err(SourceError::new("section body unavailable"));
            }
            MemorySource::new(self.bytes.clone()).read_at(offset, length)
        }
        fn length(&self) -> Result<u64, SourceError> {
            Ok(self.bytes.len() as u64)
        }
    }

    let bytes = two_section_bytes();
    let parsed = fayger_buf::parse(&bytes).expect("parse");
    let sro = parsed.raw_header.sections_offset().expect("sro");

    let src = Box::new(SkeletonOnlySource {
        bytes,
        sections_region_offset: sro,
        calls: Cell::new(0),
    });
    let mut art = load_from_source(src, LoadStrategy::Lazy).expect("skeleton ok");
    // fetch 任一段 → LDR_LAZY_SOURCE_UNAVAILABLE / phase=lazy。
    let err = art.section("code").expect_err("section unavailable");
    assert_eq!(err.code, ErrorCode::LdrLazySourceUnavailable);
    match err.context.get("phase") {
        Some(fayger_error::ContextValue::String(s)) => assert_eq!(s, "lazy"),
        other => panic!("expected phase, got {other:?}"),
    }
}
