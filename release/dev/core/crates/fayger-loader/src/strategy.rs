//! 加载策略、按需读取器与加载产出物。
//!
//! - [`LoadStrategy`]：`Eager`（返回前读全）/ `Lazy`（返回前只读骨架，Section 按需取回）。
//! - [`SectionLoader`]：Lazy 模式的按需读取器（首次访问读取 + digest 校验 + 缓存）。
//! - [`LoadedArtifact`]：统一两种策略的访问入口。

use crate::digest::sha256;
use crate::source::{read_exact, BuFSource};
use fayger_buf::format::{Header, Trailer};
use fayger_buf::manifest::types::Digest;
use fayger_buf::manifest::BuFManifest;
use fayger_buf::object::Section;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use std::collections::BTreeMap;

/// 加载阶段标注。`eager` = load 调用栈内；`lazy` = SectionLoader.fetch 调用栈内。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// load 调用栈内（Eager 全读、Lazy 读骨架/Header/Trailer、parse、digest 校验）。
    Eager,
    /// Lazy 模式 SectionLoader.fetch 调用栈内。
    Lazy,
}

impl Phase {
    /// 稳定字符串（用于 `context.phase`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eager => "eager",
            Self::Lazy => "lazy",
        }
    }
}

/// 加载策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStrategy {
    /// 返回前完成所有被选中 Section 的读取与校验。
    Eager,
    /// 返回前仅读取并校验可信骨架；Section 段体首次访问时按需取回。
    Lazy,
}

/// 单个 Section 的按需读取计划（由 manifest.sections 顺序累加构造）。
/// 单个 Section 的按需读取计划（由 manifest.sections 顺序累加构造）。
#[derive(Debug, Clone)]
pub(crate) struct SectionPlan {
    pub(crate) id: String,
    /// 相对于 Sections 段开头的偏移。
    pub(crate) offset_in_region: u64,
    pub(crate) length: u64,
    pub(crate) digest: Digest,
}

/// 由 manifest.sections 顺序累加构造按需读取计划。
pub(crate) fn build_plan(manifest: &BuFManifest) -> Vec<SectionPlan> {
    let mut plan = Vec::with_capacity(manifest.sections.len());
    let mut offset: u64 = 0;
    for sd in &manifest.sections {
        plan.push(SectionPlan {
            id: sd.id.clone(),
            offset_in_region: offset,
            length: sd.length,
            digest: sd.digest.clone(),
        });
        offset = offset.saturating_add(sd.length);
    }
    plan
}

/// Lazy 模式的按需读取器。
pub struct SectionLoader {
    source: Box<dyn BuFSource>,
    sections_region_offset: u64,
    plan: Vec<SectionPlan>,
    cache: BTreeMap<String, Vec<u8>>,
}

impl SectionLoader {
    /// 从 manifest 构造（Section 偏移由各段声明长度顺序累加）。
    fn from_manifest(
        source: Box<dyn BuFSource>,
        manifest: &BuFManifest,
        sections_region_offset: u64,
    ) -> Self {
        Self {
            source,
            sections_region_offset,
            plan: build_plan(manifest),
            cache: BTreeMap::new(),
        }
    }

    /// 首次访问触发读取 + digest 校验 + 缓存；再次访问命中缓存。
    ///
    /// # Errors
    /// - 未知 section id → `LDR_PARSE_FAIL`（reason=UnknownSection）。
    /// - source 读取失败 → `LDR_LAZY_SOURCE_UNAVAILABLE`（phase=lazy）。
    /// - digest 失配 → `LDR_LAZY_DIGEST_MISMATCH`（phase=lazy），不缓存。
    pub fn fetch(&mut self, section_id: &str) -> Result<&[u8], FaygerError> {
        if self.cache.contains_key(section_id) {
            // 命中缓存，不读 source。
            return Ok(&self.cache[section_id]);
        }
        let plan = self
            .plan
            .iter()
            .find(|p| p.id == section_id)
            .ok_or_else(|| {
                FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "unknown section id")
                    .with_context("reason", ContextValue::string("UnknownSection"))
                    .with_context("section_id", ContextValue::string(section_id.to_owned()))
            })?
            .clone();

        let abs_offset = self
            .sections_region_offset
            .checked_add(plan.offset_in_region)
            .ok_or_else(|| {
                FaygerError::new(
                    ErrorCode::LdrLazySourceUnavailable,
                    Layer::Loader,
                    "section absolute offset overflow",
                )
                .with_context("phase", ContextValue::string("lazy"))
                .with_context("section_id", ContextValue::string(section_id.to_owned()))
            })?;

        let bytes = read_exact(self.source.as_ref(), abs_offset, plan.length, Phase::Lazy)?;

        // digest 校验（M5 支持 sha256；与 M1 digest 算法白名单一致）。
        let actual = sha256(&bytes);
        if plan.digest.algorithm != "sha256" || actual.as_slice() != plan.digest.value.as_slice() {
            return Err(FaygerError::new(
                ErrorCode::LdrLazyDigestMismatch,
                Layer::Loader,
                "lazy section digest verification failed",
            )
            .with_context("phase", ContextValue::string("lazy"))
            .with_context("section_id", ContextValue::string(section_id.to_owned())));
        }

