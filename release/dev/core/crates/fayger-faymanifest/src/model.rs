//! FayManifest 数据模型与解析/校验（草案）。
//!
//! 这是 FayManifest 在 fayger **实现侧**的 Rust 单一事实源（机器可读 schema / 契约权威在 ifay，
//! 见 crate 顶层文档）：serde 结构 + `deny_unknown_fields` 提供结构级校验
//! （等价于 schema 的 `additionalProperties:false` 与 `required`），[`FayManifest::validate`]
//! 提供语义级校验（版本约束、命名空间、组件 id、重复检测）。
//!
//! 字段与蓝图 §13「FayManifest File Structure」表一一对应；JSON 字段名沿用蓝图的 camelCase
//! （`controlMode` / `driverPackage` / `modelSource` / `scenarioTags` / `authMethods`）。

use crate::constraint::VersionConstraint;
use crate::namespace::PackageId;
use fayger_buf::manifest::SemVer;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// 控制模式（蓝图 §13 "Control Modes"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
    /// 指令控制：每个动作都由人类原型显式驱动。
    Command,
    /// 人格控制：Ego 模型在约束边界内自主决策。
    Ego,
    /// 自主控制：完全自驱行为。
    Autonomous,
}

impl ControlMode {
    /// 稳定字符串表示（与 JSON 取值一致）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Ego => "ego",
            Self::Autonomous => "autonomous",
        }
    }
}

/// 模块引用（`modules[]`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleRef {
    /// 模块标识（snake_case）。
    pub id: String,
    /// 版本约束（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 厂商实现标识（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// 模块特定配置（自由结构，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

/// 协议引用（`protocols[]`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRef {
    /// 协议标识（snake_case）。
    pub id: String,
    /// 版本约束（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 协议特定配置（自由结构，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

/// 驱动配置（`drivers[]`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DriverRef {
    /// 驱动的人类可读名称。
    pub name: String,
    /// 驱动类型（蓝图示例为 `device`）。
    #[serde(rename = "type")]
    pub driver_type: String,
    /// 驱动包标识，形如 `@drone-drivers/dji-fc`。
    pub driver_package: String,
    /// 驱动特定配置（自由结构，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

/// Ego 模型配置（`ego`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EgoConfig {
    /// Ego 模型来源，形如 `@ego-models/drone-pilot-v1`。
    pub model_source: String,
    /// 适用场景标签。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenario_tags: Vec<String>,
    /// 约束参数（自由结构，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Value>,
}

/// 权限配置（`permissions`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PermissionsConfig {
    /// 固有权限。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherent: Vec<String>,
    /// 持久权限。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub persistent: Vec<String>,
    /// 支持的认证方式。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_methods: Vec<String>,
}

/// FayManifest 顶层结构。
///
/// `name` / `version` / `description` / `vendor` / `modules` / `protocols` / `controlMode`
/// 为必填（缺失时反序列化即失败）；`fayId` / `drivers` / `ego` / `permissions` 可选。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FayManifest {
    /// iFay 实现名称。
    pub name: String,
    /// 实现自身版本（SemVer）。
    pub version: String,
    /// 功能描述。
    pub description: String,
    /// 开发者 / 厂商名称。
    pub vendor: String,
    /// 可选 FayID 身份引用（缺省时由装配阶段补充身份基础设施）。
    #[serde(rename = "fayId", default, skip_serializing_if = "Option::is_none")]
    pub fay_id: Option<String>,
    /// 声明的业务模块列表。
    pub modules: Vec<ModuleRef>,
    /// 声明的协议列表。
    pub protocols: Vec<ProtocolRef>,
    /// 控制模式。
    #[serde(rename = "controlMode")]
    pub control_mode: ControlMode,
    /// 驱动配置（可选）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drivers: Vec<DriverRef>,
    /// Ego 模型配置（可选）。声明此字段会令装配阶段补充 `ego_model` 模块。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ego: Option<EgoConfig>,
    /// 权限配置（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,
}

