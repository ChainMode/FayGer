//! BuF 制品的固定字节布局段（Header 与 Trailer）。
//!
//! 参见需求文档 Requirement 1（字节布局）与设计文档 §"`format` 模块"。
//!
//! BuF 整体布局：
//!
//! ```text
//! 偏移          长度    字段
//! 0             4       magic = "BUF\0"
//! 4             4       format_version: u32 LE
//! 8             8       manifest_offset: u64 LE
//! 16            8       manifest_length: u64 LE
//! 24            8       section_index_offset: u64 LE
//! 32            8       section_index_length: u64 LE
//! 40            [可变]  Manifest CBOR
//! 40+M          [可变]  Section_Index CBOR              (M = manifest_length)
//! 40+M+I        [可变]  Sections 段                     (I = section_index_length)
//! total - 44    8       total_length: u64 LE
//! total - 36    32      manifest_digest                (M0 占位为全零)
//! total - 4     4       crc32: u32 LE                  (M0 占位为 0)
//! ```
//!
//! 本模块只提供字节级序列化/反序列化；语义校验（offset 越界、CBOR 解析等）
//! 由上层 parser 模块负责。

use binrw::{BinRead, BinWrite};

/// BuF 文件 magic：固定 4 字节 `"BUF\0"`。
pub const BUF_MAGIC: [u8; 4] = *b"BUF\0";

/// `Header` 段在文件中的固定大小（字节）。
///
/// 等于 `magic(4) + format_version(4) + manifest_offset(8) + manifest_length(8)
/// + section_index_offset(8) + section_index_length(8)` = 40 字节。
pub const HEADER_SIZE: usize = 40;

/// `Trailer` 段在文件中的固定大小（字节）。
///
/// 等于 `total_length(8) + manifest_digest(32) + crc32(4)` = 44 字节。
pub const TRAILER_SIZE: usize = 44;

/// M0 阶段的 BuF 格式版本号。后续若布局变化将抬升此值。
pub const FORMAT_VERSION_M0: u32 = 1;

/// `manifest_digest` 字段长度（字节）。固定为 sha-256 输出长度。
pub const MANIFEST_DIGEST_SIZE: usize = 32;

/// BuF 文件首部 40 字节的固定结构。
///
/// 通过 [`binrw`] 派生：读写都按小端、检查 magic 字节匹配。
#[derive(BinRead, BinWrite, Debug, PartialEq, Eq, Clone)]
#[brw(little, magic = b"BUF\0")]
pub struct Header {
    /// BuF 格式版本。M0 阶段固定为 [`FORMAT_VERSION_M0`]。
    pub format_version: u32,
    /// Manifest 段在文件中的起始偏移（绝对位置，相对文件开头）。
    pub manifest_offset: u64,
    /// Manifest 段的字节长度。
    pub manifest_length: u64,
    /// Section_Index 段在文件中的起始偏移（绝对位置）。
    pub section_index_offset: u64,
    /// Section_Index 段的字节长度。
    pub section_index_length: u64,
}

/// BuF 文件尾部 44 字节的固定结构。
///
/// M0 阶段 `manifest_digest` 与 `crc32` 仅占位（写零、不校验）。
/// 后续里程碑会启用实际计算与校验。
#[derive(BinRead, BinWrite, Debug, PartialEq, Eq, Clone)]
#[brw(little)]
pub struct Trailer {
    /// 整个 BuF 文件的字节长度。
    pub total_length: u64,
    /// Manifest 段的 sha-256 摘要。M0 阶段写全零。
    pub manifest_digest: [u8; MANIFEST_DIGEST_SIZE],
    /// 整个文件的 crc32（不含 trailer 末 4 字节自身）。M0 阶段写零。
    pub crc32: u32,
}

impl Trailer {
    /// 构造 M0 阶段的占位 Trailer：digest 全零、crc32 为 0、总长 = `total_length`。
    #[must_use]
    pub const fn placeholder(total_length: u64) -> Self {
        Self {
            total_length,
            manifest_digest: [0u8; MANIFEST_DIGEST_SIZE],
            crc32: 0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        Header, Trailer, BUF_MAGIC, FORMAT_VERSION_M0, HEADER_SIZE, MANIFEST_DIGEST_SIZE,
        TRAILER_SIZE,
    };
    use binrw::{io::Cursor, BinRead, BinWrite};

    fn sample_header() -> Header {
        Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset: HEADER_SIZE as u64,
            manifest_length: 100,
            section_index_offset: HEADER_SIZE as u64 + 100,
            section_index_length: 50,
        }
    }

