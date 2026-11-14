//! 命名空间限定的包标识（FayManifest 草案）。
//!
//! 蓝图 §13 用 npm 风格的作用域包名表达驱动包与 Ego 模型来源：
//!
//! - `@drone-drivers/dji-fc`、`@drone-drivers/gimbal-generic`（驱动包命名空间）
//! - `@ego-models/drone-pilot-v1`（Ego 模型命名空间）
//!
//! 形如 `@scope/name`。本模块提供 [`PackageId`] 解析/校验与已知命名空间识别。
//!
//! # 与体系 token 分隔符约定的关系
//!
//! 组织宪章规定**凭证/标识 token** 的分隔符统一用 `_`。包标识不是凭证 token，而是沿用
//! 蓝图既定的 npm 作用域格式（kebab-case 作用域 + kebab/点分名称）。两者是不同范畴，
//! 本草案遵循蓝图。注册表正式设计（H2）可再行收敛。

use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 蓝图约定的驱动包命名空间作用域（不含前导 `@`）。
pub const SCOPE_DRONE_DRIVERS: &str = "drone-drivers";
/// 蓝图约定的 Ego 模型命名空间作用域（不含前导 `@`）。
pub const SCOPE_EGO_MODELS: &str = "ego-models";

/// 已识别的命名空间分类。
///
/// 标记为 `#[non_exhaustive]`：注册表正式设计（H2）会追加更多受治理的命名空间。
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Namespace {
    /// `@drone-drivers/...`：驱动包。
    DroneDrivers,
    /// `@ego-models/...`：Ego 模型。
    EgoModels,
    /// 其它作用域：草案阶段允许，但标注为「未受治理（ungoverned）」。
    Other(String),
}

impl Namespace {
    /// 由作用域字符串（不含 `@`）分类。
    #[must_use]
    pub fn classify(scope: &str) -> Self {
        match scope {
            SCOPE_DRONE_DRIVERS => Self::DroneDrivers,
            SCOPE_EGO_MODELS => Self::EgoModels,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 该命名空间是否属于蓝图明确治理的命名空间（H1 草案：仅上述两个）。
    #[must_use]
    pub const fn is_governed(&self) -> bool {
        matches!(self, Self::DroneDrivers | Self::EgoModels)
    }

    /// 作用域字符串（不含 `@`）。
    #[must_use]
    pub fn scope(&self) -> &str {
        match self {
            Self::DroneDrivers => SCOPE_DRONE_DRIVERS,
            Self::EgoModels => SCOPE_EGO_MODELS,
            Self::Other(s) => s,
        }
    }
}

/// 一个命名空间限定的包标识：`@scope/name`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageId {
    scope: String,
    name: String,
}

impl PackageId {
    /// 解析并校验 `@scope/name`。
    ///
    /// 文法（与 schema `packageIdentifier` 一致）：
    ///
    /// ```text
    /// package = "@" segment "/" name
    /// segment = [a-z0-9] ( [a-z0-9-]* [a-z0-9] )?
    /// name    = [a-z0-9] ( [a-z0-9._-]* [a-z0-9] )?
    /// ```
    ///
    /// # Errors
    /// 不符合上述文法时返回 [`ErrorCode::FmInvalidNamespace`]（`context.value` 为原始串）。
    pub fn parse(s: &str) -> Result<Self, FaygerError> {
        let rest = s
            .strip_prefix('@')
            .ok_or_else(|| invalid(s, "missing leading '@'"))?;
        let (scope, name) = rest
            .split_once('/')
            .ok_or_else(|| invalid(s, "missing '/' separator"))?;

        if !is_valid_scope(scope) {
            return Err(invalid(s, "scope segment is not a valid identifier"));
        }
        if !is_valid_name(name) {
            return Err(invalid(s, "name segment is not a valid identifier"));
        }
        Ok(Self {
            scope: scope.to_owned(),
            name: name.to_owned(),
        })
    }

