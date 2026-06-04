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

    // ----- Loader 层 (M1) -----
    /// `LDR_DIGEST_MISMATCH`：加载链 Verify(Digest) 阶段失败。
    ///
    /// 触发条件包括：Manifest 整体摘要（`manifest_digest`）不匹配、某个 Section 段体
    /// 摘要不匹配、整文件 `crc32` 不匹配、或声明了不受支持的摘要算法。具体由
    /// `FaygerError::context["reason"]` 区分（`ManifestDigest` / `SectionDigest` /
    /// `Crc32` / `UnsupportedDigestAlgorithm`）。
    LdrDigestMismatch,
    /// `LDR_SCHEMA_UNSUPPORTED`：BuF 声明的 `schema_version` 不在当前 Fayger 的
    /// 支持区间内。`context` 含 `expected_range` 与实际 `schema_version`。
    LdrSchemaUnsupported,
    /// `LDR_RUNTIME_VERSION_TOO_HIGH`：BuF 声明的 `runtime_interface_min` 高于当前
    /// Fayger 提供的 Runtime_Interface 版本。`context` 含 `required` 与 `provided`。
    LdrRuntimeVersionTooHigh,

    // ----- Runtime 层 (M2) -----
    /// `RT_ILLEGAL_TRANSITION`：对当前生命周期状态不允许的迁移指令，或对终态发出任意指令。
    /// `context` 含 `current_state`、`command`、`allowed_commands`。
    RtIllegalTransition,
    /// `RT_IMPL_NOT_FOUND`：运行层路由找不到满足选择策略与版本兼容性的实现。
    /// `context` 含 `strategy` 与（视策略）`preferred_impl`、`runtime_interface_min`。
    RtImplNotFound,
    /// `RT_IMPL_MISSING_METHODS`：注册的 Runtime_Implementation 缺少必需操作。
    /// `context.missing_methods` 恰为 `REQUIRED_OPERATIONS \ supported_operations`。
    RtImplMissingMethods,
    /// `RT_INSTANCE_FAILED`：BuF_Instance 执行中发生未处理错误，进入 `Failed`。
    RtInstanceFailed,
    /// `RT_INSTANCE_NOT_FOUND`：以未知 `InstanceId` 调用运行层操作。`context.instance_id`。
    RtInstanceNotFound,

    // ----- Adapter 层 (M3) -----
    /// `ADP_NO_MATCHING_PLATFORM`：没有已注册适配器匹配当前 `HostEnvironment`，
    /// 或在未进入可服务状态时收到派发请求。`context.platform_id`（视来源）。
    AdpNoMatchingPlatform,
    /// `ADP_UNSUPPORTED_INSTRUCTION`：当前 Platform_Adapter 不支持该指令种类。
    /// `context` 含 `category`、`opcode`、`platform_id`。
    AdpUnsupportedInstruction,
    /// `ADP_CAPABILITY_DENIED`：指令所需能力或 BuF 必需能力未被授予。
    /// `context.missing` 为未满足的能力集合。
    AdpCapabilityDenied,
    /// `ADP_HOST_CALL_FAILED`：宿主调用失败（M3 mock 翻译不触发，保留给真实绑定）。
    AdpHostCallFailed,

    // ----- Loader 层 (M4) -----
    /// `LDR_SIGNATURE_FAIL`：签名验证失败。由 `context.reason` 区分：
    /// `MissingSignature`（强制模式下无签名）、`UnknownKey`（public_key_ref 不在可信根）、
    /// `InvalidSignature`（签名验证不通过或长度非法）、`UnsupportedAlgorithm`（非 ed25519）。
    LdrSignatureFail,

    // ----- Loader 层 (M5) -----
    /// `LDR_SOURCE_READ_FAILED`：BuF_Source 读取失败（read_at / length 返回错误，
    /// 或返回字节数与请求不符）。`context` 含 `offset`、`length` 或 `expected`/`actual`、`detail`。
    LdrSourceReadFailed,
    /// `LDR_LAZY_SOURCE_UNAVAILABLE`：Lazy 模式后续访问某 Section 时 BuF_Source 不可用。
    /// `context` 含 `section_id`、`detail`，`phase == "lazy"`。
    LdrLazySourceUnavailable,
    /// `LDR_LAZY_DIGEST_MISMATCH`：Lazy 模式首次访问某 Section 时 digest 校验失败。
    /// `context` 含 `section_id`，`phase == "lazy"`，且该 Section 不被缓存。
    LdrLazyDigestMismatch,

    // ----- Loader 层 (M6) -----
    /// `LDR_PROFILE_REQUIRED_SECTION_MISSING`：某 `Required` Section 在当前 `LoadProfile`
    /// 下不满足 profile 约束。`context` 含 `section_id` 与未满足的约束项列表，`phase == "eager"`。
    LdrProfileRequiredSectionMissing,

    // ----- Runtime 层 (M7) -----
    /// `RT_QUOTA_EXCEEDED`：BuF_Instance 资源用量超出 BuF_Manifest 声明的配额。
    /// 超限信息（资源类型与用量）通过运行层查询接口暴露。
    RtQuotaExceeded,
}

