//! BuF Manifest 与子类型。
//!
//! 子模块：
//!
//! - [`mod@semver`]：SemVer 字符串的最小解析器。
//! - [`mod@types`]：SectionDescriptor、SectionKind、Digest、SectionVisibility、
//!   ProfileConstraints、EntryPoint、SelectionStrategy、RuntimeRequirement、
//!   ResourceQuota、SignatureBlock。
//!
//! 顶层 [`BuFManifest`] 在本模块内定义，使用自定义 `Serialize` / `Deserialize`：
//! 反序列化时未知顶层字段自动汇入 `extensions`；序列化时 `extensions` 中的键值对
//! 与已知字段同级展开。

pub mod semver;
pub mod types;

pub use semver::{ParseSemVerError, SemVer};
pub use types::{
    Digest, EntryPoint, ProfileConstraints, ResourceQuota, RuntimeRequirement, SectionDescriptor,
    SectionKind, SectionVisibility, SelectionStrategy, SignatureBlock,
};

use ciborium::value::Value;
use fayger_error::{ContextValue, FaygerError};
use serde::de::Error as DeError;
use serde::ser::{Error as SerError, SerializeMap};
use std::collections::{BTreeMap, BTreeSet};

/// `BuFManifest` 顶层的已知字段名。任何不在此集合中的顶层 key 反序列化时进入 `extensions`。
const KNOWN_FIELDS: &[&str] = &[
    "schema_version",
    "runtime_interface_min",
    "deprecation_notice",
    "entry",
    "runtime",
    "capabilities",
    "quotas",
    "sections",
    "signature",
];

/// BuF Manifest：Fay 制品的元数据清单。
///
/// 蓝图 §3.2 与设计文档 §`manifest` 模块定义。
///
/// # 必填字段
///
/// - [`Self::schema_version`]
/// - [`Self::runtime_interface_min`]
/// - [`Self::entry`]
/// - [`Self::sections`]（可以是空列表）
///
/// 其余字段都是可选；缺省时使用 [`Default`]。
///
/// # 顶层 extensions
///
/// `BuFManifest` 实现自定义的 `Serialize` / `Deserialize`：
///
/// - 序列化时，已知字段与 `extensions` 中的键值对**在同一层**输出（extensions 不嵌一层）；
///   canonical encoder 会按 key 字节序对所有键统一排序。
/// - 反序列化时，已知字段被识别后从 map 中取走，**剩余键**进入 `extensions`。
///
/// 这一行为让未来 schema 演进可以增加新字段而不破坏旧 BuF 的 round-trip。
///
/// # `Eq` 与 `extensions`
///
/// `extensions` 字段使用 [`ciborium::value::Value`]，后者**不实现** `Eq`
/// （因为 `Value::Float` 含 NaN）。因此 [`BuFManifest`] 只实现 `PartialEq`，不实现 `Eq`。
/// M0 的 canonical 编码器拒绝写入 float，所以实际产出的 manifest 不会含 NaN。
#[derive(Debug, Clone, PartialEq)]
pub struct BuFManifest {
    /// 该 BuF 遵循的 schema 版本。
    pub schema_version: SemVer,
    /// 该 BuF 所需的 Runtime_Interface 最低版本。
    pub runtime_interface_min: SemVer,

    /// 由 Fayger 在加载时填入的弃用提示。M0 阶段总为 None。
    pub deprecation_notice: Option<String>,

    /// 入口点（M0 是字符串）。
    pub entry: EntryPoint,

    /// 运行时实现选择需求。M0 不实际使用，但保留 round-trip。
    pub runtime: Option<RuntimeRequirement>,

    /// BuF 请求的能力集合。
    pub capabilities: BTreeSet<String>,

    /// 资源配额。M0 占位。
    pub quotas: Option<ResourceQuota>,

    /// Section 描述符列表。允许为空（无 sections 的 BuF 也合法）。
    pub sections: Vec<SectionDescriptor>,

    /// 签名块。M0 总为 None。
    pub signature: Option<SignatureBlock>,

    /// 顶层 extensions：未知字段自动汇入此处，序列化时与已知字段平级展开。
    pub extensions: BTreeMap<String, Value>,
}

