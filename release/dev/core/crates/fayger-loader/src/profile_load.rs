//! profile 驱动的部分加载入口与产出物。
//!
//! [`load_with_profile`] / [`load_with_profile_signed`] 在 M5 的骨架读取之上插入
//! [`crate::select::select_sections`]，把被选中 Section 从"全部"收窄为 profile 命中的子集。
//! 被跳过的 Optional Section 不被读取；签名校验只需骨架（作用域不含 Section 段体），
//! 因此部分加载下"不同 profile 子集签名结果相同"。

use crate::digest::sha256;
use crate::select::{select_sections, LoadProfile, SelectionResult, SkipRecord};
use crate::sig::verify_signature;
use crate::source::{read_exact, BuFSource};
use crate::strategy::{build_plan, LoadStrategy, Phase, SectionPlan};
use crate::truststore::TrustStore;
use fayger_buf::format::{Header, Trailer, HEADER_SIZE, TRAILER_SIZE};
use fayger_buf::manifest::BuFManifest;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use std::collections::BTreeMap;

/// Lazy 模式下部分加载的内部状态。
struct LazyState {
    source: Box<dyn BuFSource>,
    sections_region_offset: u64,
    plan: Vec<SectionPlan>,
    cache: BTreeMap<String, Vec<u8>>,
}

/// profile 驱动的部分加载产出物。
pub struct PartialArtifact {
    /// 制品元数据清单。
    pub manifest: BuFManifest,
    /// 原始 Header。
    pub raw_header: Header,
    /// 原始 Trailer。
    pub raw_trailer: Trailer,
    /// 本次加载使用的策略。
    pub strategy: LoadStrategy,
    /// 选择结果（被选中 + 被跳过）。
    pub selection: SelectionResult,
    eager_bodies: Option<BTreeMap<String, Vec<u8>>>,
    lazy: Option<LazyState>,
}

impl core::fmt::Debug for PartialArtifact {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PartialArtifact")
            .field("strategy", &self.strategy)
            .field("selected", &self.selection.selected)
            .field("skipped_count", &self.selection.skipped.len())
            .field("is_eager", &self.eager_bodies.is_some())
            .finish_non_exhaustive()
    }
}

impl PartialArtifact {
    /// 被选中 Section 的 id 列表（manifest 顺序）。
    #[must_use]
    pub fn selected_ids(&self) -> &[String] {
        &self.selection.selected
    }

    /// 被跳过的 Optional Section 记录。
    #[must_use]
    pub fn skipped(&self) -> &[SkipRecord] {
        &self.selection.skipped
    }

    /// 该 Section 是否被选中。
    #[must_use]
    pub fn is_selected(&self, id: &str) -> bool {
        self.selection.selected.iter().any(|s| s == id)
    }

    /// 访问指定 Section 的内容。
    ///
    /// # Errors
    /// - 未被选中（被跳过或不存在）→ `LDR_PARSE_FAIL`（reason=SectionSkipped）。
    /// - Lazy 模式读取/校验失败 → `LDR_LAZY_SOURCE_UNAVAILABLE` / `LDR_LAZY_DIGEST_MISMATCH`。
    pub fn section(&mut self, id: &str) -> Result<&[u8], FaygerError> {
        if !self.is_selected(id) {
            return Err(FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "section was not selected by the load profile",
            )
            .with_context("reason", ContextValue::string("SectionSkipped"))
            .with_context("section_id", ContextValue::string(id.to_owned())));
        }
        match (&self.eager_bodies, &mut self.lazy) {
            (Some(bodies), _) => bodies.get(id).map(Vec::as_slice).ok_or_else(|| {
                FaygerError::new(
                    ErrorCode::LdrParseFail,
                    Layer::Loader,
                    "selected section missing from eager bodies",
                )
                .with_context("section_id", ContextValue::string(id.to_owned()))
            }),
            (None, Some(state)) => state.fetch(id),
            (None, None) => Err(FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "artifact has neither eager bodies nor lazy state",
            )),
        }
    }
}

