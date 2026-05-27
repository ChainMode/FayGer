//! 内部错误构造助手。
//!
//! 把 fayger-buf 内部出现的具体失败映射为 [`fayger_error::FaygerError`]。
//! M0 阶段所有 fayger-buf 错误都属于 Loader 层。

use fayger_error::{ErrorCode, FaygerError, Layer};

/// CBOR 解码相关的失败：归类为 `LDR_PARSE_FAIL`。
pub(crate) fn cbor_decode_fail(message: impl Into<String>) -> FaygerError {
    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, message)
}

/// canonical CBOR 序列化时出现内部不一致。归类为 `LDR_PARSE_FAIL`，
/// 因为对外语义上仍是「输出未能符合格式规范」。
pub(crate) fn cbor_encode_fail(message: impl Into<String>) -> FaygerError {
    FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, message)
}
