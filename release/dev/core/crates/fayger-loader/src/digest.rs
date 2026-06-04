//! 摘要与校验和基元：SHA-256 与 CRC-32。
//!
//! 这两个函数是加载链 Verify(Digest) 阶段（见 [`crate::verify`]）与封装操作
//! （见 [`mod@crate::seal`]）的底层依赖。它们都是确定性纯函数，不依赖网络、文件系统
//! 或全局可变状态。
//!
//! - [`sha256`] 用于 `manifest_digest`（Header || Manifest || Section_Index 区间）
//!   与每个 Section 段体的内容摘要。
//! - [`crc32`] 用于整文件（除末尾 4 字节 crc32 自身）的随机损坏兜底校验。

use sha2::{Digest as _, Sha256};

/// 对字节切片计算 SHA-256，返回 32 字节摘要。
///
/// # Examples
///
/// ```
/// let d = fayger_loader::digest::sha256(b"abc");
/// assert_eq!(d.len(), 32);
/// ```
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// 对字节切片计算 CRC-32（IEEE 多项式），返回 `u32`。
///
/// # Examples
///
/// ```
/// assert_eq!(fayger_loader::digest::crc32(b""), 0);
/// assert_eq!(fayger_loader::digest::crc32(b"123456789"), 0xCBF4_3926);
/// ```
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::{crc32, sha256};

    /// 把 32 字节摘要转成小写十六进制字符串，便于与已知向量比对。
    fn hex(bytes: &[u8]) -> String {
        use core::fmt::Write as _;
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    #[test]
    fn sha256_empty_string_known_vector() {
        // NIST: SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_abc_known_vector() {
        // NIST: SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_is_deterministic() {
        let a = sha256(b"fayger");
        let b = sha256(b"fayger");
        assert_eq!(a, b);
    }

    #[test]
    fn crc32_empty_is_zero() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn crc32_check_vector() {
        // 经典 CRC-32 check 值："123456789" => 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn crc32_is_deterministic() {
        assert_eq!(crc32(b"fayger"), crc32(b"fayger"));
    }

    #[test]
    fn crc32_detects_single_bit_change() {
        let a = crc32(b"fayger");
        let b = crc32(b"fayges"); // 末字符 r->s
        assert_ne!(a, b);
    }
}