impl BuFManifest {
    /// 校验必填字段非默认值且 SemVer 合法。
    ///
    /// M0 阶段的「必填」语义：
    ///
    /// - `entry.0` 必须非空字符串。
    /// - `sections` 列表中每个 `SectionDescriptor.id` 必须非空且唯一。
    /// - `schema_version` 与 `runtime_interface_min` 由类型系统保证已是合法 SemVer
    ///   （在反序列化阶段就会被 `SemVer::parse` 拒绝非法值）。
    ///
    /// 失败时返回 [`fayger_error::ErrorCode::LdrMissingRequiredField`]。
    pub fn validate(&self) -> Result<(), FaygerError> {
        let mut missing: Vec<String> = Vec::new();
        if self.entry.as_str().is_empty() {
            missing.push("entry".into());
        }

        let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
        let mut empty_id_indexes: Vec<u64> = Vec::new();
        let mut duplicate_ids: Vec<String> = Vec::new();
        for (i, sd) in self.sections.iter().enumerate() {
            if sd.id.is_empty() {
                let idx = u64::try_from(i).unwrap_or(u64::MAX);
                empty_id_indexes.push(idx);
            } else if !seen_ids.insert(sd.id.as_str()) {
                duplicate_ids.push(sd.id.clone());
            }
        }

        if !missing.is_empty() {
            return Err(missing_required_field(&missing));
        }

        if !empty_id_indexes.is_empty() || !duplicate_ids.is_empty() {
            let mut err = ldr_missing_field("BuFManifest sections list has invalid section ids")
                .with_context("reason", ContextValue::string("SectionInconsistency"));
            if !empty_id_indexes.is_empty() {
                err = err.with_context(
                    "empty_id_indexes",
                    ContextValue::List(
                        empty_id_indexes
                            .into_iter()
                            .map(ContextValue::U64)
                            .collect(),
                    ),
                );
            }
            if !duplicate_ids.is_empty() {
                err = err.with_context(
                    "duplicate_ids",
                    ContextValue::List(
                        duplicate_ids
                            .into_iter()
                            .map(ContextValue::String)
                            .collect(),
                    ),
                );
            }
            return Err(err);
        }

        Ok(())
    }
}

fn missing_required_field(fields: &[String]) -> FaygerError {
    ldr_missing_field("BuFManifest is missing required fields")
        .with_context(
            "missing_fields",
            ContextValue::List(fields.iter().cloned().map(ContextValue::String).collect()),
        )
        .with_context("reason", ContextValue::string("ManifestMissingField"))
}

pub(crate) fn ldr_missing_field(message: impl Into<String>) -> FaygerError {
    FaygerError::new(
        fayger_error::ErrorCode::LdrMissingRequiredField,
        fayger_error::Layer::Loader,
        message,
    )
}

// ----------------------- Serialize / Deserialize -----------------------

/// 把已知字段与 extensions 平级展开后实际写入的中间表示。
///
/// 用一个临时的 `BTreeMap` 收集所有键值对（已知 + extension），让
/// canonical encoder 在最终写入 CBOR 时按 key 字节序排序。
fn collect_known_entries(m: &BuFManifest) -> Result<BTreeMap<String, Value>, FaygerError> {
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    out.insert("schema_version".to_owned(), to_value(&m.schema_version)?);
    out.insert(
        "runtime_interface_min".to_owned(),
        to_value(&m.runtime_interface_min)?,
    );
    if let Some(d) = &m.deprecation_notice {
        out.insert("deprecation_notice".to_owned(), Value::Text(d.clone()));
    }
    out.insert("entry".to_owned(), to_value(&m.entry)?);
    if let Some(r) = &m.runtime {
        out.insert("runtime".to_owned(), to_value(r)?);
    }
    if !m.capabilities.is_empty() {
        out.insert("capabilities".to_owned(), to_value(&m.capabilities)?);
    }
    if let Some(q) = &m.quotas {
        out.insert("quotas".to_owned(), to_value(q)?);
    }
    out.insert("sections".to_owned(), to_value(&m.sections)?);
    if let Some(s) = &m.signature {
        out.insert("signature".to_owned(), to_value(s)?);
    }
    Ok(out)
}

fn to_value<T: serde::Serialize>(v: &T) -> Result<Value, FaygerError> {
    Value::serialized(v).map_err(|e| {
        FaygerError::new(
            fayger_error::ErrorCode::LdrParseFail,
            fayger_error::Layer::Loader,
            "serialize-to-Value failed",
        )
        .with_context("detail", ContextValue::string(format!("{e}")))
    })
}