    /// 作用域（不含 `@`）。
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// 名称段。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 该包所属的命名空间分类。
    #[must_use]
    pub fn namespace(&self) -> Namespace {
        Namespace::classify(&self.scope)
    }

    /// 还原为 `@scope/name` 字符串。
    #[must_use]
    pub fn to_identifier(&self) -> String {
        format!("@{}/{}", self.scope, self.name)
    }
}

fn invalid(value: &str, reason: &str) -> FaygerError {
    FaygerError::new(
        ErrorCode::FmInvalidNamespace,
        Layer::Assembler,
        "invalid namespace-scoped package identifier",
    )
    .with_context("value", ContextValue::string(value))
    .with_context("reason", ContextValue::string(reason))
}

/// 作用域段：`[a-z0-9]([a-z0-9-]*[a-z0-9])?`。
fn is_valid_scope(s: &str) -> bool {
    is_valid_segment(s, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
    })
}

/// 名称段：`[a-z0-9]([a-z0-9._-]*[a-z0-9])?`。
fn is_valid_name(s: &str) -> bool {
    is_valid_segment(s, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-'
    })
}

/// 通用段校验：非空；首尾为 `[a-z0-9]`；中间字符由 `mid` 判定。
fn is_valid_segment(s: &str, mid: impl Fn(char) -> bool) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0] as char;
    let last = bytes[bytes.len() - 1] as char;
    let edge_ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    if !edge_ok(first) || !edge_ok(last) {
        return false;
    }
    s.chars().all(mid)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{Namespace, PackageId, SCOPE_DRONE_DRIVERS, SCOPE_EGO_MODELS};
    use fayger_error::ErrorCode;

    #[test]
    fn parses_blueprint_driver_package() {
        let p = PackageId::parse("@drone-drivers/dji-fc").expect("valid");
        assert_eq!(p.scope(), SCOPE_DRONE_DRIVERS);
        assert_eq!(p.name(), "dji-fc");
        assert_eq!(p.namespace(), Namespace::DroneDrivers);
        assert!(p.namespace().is_governed());
        assert_eq!(p.to_identifier(), "@drone-drivers/dji-fc");
    }

    #[test]
    fn parses_blueprint_ego_model_source() {
        let p = PackageId::parse("@ego-models/drone-pilot-v1").expect("valid");
        assert_eq!(p.scope(), SCOPE_EGO_MODELS);
        assert_eq!(p.name(), "drone-pilot-v1");
        assert_eq!(p.namespace(), Namespace::EgoModels);
    }

    #[test]
    fn unknown_scope_is_other_and_ungoverned() {
        let p = PackageId::parse("@acme/widget").expect("valid");
        assert_eq!(p.namespace(), Namespace::Other("acme".to_owned()));
        assert!(!p.namespace().is_governed());
    }

    #[test]
    fn rejects_missing_at() {
        let e = PackageId::parse("drone-drivers/dji-fc").expect_err("no @");
        assert_eq!(e.code, ErrorCode::FmInvalidNamespace);
    }

    #[test]
    fn rejects_missing_slash() {
        assert!(PackageId::parse("@drone-drivers").is_err());
    }

    #[test]
    fn rejects_empty_segments() {
        assert!(PackageId::parse("@/dji-fc").is_err());
        assert!(PackageId::parse("@drone-drivers/").is_err());
    }

    #[test]
    fn rejects_uppercase_and_edges() {
        assert!(PackageId::parse("@Drone/dji").is_err());
        assert!(PackageId::parse("@-drone/dji").is_err());
        assert!(PackageId::parse("@drone-/dji").is_err());
        assert!(PackageId::parse("@drone/-dji").is_err());
        assert!(PackageId::parse("@drone/dji-").is_err());
    }

    #[test]
    fn allows_dots_and_underscores_in_name_middle() {
        assert!(PackageId::parse("@scope/a.b_c-d").is_ok());
    }
}
