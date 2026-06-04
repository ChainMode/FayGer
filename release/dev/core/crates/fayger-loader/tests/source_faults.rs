//! Feature: fayger-m5, Property 3 & 5: source failure phase tagging & read_at contract.
//!
//! Property 3：Eager 加载在 load 栈内 source 失败 → LDR_SOURCE_READ_FAILED/phase=eager；
//! Lazy fetch 阶段 source 失败 → LDR_LAZY_SOURCE_UNAVAILABLE/phase=lazy。
//! Property 5：read_at 越界请求 → 加载层 LDR_SOURCE_READ_FAILED。
//!
//! Validates: Requirements 4.1, 4.2, 4.3, 5.1, 5.2, 5.3, 1.2, 1.3, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::serialize;
use fayger_buf::testing::arb_buf_object;
use fayger_error::{ContextValue, ErrorCode};
use fayger_loader::{load_from_source, seal, BuFSource, LoadStrategy, MemorySource, SourceError};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::cell::Cell;

/// 在第 `fail_after` 次 read_at（从 1 计）开始失败的 source。
struct FaultySource {
    inner: MemorySource,
    fail_after: usize,
    calls: Cell<usize>,
}

impl FaultySource {
    fn new(bytes: Vec<u8>, fail_after: usize) -> Self {
        Self {
            inner: MemorySource::new(bytes),
            fail_after,
            calls: Cell::new(0),
        }
    }
}

impl BuFSource for FaultySource {
    fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError> {
        let n = self.calls.get() + 1;
        self.calls.set(n);
        if n >= self.fail_after {
            Err(SourceError::new("injected read failure"))
        } else {
            self.inner.read_at(offset, length)
        }
    }
    fn length(&self) -> Result<u64, SourceError> {
        self.inner.length()
    }
}

fn sealed_with_sections() -> impl Strategy<Value = Vec<u8>> {
    arb_buf_object()
        .prop_filter("non-empty sections", |o| !o.sections.is_empty())
        .prop_map(|mut obj| {
            seal(&mut obj).expect("seal");
            serialize(&obj).expect("serialize")
        })
}

fn phase_of(err: &fayger_error::FaygerError) -> Option<String> {
    match err.context.get("phase") {
        Some(ContextValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Eager 加载：第一次 read_at 即失败 → SourceReadFailed/eager。
    #[test]
    fn eager_source_failure_is_eager_phase(bytes in sealed_with_sections()) {
        let src = Box::new(FaultySource::new(bytes, 1)); // 第一次 read_at 即失败
        let err = load_from_source(src, LoadStrategy::Eager).expect_err("eager fail");
        prop_assert_eq!(err.code, ErrorCode::LdrSourceReadFailed);
        let phase = phase_of(&err);
        prop_assert_eq!(phase.as_deref(), Some("eager"));
    }

    /// Lazy 加载：骨架读取成功（前几次 read_at），fetch 阶段失败 → LazySourceUnavailable/lazy。
    #[test]
    fn lazy_fetch_failure_is_lazy_phase(bytes in sealed_with_sections()) {
        // Lazy 骨架读取需要 3 次 read_at（Header、骨架、Trailer）。让第 4 次起失败，
        // 使骨架加载成功、fetch 失败。
        let src = Box::new(FaultySource::new(bytes, 4));
        let mut art = load_from_source(src, LoadStrategy::Lazy).expect("lazy skeleton ok");
        let ids = art.section_ids();
        let id = ids.first().expect("has section");
        let err = art.section(id).expect_err("fetch fail");
        prop_assert_eq!(err.code, ErrorCode::LdrLazySourceUnavailable);
        let phase = phase_of(&err);
        prop_assert_eq!(phase.as_deref(), Some("lazy"));
    }
}

#[test]
fn read_at_out_of_bounds_maps_to_source_read_failed() {
    // 一个 length() 谎报过大、read_at 越界返回 Err 的 source。
    struct LiarSource {
        bytes: Vec<u8>,
    }
    impl BuFSource for LiarSource {
        fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError> {
            // 委派给 MemorySource：越界自然 Err
            MemorySource::new(self.bytes.clone()).read_at(offset, length)
        }
        fn length(&self) -> Result<u64, SourceError> {
            Ok((self.bytes.len() as u64) + 1000) // 谎报更大
        }
    }
    let mut obj_bytes = {
        // 构造一个真实的 sealed BuF
        use fayger_buf::manifest::{
            types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
            BuFManifest, EntryPoint, SectionDescriptor, SemVer,
        };
        use fayger_buf::object::{BuFObject, Section};
        use std::collections::{BTreeMap, BTreeSet};
        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").unwrap(),
            runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
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
        let mut o = BuFObject {
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
        };
        seal(&mut o).unwrap();
        serialize(&o).unwrap()
    };
    // Eager 读 length()=真实+1000 字节，read_at(0, total) 越界 → SourceReadFailed。
    let _ = &mut obj_bytes;
    let src = Box::new(LiarSource { bytes: obj_bytes });
    let err = load_from_source(src, LoadStrategy::Eager).expect_err("oob");
    assert_eq!(err.code, ErrorCode::LdrSourceReadFailed);
}
