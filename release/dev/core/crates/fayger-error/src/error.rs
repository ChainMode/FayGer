//! `FaygerError`：蓝图 §6.1 的统一错误对象。
//!
//! ```text
//! struct FaygerError {
//!   code: ErrorCode,
//!   source_layer: Layer,
//!   message: String,
//!   context: Map<String, ContextValue>,
//!   cause: Option<Box<FaygerError>>,
//! }
//! ```

use crate::{ContextValue, ErrorCode, Layer};
use std::collections::BTreeMap;
use std::fmt;

/// Fayger 统一错误对象。
///
/// 字段语义参见蓝图 §6.1。结构通过 `with_context` 与 `with_cause` 链式扩展，
/// 由 `code` 与 `source_layer` 决定语义；`message` 是面向人类的简短描述；
/// `context` 是面向工具的结构化字段；`cause` 用于跨层错误的因果链。
///
/// # Examples
///
/// ```
/// use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
///
/// let err = FaygerError::new(
///     ErrorCode::LdrParseFail,
///     Layer::Loader,
///     "magic bytes do not match",
/// )
/// .with_context("reason", ContextValue::string("MagicMismatch"))
/// .with_context("offset", ContextValue::U64(0));
///
/// assert_eq!(err.code, ErrorCode::LdrParseFail);
/// assert_eq!(err.source_layer, Layer::Loader);
/// ```
#[derive(Debug)]
pub struct FaygerError {
    /// 稳定错误码。
    pub code: ErrorCode,
    /// 错误最初产生的层。
    pub source_layer: Layer,
    /// 人类可读的简短描述。**不应**重复 `code` 字段已经表达的信息。
    pub message: String,
    /// 工具友好的结构化键值对。键有序（`BTreeMap`），便于稳定输出。
    pub context: BTreeMap<String, ContextValue>,
    /// 跨层因果链中的下层错误。M0 阶段总为 `None`。
    pub cause: Option<Box<Self>>,
}

impl FaygerError {
    /// 构造新错误。`message` 应保持简短；细节请放进 `context`。
    pub fn new(code: ErrorCode, source_layer: Layer, message: impl Into<String>) -> Self {
        Self {
            code,
            source_layer,
            message: message.into(),
            context: BTreeMap::new(),
            cause: None,
        }
    }

    /// 链式追加一项 context。重复 key 会覆盖。
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: ContextValue) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// 设置 cause，作为下层错误。已有 cause 会被覆盖。
    #[must_use]
    pub fn with_cause(mut self, cause: Self) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }
}

impl fmt::Display for FaygerError {
    /// 形如 `LDR_PARSE_FAIL@loader: magic bytes do not match {reason: "MagicMismatch", offset: 0}`。
    ///
    /// 不递归打印 cause 链；调用方使用 `std::error::Error::source` 遍历。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}: {}", self.code, self.source_layer, self.message)?;
        if !self.context.is_empty() {
            f.write_str(" {")?;
            for (i, (k, v)) in self.context.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{k}: {v}")?;
            }
            f.write_str("}")?;
        }
        Ok(())
    }
}

impl std::error::Error for FaygerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_deref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::FaygerError;
    use crate::{ContextValue, ErrorCode, Layer};
    use std::error::Error as StdError;

    #[test]
    fn new_sets_fields_and_default_empty_context_no_cause() {
        let err = FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "boom");
        assert_eq!(err.code, ErrorCode::LdrParseFail);
        assert_eq!(err.source_layer, Layer::Loader);
        assert_eq!(err.message, "boom");
        assert!(err.context.is_empty());
        assert!(err.cause.is_none());
    }

    #[test]
    fn with_context_chains_and_overwrites() {
        let err = FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "x")
            .with_context("a", ContextValue::U64(1))
            .with_context("b", ContextValue::string("y"))
            .with_context("a", ContextValue::U64(99)); // overwrite

        assert_eq!(err.context.len(), 2);
        assert!(matches!(err.context["a"], ContextValue::U64(99)));
    }

    #[test]
    fn display_format_is_stable() {
        let err = FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "boom")
            .with_context("reason", ContextValue::string("MagicMismatch"))
            .with_context("offset", ContextValue::U64(0));
        // BTreeMap 排序：offset < reason
        assert_eq!(
            err.to_string(),
            "LDR_PARSE_FAIL@loader: boom {offset: 0, reason: \"MagicMismatch\"}"
        );
    }

    #[test]
    fn display_without_context_omits_braces() {
        let err = FaygerError::new(ErrorCode::LdrMissingRequiredField, Layer::Loader, "miss");
        assert_eq!(err.to_string(), "LDR_MISSING_REQUIRED_FIELD@loader: miss");
    }

    #[test]
    fn cause_chain_is_traversable_via_source() {
        // 构造三层 cause 链：top (Runtime) -> mid (Adapter) -> bottom (Host)
        let bottom = FaygerError::new(ErrorCode::LdrParseFail, Layer::Host, "syscall failed");
        let mid = FaygerError::new(ErrorCode::LdrParseFail, Layer::Adapter, "adapter wrap")
            .with_cause(bottom);
        let top = FaygerError::new(ErrorCode::LdrParseFail, Layer::Runtime, "runtime wrap")
            .with_cause(mid);

        // 遍历 source 链
        let layers: Vec<_> =
            std::iter::successors(Some(&top as &(dyn StdError + 'static)), |e| (*e).source())
                .filter_map(|e| e.downcast_ref::<FaygerError>())
                .map(|e| e.source_layer)
                .collect();

        assert_eq!(
            layers,
            [Layer::Runtime, Layer::Adapter, Layer::Host],
            "cause 链应按 top → bottom 顺序产出 Runtime, Adapter, Host"
        );
    }

    #[test]
    fn with_cause_overwrites_existing_cause() {
        let first = FaygerError::new(ErrorCode::LdrParseFail, Layer::Host, "first");
        let second = FaygerError::new(ErrorCode::LdrParseFail, Layer::Host, "second");
        let err = FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "wrap")
            .with_cause(first)
            .with_cause(second);
        // 后写入的覆盖
        let cause = err.cause.as_deref().expect("cause set");
        assert_eq!(cause.message, "second");
    }
}