impl serde::Serialize for BuFManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let known = collect_known_entries(self).map_err(S::Error::custom)?;
        // 检测扩展键是否与已知键重名：这是非法的（语义不清）。
        for k in self.extensions.keys() {
            if KNOWN_FIELDS.contains(&k.as_str()) {
                return Err(S::Error::custom(format!(
                    "extensions contain reserved key: {k}"
                )));
            }
        }
        let total = known.len() + self.extensions.len();
        let mut map = serializer.serialize_map(Some(total))?;
        // 已知字段
        for (k, v) in &known {
            map.serialize_entry(k, v)?;
        }
        // extensions
        for (k, v) in &self.extensions {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for BuFManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // 先反序列化为 BTreeMap<String, Value>。
        let mut all: BTreeMap<String, Value> =
            <BTreeMap<String, Value> as serde::Deserialize>::deserialize(deserializer)?;

        // 必填字段
        let schema_version: SemVer =
            extract_required(&mut all, "schema_version").map_err(D::Error::custom)?;
        let runtime_interface_min: SemVer =
            extract_required(&mut all, "runtime_interface_min").map_err(D::Error::custom)?;
        let entry: EntryPoint = extract_required(&mut all, "entry").map_err(D::Error::custom)?;
        let sections: Vec<SectionDescriptor> =
            extract_required(&mut all, "sections").map_err(D::Error::custom)?;

        // 可选字段
        let deprecation_notice: Option<String> =
            extract_optional(&mut all, "deprecation_notice").map_err(D::Error::custom)?;
        let runtime: Option<RuntimeRequirement> =
            extract_optional(&mut all, "runtime").map_err(D::Error::custom)?;
        let capabilities: BTreeSet<String> = extract_optional(&mut all, "capabilities")
            .map_err(D::Error::custom)?
            .unwrap_or_default();
        let quotas: Option<ResourceQuota> =
            extract_optional(&mut all, "quotas").map_err(D::Error::custom)?;
        let signature: Option<SignatureBlock> =
            extract_optional(&mut all, "signature").map_err(D::Error::custom)?;

        // 剩下的就是 extensions
        let extensions: BTreeMap<String, Value> = all;

        Ok(Self {
            schema_version,
            runtime_interface_min,
            deprecation_notice,
            entry,
            runtime,
            capabilities,
            quotas,
            sections,
            signature,
            extensions,
        })
    }
}

fn extract_required<T>(all: &mut BTreeMap<String, Value>, key: &str) -> Result<T, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    // map_or_else 的版本对这两个分支的可读性损耗大于收益，保留 match。
    #[allow(clippy::option_if_let_else)]
    match all.remove(key) {
        Some(v) => v
            .deserialized()
            .map_err(|e| format!("invalid value for `{key}`: {e}")),
        None => Err(format!("missing required field `{key}`")),
    }
}

