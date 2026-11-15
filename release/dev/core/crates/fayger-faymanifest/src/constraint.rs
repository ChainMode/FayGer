//! 版本约束解析与语义（FayManifest 草案）。
//!
//! FayManifest 的 `modules[].version` / `protocols[].version` 是版本**约束**（不是单一版本）。
//! 语义沿用业界 npm/Cargo 习惯子集，建立在 [`fayger_buf::manifest::SemVer`]（解析 + 全序）之上：
//!
//! | 写法 | 含义 |
//! |------|------|
//! | `*` | 任意版本 |
//! | `1.2.3` / `=1.2.3` | 精确匹配 |
//! | `^1.2.3` | 兼容更新：`>=1.2.3 <2.0.0`（保留最左非零位） |
//! | `~1.2.3` | 近似更新：`>=1.2.3 <1.3.0`（固定 major.minor） |
//! | `>=1.2.3` `>1.2.3` `<=1.2.3` `<1.2.3` | 比较符 |
//!
//! 预发布版本（如 `1.0.0-rc.1`）按 [`SemVer`] 的全序参与比较；本草案不实现 npm
//! 「同基版本才纳入预发布」的复杂规则，留待注册表正式设计（H2）细化。

use fayger_buf::manifest::SemVer;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 一条版本约束。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionConstraint {
    /// `*`：任意版本。
    Any,
    /// 精确版本（`1.2.3` 或 `=1.2.3`）。
    Exact(SemVer),
    /// 插入符 `^`：保留最左非零位的兼容更新。
    Caret(SemVer),
    /// 波浪号 `~`：固定 major.minor 的近似更新。
    Tilde(SemVer),
    /// `>=`。
    GreaterEq(SemVer),
    /// `>`。
    Greater(SemVer),
    /// `<=`。
    LessEq(SemVer),
    /// `<`。
    Less(SemVer),
}

impl VersionConstraint {
    /// 解析一条版本约束字符串。
    ///
    /// # Errors
    /// 空串、未知操作符或版本核非合法 SemVer 时返回 [`ErrorCode::FmInvalidVersionConstraint`]
    /// （`context.constraint` 携带原始串）。
    #[allow(clippy::option_if_let_else)]
    pub fn parse(s: &str) -> Result<Self, FaygerError> {
        let t = s.trim();
        if t.is_empty() {
            return Err(invalid(s, "empty constraint"));
        }
        if t == "*" {
            return Ok(Self::Any);
        }
        // 顺序敏感：先匹配两字符操作符（>= <=），再单字符。
        let (ctor, rest): (fn(SemVer) -> Self, &str) = if let Some(r) = t.strip_prefix(">=") {
            (Self::GreaterEq, r)
        } else if let Some(r) = t.strip_prefix("<=") {
            (Self::LessEq, r)
        } else if let Some(r) = t.strip_prefix('>') {
            (Self::Greater, r)
        } else if let Some(r) = t.strip_prefix('<') {
            (Self::Less, r)
        } else if let Some(r) = t.strip_prefix('^') {
            (Self::Caret, r)
        } else if let Some(r) = t.strip_prefix('~') {
            (Self::Tilde, r)
        } else if let Some(r) = t.strip_prefix('=') {
            (Self::Exact, r)
        } else {
            (Self::Exact, t)
        };

        let core = SemVer::parse(rest.trim())
            .map_err(|e| invalid(s, &format!("invalid version core: {e}")))?;
        Ok(ctor(core))
    }

    /// 判断给定版本是否满足本约束（遵循 npm 风格同基版本预发布规则，见 ADR 0005）。
    #[must_use]
    pub fn satisfies(&self, v: &SemVer) -> bool {
        match self {
            Self::Any => v.pre.is_none(),
            Self::Exact(base) => v == base,
            Self::GreaterEq(base) => prerelease_allowed(base, v) && v >= base,
            Self::Greater(base) => prerelease_allowed(base, v) && v > base,
            Self::LessEq(base) => prerelease_allowed(base, v) && v <= base,
            Self::Less(base) => prerelease_allowed(base, v) && v < base,
            Self::Caret(base) => prerelease_allowed(base, v) && v >= base && v < &caret_upper(base),
            Self::Tilde(base) => prerelease_allowed(base, v) && v >= base && v < &tilde_upper(base),
        }
    }

    /// 规范化的可读表示（用于报告输出）。
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Any => "*".to_owned(),
            Self::Exact(v) => v.to_string(),
            Self::Caret(v) => format!("^{v}"),
            Self::Tilde(v) => format!("~{v}"),
            Self::GreaterEq(v) => format!(">={v}"),
            Self::Greater(v) => format!(">{v}"),
            Self::LessEq(v) => format!("<={v}"),
            Self::Less(v) => format!("<{v}"),
        }
    }
}

/// 仅当候选非预发布，或约束基版本本身带预发布且共享 `(major, minor, patch)` 时允许匹配。
const fn prerelease_allowed(base: &SemVer, v: &SemVer) -> bool {
    if v.pre.is_none() {
        return true;
    }
    base.pre.is_some() && base.major == v.major && base.minor == v.minor && base.patch == v.patch
}

