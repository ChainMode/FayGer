//! CLI 错误与退出码（M9）。
//!
//! [`CliError`] 是 CLI 层的呈现型错误，区分「用法错误」（[`CliError::Usage`]）、「库层失败」
//! （[`CliError::Failure`] 包裹 [`fayger_error::FaygerError`]）与「文件 I/O 失败」
//! （[`CliError::Io`]）。[`exit_code_of`] 把执行结果确定性地映射为进程退出码。

use fayger_error::FaygerError;
use std::fmt;

/// CLI 执行失败的统一错误。
#[derive(Debug)]
pub enum CliError {
    /// 用法错误（未知命令 / 缺参 / 参数格式错误）。退出码 2。
    Usage(String),
    /// 库层失败（加载链 / 运行 / 适配选择等返回的 `FaygerError`）。退出码 1。
    Failure(FaygerError),
    /// 文件 I/O 失败。退出码 1。
    Io {
        /// 涉及的路径。
        path: String,
        /// 失败细节。
        detail: String,
    },
}

impl From<FaygerError> for CliError {
    fn from(e: FaygerError) -> Self {
        Self::Failure(e)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(msg) => write!(f, "usage error: {msg}"),
            Self::Failure(err) => write!(f, "error: {err}"),
            Self::Io { path, detail } => write!(f, "io error on {path}: {detail}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failure(err) => Some(err),
            _ => None,
        }
    }
}

/// 进程退出码。约定：成功 0；执行失败 1；用法错误 2。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// 成功。
    Success = 0,
    /// 执行失败（库层 / I/O）。
    Failure = 1,
    /// 用法错误。
    Usage = 2,
}

impl ExitCode {
    /// 转为进程退出整数码。
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// 把一次 CLI 执行结果确定性地映射为退出码（纯函数）。
#[must_use]
pub const fn exit_code_of(result: &Result<(), CliError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::Success,
        Err(CliError::Usage(_)) => ExitCode::Usage,
        Err(CliError::Failure(_) | CliError::Io { .. }) => ExitCode::Failure,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{exit_code_of, CliError, ExitCode};
    use fayger_error::{ErrorCode, FaygerError, Layer};

    #[test]
    fn ok_maps_to_success() {
        assert_eq!(exit_code_of(&Ok(())), ExitCode::Success);
        assert_eq!(ExitCode::Success.as_i32(), 0);
    }

    #[test]
    fn usage_maps_to_two() {
        let r = Err(CliError::Usage("bad".into()));
        assert_eq!(exit_code_of(&r), ExitCode::Usage);
        assert_eq!(ExitCode::Usage.as_i32(), 2);
    }

    #[test]
    fn failure_maps_to_one() {
        let r = Err(CliError::Failure(FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "boom",
        )));
        assert_eq!(exit_code_of(&r), ExitCode::Failure);
        assert_eq!(ExitCode::Failure.as_i32(), 1);
    }

    #[test]
    fn io_maps_to_one() {
        let r = Err(CliError::Io {
            path: "x".into(),
            detail: "missing".into(),
        });
        assert_eq!(exit_code_of(&r), ExitCode::Failure);
    }

    #[test]
    fn from_fayger_error_is_failure() {
        let e: CliError = FaygerError::new(ErrorCode::LdrParseFail, Layer::Loader, "x").into();
        assert!(matches!(e, CliError::Failure(_)));
    }
}
