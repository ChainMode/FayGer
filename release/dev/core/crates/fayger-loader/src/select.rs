//! 按运行环境的 Section 选择（Profile-Driven Partial Loading）。
//!
//! [`LoadProfile`] 描述运行环境；[`select_sections`] 基于每个 Section 的 `profile_constraints`
//! 确定性地决定哪些被选中、哪些被跳过。`Required` 缺失即整体失败；`Optional` 不满足时跳过降级。

use fayger_buf::manifest::types::{SectionDescriptor, SectionVisibility};
use fayger_buf::manifest::BuFManifest;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use std::collections::BTreeSet;

/// 运行环境画像。
///
/// `target_class` 是描述性字段（M6 不据它选择，见需求 §9.6）；选择约束来自
/// `capabilities` / `features` / `max_section_bytes`。
#[derive(Debug, Clone, Default)]
pub struct LoadProfile {
    /// 目标终端类别（描述性，如 `desktop` / `drone` / `camera`）。
    pub target_class: String,
    /// 当前终端可用能力集。
    pub capabilities: BTreeSet<String>,
    /// 单段允许读入的体积上限（字节）。`None` 表示无上限。
    pub max_section_bytes: Option<u64>,
    /// 运行时特征位集合。
    pub features: BTreeSet<String>,
}

impl LoadProfile {
    /// 构造一个画像。
    #[must_use]
    pub fn new(
        target_class: impl Into<String>,
        capabilities: BTreeSet<String>,
        max_section_bytes: Option<u64>,
        features: BTreeSet<String>,
    ) -> Self {
        Self {
            target_class: target_class.into(),
            capabilities,
            max_section_bytes,
            features,
        }
    }
}

/// 跳过原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason {
    /// 该 Section 要求的能力未在 profile 中提供。
    CapabilityNotGranted,
    /// 该 Section 要求的特征未在 profile 中提供。
    FeatureMissing,
    /// 该 Section 体积超过 section/profile 声明的上限。
    OverMaxSize,
}

impl SkipReason {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapabilityNotGranted => "CapabilityNotGranted",
            Self::FeatureMissing => "FeatureMissing",
            Self::OverMaxSize => "OverMaxSize",
        }
    }
}

/// 被跳过的 Optional Section 记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipRecord {
    /// 被跳过的 Section id。
    pub section_id: String,
    /// 非空的跳过原因集合。
    pub reasons: BTreeSet<SkipReason>,
}

/// 选择结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionResult {
    /// 被选中 Section 的 id（按 manifest 顺序）。
    pub selected: Vec<String>,
    /// 被跳过的 Optional Section（按 manifest 顺序）。
    pub skipped: Vec<SkipRecord>,
}

/// 计算某 Section 在给定 profile 下未满足的约束原因集合。空集表示 fits。
#[must_use]
pub fn unmet_reasons(section: &SectionDescriptor, profile: &LoadProfile) -> BTreeSet<SkipReason> {
    let mut reasons = BTreeSet::new();
    let c = &section.profile_constraints;

    if !c.required_capabilities.is_subset(&profile.capabilities) {
        reasons.insert(SkipReason::CapabilityNotGranted);
    }
    if !c.required_features.is_subset(&profile.features) {
        reasons.insert(SkipReason::FeatureMissing);
    }
    let over_section_max = c.max_size.is_some_and(|m| section.length > m);
    let over_profile_max = profile
        .max_section_bytes
        .is_some_and(|p| section.length > p);
    if over_section_max || over_profile_max {
        reasons.insert(SkipReason::OverMaxSize);
    }
    reasons
}

