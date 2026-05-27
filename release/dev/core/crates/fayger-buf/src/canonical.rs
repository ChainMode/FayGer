//! Canonical CBOR 编解码层 (RFC 8949 §4.2)。
//!
//! ciborium 默认输出**不**遵循 canonical 规则：map keys 不排序、整数也未必最短。
//! 本模块在 ciborium 之上加一薄层，把任意 [`ciborium::value::Value`] 写成
//! 满足 canonical 约束的字节序列；以及把字节序列读回 [`Value`] 并验证它确实
//! 来自 canonical 编码。
//!
//! ## 落地的 RFC 8949 §4.2 子集
//!
//! M0 阶段实现下列 canonical 规则：
//!
//! - **整数最短编码**：所有整数（major type 0/1）都用尽可能短的 additional info
//!   表示。具体规则参见 §4.2.1。
//! - **definite length**：字符串、字节串、数组、映射全部使用 definite length；
//!   不输出 indefinite length（0x5f / 0x7f / 0x9f / 0xbf）。
//! - **map keys 字节序列升序**：map 的 key 按已编码字节序列做无符号 lex 比较，
//!   小者在前。等键视为 canonical 错误。
//! - **拒绝浮点**：M0 阶段 BuF 不允许出现浮点；canonical 输入若含 float 视为错误。
//! - **拒绝 tag**：M0 阶段不引入 CBOR tag。
//!
//! ## 不在 M0 范围内
//!
//! - half-precision float 选最短表示（M0 拒绝所有 float）。
//! - NaN canonicalization。
//! - 非确定的 tag 编码差异。

use crate::error::{cbor_decode_fail, cbor_encode_fail};
use ciborium::value::Value;
use fayger_error::{ContextValue, FaygerError};

/// Canonical CBOR 编解码失败的内部错误。
///
/// 不直接对外暴露；调用方拿到的是 [`FaygerError`]。本结构作为 lib 内部模块间的
/// 中间表示，便于 parser/serializer 统一映射成 `LDR_PARSE_FAIL`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalError {
    /// 输入字节流不是合法 CBOR（ciborium 解码失败）。
    NotCbor(String),
    /// 输入是 CBOR 但违反 canonical 规则。
    NotCanonical(NonCanonicalReason),
    /// 序列化时遇到不允许的值（如浮点）。
    UnsupportedValue(&'static str),
}

/// 非 canonical 输入的具体原因。便于把信息透到 `context.reason`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NonCanonicalReason {
    /// 整数没有用最短编码。
    IntegerNotShortest,
    /// 出现 indefinite length 容器。
    IndefiniteLength,
    /// map keys 未按字节序列升序。
    MapKeysNotSorted,
    /// map 中出现重复 key。
    DuplicateMapKey,
    /// 出现浮点数（M0 不允许）。
    FloatPresent,
    /// 出现 CBOR tag（M0 不允许）。
    TagPresent,
}

impl NonCanonicalReason {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::IntegerNotShortest => "IntegerNotShortest",
            Self::IndefiniteLength => "IndefiniteLength",
            Self::MapKeysNotSorted => "MapKeysNotSorted",
            Self::DuplicateMapKey => "DuplicateMapKey",
            Self::FloatPresent => "FloatPresent",
            Self::TagPresent => "TagPresent",
        }
    }
}

impl CanonicalError {
    /// 把内部错误映射为外部 `FaygerError`（LDR_PARSE_FAIL）。
    pub(crate) fn into_fayger_error(self, kind: CanonicalErrorKind) -> FaygerError {
        match self {
            Self::NotCbor(msg) => match kind {
                CanonicalErrorKind::Decode => cbor_decode_fail("not valid CBOR")
                    .with_context("reason", ContextValue::string("CborDecodeFail"))
                    .with_context("detail", ContextValue::string(msg)),
                CanonicalErrorKind::Encode => cbor_encode_fail("internal CBOR encoding failure")
                    .with_context("reason", ContextValue::string("CanonicalEncodeFail"))
                    .with_context("detail", ContextValue::string(msg)),
            },
            Self::NotCanonical(reason) => cbor_decode_fail("input is CBOR but not canonical")
                .with_context("reason", ContextValue::string("NonCanonicalCbor"))
                .with_context("rule", ContextValue::string(reason.as_str())),
            Self::UnsupportedValue(what) => {
                cbor_encode_fail("unsupported value in canonical encode")
                    .with_context("reason", ContextValue::string("UnsupportedValue"))
                    .with_context("kind", ContextValue::string(what))
            }
        }
    }
}

