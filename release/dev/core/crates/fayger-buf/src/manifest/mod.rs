//! BuF Manifest 与子类型。
//!
//! 子模块：
//!
//! - [`mod@semver`]：SemVer 字符串的最小解析器。
//! - [`mod@types`]：SectionDescriptor、SectionKind、Digest、SectionVisibility、
//!   ProfileConstraints、EntryPoint、SelectionStrategy、RuntimeRequirement、
//!   ResourceQuota、SignatureBlock。
//!
//! 顶层 [`BuFManifest`] 在本模块内定义。M0 阶段使用 plain `serde` derive；
//! 顶层未知字段进 `extensions` 的自定义反序列化由任务 7 完成。

pub mod semver;
pub mod types;

pub use semver::{ParseSemVerError, SemVer};
pub use types::{
    Digest, EntryPoint, ProfileConstraints, ResourceQuota, RuntimeRequirement, SectionDescriptor,
    SectionKind, SectionVisibility, SelectionStrategy, SignatureBlock,
};

use ciborium::value::Value;
use fayger_error::{ContextValue, FaygerError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// BuF Manifest：Fay 制品的元数据清单。
///
/// 蓝图 §3.2 与设计文档 §`manifest` 模块定义。M0 阶段用 plain serde derive；
/// 任务 7 会替换 Serialize/Deserialize 为支持顶层 `extensions` 的自定义实现。
///
/// 必填字段：
///
/// - [`Self::schema_version`]
/// - [`Self::runtime_interface_min`]
/// - [`Self::entry`]
/// - [`Self::sections`]（可以是空列表）
///
/// 可选字段（默认为缺省）：其余皆是。
///
/// # Eq 与 extensions
///
/// `extensions` 字段使用 [`ciborium::value::Value`]，后者**不实现** `Eq`
/// （因为 `Value::Float` 含 NaN）。因此 [`BuFManifest`] 只实现 `PartialEq`，
/// 不实现 `Eq`。M0 的 canonical 编码器拒绝写入 float，所以实际产出的 manifest
/// 不会含 NaN，但类型层面我们尊重 ciborium 的限制。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuFManifest {
    /// 该 BuF 遵循的 schema 版本。
    pub schema_version: SemVer,
    /// 该 BuF 所需的 Runtime_Interface 最低版本。
    pub runtime_interface_min: SemVer,

    /// 由 Fayger 在加载时填入的弃用提示。M0 阶段总为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation_notice: Option<String>,

    /// 入口点（M0 是字符串）。
    pub entry: EntryPoint,

    /// 运行时实现选择需求。M0 不实际使用，但保留 round-trip。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeRequirement>,

    /// BuF 请求的能力集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<String>,

    /// 资源配额。M0 占位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quotas: Option<ResourceQuota>,

    /// Section 描述符列表。允许为空（无 sections 的 BuF 也合法）。
    pub sections: Vec<SectionDescriptor>,

    /// 签名块。M0 总为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureBlock>,

    /// 顶层 extensions：保留未知字段以便未来 schema 演进。
    ///
    /// M0 阶段：通过 plain serde derive 时只能容纳显式列出的 `extensions` 键；
    /// 任务 7 会改为自定义 Serialize/Deserialize，让任何未知顶层键自动汇入此处。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
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
    /// 失败时返回 [`fayger_error::ErrorCode::LdrMissingRequiredField`]，
    /// `context.missing_fields` 列出所有缺失字段名（与 `BuFManifest` 字段名一致），
    /// 或在 sections 一致性失败时填 `context.reason = "SectionInconsistency"`。
    pub fn validate(&self) -> Result<(), FaygerError> {
        let mut missing: Vec<String> = Vec::new();
        if self.entry.as_str().is_empty() {
            missing.push("entry".into());
        }
        // schema_version / runtime_interface_min 在 SemVer 反序列化阶段已经过解析，
        // 此处不再做额外检查。

        // sections 列表自身允许为空，但其中条目不能空 id 或重复 id
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
            let mut err = FaygerError::new(
                fayger_error::ErrorCode::LdrMissingRequiredField,
                fayger_error::Layer::Loader,
                "BuFManifest sections list has invalid section ids",
            )
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

/// 给定缺失字段列表，构造 LDR_MISSING_REQUIRED_FIELD 错误。
fn missing_required_field(fields: &[String]) -> FaygerError {
    ldr_missing_field("BuFManifest is missing required fields")
        .with_context(
            "missing_fields",
            ContextValue::List(fields.iter().cloned().map(ContextValue::String).collect()),
        )
        .with_context("reason", ContextValue::string("ManifestMissingField"))
}

/// 构造一个 `LDR_MISSING_REQUIRED_FIELD` 错误。供 manifest 与 object 模块共用。
pub(crate) fn ldr_missing_field(message: impl Into<String>) -> FaygerError {
    FaygerError::new(
        fayger_error::ErrorCode::LdrMissingRequiredField,
        fayger_error::Layer::Loader,
        message,
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::canonical::{canonical_decode, canonical_encode};

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
        // entry 应该出现在 missing_fields
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
            id: "code".into(), // 与已有 section 重复
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
        // duplicate_ids 应包含 "code"
        let dups = err.context.get("duplicate_ids").expect("has duplicate_ids");
        assert!(matches!(dups, ContextValue::List(l) if l.iter().any(|v|
            matches!(v, ContextValue::String(s) if s == "code"))));
    }

    #[test]
    fn invalid_semver_string_is_rejected_at_decode() {
        // 手工拼一个 BuFManifest map，但 schema_version 是非法字符串。
        use ciborium::value::Value;
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
        crate::canonical::write_canonical(&m, &mut bytes).expect("write");
        let r: Result<BuFManifest, _> = canonical_decode(&bytes);
        assert!(r.is_err(), "decoder should reject invalid SemVer string");
    }

    #[test]
    fn sections_field_order_in_canonical_cbor_is_deterministic() {
        // 同一 manifest 序列化两次应得到相同字节。
        let m = sample_manifest();
        let a = canonical_encode(&m).expect("encode 1");
        let b = canonical_encode(&m).expect("encode 2");
        assert_eq!(a, b);
    }
}