fn extract_optional<T>(all: &mut BTreeMap<String, Value>, key: &str) -> Result<Option<T>, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    #[allow(clippy::option_if_let_else)]
    match all.remove(key) {
        Some(v) => v
            .deserialized()
            .map(Some)
            .map_err(|e| format!("invalid value for `{key}`: {e}")),
        None => Ok(None),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::canonical::{canonical_decode, canonical_encode, write_canonical};
    use ciborium::value::Value;

    fn sample_manifest() -> BuFManifest {
        BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![SectionDescriptor {
                id: "code".into(),
                kind: SectionKind::Code,
                digest: Digest::placeholder(),
                length: 4,
                visibility: SectionVisibility::Required,
                profile_constraints: ProfileConstraints::default(),
            }],
            signature: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn sample_manifest_validates_ok() {
        sample_manifest().validate().expect("validate ok");
    }

    #[test]
    fn empty_sections_is_allowed_by_validate() {
        let mut m = sample_manifest();
        m.sections.clear();
        m.validate().expect("empty sections is valid");
    }

    #[test]
    fn manifest_round_trips_through_canonical_cbor() {
        let m = sample_manifest();
        let bytes = canonical_encode(&m).expect("encode");
        let back: BuFManifest = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, m);

        // 编码确定性
        let bytes2 = canonical_encode(&back).expect("re-encode");
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn validate_rejects_empty_entry() {
        let mut m = sample_manifest();
        m.entry = EntryPoint::new("");
        let err = m.validate().expect_err("expected error");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        let mf = err
            .context
            .get("missing_fields")
            .expect("has missing_fields");
        let list = match mf {
            ContextValue::List(l) => l,
            other => panic!("expected list, got {other:?}"),
        };
        assert!(list
            .iter()
            .any(|v| matches!(v, ContextValue::String(s) if s == "entry"),));
    }

    #[test]
    fn validate_rejects_empty_section_id() {
        let mut m = sample_manifest();
        m.sections[0].id = String::new();
        let err = m.validate().expect_err("expected error");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "SectionInconsistency"
        ));
    }

    #[test]
    fn validate_rejects_duplicate_section_ids() {
        let mut m = sample_manifest();
        m.sections.push(SectionDescriptor {
            id: "code".into(),
            kind: SectionKind::Data,
            digest: Digest::placeholder(),
            length: 0,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        });
        let err = m.validate().expect_err("expected error");
        assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        assert!(matches!(
            err.context.get("reason"),
            Some(ContextValue::String(s)) if s == "SectionInconsistency"
        ));
        let dups = err.context.get("duplicate_ids").expect("has duplicate_ids");
        assert!(matches!(dups, ContextValue::List(l) if l.iter().any(|v|
            matches!(v, ContextValue::String(s) if s == "code"))));
    }

    #[test]
    fn invalid_semver_string_is_rejected_at_decode() {
        let m = Value::Map(vec![
            (
                Value::Text("schema_version".into()),
                Value::Text("not-a-semver".into()),
            ),
            (
                Value::Text("runtime_interface_min".into()),
                Value::Text("1.0.0".into()),
            ),
            (Value::Text("entry".into()), Value::Text("main".into())),
            (Value::Text("sections".into()), Value::Array(vec![])),
        ]);
        let mut bytes = Vec::new();
        write_canonical(&m, &mut bytes).expect("write");
        let r: Result<BuFManifest, _> = canonical_decode(&bytes);
        assert!(r.is_err(), "decoder should reject invalid SemVer string");
    }

    #[test]
    fn sections_field_order_in_canonical_cbor_is_deterministic() {
        let m = sample_manifest();
        let a = canonical_encode(&m).expect("encode 1");
        let b = canonical_encode(&m).expect("encode 2");
        assert_eq!(a, b);
    }

    // -------- 任务 7 的核心测试：顶层 extensions 行为 --------

    #[test]
    fn unknown_top_level_field_flows_into_extensions() {
        // 构造 CBOR map：含一个未知键 "x_custom"。
        let m = Value::Map(vec![
            (
                Value::Text("schema_version".into()),
                Value::Text("1.0.0".into()),
            ),
            (
                Value::Text("runtime_interface_min".into()),
                Value::Text("1.0.0".into()),
            ),
            (Value::Text("entry".into()), Value::Text("main".into())),
            (Value::Text("sections".into()), Value::Array(vec![])),
            (Value::Text("x_custom".into()), Value::Text("hello".into())),
        ]);
        let mut bytes = Vec::new();
        write_canonical(&m, &mut bytes).expect("write");

        let parsed: BuFManifest = canonical_decode(&bytes).expect("decode");
        assert!(parsed.extensions.contains_key("x_custom"));
        assert!(matches!(
            parsed.extensions.get("x_custom"),
            Some(Value::Text(s)) if s == "hello"
        ));
    }

    #[test]
    fn extensions_round_trip() {
        let mut m = sample_manifest();
        m.extensions
            .insert("x_int".to_owned(), Value::Integer(42_i64.into()));
        m.extensions
            .insert("z_bytes".to_owned(), Value::Bytes(vec![1, 2, 3]));

        let bytes = canonical_encode(&m).expect("encode");
        let back: BuFManifest = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, m, "extensions should round-trip");

        // 字节级强往返
        let bytes2 = canonical_encode(&back).expect("re-encode");
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn extensions_serialized_at_top_level_not_nested() {
        // 序列化后 manifest 是一个顶层 map；extensions 中的键应在顶层，
        // 不出现以 "extensions" 为名的嵌套字段。
        let mut m = sample_manifest();
        m.extensions
            .insert("foo".to_owned(), Value::Text("bar".into()));

        let bytes = canonical_encode(&m).expect("encode");
        let raw: Value = canonical_decode(&bytes).expect("decode raw");
        let entries = match raw {
            Value::Map(e) => e,
            other => panic!("expected top-level map, got {other:?}"),
        };
        let keys: Vec<String> = entries
            .iter()
            .filter_map(|(k, _)| match k {
                Value::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert!(
            keys.contains(&"foo".to_owned()),
            "foo should be at top level"
        );
        assert!(
            !keys.contains(&"extensions".to_owned()),
            "extensions key should not be present in serialized form"
        );
    }

    #[test]
    fn extensions_with_known_key_is_rejected_at_serialize() {
        // 把 "schema_version" 放到 extensions 中应当导致序列化失败：
        // 这避免与已知字段语义冲突。
        let mut m = sample_manifest();
        m.extensions
            .insert("schema_version".to_owned(), Value::Text("9.9.9".into()));
        let r = canonical_encode(&m);
        assert!(
            r.is_err(),
            "extensions containing reserved keys should fail to serialize"
        );
    }

    #[test]
    fn missing_required_field_in_decode_fails_with_clear_message() {
        // 没 entry 字段
        let m = Value::Map(vec![
            (
                Value::Text("schema_version".into()),
                Value::Text("1.0.0".into()),
            ),
            (
                Value::Text("runtime_interface_min".into()),
                Value::Text("1.0.0".into()),
            ),
            (Value::Text("sections".into()), Value::Array(vec![])),
        ]);
        let mut bytes = Vec::new();
        write_canonical(&m, &mut bytes).expect("write");
        let r: Result<BuFManifest, _> = canonical_decode(&bytes);
        let err = r.expect_err("expected missing-field error");
        // FaygerError 的 message 包含 detail 里的 "missing required field `entry`"
        assert!(
            format!("{err}").contains("entry"),
            "error should mention missing field `entry`: got {err}"
        );
    }
}