impl LazyState {
    /// 首次访问读取 + digest 校验 + 缓存（同 M5 Lazy 语义）。
    fn fetch(&mut self, id: &str) -> Result<&[u8], FaygerError> {
        if self.cache.contains_key(id) {
            return Ok(&self.cache[id]);
        }
        let plan = self
            .plan
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| {
                FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "unknown section id")
                    .with_context("reason", ContextValue::string("UnknownSection"))
                    .with_context("section_id", ContextValue::string(id.to_owned()))
            })?
            .clone();
        let abs = self
            .sections_region_offset
            .checked_add(plan.offset_in_region)
            .ok_or_else(|| {
                FaygerError::new(
                    ErrorCode::LdrLazySourceUnavailable,
                    Layer::Loader,
                    "section absolute offset overflow",
                )
                .with_context("phase", ContextValue::string("lazy"))
                .with_context("section_id", ContextValue::string(id.to_owned()))
            })?;
        let bytes = read_exact(self.source.as_ref(), abs, plan.length, Phase::Lazy)?;
        let actual = sha256(&bytes);
        if plan.digest.algorithm != "sha256" || actual.as_slice() != plan.digest.value.as_slice() {
            return Err(FaygerError::new(
                ErrorCode::LdrLazyDigestMismatch,
                Layer::Loader,
                "lazy section digest verification failed",
            )
            .with_context("phase", ContextValue::string("lazy"))
            .with_context("section_id", ContextValue::string(id.to_owned())));
        }
        // 选中性已在 PartialArtifact::section 入口校验；此处直接缓存。
        self.cache.insert(id.to_owned(), bytes);
        Ok(&self.cache[id])
    }
}

/// 部分加载的内部骨架读取结果。
struct Skeleton {
    bytes: Vec<u8>,
    obj: fayger_buf::object::BuFObject,
    sections_region_offset: u64,
}

/// 读取并校验可信骨架（Header + Manifest + Section_Index + Trailer），校验 manifest_digest。
fn read_and_verify_skeleton(source: &dyn BuFSource) -> Result<Skeleton, FaygerError> {
    let total = source.length().map_err(|e| {
        FaygerError::new(
            ErrorCode::LdrSourceReadFailed,
            Layer::Loader,
            "failed to query BuF source length",
        )
        .with_context("phase", ContextValue::string("eager"))
        .with_context("detail", ContextValue::string(e.message))
    })?;

    let header_bytes = read_exact(
        source,
        0,
        u64::try_from(HEADER_SIZE).unwrap_or(u64::MAX),
        Phase::Eager,
    )?;
    let header = Header::read_from_prefix(&header_bytes).map_err(|e| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "failed to parse header",
        )
        .with_context("phase", ContextValue::string("eager"))
        .with_context("detail", ContextValue::string(format!("{e}")))
    })?;
    let sections_region_offset = header.sections_offset().ok_or_else(|| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "sections offset overflow in header",
        )
        .with_context("phase", ContextValue::string("eager"))
    })?;

    let skeleton_bytes = read_exact(source, 0, sections_region_offset, Phase::Eager)?;
    let trailer_offset = total
        .checked_sub(u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX))
        .ok_or_else(|| {
            FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "total length too small for trailer",
            )
            .with_context("phase", ContextValue::string("eager"))
        })?;
    let trailer_bytes = read_exact(
        source,
        trailer_offset,
        u64::try_from(TRAILER_SIZE).unwrap_or(u64::MAX),
        Phase::Eager,
    )?;

    let obj = fayger_buf::parse_skeleton(&skeleton_bytes, &trailer_bytes, total)?;
    // manifest_digest 校验。
    let end = usize::try_from(sections_region_offset).map_err(|_| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "sections offset exceeds addressable range",
        )
        .with_context("phase", ContextValue::string("eager"))
    })?;
    let region = skeleton_bytes.get(..end).ok_or_else(|| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "manifest digest region out of bounds",
        )
        .with_context("phase", ContextValue::string("eager"))
    })?;
    if sha256(region) != obj.raw_trailer.manifest_digest {
        return Err(FaygerError::new(
            ErrorCode::LdrDigestMismatch,
            Layer::Loader,
            "manifest digest does not match (profile skeleton)",
        )
        .with_context("phase", ContextValue::string("eager"))
        .with_context("reason", ContextValue::string("ManifestDigest")));
    }

    Ok(Skeleton {
        bytes: skeleton_bytes,
        obj,
        sections_region_offset,
    })
}