/// 与 [`CanonicalError::into_fayger_error`] 配合使用，标识此错误来自 encode 还是 decode 路径。
#[derive(Debug, Clone, Copy)]
pub(crate) enum CanonicalErrorKind {
    Encode,
    Decode,
}

// ----------------------- Encoder -----------------------

/// 把 [`Value`] 写成 canonical CBOR 字节。
///
/// 见模块文档中列出的 canonical 规则集。
pub(crate) fn write_canonical(value: &Value, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    write_value(value, out)
}

fn write_value(value: &Value, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    match value {
        Value::Integer(i) => write_integer(*i, out),
        Value::Bytes(b) => {
            // major type 2
            write_head(2, len_as_u64(b.len())?, out);
            out.extend_from_slice(b);
            Ok(())
        }
        Value::Text(s) => {
            // major type 3
            let bytes = s.as_bytes();
            write_head(3, len_as_u64(bytes.len())?, out);
            out.extend_from_slice(bytes);
            Ok(())
        }
        Value::Array(items) => {
            write_head(4, len_as_u64(items.len())?, out);
            for item in items {
                write_value(item, out)?;
            }
            Ok(())
        }
        Value::Map(entries) => write_map(entries, out),
        Value::Bool(b) => {
            // major type 7, simple values: 20=false 21=true
            out.push(if *b { 0xf5 } else { 0xf4 });
            Ok(())
        }
        Value::Null => {
            // major type 7, simple value 22
            out.push(0xf6);
            Ok(())
        }
        Value::Float(_) => Err(CanonicalError::UnsupportedValue("float")),
        Value::Tag(_, _) => Err(CanonicalError::UnsupportedValue("tag")),
        // ciborium::Value 是非穷尽的 enum；我们已覆盖 M0 全部允许形态。
        // 兜底防御：把出现的未知变体当成不支持的值。
        _ => Err(CanonicalError::UnsupportedValue("unknown")),
    }
}

/// 把容器长度（usize）转 u64。在 64 位平台上必然成功；保留 `try_from` 以防 32 位。
fn len_as_u64(n: usize) -> Result<u64, CanonicalError> {
    u64::try_from(n).map_err(|_| CanonicalError::UnsupportedValue("length exceeds u64"))
}

fn write_integer(i: ciborium::value::Integer, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    // ciborium::Integer 内部用 i128 表示，覆盖 [-2^64, 2^64-1]。
    // CBOR 中 major type 0 表示无符号整数 v；major type 1 表示负整数 -1-v。
    let n: i128 = i.into();
    if n >= 0 {
        write_head(0, u64_from_i128_unsigned(n)?, out);
    } else {
        // CBOR 中负数编码为 -1 - v，所以 v = -1 - n（n 是负的）。
        // 例如 n = -1 => v = 0；n = -100 => v = 99。
        let v = -1i128 - n;
        write_head(1, u64_from_i128_unsigned(v)?, out);
    }
    Ok(())
}

fn u64_from_i128_unsigned(v: i128) -> Result<u64, CanonicalError> {
    // 仅 M0 范围：CBOR 整数最大 u64::MAX，超过返回错误。
    if !(0..=i128::from(u64::MAX)).contains(&v) {
        return Err(CanonicalError::UnsupportedValue("integer out of u64 range"));
    }
    // safe: 已确认 v 在 [0, u64::MAX]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let r = v as u64;
    Ok(r)
}

