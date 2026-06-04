//! 文件 I/O 薄封装（M9）。
//!
//! 仅这两个函数触达文件系统；命令编排在 [`crate::run_cli`] 中把它们与纯核心拼接，
//! 以便纯核心可在内存中完整测试。

use crate::error::CliError;

/// 读取整个文件为字节。
///
/// # Errors
/// 文件不存在 / 不可读时返回 [`CliError::Io`]。
pub fn read_file(path: &str) -> Result<Vec<u8>, CliError> {
    std::fs::read(path).map_err(|e| CliError::Io {
        path: path.to_owned(),
        detail: e.to_string(),
    })
}

/// 把字节写入文件（覆盖）。
///
/// # Errors
/// 不可写时返回 [`CliError::Io`]。
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), CliError> {
    std::fs::write(path, bytes).map_err(|e| CliError::Io {
        path: path.to_owned(),
        detail: e.to_string(),
    })
}