impl FayManifest {
    /// 从 JSON 字符串解析（结构级校验由 serde + `deny_unknown_fields` 完成）。
    ///
    /// # Errors
    /// JSON 语法错误、缺顶层必填键、未知字段、类型不符或 `controlMode` 非法取值时返回
    /// [`ErrorCode::FmParseFail`]（`context.detail` 携带解析器错误描述）。
    pub fn from_json_str(s: &str) -> Result<Self, FaygerError> {
        serde_json::from_str(s).map_err(|e| {
            FaygerError::new(
                ErrorCode::FmParseFail,
                Layer::Assembler,
                "failed to parse FayManifest JSON",
            )
            .with_context("detail", ContextValue::string(e.to_string()))
        })
    }

    /// 解析后的实现版本（已在 [`Self::validate`] 校验合法）。
    ///
    /// # Errors
    /// `version` 非合法 SemVer 时返回 [`ErrorCode::FmValidationFailed`]。
    pub fn semver(&self) -> Result<SemVer, FaygerError> {
        SemVer::parse(&self.version).map_err(|e| {
            validation("InvalidVersion")
                .with_context("field", ContextValue::string("version"))
                .with_context("detail", ContextValue::string(e.to_string()))
        })
    }

    /// 语义校验：必填非空、版本合法、组件 id 合法且不重复、版本约束与命名空间合法。
    ///
    /// # Errors
    /// 见各 `Fm*` 错误码：[`ErrorCode::FmValidationFailed`]、
    /// [`ErrorCode::FmInvalidVersionConstraint`]、[`ErrorCode::FmInvalidNamespace`]。
    pub fn validate(&self) -> Result<(), FaygerError> {
        // 1. 必填字符串非空。
        for (field, val) in [
            ("name", &self.name),
            ("version", &self.version),
            ("description", &self.description),
            ("vendor", &self.vendor),
        ] {
            if val.trim().is_empty() {
                return Err(validation("EmptyRequiredField")
                    .with_context("field", ContextValue::string(field)));
            }
        }

        // 2. 顶层 version 是合法 SemVer。
        self.semver()?;

        // 3. 模块 / 协议：id 合法、不重复、版本约束合法。
        validate_component_ids(self.modules.iter().map(|m| m.id.as_str()), "module")?;
        validate_component_ids(self.protocols.iter().map(|p| p.id.as_str()), "protocol")?;

        for m in &self.modules {
            if let Some(c) = &m.version {
                parse_constraint(c, &m.id)?;
            }
        }
        for p in &self.protocols {
            if let Some(c) = &p.version {
                parse_constraint(c, &p.id)?;
            }
        }

        // 4. 驱动包与 Ego 模型来源是合法命名空间包标识。
        for d in &self.drivers {
            if d.name.trim().is_empty() {
                return Err(validation("EmptyRequiredField")
                    .with_context("field", ContextValue::string("drivers[].name")));
            }
            if d.driver_type.trim().is_empty() {
                return Err(validation("EmptyRequiredField")
                    .with_context("field", ContextValue::string("drivers[].type")));
            }
            PackageId::parse(&d.driver_package).map_err(|e| {
                e.with_context("field", ContextValue::string("drivers[].driverPackage"))
            })?;
        }
        if let Some(ego) = &self.ego {
            PackageId::parse(&ego.model_source)
                .map_err(|e| e.with_context("field", ContextValue::string("ego.modelSource")))?;
        }

        Ok(())
    }
}

/// 组件 id 文法：`^[a-z][a-z0-9_]*$`。
#[must_use]
pub fn is_valid_component_id(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// 校验一组组件 id：每个合法且整体不重复。
fn validate_component_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    kind: &str,
) -> Result<(), FaygerError> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for id in ids {
        if !is_valid_component_id(id) {
            return Err(validation("InvalidComponentId")
                .with_context("kind", ContextValue::string(kind))
                .with_context("id", ContextValue::string(id)));
        }
        if !seen.insert(id) {
            return Err(validation("DuplicateComponent")
                .with_context("kind", ContextValue::string(kind))
                .with_context("id", ContextValue::string(id)));
        }
    }
    Ok(())
}

fn parse_constraint(c: &str, component: &str) -> Result<VersionConstraint, FaygerError> {
    VersionConstraint::parse(c)
        .map_err(|e| e.with_context("component", ContextValue::string(component)))
}

