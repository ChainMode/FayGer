//! Fayger 统一错误模型 (蓝图 §6.1)。
//!
//! 这个 crate 提供 [`FaygerError`] 与三个配套类型：[`ErrorCode`]、[`Layer`]、[`ContextValue`]。
//! 所有 Fayger 各层（loader / runtime / adapter / host）对外的错误都通过这一对象表达，
//! 便于在三层之间组装错误链而不丢失诊断信息。
//!
//! M0 阶段只实现 Loader 层的两个 [`ErrorCode`] 变体：
//! - [`ErrorCode::LdrParseFail`]
//! - [`ErrorCode::LdrMissingRequiredField`]
//!
//! 后续里程碑会按蓝图 §6.4 表格追加更多变体。所有公共枚举均带 `#[non_exhaustive]`，
//! 以保证后续追加变体不破坏既有调用代码。
//!
//! # Example
//!
//! ```
//! use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
//!
//! let err = FaygerError::new(
//!     ErrorCode::LdrParseFail,
//!     Layer::Loader,
//!     "magic bytes do not match",
//! )
//! .with_context("reason", ContextValue::string("MagicMismatch"));
//!
//! assert_eq!(err.code, ErrorCode::LdrParseFail);
//! assert!(err.context.contains_key("reason"));
//! ```

#![doc(html_root_url = "https://docs.rs/fayger-error/0.0.0")]

mod code;
mod context;
mod error;
mod layer;

pub use code::ErrorCode;
pub use context::ContextValue;
pub use error::FaygerError;
pub use layer::Layer;
