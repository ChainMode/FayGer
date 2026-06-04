//! BuF_Source 抽象与内置 [`MemorySource`]。
//!
//! 把"从哪里读字节"从加载链中解耦。任意随机访问存储后端通过实现 [`BuFSource`] 接入，
//! 最低要求 `read_at` 与 `length`（不要求写、追加或顺序流语义）。

use crate::strategy::Phase;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 源读取错误（实现方返回；加载层映射为 [`FaygerError`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    /// 可读的底层错误描述（不含指针/地址等不可序列化信息）。
    pub message: String,
}

impl SourceError {
    /// 便利构造。
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// BuF 的存储后端抽象。最低要求随机访问读 + 总长度。
pub trait BuFSource {
    /// 读取 `[offset, offset+length)` 区间，必须返回恰好 `length` 字节，否则视为失败。
    ///
    /// # Errors
    /// 区间越界、后端不可用等情况返回 [`SourceError`]。
    fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError>;

    /// 制品总长度（字节）。
    ///
    /// # Errors
    /// 后端不可用时返回 [`SourceError`]。
    fn length(&self) -> Result<u64, SourceError>;
}

/// 内置内存后端：把一段字节作为 BuF_Source。
#[derive(Debug, Clone)]
pub struct MemorySource {
    bytes: Vec<u8>,
}

impl MemorySource {
    /// 用一段字节构造。
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }
}

impl BuFSource for MemorySource {
    fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError> {
        let start = usize::try_from(offset)
            .map_err(|_| SourceError::new("offset exceeds addressable range"))?;
        let len = usize::try_from(length)
            .map_err(|_| SourceError::new("length exceeds addressable range"))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| SourceError::new("offset + length overflow"))?;
        self.bytes
            .get(start..end)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| {
                SourceError::new(format!(
                    "range [{start}, {end}) out of bounds for source of length {}",
                    self.bytes.len()
                ))
            })
    }

    fn length(&self) -> Result<u64, SourceError> {
        Ok(u64::try_from(self.bytes.len()).unwrap_or(u64::MAX))
    }
}

/// 从 source 读取恰好 `length` 字节，并把失败映射为统一错误（按阶段）。
///
/// - `read_at` 出错：`eager` → `LDR_SOURCE_READ_FAILED`；`lazy` → `LDR_LAZY_SOURCE_UNAVAILABLE`。
/// - 返回字节数 != `length`：统一 `LDR_SOURCE_READ_FAILED`（携带 expected/actual）。
///
/// # Errors
/// 见上。
pub(crate) fn read_exact(
    source: &dyn BuFSource,
    offset: u64,
    length: u64,
    phase: Phase,
) -> Result<Vec<u8>, FaygerError> {
    match source.read_at(offset, length) {
        Ok(bytes) => {
            let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            if actual == length {
                Ok(bytes)
            } else {
                Err(source_read_failed(phase)
                    .with_context("offset", ContextValue::U64(offset))
                    .with_context("expected", ContextValue::U64(length))
                    .with_context("actual", ContextValue::U64(actual)))
            }
        }
        Err(e) => Err(match phase {
            Phase::Eager => source_read_failed(phase)
                .with_context("offset", ContextValue::U64(offset))
                .with_context("length", ContextValue::U64(length))
                .with_context("detail", ContextValue::string(e.message)),
            Phase::Lazy => FaygerError::new(
                ErrorCode::LdrLazySourceUnavailable,
                Layer::Loader,
                "BuF source is unavailable during lazy section access",
            )
            .with_context("phase", ContextValue::string("lazy"))
            .with_context("offset", ContextValue::U64(offset))
            .with_context("length", ContextValue::U64(length))
            .with_context("detail", ContextValue::string(e.message)),
        }),
    }
}

fn source_read_failed(phase: Phase) -> FaygerError {
    FaygerError::new(
        ErrorCode::LdrSourceReadFailed,
        Layer::Loader,
        "failed to read from BuF source",
    )
    .with_context("phase", ContextValue::string(phase.as_str()))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{read_exact, BuFSource, MemorySource, SourceError};
    use crate::strategy::Phase;
    use fayger_error::ErrorCode;

    #[test]
    fn memory_source_reads_exact_range() {
        let src = MemorySource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(src.read_at(1, 3).expect("read"), vec![1, 2, 3]);
        assert_eq!(src.length().expect("len"), 6);
    }

    #[test]
    fn memory_source_zero_length_read() {
        let src = MemorySource::new(vec![9, 9]);
        assert_eq!(src.read_at(0, 0).expect("read"), Vec::<u8>::new());
    }

    #[test]
    fn memory_source_out_of_bounds_errors() {
        let src = MemorySource::new(vec![0, 1, 2]);
        assert!(src.read_at(2, 5).is_err());
        assert!(src.read_at(10, 1).is_err());
    }

    #[test]
    fn read_exact_maps_eager_error() {
        struct AlwaysFail;
        impl BuFSource for AlwaysFail {
            fn read_at(&self, _: u64, _: u64) -> Result<Vec<u8>, SourceError> {
                Err(SourceError::new("boom"))
            }
            fn length(&self) -> Result<u64, SourceError> {
                Ok(100)
            }
        }
        let err = read_exact(&AlwaysFail, 0, 10, Phase::Eager).expect_err("fail");
        assert_eq!(err.code, ErrorCode::LdrSourceReadFailed);
    }

    #[test]
    fn read_exact_maps_lazy_error() {
        struct AlwaysFail;
        impl BuFSource for AlwaysFail {
            fn read_at(&self, _: u64, _: u64) -> Result<Vec<u8>, SourceError> {
                Err(SourceError::new("boom"))
            }
            fn length(&self) -> Result<u64, SourceError> {
                Ok(100)
            }
        }
        let err = read_exact(&AlwaysFail, 0, 10, Phase::Lazy).expect_err("fail");
        assert_eq!(err.code, ErrorCode::LdrLazySourceUnavailable);
    }

    #[test]
    fn read_exact_short_read_is_failure() {
        // 返回的字节数少于请求 → SourceReadFailed。
        struct ShortSource;
        impl BuFSource for ShortSource {
            fn read_at(&self, _: u64, _: u64) -> Result<Vec<u8>, SourceError> {
                Ok(vec![1, 2]) // 永远只返回 2 字节
            }
            fn length(&self) -> Result<u64, SourceError> {
                Ok(100)
            }
        }
        let err = read_exact(&ShortSource, 0, 10, Phase::Eager).expect_err("short");
        assert_eq!(err.code, ErrorCode::LdrSourceReadFailed);
    }
}
