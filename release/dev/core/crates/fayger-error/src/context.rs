//! `ContextValue`：结构化错误上下文的值类型。
//!
//! 蓝图 §6.1 把 `context` 描述为「面向工具的结构化键值对」。
//! 我们用一个不依赖 `ciborium`/`serde_json` 的小型枚举承载常见标量与容器，
//! 既避免 fayger-error 反向依赖 fayger-buf，也避免对外引入第三方序列化库。

use std::collections::BTreeMap;
use std::fmt;

/// 错误上下文中允许出现的值。
///
/// 设计原则：
/// - 标量层覆盖最常用的 4 类（字符串、有/无符号 64 位整数、布尔）。
/// - 容器层支持列表与映射，足以表达 `missing_fields: ["a", "b"]`、
///   `expected_range: { "min": "1.0.0", "max": "2.0.0" }` 这类场景。
/// - 字节串单独成一类，便于承载 magic、digest 等定长二进制片段。
/// - 不引入 float、不引入 tag。M0 阶段没有这两类需求；后续如有再追加变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextValue {
    /// UTF-8 字符串。
    String(String),
    /// 无符号 64 位整数。
    U64(u64),
    /// 有符号 64 位整数。
    I64(i64),
    /// 布尔值。
    Bool(bool),
    /// 二进制片段（magic、digest 等）。
    Bytes(Vec<u8>),
    /// 异构列表。
    List(Vec<Self>),
    /// 字符串键的有序映射；遍历顺序与插入顺序无关，由键的 Ord 决定。
    Map(BTreeMap<String, Self>),
}

impl ContextValue {
    /// 便利构造：把 `&str` 转为 `String` 变体。
    #[inline]
    #[must_use]
    pub fn string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    /// 便利构造：把 `&[u8]` 拷贝到 `Bytes` 变体。
    #[must_use]
    pub fn bytes(b: impl AsRef<[u8]>) -> Self {
        Self::Bytes(b.as_ref().to_vec())
    }
}

// 一组 From 实现，便于在错误构造现场直接传入原生类型。
impl From<String> for ContextValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}
impl From<&str> for ContextValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_owned())
    }
}
impl From<u64> for ContextValue {
    fn from(v: u64) -> Self {
        Self::U64(v)
    }
}
impl From<i64> for ContextValue {
    fn from(v: i64) -> Self {
        Self::I64(v)
    }
}
impl From<bool> for ContextValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<Vec<u8>> for ContextValue {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}

impl fmt::Display for ContextValue {
    /// 紧凑、人类友好但稳定的 Display。
    /// 不是 JSON 兼容；仅用于日志与调试。完整结构化输出请直接读 `Debug`。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "\"{s}\""),
            Self::U64(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Bytes(b) => {
                f.write_str("0x")?;
                for byte in b {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
            Self::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Self::Map(m) => {
                f.write_str("{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                f.write_str("}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ContextValue;
    use std::collections::BTreeMap;

    #[test]
    fn string_and_int_display() {
        assert_eq!(ContextValue::string("hi").to_string(), "\"hi\"");
        assert_eq!(ContextValue::U64(42).to_string(), "42");
        assert_eq!(ContextValue::I64(-7).to_string(), "-7");
        assert_eq!(ContextValue::Bool(true).to_string(), "true");
    }

    #[test]
    fn bytes_display_is_lowercase_hex() {
        assert_eq!(
            ContextValue::bytes([0x42, 0x55, 0x46, 0x00]).to_string(),
            "0x42554600"
        );
    }

    #[test]
    fn list_display_is_stable() {
        let v = ContextValue::List(vec![
            ContextValue::string("a"),
            ContextValue::U64(1),
            ContextValue::Bool(false),
        ]);
        assert_eq!(v.to_string(), "[\"a\", 1, false]");
    }

    #[test]
    fn map_display_orders_by_key() {
        let mut m = BTreeMap::new();
        m.insert("z".into(), ContextValue::U64(2));
        m.insert("a".into(), ContextValue::U64(1));
        let v = ContextValue::Map(m);
        // BTreeMap 保证遍历按键升序。
        assert_eq!(v.to_string(), "{a: 1, z: 2}");
    }

    #[test]
    fn from_impls_compile() {
        let _: ContextValue = "x".into();
        let _: ContextValue = String::from("x").into();
        let _: ContextValue = 1u64.into();
        let _: ContextValue = (-1i64).into();
        let _: ContextValue = true.into();
        let _: ContextValue = vec![1u8, 2].into();
    }
}