/// profile 驱动的部分加载（不含签名）。
///
/// # Errors
/// 结构 / 摘要 / 源读取 / Required 缺失（`LDR_PROFILE_REQUIRED_SECTION_MISSING`）等。
pub fn load_with_profile(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
    profile: &LoadProfile,
) -> Result<PartialArtifact, FaygerError> {
    let skel = read_and_verify_skeleton(source.as_ref())?;
    finish_partial_load(source, strategy, profile, skel)
}

/// profile 驱动的部分加载（含签名校验，复用 M4）。
///
/// # Errors
/// 同 [`load_with_profile`]，外加签名失败 `LDR_SIGNATURE_FAIL`（在选择之前）。
pub fn load_with_profile_signed(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
    profile: &LoadProfile,
    truststore: &TrustStore,
) -> Result<PartialArtifact, FaygerError> {
    let skel = read_and_verify_skeleton(source.as_ref())?;
    // 签名校验只需骨架字节（signed_scope 用 Header+Manifest+Section_Index，全在骨架内）。
    verify_signature(&skel.bytes, &skel.obj, truststore)?;
    finish_partial_load(source, strategy, profile, skel)
}

/// 选择 + 按策略加载被选中 Section。
fn finish_partial_load(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
    profile: &LoadProfile,
    skel: Skeleton,
) -> Result<PartialArtifact, FaygerError> {
    let selection = select_sections(&skel.obj.manifest, profile)?;
    let plan = build_plan(&skel.obj.manifest);

    match strategy {
        LoadStrategy::Eager => {
            let mut bodies = BTreeMap::new();
            for id in &selection.selected {
                let p = plan
                    .iter()
                    .find(|p| &p.id == id)
                    .ok_or_else(|| internal("selected section missing from plan"))?;
                let abs = skel
                    .sections_region_offset
                    .checked_add(p.offset_in_region)
                    .ok_or_else(|| internal("section absolute offset overflow"))?;
                let bytes = read_exact(source.as_ref(), abs, p.length, Phase::Eager)?;
                let actual = sha256(&bytes);
                if p.digest.algorithm != "sha256" || actual.as_slice() != p.digest.value.as_slice()
                {
                    return Err(FaygerError::new(
                        ErrorCode::LdrDigestMismatch,
                        Layer::Loader,
                        "selected section digest verification failed",
                    )
                    .with_context("phase", ContextValue::string("eager"))
                    .with_context("section_id", ContextValue::string(id.clone())));
                }
                bodies.insert(id.clone(), bytes);
            }
            Ok(PartialArtifact {
                manifest: skel.obj.manifest,
                raw_header: skel.obj.raw_header,
                raw_trailer: skel.obj.raw_trailer,
                strategy,
                selection,
                eager_bodies: Some(bodies),
                lazy: None,
            })
        }
        LoadStrategy::Lazy => Ok(PartialArtifact {
            manifest: skel.obj.manifest,
            raw_header: skel.obj.raw_header,
            raw_trailer: skel.obj.raw_trailer,
            strategy,
            selection,
            eager_bodies: None,
            lazy: Some(LazyState {
                source,
                sections_region_offset: skel.sections_region_offset,
                plan,
                cache: BTreeMap::new(),
            }),
        }),
    }
}

