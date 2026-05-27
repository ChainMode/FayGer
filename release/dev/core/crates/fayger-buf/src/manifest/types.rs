//! Manifest 中的子结构与枚举类型。
//!
//! 蓝图 §3.2 与设计文档 §`manifest` 模块的 plain serde derive 实现。
//! 嵌套结构使用 `#[serde(deny_unknown_fields)]` 严格匹配字段集合。

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// ----------------------- Section 描述符 -----------------------

/// 一个 Section 的元数据。
///
/// 与 BuF 的 Section_Index 段中每条记录的字段一一对应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDescriptor {
    /// Section 标识。同一个 BuF 内必须唯一。
    pub id: String,
    /// Section 类型。
    pub kind: SectionKind,
    /// Section 内容的摘要（M0 阶段仅占位，不参与校验）。
    pub digest: Digest,
    /// Section payload 的字节长度。
    pub length: u64,
    /// Section 在当前 LoadProfile 下是必需还是可选。M0 默认 [`SectionVisibility::Required`]。
    pub visibility: SectionVisibility,
    /// 加载该 Section 需要满足的 profile 约束。M0 默认 [`ProfileConstraints::default`]。
    pub profile_constraints: ProfileConstraints,
}

/// Section 类型枚举。
///
/// `Custom(name)` 允许第三方扩展，但仍需走标准 CBOR 编码（不开启未知字符串）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// 可执行代码段。
    Code,
    /// 普通数据段。
    Data,
    /// 资源段（图片、字符串表等）。
    Asset,
    /// 签名段。M0 阶段未启用签名，但保留枚举值。
    Signature,
    /// 自定义类别；字符串作为名称，由具体扩展定义。
    Custom(String),
}

/// 内容摘要。M0 阶段仅占位，digest 字段不参与一致性校验。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Digest {
    /// 摘要算法标识。例如 `"sha256"`。
    pub algorithm: String,
    /// 摘要字节。
    pub value: Vec<u8>,
}

impl Digest {
    /// M0 阶段的占位 digest：算法标识为 `sha256`，值为全零。
    #[must_use]
    pub fn placeholder() -> Self {
        Self {
            algorithm: "sha256".into(),
            value: vec![0u8; 32],
        }
    }
}

// ----------------------- 可见性与 profile 约束 -----------------------

/// Section 在 LoadProfile 下的可见性等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionVisibility {
    /// 缺失视为加载失败。
    Required,
    /// 缺失允许降级处理。
    Optional,
}

/// Section 的 profile 约束。M0 默认空（所有字段为空集 / `None`），不在加载阶段实际生效。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileConstraints {
    /// 加载该 Section 所需的能力集（capabilities）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_capabilities: BTreeSet<String>,
    /// Section 体积上限。`None` 表示无上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    /// 加载该 Section 所需的运行时特性集（features）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_features: BTreeSet<String>,
}

// ----------------------- Entry / Runtime / Quotas / Signature -----------------------

/// BuF 入口点。M0 阶段是简单字符串；后续里程碑会扩展为带参数的结构。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntryPoint(pub String);

impl EntryPoint {
    /// 从字符串构造。
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// 取入口名。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 运行时实现选择策略。蓝图 §3.2。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    /// 必须使用 `preferred_impl`，不可降级。
    Strict,
    /// 优先 `preferred_impl`；若不可用则任意兼容实现。
    PreferThenAny,
    /// 任意兼容实现。
    Any,
}

/// Runtime 选择需求。M0 不实际使用，但需要保留 round-trip。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRequirement {
    /// 偏好的实现 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_impl: Option<String>,
    /// 选择策略。
    pub selection_strategy: SelectionStrategy,
}

/// 资源配额声明。M0 仅占位字段。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceQuota {
    /// 每秒允许使用的 CPU 毫秒数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_ms_per_sec: Option<u32>,
    /// 内存上限（字节）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    /// 每秒允许的 I/O 流量（字节）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_bytes_per_sec: Option<u64>,
}

