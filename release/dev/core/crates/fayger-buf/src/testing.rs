//! Property-based-test 生成器（feature-gated）。
//!
//! 通过启用 `testing` feature 把这一组 [`Strategy`] 函数暴露给依赖方与本 crate
//! 的集成测试。M0 的属性测试（任务 12-15）建立在这些生成器之上；后续里程碑
//! 也可以复用同一套生成器构造端到端用例。
//!
//! ## 生成器覆盖
//!
//! - [`arb_semver`]：SemVer 字符串（覆盖普通版本与含 pre 段的版本）。
//! - [`arb_section`]：Section payload 0..=1024 字节，id 是 ASCII 标识。
//! - [`arb_section_kind`]：覆盖 5 个 SectionKind 变体。
//! - [`arb_buf_manifest`]：含 0..=8 个 SectionDescriptor。
//! - [`arb_buf_object`]：完整 BuFObject，sections 与 manifest 一一对应。
//!
//! 生成器有意保守：M0 测试聚焦字节布局与 round-trip 性质，不引入 float、tag、
//! 嵌套 extensions 等 canonical 编码层禁止的形态。

use crate::format::{Header, Trailer, FORMAT_VERSION_M0, HEADER_SIZE, MANIFEST_DIGEST_SIZE};
use crate::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use crate::object::{BuFObject, Section};
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// 生成 0..=255 范围内的版本号分量；上限保守，避免 SemVer 字符串过长拖慢 PBT。
fn arb_version_component() -> impl Strategy<Value = u32> {
    (0u32..=255).boxed()
}

/// 生成合法的 SemVer。允许 `None` 或 `Some(短小 ASCII 标识)` 的 pre 段。
pub fn arb_semver() -> impl Strategy<Value = SemVer> {
    (
        arb_version_component(),
        arb_version_component(),
        arb_version_component(),
        prop::option::of("[a-z0-9]{1,8}"),
    )
        .prop_map(|(major, minor, patch, pre)| SemVer {
            major,
            minor,
            patch,
            pre,
        })
}

/// 生成合法的 Section.id：ASCII 字母 / 数字 / 下划线 / 连字符，1..=32 字符。
pub fn arb_section_id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_\\-]{0,31}".prop_map(String::from)
}

/// 生成 SectionKind。5 个变体等概率。
pub fn arb_section_kind() -> impl Strategy<Value = SectionKind> {
    prop_oneof![
        Just(SectionKind::Code),
        Just(SectionKind::Data),
        Just(SectionKind::Asset),
        Just(SectionKind::Signature),
        "[a-z][a-z0-9_]{0,7}".prop_map(SectionKind::Custom),
    ]
}

/// 生成 Section（id 与 payload 独立选择）。
pub fn arb_section() -> impl Strategy<Value = Section> {
    (
        arb_section_id(),
        prop::collection::vec(any::<u8>(), 0..=1024),
    )
        .prop_map(|(id, payload)| Section { id, payload })
}

/// 生成 BuFObject。
///
/// 流程：先生成一组带唯一 id 的 sections，再据此构造 manifest 与 BuFObject。
/// 这样 BuFObject 在 [`BuFObject::check_consistency`] 上恒为有效。
pub fn arb_buf_object() -> impl Strategy<Value = BuFObject> {
    (arb_semver(), arb_semver(), arb_unique_sections()).prop_map(
        |(schema_version, runtime_interface_min, sections)| {
            build_object(schema_version, runtime_interface_min, sections)
        },
    )
}

/// 生成不重复 id 的 sections 列表，长度 0..=8。
fn arb_unique_sections() -> impl Strategy<Value = Vec<Section>> {
    prop::collection::vec(arb_section(), 0..=8).prop_map(|sections| {
        // 简单去重：保留首次出现的 id，丢弃后续同名条目
        let mut seen: BTreeSet<String> = BTreeSet::new();
        sections
            .into_iter()
            .filter(|s| seen.insert(s.id.clone()))
            .collect()
    })
}

/// 生成 BuFManifest。
///
/// 不含 sections 的 manifest 也合法。
pub fn arb_buf_manifest() -> impl Strategy<Value = BuFManifest> {
    arb_buf_object().prop_map(|obj| obj.manifest)
}

/// 内部：根据已经生成的 sections 构造完整 BuFObject。
fn build_object(
    schema_version: SemVer,
    runtime_interface_min: SemVer,
    sections: Vec<Section>,
) -> BuFObject {
    let descriptors: Vec<SectionDescriptor> = sections
        .iter()
        .map(|s| SectionDescriptor {
            id: s.id.clone(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: u64::try_from(s.payload.len()).unwrap_or(u64::MAX),
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();

    let manifest = BuFManifest {
        schema_version,
        runtime_interface_min,
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: descriptors,
        signature: None,
        extensions: BTreeMap::new(),
    };

    let raw_header = Header {
        format_version: FORMAT_VERSION_M0,
        manifest_offset: u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX),
        manifest_length: 0,
        section_index_offset: u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX),
        section_index_length: 0,
    };
    let raw_trailer = Trailer {
        total_length: 0,
        manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
        crc32: 0,
    };

    BuFObject {
        manifest,
        sections,
        raw_header,
        raw_trailer,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    //! 简单的 sanity 测试：确认每个 strategy 至少能产出有效样本。

    use super::*;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::{Config, TestRunner};

    fn sample<T: std::fmt::Debug>(strategy: impl Strategy<Value = T>) -> T {
        let mut runner = TestRunner::new(Config::default());
        strategy
            .new_tree(&mut runner)
            .expect("create tree")
            .current()
    }

    #[test]
    fn semver_sample_displays_to_parseable_string() {
        let v = sample(arb_semver());
        let s = v.to_string();
        SemVer::parse(&s).expect("displayed SemVer should be re-parseable");
    }

    #[test]
    fn section_kind_sample_round_trips_through_canonical_cbor() {
        let kind = sample(arb_section_kind());
        let bytes = crate::canonical::canonical_encode(&kind).expect("encode");
        let back: SectionKind = crate::canonical::canonical_decode(&bytes).expect("decode");
        assert_eq!(kind, back);
    }

    #[test]
    fn section_sample_has_consistent_id_and_payload() {
        let s = sample(arb_section());
        assert!(!s.id.is_empty());
        assert!(s.payload.len() <= 1024);
    }

    #[test]
    fn buf_object_sample_passes_manifest_validate_and_consistency() {
        let obj = sample(arb_buf_object());
        obj.manifest.validate().expect("manifest valid");
        obj.check_consistency().expect("object consistent");
    }

    #[test]
    fn buf_object_sample_round_trips_via_serialize_parse() {
        let obj = sample(arb_buf_object());
        let bytes = crate::serialize(&obj).expect("serialize");
        let back = crate::parse(&bytes).expect("parse");
        // 结构等价（蓝图 Property 1）：manifest 与 sections 相等。
        // raw_header / raw_trailer 中的 offset / length 字段会被 serialize 重新计算填回，
        // 不参与「结构等价」比较——这是预期行为。
        assert_eq!(back.manifest, obj.manifest);
        assert_eq!(back.sections, obj.sections);
    }
}
