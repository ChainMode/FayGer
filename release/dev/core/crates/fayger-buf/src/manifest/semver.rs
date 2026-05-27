//! SemVer 字符串的最小解析器与格式化器。
//!
//! 蓝图 §3.2 与设计文档要求 `schema_version` / `runtime_interface_min` 是 SemVer 字符串。
//! M0 阶段我们不引入 `semver` crate；只覆盖 BuF 必要语法子集：
//!
//! - 主版本 / 次版本 / 修订号是 ASCII 数字，无前导零（除单独的 0）。
//! - 可选 `-pre` 段：只允许 `[a-zA-Z0-9.-]+`。
//! - **不**支持 build metadata（`+...`）；BuF 内不需要。
//!
//! 这是个故意做小的解析器：M0 只关心「合法 SemVer 字符串」这一性质，至于版本比较、
//! 兼容性判定等留给 M1+ 的负责模块。

use core::fmt;
use serde::{Deserialize, Serialize};

/// 最小 SemVer 表示。
///
/// 不实现版本比较。M0 只关心解析合法性与回写一致性。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemVer {
    /// 主版本号。
    pub major: u32,
    /// 次版本号。
    pub minor: u32,
    /// 修订号。
    pub patch: u32,
    /// 可选预发布段（不含前导 `-`）。`None` 时为正式版本。
    pub pre: Option<String>,
}

/// SemVer 解析失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSemVerError {
    /// 失败原因（人类可读，便于贴进错误 context）。
    pub reason: String,
}

impl fmt::Display for ParseSemVerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid SemVer: {}", self.reason)
    }
}

impl std::error::Error for ParseSemVerError {}

impl SemVer {
    /// 解析一个 SemVer 字符串。
    ///
    /// 文法：
    ///
    /// ```text
    /// version    = numeric "." numeric "." numeric [ "-" pre ]
    /// numeric    = "0" | <[1-9][0-9]*>
    /// pre        = <[A-Za-z0-9.\-]+>
    /// ```
    pub fn parse(s: &str) -> Result<Self, ParseSemVerError> {
        if s.is_empty() {
            return Err(ParseSemVerError {
                reason: "empty".into(),
            });
        }

        // 切出预发布段（如有）
        let (core, pre) = s
            .find('-')
            .map_or((s, None), |i| (&s[..i], Some(&s[i + 1..])));

        // 切出主.次.修订
        let mut parts = core.split('.');
        let major = parts.next().ok_or_else(|| ParseSemVerError {
            reason: "missing major".into(),
        })?;
        let minor = parts.next().ok_or_else(|| ParseSemVerError {
            reason: "missing minor".into(),
        })?;
        let patch = parts.next().ok_or_else(|| ParseSemVerError {
            reason: "missing patch".into(),
        })?;
        if parts.next().is_some() {
            return Err(ParseSemVerError {
                reason: "too many version parts".into(),
            });
        }

        let major = parse_numeric(major, "major")?;
        let minor = parse_numeric(minor, "minor")?;
        let patch = parse_numeric(patch, "patch")?;

        let pre = match pre {
            None => None,
            Some("") => {
                return Err(ParseSemVerError {
                    reason: "empty pre-release segment".into(),
                });
            }
            Some(p) => {
                validate_pre(p)?;
                Some(p.to_owned())
            }
        };

        Ok(Self {
            major,
            minor,
            patch,
            pre,
        })
    }
}

fn parse_numeric(s: &str, label: &str) -> Result<u32, ParseSemVerError> {
    if s.is_empty() {
        return Err(ParseSemVerError {
            reason: format!("{label} is empty"),
        });
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(ParseSemVerError {
            reason: format!("{label} has leading zero: {s}"),
        });
    }
    if !s.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseSemVerError {
            reason: format!("{label} contains non-digit: {s}"),
        });
    }
    s.parse::<u32>().map_err(|_| ParseSemVerError {
        reason: format!("{label} overflows u32: {s}"),
    })
}

fn validate_pre(s: &str) -> Result<(), ParseSemVerError> {
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err(ParseSemVerError {
            reason: format!("pre-release contains illegal character: {s}"),
        });
    }
    Ok(())
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

// SemVer 在 CBOR 中以字符串形式保存，便于人类阅读且 canonical encoder 处理简单。

impl Serialize for SemVer {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for SemVer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <String as Deserialize>::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::SemVer;

    #[test]
    fn parses_simple() {
        let v = SemVer::parse("1.2.3").expect("ok");
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_none());
    }

    #[test]
    fn parses_with_pre() {
        let v = SemVer::parse("1.0.0-alpha.1").expect("ok");
        assert_eq!(v.major, 1);
        assert_eq!(v.pre.as_deref(), Some("alpha.1"));
    }

    #[test]
    fn parses_zero_zero_zero() {
        let v = SemVer::parse("0.0.0").expect("ok");
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn rejects_leading_zero() {
        assert!(SemVer::parse("01.0.0").is_err());
        assert!(SemVer::parse("0.01.0").is_err());
        assert!(SemVer::parse("0.0.01").is_err());
    }

    #[test]
    fn rejects_non_digits_in_numeric() {
        assert!(SemVer::parse("1.2.x").is_err());
        assert!(SemVer::parse("a.0.0").is_err());
    }

    #[test]
    fn rejects_too_many_parts() {
        assert!(SemVer::parse("1.2.3.4").is_err());
    }

    #[test]
    fn rejects_too_few_parts() {
        assert!(SemVer::parse("1.2").is_err());
        assert!(SemVer::parse("1").is_err());
        assert!(SemVer::parse("").is_err());
    }

    #[test]
    fn rejects_empty_pre() {
        assert!(SemVer::parse("1.0.0-").is_err());
    }

    #[test]
    fn rejects_illegal_pre_chars() {
        assert!(SemVer::parse("1.0.0-foo!bar").is_err());
        assert!(SemVer::parse("1.0.0-_underscore").is_err());
    }

    #[test]
    fn does_not_support_build_metadata() {
        // BuF 用不到 build metadata；M0 显式不支持。
        assert!(SemVer::parse("1.0.0+meta").is_err());
    }

    #[test]
    fn display_roundtrips_through_parse() {
        for s in ["0.0.0", "1.2.3", "10.20.30", "1.0.0-alpha", "2.5.1-rc.1"] {
            let v = SemVer::parse(s).expect("parse");
            assert_eq!(v.to_string(), s);
            let v2 = SemVer::parse(&v.to_string()).expect("parse roundtrip");
            assert_eq!(v, v2);
        }
    }

    #[test]
    fn rejects_overflow() {
        // u32::MAX = 4_294_967_295；超过应失败。
        assert!(SemVer::parse("99999999999.0.0").is_err());
    }

    #[test]
    fn cbor_roundtrips_via_serde() {
        use crate::canonical::{canonical_decode, canonical_encode};
        let v = SemVer::parse("1.2.3-rc.1").expect("parse");
        let bytes = canonical_encode(&v).expect("encode");
        let v2: SemVer = canonical_decode(&bytes).expect("decode");
        assert_eq!(v, v2);
    }
}
