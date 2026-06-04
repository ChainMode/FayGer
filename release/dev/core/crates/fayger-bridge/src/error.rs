//! 跨层错误链包装（M10）。
//!
//! 当一条指令在适配层被拒（`ADP_*`），[`wrap_dispatch_failure`] 把它包装为
//! `RT_INSTANCE_FAILED`（`source_layer = Runtime`）的 cause，构造蓝图 §6.3 的
//! `[Runtime, Adapter]` 跨层错误链。

use fayger_error::{ErrorCode, FaygerError, Layer};

/// 把一个适配层错误包装为 `RT_INSTANCE_FAILED` 的 cause（跨层错误链）。
///
/// 结果 `layer_chain() == [Runtime, Adapter]`；`root_cause()` 是传入的适配层错误；
/// 被包装错误的 `code` / `source_layer` / `context` 不被改写（由 [`FaygerError::wrap`] 保证）。
#[must_use]
pub fn wrap_dispatch_failure(adp_error: FaygerError) -> FaygerError {
    FaygerError::wrap(
        ErrorCode::RtInstanceFailed,
        Layer::Runtime,
        "instruction dispatch failed in the adapter layer",
        adp_error,
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::wrap_dispatch_failure;
    use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

    fn adp_denied() -> FaygerError {
        FaygerError::new(ErrorCode::AdpCapabilityDenied, Layer::Adapter, "denied")
            .with_context("missing", ContextValue::string("proc.spawn"))
    }

    #[test]
    fn wraps_with_runtime_outer_and_adapter_cause() {
        let wrapped = wrap_dispatch_failure(adp_denied());
        assert_eq!(wrapped.code, ErrorCode::RtInstanceFailed);
        assert_eq!(wrapped.source_layer, Layer::Runtime);
        assert_eq!(wrapped.layer_chain(), [Layer::Runtime, Layer::Adapter]);
    }

    #[test]
    fn root_cause_is_the_adapter_error() {
        let wrapped = wrap_dispatch_failure(adp_denied());
        let root = wrapped.root_cause();
        assert_eq!(root.code, ErrorCode::AdpCapabilityDenied);
        assert_eq!(root.source_layer, Layer::Adapter);
        // context 未被改写
        assert!(root.context.contains_key("missing"));
    }

    #[test]
    fn depth_increases_by_one() {
        let adp = adp_denied();
        let base_depth = adp.depth();
        let wrapped = wrap_dispatch_failure(adp);
        assert_eq!(wrapped.depth(), base_depth + 1);
    }

    #[test]
    fn works_for_unsupported_instruction() {
        let adp = FaygerError::new(
            ErrorCode::AdpUnsupportedInstruction,
            Layer::Adapter,
            "unsupported",
        );
        let wrapped = wrap_dispatch_failure(adp);
        assert_eq!(wrapped.layer_chain(), [Layer::Runtime, Layer::Adapter]);
        assert_eq!(
            wrapped.root_cause().code,
            ErrorCode::AdpUnsupportedInstruction
        );
    }
}