fn validation(reason: &str) -> FaygerError {
    FaygerError::new(
        ErrorCode::FmValidationFailed,
        Layer::Assembler,
        "FayManifest semantic validation failed",
    )
    .with_context("reason", ContextValue::string(reason))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{ControlMode, FayManifest};
    use fayger_error::ErrorCode;

    const MINIMAL: &str = r#"{
        "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
        "modules": [], "protocols": [], "controlMode": "command"
    }"#;

    #[test]
    fn parses_minimal_manifest() {
        let m = FayManifest::from_json_str(MINIMAL).expect("parse");
        assert_eq!(m.name, "x");
        assert_eq!(m.control_mode, ControlMode::Command);
        m.validate().expect("valid");
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let s = r#"{
            "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
            "modules": [], "protocols": [], "controlMode": "command", "bogus": 1
        }"#;
        let e = FayManifest::from_json_str(s).expect_err("unknown field");
        assert_eq!(e.code, ErrorCode::FmParseFail);
    }

    #[test]
    fn rejects_missing_required_key() {
        let s = r#"{ "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [], "protocols": [] }"#;
        let e = FayManifest::from_json_str(s).expect_err("missing controlMode");
        assert_eq!(e.code, ErrorCode::FmParseFail);
    }

    #[test]
    fn rejects_bad_control_mode() {
        let s = r#"{ "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [], "protocols": [], "controlMode": "telepathic" }"#;
        let e = FayManifest::from_json_str(s).expect_err("bad mode");
        assert_eq!(e.code, ErrorCode::FmParseFail);
    }

    #[test]
    fn validate_rejects_empty_required_field() {
        let s = r#"{ "name": "  ", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [], "protocols": [], "controlMode": "ego" }"#;
        let m = FayManifest::from_json_str(s).expect("parse");
        let e = m.validate().expect_err("empty name");
        assert_eq!(e.code, ErrorCode::FmValidationFailed);
    }

    #[test]
    fn validate_rejects_invalid_top_version() {
        let s = r#"{ "name": "x", "version": "1.0", "description": "d", "vendor": "v",
                     "modules": [], "protocols": [], "controlMode": "ego" }"#;
        let m = FayManifest::from_json_str(s).expect("parse");
        assert_eq!(
            m.validate().expect_err("bad version").code,
            ErrorCode::FmValidationFailed
        );
    }

    #[test]
    fn validate_rejects_duplicate_module() {
        let s = r#"{ "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [{"id":"sensor"},{"id":"sensor"}], "protocols": [],
                     "controlMode": "ego" }"#;
        let m = FayManifest::from_json_str(s).expect("parse");
        assert_eq!(
            m.validate().expect_err("dup").code,
            ErrorCode::FmValidationFailed
        );
    }

    #[test]
    fn validate_rejects_bad_component_id() {
        let s = r#"{ "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [{"id":"Sensor"}], "protocols": [], "controlMode": "ego" }"#;
        let m = FayManifest::from_json_str(s).expect("parse");
        assert_eq!(
            m.validate().expect_err("bad id").code,
            ErrorCode::FmValidationFailed
        );
    }

    #[test]
    fn validate_rejects_bad_version_constraint() {
        let s = r#"{ "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [{"id":"sensor","version":"not-a-range"}], "protocols": [],
                     "controlMode": "ego" }"#;
        let m = FayManifest::from_json_str(s).expect("parse");
        assert_eq!(
            m.validate().expect_err("bad constraint").code,
            ErrorCode::FmInvalidVersionConstraint
        );
    }

    #[test]
    fn validate_rejects_bad_driver_package_namespace() {
        let s = r#"{ "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
                     "modules": [], "protocols": [], "controlMode": "command",
                     "drivers": [{"name":"n","type":"device","driverPackage":"no-scope"}] }"#;
        let m = FayManifest::from_json_str(s).expect("parse");
        assert_eq!(
            m.validate().expect_err("bad ns").code,
            ErrorCode::FmInvalidNamespace
        );
    }

    #[test]
    fn is_valid_component_id_rules() {
        assert!(super::is_valid_component_id("device_driver_hub"));
        assert!(super::is_valid_component_id("a"));
        assert!(!super::is_valid_component_id(""));
        assert!(!super::is_valid_component_id("1abc"));
        assert!(!super::is_valid_component_id("Abc"));
        assert!(!super::is_valid_component_id("a-b"));
    }
}