/// 写一个 CBOR 头：major type (3 bits) + additional info 选最短编码。
///
/// `value` 是头需要承载的无符号值（数据项的长度或整数本身）。
fn write_head(major: u8, value: u64, out: &mut Vec<u8>) {
    debug_assert!(major <= 7);
    let initial = major << 5;
    if value <= 23 {
        // 直接嵌在 additional info 里。value 已确保 <=23，转 u8 不会截断。
        // 这里是安全的窄化转换。
        #[allow(clippy::cast_possible_truncation)]
        out.push(initial | (value as u8));
    } else if let Ok(v) = u8::try_from(value) {
        out.push(initial | 0x18);
        out.push(v);
    } else if let Ok(v) = u16::try_from(value) {
        out.push(initial | 0x19);
        out.extend_from_slice(&v.to_be_bytes());
    } else if let Ok(v) = u32::try_from(value) {
        out.push(initial | 0x1a);
        out.extend_from_slice(&v.to_be_bytes());
    } else {
        out.push(initial | 0x1b);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

fn write_map(entries: &[(Value, Value)], out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    // 1. 编码每个 key 到本地 buffer，配上 value（递归）。
    // 2. 按 key 字节序排序。等键视为重复 -> 错误。
    // 3. 写 map header (len)，然后依次写 (key_bytes, value_bytes)。
    let mut encoded: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(entries.len());
    for (k, v) in entries {
        let mut kb = Vec::new();
        write_value(k, &mut kb)?;
        let mut vb = Vec::new();
        write_value(v, &mut vb)?;
        encoded.push((kb, vb));
    }
    encoded.sort_by(|a, b| a.0.cmp(&b.0));

    // 检测重复 key
    for w in encoded.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(CanonicalError::NotCanonical(
                NonCanonicalReason::DuplicateMapKey,
            ));
        }
    }

    write_head(5, len_as_u64(encoded.len())?, out);
    for (kb, vb) in encoded {
        out.extend_from_slice(&kb);
        out.extend_from_slice(&vb);
    }
    Ok(())
}

// ----------------------- Decoder & Validator -----------------------

/// 把 canonical CBOR 字节解码为 [`Value`]。同时强制 canonical 规则。
///
/// 输入必须严格 canonical：违反任何规则都返回 `NonCanonical`。
pub(crate) fn read_canonical(bytes: &[u8]) -> Result<Value, CanonicalError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value: Value = ciborium::de::from_reader(&mut cursor)
        .map_err(|e| CanonicalError::NotCbor(format!("{e}")))?;
    if usize::try_from(cursor.position())
        .map_err(|_| CanonicalError::NotCbor("cursor position too large".to_owned()))?
        != bytes.len()
    {
        return Err(CanonicalError::NotCbor(
            "trailing bytes after CBOR value".to_owned(),
        ));
    }
    // 先做 M0 范围内禁止的值类型检查，否则它们会先于差分检查触发
    // `UnsupportedValue`，错失更精准的 `NonCanonical(FloatPresent / TagPresent)` 分类。
    if contains_float(&value) {
        return Err(CanonicalError::NotCanonical(
            NonCanonicalReason::FloatPresent,
        ));
    }
    if contains_tag(&value) {
        return Err(CanonicalError::NotCanonical(NonCanonicalReason::TagPresent));
    }
    // 通过差分验证 canonical 性：把解码出的值再 write_canonical 一遍，
    // 比对是否等于原字节流。等于即 canonical；不等即非 canonical。
    let mut reencoded = Vec::with_capacity(bytes.len());
    write_canonical(&value, &mut reencoded)?;
    if reencoded != bytes {
        // 找出第一个不一致的字节，给出更精确的诊断
        let reason = diagnose_non_canonical(&value, bytes);
        return Err(CanonicalError::NotCanonical(reason));
    }
    Ok(value)
}

/// 简单的非 canonical 原因诊断。把 value 的部分形态映射回最可能的违规规则。
fn diagnose_non_canonical(value: &Value, bytes: &[u8]) -> NonCanonicalReason {
    // 如果含 float / tag，肯定就是这两类
    if contains_float(value) {
        return NonCanonicalReason::FloatPresent;
    }
    if contains_tag(value) {
        return NonCanonicalReason::TagPresent;
    }
    // 检查 indefinite length：原字节流首字节如果某些 additional info=31 且 major type
    // 为 2/3/4/5 就是 indefinite。
    if bytes.iter().any(|b| {
        let major = b >> 5;
        let info = b & 0x1f;
        info == 31 && (major == 2 || major == 3 || major == 4 || major == 5)
    }) {
        return NonCanonicalReason::IndefiniteLength;
    }
    // 退而求其次：若非以上几类，多半是 integer 不最短或 map 未排序。
    // 我们这里返回 IntegerNotShortest 作为默认，但若 value 含 map，则更可能是 MapKeysNotSorted。
    if contains_map(value) {
        NonCanonicalReason::MapKeysNotSorted
    } else {
        NonCanonicalReason::IntegerNotShortest
    }
}