/// 按 profile 选择 Section。
///
/// 被选中集合 = `{ s : unmet_reasons(s, profile) = ∅ }`（manifest 顺序）。
///
/// # Errors
/// 若存在 `fits` 为假的 `Required` Section，返回 `LDR_PROFILE_REQUIRED_SECTION_MISSING`
/// （按 manifest 顺序的第一个），`context` 含 `section_id`、`unmet`（原因列表）、`phase=eager`。
pub fn select_sections(
    manifest: &BuFManifest,
    profile: &LoadProfile,
) -> Result<SelectionResult, FaygerError> {
    let mut selected = Vec::new();
    let mut skipped = Vec::new();

    for sd in &manifest.sections {
        let reasons = unmet_reasons(sd, profile);
        if reasons.is_empty() {
            selected.push(sd.id.clone());
        } else if sd.visibility == SectionVisibility::Required {
            let unmet: Vec<ContextValue> = reasons
                .iter()
                .map(|r| ContextValue::string(r.as_str()))
                .collect();
            return Err(FaygerError::new(
                ErrorCode::LdrProfileRequiredSectionMissing,
                Layer::Loader,
                "a required section does not satisfy the load profile",
            )
            .with_context("phase", ContextValue::string("eager"))
            .with_context("section_id", ContextValue::string(sd.id.clone()))
            .with_context("unmet", ContextValue::List(unmet)));
        } else {
            skipped.push(SkipRecord {
                section_id: sd.id.clone(),
                reasons,
            });
        }
    }

    Ok(SelectionResult { selected, skipped })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{select_sections, unmet_reasons, LoadProfile, SkipReason};
    use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
    use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
    use fayger_error::ErrorCode;
    use std::collections::{BTreeMap, BTreeSet};

    fn caps(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn section(
        id: &str,
        length: u64,
        visibility: SectionVisibility,
        constraints: ProfileConstraints,
    ) -> SectionDescriptor {
        SectionDescriptor {
            id: id.into(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length,
            visibility,
            profile_constraints: constraints,
        }
    }

    fn manifest(sections: Vec<SectionDescriptor>) -> BuFManifest {
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

    fn profile(caps_: &[&str], max: Option<u64>, features: &[&str]) -> LoadProfile {
        LoadProfile::new(
            "test",
            caps(caps_),
            max,
            features.iter().map(|s| (*s).to_string()).collect(),
        )
    }

    #[test]
    fn no_constraints_always_fits() {
        let s = section(
            "a",
            100,
            SectionVisibility::Required,
            ProfileConstraints::default(),
        );
        assert!(unmet_reasons(&s, &profile(&[], None, &[])).is_empty());
    }

    #[test]
    fn capability_missing_reason() {
        let s = section(
            "a",
            10,
            SectionVisibility::Optional,
            ProfileConstraints {
                required_capabilities: caps(&["gpu"]),
                max_size: None,
                required_features: BTreeSet::new(),
            },
        );
        let r = unmet_reasons(&s, &profile(&[], None, &[]));
        assert!(r.contains(&SkipReason::CapabilityNotGranted));
    }

    #[test]
    fn feature_missing_reason() {
        let s = section(
            "a",
            10,
            SectionVisibility::Optional,
            ProfileConstraints {
                required_capabilities: BTreeSet::new(),
                max_size: None,
                required_features: caps(&["realtime"]),
            },
        );
        let r = unmet_reasons(&s, &profile(&[], None, &[]));
        assert!(r.contains(&SkipReason::FeatureMissing));
    }

    #[test]
    fn over_max_size_section_and_profile() {
        // section.max_size
        let s = section(
            "a",
            100,
            SectionVisibility::Optional,
            ProfileConstraints {
                required_capabilities: BTreeSet::new(),
                max_size: Some(10),
                required_features: BTreeSet::new(),
            },
        );
        assert!(unmet_reasons(&s, &profile(&[], None, &[])).contains(&SkipReason::OverMaxSize));
        // profile.max_section_bytes
        let s2 = section(
            "b",
            100,
            SectionVisibility::Optional,
            ProfileConstraints::default(),
        );
        assert!(unmet_reasons(&s2, &profile(&[], Some(10), &[])).contains(&SkipReason::OverMaxSize));
    }

    #[test]
    fn select_all_when_all_fit() {
        let m = manifest(vec![
            section(
                "a",
                4,
                SectionVisibility::Required,
                ProfileConstraints::default(),
            ),
            section(
                "b",
                2,
                SectionVisibility::Optional,
                ProfileConstraints::default(),
            ),
        ]);
        let sel = select_sections(&m, &profile(&[], None, &[])).expect("ok");
        assert_eq!(sel.selected, vec!["a", "b"]);
        assert!(sel.skipped.is_empty());
    }

    #[test]
    fn optional_unmet_is_skipped_with_reasons() {
        let m = manifest(vec![
            section(
                "a",
                4,
                SectionVisibility::Required,
                ProfileConstraints::default(),
            ),
            section(
                "gpu",
                2,
                SectionVisibility::Optional,
                ProfileConstraints {
                    required_capabilities: BTreeSet::new(),
                    max_size: None,
                    required_features: caps(&["gpu"]),
                },
            ),
        ]);
        let sel = select_sections(&m, &profile(&[], None, &[])).expect("ok");
        assert_eq!(sel.selected, vec!["a"]);
        assert_eq!(sel.skipped.len(), 1);
        assert_eq!(sel.skipped[0].section_id, "gpu");
        assert!(sel.skipped[0].reasons.contains(&SkipReason::FeatureMissing));
    }

    #[test]
    fn required_unmet_fails() {
        let m = manifest(vec![section(
            "core",
            4,
            SectionVisibility::Required,
            ProfileConstraints {
                required_capabilities: caps(&["secure-enclave"]),
                max_size: None,
                required_features: BTreeSet::new(),
            },
        )]);
        let err = select_sections(&m, &profile(&[], None, &[])).expect_err("fail");
        assert_eq!(err.code, ErrorCode::LdrProfileRequiredSectionMissing);
        assert!(err.context.contains_key("section_id"));
        assert!(err.context.contains_key("unmet"));
    }

    #[test]
    fn first_required_failure_is_reported() {
        let m = manifest(vec![
            section(
                "r1",
                4,
                SectionVisibility::Required,
                ProfileConstraints {
                    required_capabilities: caps(&["x"]),
                    max_size: None,
                    required_features: BTreeSet::new(),
                },
            ),
            section(
                "r2",
                4,
                SectionVisibility::Required,
                ProfileConstraints {
                    required_capabilities: caps(&["y"]),
                    max_size: None,
                    required_features: BTreeSet::new(),
                },
            ),
        ]);
        let err = select_sections(&m, &profile(&[], None, &[])).expect_err("fail");
        assert!(matches!(
            err.context.get("section_id"),
            Some(fayger_error::ContextValue::String(s)) if s == "r1"
        ));
    }

    #[test]
    fn empty_manifest_selects_nothing() {
        let m = manifest(vec![]);
        let sel = select_sections(&m, &profile(&[], None, &[])).expect("ok");
        assert!(sel.selected.is_empty());
        assert!(sel.skipped.is_empty());
    }

    #[test]
    fn selection_is_deterministic() {
        let m = manifest(vec![
            section(
                "a",
                4,
                SectionVisibility::Required,
                ProfileConstraints::default(),
            ),
            section(
                "gpu",
                2,
                SectionVisibility::Optional,
                ProfileConstraints {
                    required_capabilities: BTreeSet::new(),
                    max_size: None,
                    required_features: caps(&["gpu"]),
                },
            ),
        ]);
        let p = profile(&[], None, &[]);
        assert_eq!(
            select_sections(&m, &p).unwrap(),
            select_sections(&m, &p).unwrap()
        );
    }
}
