//! `ErrorCode`：稳定的、可文档化的错误标识。
//!
//! 蓝图 §6.4 把每条错误码定义为跨版本稳定的语义：调用方依据 code 做分支决策；
//! 同一 code 的语义在所有版本中不可变。新错误条件总是新增 code，不复用既有 code。
//!
//! M0 阶段只暴露 Loader 相关的两个 code：
//! - `LdrParseFail`
//! - `LdrMissingRequiredField`
//!
//! 后续里程碑会按蓝图 §6.4 表追加 code。

use std::fmt;

/// Fayger 稳定错误码。
///
/// 所有 code 均以 `<层前缀>_<原因>` 形式命名（蓝图惯例）。Display 输出大写蛇形命名，
/// 便于日志与调用方分支匹配。
///
/// 标记为 `#[non_exhaustive]`，后续里程碑追加 code 不破坏既有匹配代码。
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // ----- Loader 层 (M0) -----
    /// `LDR_PARSE_FAIL`：BuF 字节流不符合格式规范。
    ///
    /// 触发条件包括但不限于：magic 不匹配、Header 偏移越界、Manifest CBOR 解码失败、
    /// Section_Index 自相矛盾、Trailer 长度不一致、字节流被截断。具体原因由
    /// `FaygerError::context["reason"]` 区分。
    LdrParseFail,
    /// `LDR_MISSING_REQUIRED_FIELD`：序列化时 `BuFManifest` 缺必填字段，
    /// 或 `BuFObject` 内部 sections 与 `manifest.sections` 描述不一致。
    LdrMissingRequiredField,
}

impl ErrorCode {
    /// 返回稳定的字符串编码（蓝图 §6.4 的 code 字段）。
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LdrParseFail => "LDR_PARSE_FAIL",
            Self::LdrMissingRequiredField => "LDR_MISSING_REQUIRED_FIELD",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;

    #[test]
    fn as_str_returns_stable_identifiers() {
        assert_eq!(ErrorCode::LdrParseFail.as_str(), "LDR_PARSE_FAIL");
        assert_eq!(
            ErrorCode::LdrMissingRequiredField.as_str(),
            "LDR_MISSING_REQUIRED_FIELD"
        );
    }

    #[test]
    fn display_matches_as_str() {
        for code in [ErrorCode::LdrParseFail, ErrorCode::LdrMissingRequiredField] {
            assert_eq!(code.to_string(), code.as_str());
        }
    }

    #[test]
    fn equality_and_copy_semantics() {
        let a = ErrorCode::LdrParseFail;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(ErrorCode::LdrParseFail, ErrorCode::LdrMissingRequiredField);
    }
}