fn contains_float(v: &Value) -> bool {
    match v {
        Value::Float(_) => true,
        Value::Array(items) => items.iter().any(contains_float),
        Value::Map(entries) => entries
            .iter()
            .any(|(k, v)| contains_float(k) || contains_float(v)),
        Value::Tag(_, inner) => contains_float(inner),
        _ => false,
    }
}

fn contains_tag(v: &Value) -> bool {
    match v {
        Value::Tag(_, _) => true,
        Value::Array(items) => items.iter().any(contains_tag),
        Value::Map(entries) => entries
            .iter()
            .any(|(k, v)| contains_tag(k) || contains_tag(v)),
        _ => false,
    }
}

fn contains_map(v: &Value) -> bool {
    match v {
        Value::Map(_) => true,
        Value::Array(items) => items.iter().any(contains_map),
        Value::Tag(_, inner) => contains_map(inner),
        _ => false,
    }
}

// ----------------------- Public-ish helpers -----------------------

/// 高层入口：把任意可序列化结构编码为 canonical CBOR 字节。
///
/// 流程：先用 ciborium 把值转成 [`Value`]，再走内部的 canonical writer。
pub fn canonical_encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, FaygerError> {
    let v: Value = ciborium::value::Value::serialized(value).map_err(|e| {
        cbor_encode_fail("serialize-to-Value failed")
            .with_context("detail", ContextValue::string(format!("{e}")))
    })?;
    let mut out = Vec::new();
    write_canonical(&v, &mut out).map_err(|e| e.into_fayger_error(CanonicalErrorKind::Encode))?;
    Ok(out)
}

