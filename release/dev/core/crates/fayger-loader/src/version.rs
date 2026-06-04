//! Negotiate(Version)：加载链的版本协商阶段。
//!
//! 基于 [`LoadPolicy`] 对 BuF 的 `schema_version` 做支持范围检查与弃用提示，对
//! `runtime_interface_min` 做最低版本检查。
//!
//! 决策顺序（schema 错误优先于 runtime 错误）：
//!
//! 1. `schema_version ∉ [low, high]` → [`fayger_error::ErrorCode::LdrSchemaUnsupported`]。
//! 2. `runtime_interface_min > provided` → [`fayger_error::ErrorCode::LdrRuntimeVersionTooHigh`]。
//! 3. `schema_version ∈ deprecated` → `Ok(Some(notice))`。
//! 4. 否则 → `Ok(None)`。

use fayger_buf::manifest::{BuFManifest, SemVer};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use std::collections::BTreeSet;

/// 加载方提供的版本策略。
///
/// `supported_schema_range` 是闭区间 `[low, high]`（含两端）。`deprecated_schemas`
/// 是仍受支持但应给出提示的 schema 版本集合（应是 `supported_schema_range` 的子集，
/// 但本类型不强制——区间外的弃用项永远不会被命中）。
#[derive(Debug, Clone)]
pub struct LoadPolicy {
    /// 支持的 schema 版本闭区间 `[low, high]`。
    pub supported_schema_range: (SemVer, SemVer),
    /// 弃用但仍支持的 schema 版本集合。
    pub deprecated_schemas: BTreeSet<SemVer>,
    /// 当前 Fayger 提供的 Runtime_Interface 版本。
    pub runtime_interface_provided: SemVer,
}

impl LoadPolicy {
    /// 构造一个策略。
    #[must_use]
    pub const fn new(
        supported_schema_range: (SemVer, SemVer),
        deprecated_schemas: BTreeSet<SemVer>,
        runtime_interface_provided: SemVer,
    ) -> Self {
        Self {
            supported_schema_range,
            deprecated_schemas,
            runtime_interface_provided,
        }
    }
}

/// 执行版本协商。
///
/// 返回 `Ok(Some(notice))` 表示加载通过且命中弃用（notice 为非空提示），
/// `Ok(None)` 表示加载通过且无弃用。
///
/// # Errors
///
/// - [`fayger_error::ErrorCode::LdrSchemaUnsupported`]：`schema_version` 不在支持区间内。
/// - [`fayger_error::ErrorCode::LdrRuntimeVersionTooHigh`]：`runtime_interface_min` 高于提供版本。
///
/// 当两者同时不满足时，优先返回 schema 错误。
pub fn negotiate_version(
    manifest: &BuFManifest,
    policy: &LoadPolicy,
) -> Result<Option<String>, FaygerError> {
    let schema = &manifest.schema_version;
    let (low, high) = &policy.supported_schema_range;

    // 1. schema 范围检查（优先）。
    if schema < low || schema > high {
        return Err(FaygerError::new(
            ErrorCode::LdrSchemaUnsupported,
            Layer::Loader,
            "schema version is outside the supported range",
        )
        .with_context("schema_version", ContextValue::string(schema.to_string()))
        .with_context(
            "expected_range",
            ContextValue::List(vec![
                ContextValue::string(low.to_string()),
                ContextValue::string(high.to_string()),
            ]),
        ));
    }

    // 2. runtime 最低版本检查。
    let required = &manifest.runtime_interface_min;
    let provided = &policy.runtime_interface_provided;
    if required > provided {
        return Err(FaygerError::new(
            ErrorCode::LdrRuntimeVersionTooHigh,
            Layer::Loader,
            "runtime_interface_min exceeds the version provided by this Fayger",
        )
        .with_context("required", ContextValue::string(required.to_string()))
        .with_context("provided", ContextValue::string(provided.to_string())));
    }

    // 3. 弃用提示。
    if policy.deprecated_schemas.contains(schema) {
        return Ok(Some(format!("schema version {schema} is deprecated")));
    }

    // 4. 正常通过。
    Ok(None)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{negotiate_version, LoadPolicy};
    use fayger_buf::manifest::{BuFManifest, EntryPoint, SemVer};
    use fayger_error::ErrorCode;
    use std::collections::{BTreeMap, BTreeSet};

    fn v(s: &str) -> SemVer {
        SemVer::parse(s).expect("parse")
    }

    fn manifest_with(schema: &str, rt_min: &str) -> BuFManifest {
        BuFManifest {
            schema_version: v(schema),
            runtime_interface_min: v(rt_min),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![],
            signature: None,
            extensions: BTreeMap::new(),
        }
    }

    fn policy() -> LoadPolicy {
        LoadPolicy::new(
            (v("1.0.0"), v("1.9.9")),
            std::iter::once(v("1.0.0")).collect(),
            v("1.2.0"),
        )
    }

    #[test]
    fn accepts_supported_non_deprecated_schema() {
        let m = manifest_with("1.5.0", "1.0.0");
        assert_eq!(negotiate_version(&m, &policy()).expect("ok"), None);
    }

    #[test]
    fn accepts_deprecated_schema_with_notice() {
        let m = manifest_with("1.0.0", "1.0.0");
        let notice = negotiate_version(&m, &policy()).expect("ok");
        assert!(notice.is_some());
        assert!(notice.unwrap().contains("1.0.0"));
    }

    #[test]
    fn rejects_schema_below_range() {
        let m = manifest_with("0.9.0", "1.0.0");
        let err = negotiate_version(&m, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrSchemaUnsupported);
        assert!(err.context.contains_key("expected_range"));
    }

    #[test]
    fn rejects_schema_above_range() {
        let m = manifest_with("2.0.0", "1.0.0");
        let err = negotiate_version(&m, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrSchemaUnsupported);
    }

    #[test]
    fn accepts_schema_at_range_boundaries() {
        let low = manifest_with("1.0.0", "1.0.0"); // 下界（恰好弃用）
        assert!(negotiate_version(&low, &policy()).expect("ok").is_some());
        let high = manifest_with("1.9.9", "1.0.0"); // 上界
        assert_eq!(negotiate_version(&high, &policy()).expect("ok"), None);
    }

    #[test]
    fn rejects_runtime_version_too_high() {
        let m = manifest_with("1.5.0", "1.5.0"); // provided 是 1.2.0
        let err = negotiate_version(&m, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrRuntimeVersionTooHigh);
        assert!(err.context.contains_key("required"));
        assert!(err.context.contains_key("provided"));
    }

    #[test]
    fn accepts_runtime_version_equal_to_provided() {
        let m = manifest_with("1.5.0", "1.2.0");
        assert_eq!(negotiate_version(&m, &policy()).expect("ok"), None);
    }

    #[test]
    fn schema_error_takes_priority_over_runtime_error() {
        // schema 超界 且 runtime 过高：应返回 schema 错误。
        let m = manifest_with("2.0.0", "9.0.0");
        let err = negotiate_version(&m, &policy()).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::LdrSchemaUnsupported);
    }
}