        self.cache.insert(section_id.to_owned(), bytes);
        Ok(&self.cache[section_id])
    }

    /// 该 Section 是否已加载（缓存中）。
    #[must_use]
    pub fn is_loaded(&self, section_id: &str) -> bool {
        self.cache.contains_key(section_id)
    }

    /// 已加载的 Section id 列表（有序）。
    #[must_use]
    pub fn loaded_ids(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }

    /// 计划中全部 Section id（有序，按 manifest 顺序）。
    #[must_use]
    pub fn all_ids(&self) -> Vec<String> {
        self.plan.iter().map(|p| p.id.clone()).collect()
    }
}

/// source 加载链的产出物。统一 Eager / Lazy 两种策略的访问入口。
pub struct LoadedArtifact {
    /// 制品元数据清单。
    pub manifest: BuFManifest,
    /// 原始 Header。
    pub raw_header: Header,
    /// 原始 Trailer。
    pub raw_trailer: Trailer,
    /// 本次加载使用的策略。
    pub strategy: LoadStrategy,
    eager_sections: Option<Vec<Section>>,
    loader: Option<SectionLoader>,
}

impl core::fmt::Debug for LoadedArtifact {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // 不打印 loader 的 source（dyn BuFSource 非 Debug）；只暴露策略与已加载状态。
        f.debug_struct("LoadedArtifact")
            .field("strategy", &self.strategy)
            .field("section_count", &self.manifest.sections.len())
            .field("is_eager", &self.eager_sections.is_some())
            .field(
                "loaded_ids",
                &self.loader.as_ref().map(SectionLoader::loaded_ids),
            )
            .finish_non_exhaustive()
    }
}

impl LoadedArtifact {
    /// 构造 Eager 产出（全部 Section 已在内存）。
    pub(crate) const fn eager(
        manifest: BuFManifest,
        raw_header: Header,
        raw_trailer: Trailer,
        sections: Vec<Section>,
    ) -> Self {
        Self {
            manifest,
            raw_header,
            raw_trailer,
            strategy: LoadStrategy::Eager,
            eager_sections: Some(sections),
            loader: None,
        }
    }

    /// 构造 Lazy 产出（持有按需 loader）。
    pub(crate) fn lazy(
        manifest: BuFManifest,
        raw_header: Header,
        raw_trailer: Trailer,
        source: Box<dyn BuFSource>,
        sections_region_offset: u64,
    ) -> Self {
        let loader = SectionLoader::from_manifest(source, &manifest, sections_region_offset);
        Self {
            manifest,
            raw_header,
            raw_trailer,
            strategy: LoadStrategy::Lazy,
            eager_sections: None,
            loader: Some(loader),
        }
    }

    /// 是否为 Eager 加载。
    #[must_use]
    pub const fn is_eager(&self) -> bool {
        matches!(self.strategy, LoadStrategy::Eager)
    }

    /// 统一访问入口：返回指定 Section 的内容。
    ///
    /// Eager 模式从内存直接返回；Lazy 模式委派 [`SectionLoader::fetch`]（首次触发读取与校验）。
    ///
    /// # Errors
    /// Eager 模式未知 id → `LDR_PARSE_FAIL`；Lazy 模式见 [`SectionLoader::fetch`]。
    pub fn section(&mut self, section_id: &str) -> Result<&[u8], FaygerError> {
        match (&self.eager_sections, &mut self.loader) {
            (Some(sections), _) => sections
                .iter()
                .find(|s| s.id == section_id)
                .map(|s| s.payload.as_slice())
                .ok_or_else(|| {
                    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "unknown section id")
                        .with_context("reason", ContextValue::string("UnknownSection"))
                        .with_context("section_id", ContextValue::string(section_id.to_owned()))
                }),
            (None, Some(loader)) => loader.fetch(section_id),
            (None, None) => Err(FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "artifact has neither eager sections nor lazy loader",
            )),
        }
    }

    /// Lazy 模式下访问 loader（测试用，断言读取次数/缓存）。
    #[must_use]
    pub const fn loader(&self) -> Option<&SectionLoader> {
        self.loader.as_ref()
    }

    /// 全部 Section id（按 manifest 顺序）。
    #[must_use]
    pub fn section_ids(&self) -> Vec<String> {
        self.manifest
            .sections
            .iter()
            .map(|s| s.id.clone())
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{build_plan, LoadStrategy, Phase};
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn manifest_with(lengths: &[u64]) -> BuFManifest {
        let sections = lengths
            .iter()
            .enumerate()
            .map(|(i, &len)| SectionDescriptor {
                id: format!("s{i}"),
                kind: SectionKind::Code,
                digest: Digest::placeholder(),
                length: len,
                visibility: SectionVisibility::Required,
                profile_constraints: ProfileConstraints::default(),
            })
            .collect();
        BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections,
            signature: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn plan_offsets_accumulate() {
        let m = manifest_with(&[4, 2, 8]);
        let plan = build_plan(&m);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].offset_in_region, 0);
        assert_eq!(plan[1].offset_in_region, 4);
        assert_eq!(plan[2].offset_in_region, 6);
    }

    #[test]
    fn empty_manifest_yields_empty_plan() {
        let m = manifest_with(&[]);
        assert!(build_plan(&m).is_empty());
    }

    #[test]
    fn phase_str() {
        assert_eq!(Phase::Eager.as_str(), "eager");
        assert_eq!(Phase::Lazy.as_str(), "lazy");
    }

    #[test]
    fn load_strategy_equality() {
        assert_eq!(LoadStrategy::Eager, LoadStrategy::Eager);
        assert_ne!(LoadStrategy::Eager, LoadStrategy::Lazy);
    }
}