/// 签名块。M0 阶段 BuFManifest::signature 总为 None；保留结构以便序列化兼容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureBlock {
    /// 签名算法标识。例如 `"ed25519"`。
    pub algorithm: String,
    /// 公钥引用。
    pub public_key_ref: String,
    /// 签名字节。
    pub signed_digest: Vec<u8>,
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::canonical::{canonical_decode, canonical_encode};

    #[test]
    fn section_kind_unit_variants_serialize_as_strings() {
        // 单元 variant 默认序列化为字符串。
        let bytes = canonical_encode(&SectionKind::Code).expect("encode");
        let back: SectionKind = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, SectionKind::Code);
    }

    #[test]
    fn section_kind_custom_round_trips() {
        let kind = SectionKind::Custom("debug".into());
        let bytes = canonical_encode(&kind).expect("encode");
        let back: SectionKind = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, kind);
    }

    #[test]
    fn digest_placeholder_has_sha256_zeros() {
        let d = Digest::placeholder();
        assert_eq!(d.algorithm, "sha256");
        assert_eq!(d.value, vec![0u8; 32]);
    }

    #[test]
    fn section_visibility_serializes_snake_case() {
        let bytes = canonical_encode(&SectionVisibility::Required).expect("encode");
        let back: SectionVisibility = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, SectionVisibility::Required);

        let bytes = canonical_encode(&SectionVisibility::Optional).expect("encode");
        let back: SectionVisibility = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, SectionVisibility::Optional);
    }

    #[test]
    fn profile_constraints_default_is_empty() {
        let pc = ProfileConstraints::default();
        assert!(pc.required_capabilities.is_empty());
        assert!(pc.max_size.is_none());
        assert!(pc.required_features.is_empty());
    }

    #[test]
    fn profile_constraints_round_trip() {
        let pc = ProfileConstraints {
            required_capabilities: ["fs.read", "net.http"]
                .into_iter()
                .map(String::from)
                .collect(),
            max_size: Some(1024),
            required_features: std::iter::once("realtime".into()).collect(),
        };
        let bytes = canonical_encode(&pc).expect("encode");
        let back: ProfileConstraints = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, pc);
    }

    #[test]
    fn section_descriptor_round_trip() {
        let sd = SectionDescriptor {
            id: "code-main".into(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: 4096,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        };
        let bytes = canonical_encode(&sd).expect("encode");
        let back: SectionDescriptor = canonical_decode(&bytes).expect("decode");
        assert_eq!(back, sd);
    }

    #[test]
    fn section_descriptor_rejects_unknown_field() {
        // 故意手工拼一个含未知字段的 CBOR map。
        // sections 内部结构应当 deny_unknown_fields。
        use ciborium::value::Value;
        let extra = Value::Map(vec![
            (Value::Text("id".into()), Value::Text("x".into())),
            (Value::Text("kind".into()), Value::Text("code".into())),
            (
                Value::Text("digest".into()),
                Value::Map(vec![
                    (
                        Value::Text("algorithm".into()),
                        Value::Text("sha256".into()),
                    ),
                    (Value::Text("value".into()), Value::Bytes(vec![0u8; 32])),
                ]),
            ),
            (Value::Text("length".into()), Value::Integer(0_i64.into())),
            (
                Value::Text("visibility".into()),
                Value::Text("required".into()),
            ),
            (
                Value::Text("profile_constraints".into()),
                Value::Map(vec![]),
            ),
            // 未知字段
            (
                Value::Text("zzz_unknown".into()),
                Value::Text("nope".into()),
            ),
        ]);
        let mut bytes = Vec::new();
        crate::canonical::write_canonical(&extra, &mut bytes).expect("write");
        let r: Result<SectionDescriptor, _> = canonical_decode(&bytes);
        assert!(
            r.is_err(),
            "嵌套结构 SectionDescriptor 应拒绝未知字段，但通过了"
        );
    }

    #[test]
    fn entry_point_is_transparent_string() {
        let ep = EntryPoint::new("main");
        let bytes = canonical_encode(&ep).expect("encode");
        // EntryPoint 应当与 String 同形：CBOR text "main"。
        let back: String = canonical_decode(&bytes).expect("decode as String");
        assert_eq!(back, "main");
    }

    #[test]
    fn selection_strategy_round_trip() {
        for s in [
            SelectionStrategy::Strict,
            SelectionStrategy::PreferThenAny,
            SelectionStrategy::Any,
        ] {
            let bytes = canonical_encode(&s).expect("encode");
            let back: SelectionStrategy = canonical_decode(&bytes).expect("decode");
            assert_eq!(back, s);
        }
    }
}