    #[test]
    fn header_size_constant_matches_serialized_size() {
        let h = sample_header();
        let mut buf = Cursor::new(Vec::new());
        h.write(&mut buf).expect("write header");
        let bytes = buf.into_inner();
        assert_eq!(bytes.len(), HEADER_SIZE);
    }

    #[test]
    fn header_round_trip() {
        let h = sample_header();
        let mut buf = Cursor::new(Vec::new());
        h.write(&mut buf).expect("write header");
        let bytes = buf.into_inner();

        let mut cursor = Cursor::new(&bytes);
        let h2 = Header::read(&mut cursor).expect("read header");
        assert_eq!(h, h2);
    }

    #[test]
    fn header_starts_with_magic() {
        let h = sample_header();
        let mut buf = Cursor::new(Vec::new());
        h.write(&mut buf).expect("write header");
        let bytes = buf.into_inner();
        assert_eq!(&bytes[..4], &BUF_MAGIC);
    }

    #[test]
    fn header_uses_little_endian() {
        let h = Header {
            format_version: 0x0102_0304,
            manifest_offset: 0x0807_0605_0403_0201,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        };
        let mut buf = Cursor::new(Vec::new());
        h.write(&mut buf).expect("write");
        let bytes = buf.into_inner();
        // magic 之后是 format_version：小端字节为 04 03 02 01
        assert_eq!(&bytes[4..8], &[0x04, 0x03, 0x02, 0x01]);
        // 接着 8 字节 manifest_offset：小端字节为 01..08
        assert_eq!(
            &bytes[8..16],
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }

    #[test]
    fn header_rejects_wrong_magic() {
        // 构造 40 字节缓冲，前 4 字节是错误 magic
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"XXX\0");

        let mut cursor = Cursor::new(&bytes);
        let r = Header::read(&mut cursor);
        assert!(r.is_err(), "expected read to fail on wrong magic");
    }

    #[test]
    fn header_rejects_truncated_input() {
        // 只给 8 字节
        let bytes = [0u8; 8];
        let mut cursor = Cursor::new(&bytes[..]);
        let r = Header::read(&mut cursor);
        assert!(r.is_err(), "expected read to fail on truncated input");
    }

    fn sample_trailer() -> Trailer {
        let mut digest = [0u8; MANIFEST_DIGEST_SIZE];
        // 用一些非零字节，以验证 round-trip 不只是因为全零
        for (i, b) in digest.iter_mut().enumerate() {
            *b = u8::try_from(i & 0xff).expect("0..32 fits in u8");
        }
        Trailer {
            total_length: 1_000_000,
            manifest_digest: digest,
            crc32: 0xDEAD_BEEF,
        }
    }

    #[test]
    fn trailer_size_constant_matches_serialized_size() {
        let t = sample_trailer();
        let mut buf = Cursor::new(Vec::new());
        t.write(&mut buf).expect("write trailer");
        let bytes = buf.into_inner();
        assert_eq!(bytes.len(), TRAILER_SIZE);
    }

    #[test]
    fn trailer_round_trip() {
        let t = sample_trailer();
        let mut buf = Cursor::new(Vec::new());
        t.write(&mut buf).expect("write trailer");
        let bytes = buf.into_inner();

        let mut cursor = Cursor::new(&bytes);
        let t2 = Trailer::read(&mut cursor).expect("read trailer");
        assert_eq!(t, t2);
    }

    #[test]
    fn placeholder_trailer_has_zero_digest_and_crc() {
        let t = Trailer::placeholder(123);
        assert_eq!(t.total_length, 123);
        assert_eq!(t.manifest_digest, [0u8; MANIFEST_DIGEST_SIZE]);
        assert_eq!(t.crc32, 0);
    }

    #[test]
    fn placeholder_trailer_round_trips() {
        let t = Trailer::placeholder(42);
        let mut buf = Cursor::new(Vec::new());
        t.write(&mut buf).expect("write");
        let bytes = buf.into_inner();
        assert_eq!(bytes.len(), TRAILER_SIZE);

        let mut cursor = Cursor::new(&bytes);
        let t2 = Trailer::read(&mut cursor).expect("read");
        assert_eq!(t, t2);
    }

    #[test]
    fn header_size_matches_documented_constant() {
        // 静态确认：常量与字段累加一致。
        // magic(4) + format_version(4) + 4 个 u64 = 40
        const EXPECTED: usize = 4 + 4 + 8 * 4;
        assert_eq!(HEADER_SIZE, EXPECTED);
    }

    #[test]
    fn trailer_size_matches_documented_constant() {
        // total_length(8) + manifest_digest(32) + crc32(4) = 44
        const EXPECTED: usize = 8 + 32 + 4;
        assert_eq!(TRAILER_SIZE, EXPECTED);
    }
}