/// 高层入口：把 canonical CBOR 字节解码为任意可反序列化结构。
///
/// 输入必须严格 canonical；非 canonical 或非 CBOR 输入返回 LDR_PARSE_FAIL。
pub fn canonical_decode<T: for<'de> serde::Deserialize<'de>>(
    bytes: &[u8],
) -> Result<T, FaygerError> {
    let v = read_canonical(bytes).map_err(|e| e.into_fayger_error(CanonicalErrorKind::Decode))?;
    v.deserialized().map_err(|e| {
        cbor_decode_fail("deserialize-from-Value failed")
            .with_context("detail", ContextValue::string(format!("{e}")))
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        canonical_decode, canonical_encode, read_canonical, write_canonical, CanonicalError,
        NonCanonicalReason,
    };
    use ciborium::value::Value;

    fn enc(v: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        write_canonical(v, &mut out).expect("encode");
        out
    }

    // ---- shortest integer encoding ----

    #[test]
    fn int_zero_is_one_byte() {
        assert_eq!(enc(&Value::Integer(0_i64.into())), vec![0x00]);
    }

    #[test]
    fn int_23_is_one_byte() {
        assert_eq!(enc(&Value::Integer(23_i64.into())), vec![0x17]);
    }

    #[test]
    fn int_24_uses_one_byte_extension() {
        assert_eq!(enc(&Value::Integer(24_i64.into())), vec![0x18, 0x18]);
    }

    #[test]
    fn int_255_uses_one_byte_extension() {
        assert_eq!(enc(&Value::Integer(255_i64.into())), vec![0x18, 0xff]);
    }

    #[test]
    fn int_256_uses_two_byte_extension() {
        assert_eq!(enc(&Value::Integer(256_i64.into())), vec![0x19, 0x01, 0x00]);
    }

    #[test]
    fn int_negative_one() {
        // CBOR major type 1, value v=0 means -1
        assert_eq!(enc(&Value::Integer((-1_i64).into())), vec![0x20]);
    }

    #[test]
    fn int_negative_100() {
        // -100 -> v = 99
        assert_eq!(enc(&Value::Integer((-100_i64).into())), vec![0x38, 0x63]);
    }

    // ---- definite length collections ----

    #[test]
    fn empty_array() {
        assert_eq!(enc(&Value::Array(vec![])), vec![0x80]);
    }

    #[test]
    fn array_three_ints() {
        let v = Value::Array(vec![
            Value::Integer(1.into()),
            Value::Integer(2.into()),
            Value::Integer(3.into()),
        ]);
        // 0x83 = array(3); 0x01,0x02,0x03 are the ints
        assert_eq!(enc(&v), vec![0x83, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn empty_map() {
        assert_eq!(enc(&Value::Map(vec![])), vec![0xa0]);
    }

    // ---- map key sorting ----

    #[test]
    fn map_keys_sorted_by_byte_sequence() {
        // 输入乱序：z, a；输出应当按 a, z 排列
        let v = Value::Map(vec![
            (Value::Text("z".into()), Value::Integer(2.into())),
            (Value::Text("a".into()), Value::Integer(1.into())),
        ]);
        let bytes = enc(&v);
        // 0xa2 = map(2)
        // 编码 "a" -> 0x61 0x61；编码 "z" -> 0x61 0x7a
        // 期望顺序：a -> 1, z -> 2
        assert_eq!(
            bytes,
            vec![0xa2, 0x61, 0x61, 0x01, 0x61, 0x7a, 0x02],
            "map keys must be sorted by canonical byte sequence"
        );
    }

    #[test]
    fn map_with_duplicate_keys_returns_error() {
        let v = Value::Map(vec![
            (Value::Text("k".into()), Value::Integer(1.into())),
            (Value::Text("k".into()), Value::Integer(2.into())),
        ]);
        let mut out = Vec::new();
        let r = write_canonical(&v, &mut out);
        assert!(matches!(
            r,
            Err(CanonicalError::NotCanonical(
                NonCanonicalReason::DuplicateMapKey
            ))
        ));
    }

    #[test]
    fn map_keys_compared_by_canonical_bytes_not_lexical_text() {
        // Canonical 规则：按 key 的「编码字节序列」排序，而不是按 key 的字面值排序。
        // 数字 10 -> 0x0a；字符 "a" -> 0x61 0x61。
        // 字节序列上 10 < "a"，所以 10 应该在前。
        let v = Value::Map(vec![
            (Value::Text("a".into()), Value::Integer(2.into())),
            (Value::Integer(10.into()), Value::Integer(1.into())),
        ]);
        let bytes = enc(&v);
        assert_eq!(
            bytes,
            vec![0xa2, 0x0a, 0x01, 0x61, 0x61, 0x02],
            "integer key 10 should sort before text key \"a\""
        );
    }

    // ---- bytes / text ----

    #[test]
    fn bytes_short_encoding() {
        let v = Value::Bytes(b"hi".to_vec());
        // 0x42 = bytes(2); 0x68, 0x69 = "hi"
        assert_eq!(enc(&v), vec![0x42, 0x68, 0x69]);
    }

    #[test]
    fn text_short_encoding() {
        let v = Value::Text("hi".into());
        // 0x62 = text(2); 0x68, 0x69
        assert_eq!(enc(&v), vec![0x62, 0x68, 0x69]);
    }

    // ---- bool / null ----

    #[test]
    fn bool_false_and_true() {
        assert_eq!(enc(&Value::Bool(false)), vec![0xf4]);
        assert_eq!(enc(&Value::Bool(true)), vec![0xf5]);
    }

    #[test]
    fn null_encoding() {
        assert_eq!(enc(&Value::Null), vec![0xf6]);
    }

    // ---- forbidden values ----

    #[test]
    fn float_is_rejected() {
        let v = Value::Float(1.5);
        let mut out = Vec::new();
        let r = write_canonical(&v, &mut out);
        assert!(matches!(r, Err(CanonicalError::UnsupportedValue("float"))));
    }

    #[test]
    fn tag_is_rejected() {
        let v = Value::Tag(0, Box::new(Value::Text("2026-01-01".into())));
        let mut out = Vec::new();
        let r = write_canonical(&v, &mut out);
        assert!(matches!(r, Err(CanonicalError::UnsupportedValue("tag"))));
    }

    // ---- read_canonical: round-trip ----

    #[test]
    fn read_canonical_round_trips_a_map() {
        let v = Value::Map(vec![
            (Value::Text("a".into()), Value::Integer(1.into())),
            (Value::Text("z".into()), Value::Integer(2.into())),
        ]);
        let bytes = enc(&v);
        let parsed = read_canonical(&bytes).expect("decode");
        let bytes2 = enc(&parsed);
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn read_canonical_rejects_unsorted_map() {
        // 手工拼一个 map(2)，key 顺序为 z, a（违反 canonical）
        let mut bytes = Vec::new();
        bytes.push(0xa2);
        bytes.extend_from_slice(&[0x61, 0x7a, 0x02]); // "z" -> 2
        bytes.extend_from_slice(&[0x61, 0x61, 0x01]); // "a" -> 1

        let r = read_canonical(&bytes);
        assert!(
            matches!(
                r,
                Err(CanonicalError::NotCanonical(
                    NonCanonicalReason::MapKeysNotSorted
                ))
            ),
            "expected MapKeysNotSorted, got {r:?}"
        );
    }

    #[test]
    fn read_canonical_rejects_non_shortest_int() {
        // 24 编码成 0x18 0x18 是 canonical；但若用 0x19 0x00 0x18 则不最短。
        let bytes = vec![0x19, 0x00, 0x18];
        let r = read_canonical(&bytes);
        assert!(matches!(
            r,
            Err(CanonicalError::NotCanonical(
                NonCanonicalReason::IntegerNotShortest
            ))
        ));
    }

    #[test]
    fn read_canonical_rejects_indefinite_length_array() {
        // 0x9f = array indefinite; 0x01,0x02,0xff = 1, 2, break
        let bytes = vec![0x9f, 0x01, 0x02, 0xff];
        let r = read_canonical(&bytes);
        assert!(matches!(
            r,
            Err(CanonicalError::NotCanonical(
                NonCanonicalReason::IndefiniteLength
            ))
        ));
    }

    #[test]
    fn read_canonical_rejects_float() {
        // 0xfa = float32; 0x40, 0xc0, 0x00, 0x00 = 6.0
        let bytes = vec![0xfa, 0x40, 0xc0, 0x00, 0x00];
        let r = read_canonical(&bytes);
        assert!(matches!(
            r,
            Err(CanonicalError::NotCanonical(
                NonCanonicalReason::FloatPresent
            ))
        ));
    }

    #[test]
    fn read_canonical_rejects_tag() {
        // 0xc0 = tag(0); 0x6a..."2026-01-01" 是日期标签的常见编码
        // 0xc0 + 0x6a + "2026-01-01" 字节
        let mut bytes = vec![0xc0, 0x6a];
        bytes.extend_from_slice(b"2026-01-01");
        let r = read_canonical(&bytes);
        assert!(matches!(
            r,
            Err(CanonicalError::NotCanonical(NonCanonicalReason::TagPresent))
        ));
    }

    #[test]
    fn read_canonical_rejects_trailing_bytes() {
        let v = Value::Integer(1.into());
        let mut bytes = enc(&v);
        bytes.push(0x00); // 多余字节
        let r = read_canonical(&bytes);
        assert!(matches!(r, Err(CanonicalError::NotCbor(_))));
    }

    // ---- high-level encode/decode ----

    #[test]
    fn high_level_encode_decode_round_trip() {
        use std::collections::BTreeMap;
        let mut m = BTreeMap::new();
        m.insert("alpha".to_owned(), 1u32);
        m.insert("beta".to_owned(), 2u32);

        let bytes = canonical_encode(&m).expect("encode");
        let m2: BTreeMap<String, u32> = canonical_decode(&bytes).expect("decode");
        assert_eq!(m, m2);

        // Encoding is deterministic
        let bytes2 = canonical_encode(&m2).expect("re-encode");
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn rfc_8949_examples() {
        // RFC 8949 §3.4.1：选取若干已发布的样例进行字节级核对。
        // Integer 0
        assert_eq!(enc(&Value::Integer(0.into())), vec![0x00]);
        // Integer 1
        assert_eq!(enc(&Value::Integer(1.into())), vec![0x01]);
        // Integer 10
        assert_eq!(enc(&Value::Integer(10.into())), vec![0x0a]);
        // Integer 23
        assert_eq!(enc(&Value::Integer(23.into())), vec![0x17]);
        // Integer 24 (boundary: 24 needs 1-byte extension)
        assert_eq!(enc(&Value::Integer(24.into())), vec![0x18, 0x18]);
        // Integer 100
        assert_eq!(enc(&Value::Integer(100.into())), vec![0x18, 0x64]);
        // Integer 1000
        assert_eq!(enc(&Value::Integer(1000.into())), vec![0x19, 0x03, 0xe8]);
        // Integer 1000000 (1_000_000)
        assert_eq!(
            enc(&Value::Integer(1_000_000.into())),
            vec![0x1a, 0x00, 0x0f, 0x42, 0x40]
        );
        // Integer -1
        assert_eq!(enc(&Value::Integer((-1).into())), vec![0x20]);
        // Integer -10
        assert_eq!(enc(&Value::Integer((-10).into())), vec![0x29]);
        // Integer -100
        assert_eq!(enc(&Value::Integer((-100).into())), vec![0x38, 0x63]);
    }
}