/// `^` 的排他上界：保留最左非零位。
const fn caret_upper(b: &SemVer) -> SemVer {
    let (major, minor, patch) = if b.major > 0 {
        (b.major.saturating_add(1), 0, 0)
    } else if b.minor > 0 {
        (0, b.minor.saturating_add(1), 0)
    } else {
        (0, 0, b.patch.saturating_add(1))
    };
    SemVer {
        major,
        minor,
        patch,
        pre: None,
    }
}

/// `~` 的排他上界：固定 major.minor，下一个 minor。
const fn tilde_upper(b: &SemVer) -> SemVer {
    SemVer {
        major: b.major,
        minor: b.minor.saturating_add(1),
        patch: 0,
        pre: None,
    }
}

fn invalid(value: &str, reason: &str) -> FaygerError {
    FaygerError::new(
        ErrorCode::FmInvalidVersionConstraint,
        Layer::Assembler,
        "invalid version constraint",
    )
    .with_context("constraint", ContextValue::string(value))
    .with_context("reason", ContextValue::string(reason))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::VersionConstraint as VC;
    use fayger_buf::manifest::SemVer;

    fn v(s: &str) -> SemVer {
        SemVer::parse(s).expect("semver")
    }

    #[test]
    fn star_is_any() {
        assert_eq!(VC::parse("*").expect("ok"), VC::Any);
        assert!(VC::Any.satisfies(&v("0.0.1")));
        assert!(VC::Any.satisfies(&v("99.0.0")));
    }

    #[test]
    fn exact_with_and_without_equals() {
        let a = VC::parse("1.2.3").expect("ok");
        let b = VC::parse("=1.2.3").expect("ok");
        assert_eq!(a, b);
        assert!(a.satisfies(&v("1.2.3")));
        assert!(!a.satisfies(&v("1.2.4")));
    }

    #[test]
    fn caret_keeps_leftmost_nonzero_major() {
        let c = VC::parse("^1.2.3").expect("ok");
        assert!(c.satisfies(&v("1.2.3")));
        assert!(c.satisfies(&v("1.9.9")));
        assert!(!c.satisfies(&v("2.0.0")));
        assert!(!c.satisfies(&v("1.2.2")));
    }

    #[test]
    fn caret_zero_major_locks_minor() {
        let c = VC::parse("^0.2.3").expect("ok");
        assert!(c.satisfies(&v("0.2.3")));
        assert!(c.satisfies(&v("0.2.9")));
        assert!(!c.satisfies(&v("0.3.0")));
    }

    #[test]
    fn caret_zero_zero_locks_patch() {
        let c = VC::parse("^0.0.3").expect("ok");
        assert!(c.satisfies(&v("0.0.3")));
        assert!(!c.satisfies(&v("0.0.4")));
    }

    #[test]
    fn tilde_locks_major_minor() {
        let t = VC::parse("~1.2.3").expect("ok");
        assert!(t.satisfies(&v("1.2.3")));
        assert!(t.satisfies(&v("1.2.99")));
        assert!(!t.satisfies(&v("1.3.0")));
        assert!(!t.satisfies(&v("1.2.2")));
    }

    #[test]
    fn comparators() {
        assert!(VC::parse(">=1.2.3").expect("ok").satisfies(&v("1.2.3")));
        assert!(VC::parse(">=1.2.3").expect("ok").satisfies(&v("2.0.0")));
        assert!(!VC::parse(">1.2.3").expect("ok").satisfies(&v("1.2.3")));
        assert!(VC::parse("<=1.2.3").expect("ok").satisfies(&v("1.2.3")));
        assert!(VC::parse("<1.2.3").expect("ok").satisfies(&v("1.2.2")));
        assert!(!VC::parse("<1.2.3").expect("ok").satisfies(&v("1.2.3")));
    }

    #[test]
    fn rejects_empty_and_garbage() {
        assert!(VC::parse("").is_err());
        assert!(VC::parse("^").is_err());
        assert!(VC::parse(">=x.y.z").is_err());
        assert!(VC::parse("1.2").is_err());
    }

    #[test]
    fn render_round_trips_operator() {
        assert_eq!(VC::parse("^1.0.0").expect("ok").render(), "^1.0.0");
        assert_eq!(VC::parse("~2.3.4").expect("ok").render(), "~2.3.4");
        assert_eq!(VC::parse(">=1.0.0").expect("ok").render(), ">=1.0.0");
        assert_eq!(VC::parse("*").expect("ok").render(), "*");
    }

    #[test]
    fn npm_style_prerelease_gating() {
        let stable_caret = VC::parse("^1.0.0").expect("ok");
        assert!(!stable_caret.satisfies(&v("1.0.0-rc.1")));
        assert!(!stable_caret.satisfies(&v("1.2.0-alpha.1")));
        assert!(stable_caret.satisfies(&v("1.2.0")));

        let pre_caret = VC::parse("^1.2.0-alpha.1").expect("ok");
        assert!(pre_caret.satisfies(&v("1.2.0-alpha.1")));
        assert!(pre_caret.satisfies(&v("1.2.0-alpha.2")));
        assert!(pre_caret.satisfies(&v("1.2.0")));
        assert!(pre_caret.satisfies(&v("1.3.0")));
        assert!(!pre_caret.satisfies(&v("1.3.0-alpha.1")));

        assert!(!VC::Any.satisfies(&v("1.0.0-alpha.1")));
        assert!(VC::parse("1.0.0-alpha.1")
            .expect("ok")
            .satisfies(&v("1.0.0-alpha.1")));
    }
}