impl ErrorCode {
    /// 返回稳定的字符串编码（蓝图 §6.4 的 code 字段）。
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LdrParseFail => "LDR_PARSE_FAIL",
            Self::LdrMissingRequiredField => "LDR_MISSING_REQUIRED_FIELD",
            Self::LdrDigestMismatch => "LDR_DIGEST_MISMATCH",
            Self::LdrSchemaUnsupported => "LDR_SCHEMA_UNSUPPORTED",
            Self::LdrRuntimeVersionTooHigh => "LDR_RUNTIME_VERSION_TOO_HIGH",
            Self::RtIllegalTransition => "RT_ILLEGAL_TRANSITION",
            Self::RtImplNotFound => "RT_IMPL_NOT_FOUND",
            Self::RtImplMissingMethods => "RT_IMPL_MISSING_METHODS",
            Self::RtInstanceFailed => "RT_INSTANCE_FAILED",
            Self::RtInstanceNotFound => "RT_INSTANCE_NOT_FOUND",
            Self::AdpNoMatchingPlatform => "ADP_NO_MATCHING_PLATFORM",
            Self::AdpUnsupportedInstruction => "ADP_UNSUPPORTED_INSTRUCTION",
            Self::AdpCapabilityDenied => "ADP_CAPABILITY_DENIED",
            Self::AdpHostCallFailed => "ADP_HOST_CALL_FAILED",
            Self::LdrSignatureFail => "LDR_SIGNATURE_FAIL",
            Self::LdrSourceReadFailed => "LDR_SOURCE_READ_FAILED",
            Self::LdrLazySourceUnavailable => "LDR_LAZY_SOURCE_UNAVAILABLE",
            Self::LdrLazyDigestMismatch => "LDR_LAZY_DIGEST_MISMATCH",
            Self::LdrProfileRequiredSectionMissing => "LDR_PROFILE_REQUIRED_SECTION_MISSING",
            Self::RtQuotaExceeded => "RT_QUOTA_EXCEEDED",
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
        assert_eq!(ErrorCode::LdrDigestMismatch.as_str(), "LDR_DIGEST_MISMATCH");
        assert_eq!(
            ErrorCode::LdrSchemaUnsupported.as_str(),
            "LDR_SCHEMA_UNSUPPORTED"
        );
        assert_eq!(
            ErrorCode::LdrRuntimeVersionTooHigh.as_str(),
            "LDR_RUNTIME_VERSION_TOO_HIGH"
        );
        assert_eq!(
            ErrorCode::RtIllegalTransition.as_str(),
            "RT_ILLEGAL_TRANSITION"
        );
        assert_eq!(ErrorCode::RtImplNotFound.as_str(), "RT_IMPL_NOT_FOUND");
        assert_eq!(
            ErrorCode::RtImplMissingMethods.as_str(),
            "RT_IMPL_MISSING_METHODS"
        );
        assert_eq!(ErrorCode::RtInstanceFailed.as_str(), "RT_INSTANCE_FAILED");
        assert_eq!(
            ErrorCode::RtInstanceNotFound.as_str(),
            "RT_INSTANCE_NOT_FOUND"
        );
        assert_eq!(
            ErrorCode::AdpNoMatchingPlatform.as_str(),
            "ADP_NO_MATCHING_PLATFORM"
        );
        assert_eq!(
            ErrorCode::AdpUnsupportedInstruction.as_str(),
            "ADP_UNSUPPORTED_INSTRUCTION"
        );
        assert_eq!(
            ErrorCode::AdpCapabilityDenied.as_str(),
            "ADP_CAPABILITY_DENIED"
        );
        assert_eq!(
            ErrorCode::AdpHostCallFailed.as_str(),
            "ADP_HOST_CALL_FAILED"
        );
        assert_eq!(ErrorCode::LdrSignatureFail.as_str(), "LDR_SIGNATURE_FAIL");
        assert_eq!(
            ErrorCode::LdrSourceReadFailed.as_str(),
            "LDR_SOURCE_READ_FAILED"
        );
        assert_eq!(
            ErrorCode::LdrLazySourceUnavailable.as_str(),
            "LDR_LAZY_SOURCE_UNAVAILABLE"
        );
        assert_eq!(
            ErrorCode::LdrLazyDigestMismatch.as_str(),
            "LDR_LAZY_DIGEST_MISMATCH"
        );
        assert_eq!(
            ErrorCode::LdrProfileRequiredSectionMissing.as_str(),
            "LDR_PROFILE_REQUIRED_SECTION_MISSING"
        );
        assert_eq!(ErrorCode::RtQuotaExceeded.as_str(), "RT_QUOTA_EXCEEDED");
    }

    #[test]
    fn display_matches_as_str() {
        for code in [
            ErrorCode::LdrParseFail,
            ErrorCode::LdrMissingRequiredField,
            ErrorCode::LdrDigestMismatch,
            ErrorCode::LdrSchemaUnsupported,
            ErrorCode::LdrRuntimeVersionTooHigh,
            ErrorCode::RtIllegalTransition,
            ErrorCode::RtImplNotFound,
            ErrorCode::RtImplMissingMethods,
            ErrorCode::RtInstanceFailed,
            ErrorCode::RtInstanceNotFound,
            ErrorCode::AdpNoMatchingPlatform,
            ErrorCode::AdpUnsupportedInstruction,
            ErrorCode::AdpCapabilityDenied,
            ErrorCode::AdpHostCallFailed,
            ErrorCode::LdrSignatureFail,
            ErrorCode::LdrSourceReadFailed,
            ErrorCode::LdrLazySourceUnavailable,
            ErrorCode::LdrLazyDigestMismatch,
            ErrorCode::LdrProfileRequiredSectionMissing,
            ErrorCode::RtQuotaExceeded,
        ] {
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