fn internal(message: &'static str) -> FaygerError {
    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, message)
        .with_context("reason", ContextValue::string("ProfileLoadInternal"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{load_with_profile, load_with_profile_signed};
    use crate::seal::seal;
    use crate::select::{LoadProfile, SkipReason};
    use crate::sign::{ed25519_public_key, seal_and_sign};
    use crate::source::MemorySource;
    use crate::strategy::LoadStrategy;
    use crate::truststore::{TrustStore, TrustedRoot};
    use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
    use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
    use fayger_buf::object::{BuFObject, Section};
    use fayger_buf::serialize;
    use fayger_error::ErrorCode;
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

    /// 构造 code(Required,无约束) + gpu(Optional, features={gpu}) 的 BuF（已 seal）。
    fn sealed_two() -> Vec<u8> {
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
            ],
            signature: None,
            extensions: BTreeMap::new(),
        };
        let mut obj = BuFObject {
            manifest,
            sections: vec![
                Section::new("code", vec![1, 2, 3, 4]),
                Section::new("gpu", vec![7, 8, 9]),
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

    fn profile(features: &[&str]) -> LoadProfile {
        LoadProfile::new(
            "test",
            BTreeSet::new(),
            None,
            features.iter().map(|s| (*s).to_string()).collect(),
        )
    }

    #[test]
    fn eager_selects_and_skips() {
        // 无 gpu 特征 → 跳过 gpu，仅选中 code。
        let mut art = load_with_profile(
            Box::new(MemorySource::new(sealed_two())),
            LoadStrategy::Eager,
            &profile(&[]),
        )
        .expect("load");
        assert_eq!(art.selected_ids(), &["code".to_string()]);
        assert_eq!(art.skipped().len(), 1);
        assert_eq!(art.skipped()[0].section_id, "gpu");
        assert!(art.skipped()[0]
            .reasons
            .contains(&SkipReason::FeatureMissing));
        assert_eq!(art.section("code").unwrap(), &[1, 2, 3, 4]);
        // 访问被跳过段 → SectionSkipped
        let err = art.section("gpu").expect_err("skipped");
        assert_eq!(err.code, ErrorCode::LdrParseFail);
    }

    #[test]
    fn eager_selects_all_when_profile_fits() {
        let mut art = load_with_profile(
            Box::new(MemorySource::new(sealed_two())),
            LoadStrategy::Eager,
            &profile(&["gpu"]),
        )
        .expect("load");
        assert_eq!(art.selected_ids(), &["code".to_string(), "gpu".to_string()]);
        assert!(art.skipped().is_empty());
        assert_eq!(art.section("gpu").unwrap(), &[7, 8, 9]);
    }

    #[test]
    fn lazy_selects_and_fetches() {
        let mut art = load_with_profile(
            Box::new(MemorySource::new(sealed_two())),
            LoadStrategy::Lazy,
            &profile(&["gpu"]),
        )
        .expect("load");
        assert!(!art.is_selected("nonexistent"));
        assert_eq!(art.section("gpu").unwrap(), &[7, 8, 9]);
        assert_eq!(art.section("code").unwrap(), &[1, 2, 3, 4]);
    }

    #[test]
    fn required_unmet_fails_before_reading_bodies() {
        // code 改成 Required + 要求 feature "secure"；profile 不满足 → 失败。
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
        let err = load_with_profile(
            Box::new(MemorySource::new(bytes)),
            LoadStrategy::Eager,
            &profile(&[]),
        )
        .expect_err("required missing");
        assert_eq!(err.code, ErrorCode::LdrProfileRequiredSectionMissing);
    }

    #[test]
    fn signed_profile_load_succeeds_with_matching_root() {
        let seed = [5u8; 32];
        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").unwrap(),
            runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![sd(
                "code",
                4,
                SectionVisibility::Required,
                ProfileConstraints::default(),
            )],
            signature: None,
            extensions: BTreeMap::new(),
        };
        let mut obj = BuFObject {
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
        seal_and_sign(&mut obj, &seed, "k1").expect("sign");
        let bytes = serialize(&obj).expect("serialize");
        let pk = ed25519_public_key(&seed);
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", pk.to_vec())], true);
        let art = load_with_profile_signed(
            Box::new(MemorySource::new(bytes.clone())),
            LoadStrategy::Eager,
            &profile(&[]),
            &ts,
        )
        .expect("signed load");
        assert_eq!(art.selected_ids(), &["code".to_string()]);

        // 不含公钥 → UnknownKey
        let empty = TrustStore::new(vec![], false);
        let err = load_with_profile_signed(
            Box::new(MemorySource::new(bytes)),
            LoadStrategy::Eager,
            &profile(&[]),
            &empty,
        )
        .expect_err("unknown key");
        assert_eq!(err.code, ErrorCode::LdrSignatureFail);
    }
}
