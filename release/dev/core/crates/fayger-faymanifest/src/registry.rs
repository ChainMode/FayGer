//! 组件 / 包注册表抽象与静态已签名注册表后端（H2 · ADR 0005）。
//!
//! 依赖解析需要一个「注册表」回答：某模块 / 协议 / `@scope/name` 包是否存在、有哪些可用版本、
//! 来源是否可信、命名空间归属何种治理层级，以及当前回答来自在线索引还是离线缓存降级。
//!
//! 本模块提供 [`Registry`] 抽象与三类实现：
//!
//! - [`StubRegistry`]：H1 兼容占位实现，所有查询返回 [`ResolutionOutcome::Stub`]。
//! - [`InMemoryRegistry`]：内存实现，供单测显式登记已知项与可用版本。
//! - [`StaticIndexRegistry`]：H2 静态已签名索引后端（支持 RFC 8785 JCS + Ed25519 信任锚验签、
//!   核心与组织命名空间治理校验、不可信来源隔离、离线缓存降级 [`RegistrySourceMode::CachedVerified`]
//!   以及无缓存离线不可判定 [`RegistrySourceMode::OfflineUnavailable`]）。

use crate::namespace::PackageId;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use fayger_buf::manifest::SemVer;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// 静态注册表索引的域分离签名前缀（对齐 FayID / CAP 信任根约定）。
pub const REGISTRY_INDEX_SIGNING_DOMAIN: &[u8] = b"ifay.fayger.registry-index.v1:";

/// 静态注册表索引的固定 schema 版本。
pub const REGISTRY_INDEX_SCHEMA_VERSION: &str = "fayger-registry-index/1.0";

/// 注册表数据来源模式（用于在装配报告抬头如实区分在线、缓存降级、离线与占位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrySourceMode {
    /// H1 占位实现（未接入注册表）。
    Stub,
    /// 测试用内存注册表。
    InMemory,
    /// 在线主索引已通过 Ed25519 信任锚验签。
    OnlineVerified,
    /// 在线不可达，已降级到本地经 Ed25519 验签的缓存索引。
    CachedVerified,
    /// 在线与本地缓存均不可用，依赖存在性处于不可判定状态。
    OfflineUnavailable,
}

impl RegistrySourceMode {
    /// 面向装配报告的稳定人类可读标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stub => "stub (not connected)",
            Self::InMemory => "in-memory",
            Self::OnlineVerified => "online_verified",
            Self::CachedVerified => "cached_verified (offline degraded)",
            Self::OfflineUnavailable => "offline_unavailable (degraded indeterminate)",
        }
    }
}

/// 命名空间包的治理归属状态（ADR 0005 · 裁定 OQ-1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageGovernance {
    /// 官方核心受治理作用域（`@ifay/*`、`@cap-drivers/*`、`@drone-drivers/*`、`@ego-models/*`）。
    CoreGoverned,
    /// 第三方作用域但绑定了经校验的组织/代表身份（`org_*` / `cofay_*` / `ifay_*`）。
    OrganizationGoverned {
        /// 签发或背书该包的规范 FayID。
        signer_fay_id: String,
    },
    /// 未受治理的第三方作用域（报告中必须显式标注，不得静默视同受治理）。
    Ungoverned,
}

impl PackageGovernance {
    /// 面向装配报告的短标签。
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::CoreGoverned => "core-governed".to_owned(),
            Self::OrganizationGoverned { signer_fay_id } => {
                format!("org-governed:{signer_fay_id}")
            }
            Self::Ungoverned => "ungoverned".to_owned(),
        }
    }
}

/// 注册表对一次查询的回答。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionOutcome {
    /// 注册表已知该项且来源可信，附可用版本列表（空列表表示已知但无版本信息）。
    Known(Vec<SemVer>),
    /// 注册表确定不存在该项。
    Unknown,
    /// 注册表未实现 / 未接入该类查询（占位）。
    Stub,
    /// 因离线且无本地缓存而无法判定（不可判定不等于缺失）。
    Indeterminate,
    /// 条目存在但来源信任或命名空间治理校验失败（不可信不等于已解析）。
    Untrusted {
        /// 不可信原因编码或说明。
        reason: String,
    },
}

/// 组件与包的解析后端。
pub trait Registry {
    /// 查询模块 id。
    fn lookup_module(&self, id: &str) -> ResolutionOutcome;
    /// 查询协议 id。
    fn lookup_protocol(&self, id: &str) -> ResolutionOutcome;
    /// 查询命名空间包标识（`@scope/name`）。
    fn lookup_package(&self, identifier: &str) -> ResolutionOutcome;

    /// 查询 `@scope/name` 包的命名空间治理分类。
    fn package_governance(&self, identifier: &str) -> PackageGovernance {
        PackageId::parse(identifier).map_or(PackageGovernance::Ungoverned, |pkg| {
            if pkg.namespace().is_governed() {
                PackageGovernance::CoreGoverned
            } else {
                PackageGovernance::Ungoverned
            }
        })
    }

    /// 注册表当前的数据源模式。
    fn source_mode(&self) -> RegistrySourceMode {
        if self.is_stub() {
            RegistrySourceMode::Stub
        } else {
            RegistrySourceMode::InMemory
        }
    }

    /// 该注册表是否为占位实现（未接入真实后端）。默认 `false`；[`StubRegistry`] 覆盖为 `true`。
    fn is_stub(&self) -> bool {
        false
    }
}

/// H1 占位注册表：所有查询返回 [`ResolutionOutcome::Stub`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct StubRegistry;

impl Registry for StubRegistry {
    fn lookup_module(&self, _id: &str) -> ResolutionOutcome {
        ResolutionOutcome::Stub
    }
    fn lookup_protocol(&self, _id: &str) -> ResolutionOutcome {
        ResolutionOutcome::Stub
    }
    fn lookup_package(&self, _identifier: &str) -> ResolutionOutcome {
        ResolutionOutcome::Stub
    }
    fn source_mode(&self) -> RegistrySourceMode {
        RegistrySourceMode::Stub
    }
    fn is_stub(&self) -> bool {
        true
    }
}

/// 内存注册表（测试 / 本地实验）：显式登记已知模块 / 协议 / 包及其可用版本。
#[derive(Debug, Clone, Default)]
pub struct InMemoryRegistry {
    modules: BTreeMap<String, Vec<SemVer>>,
    protocols: BTreeMap<String, Vec<SemVer>>,
    packages: BTreeMap<String, Vec<SemVer>>,
}

impl InMemoryRegistry {
    /// 空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个模块及其可用版本（链式）。
    #[must_use]
    pub fn with_module(mut self, id: &str, versions: &[&str]) -> Self {
        self.modules.insert(id.to_owned(), parse_versions(versions));
        self
    }

    /// 登记一个协议及其可用版本（链式）。
    #[must_use]
    pub fn with_protocol(mut self, id: &str, versions: &[&str]) -> Self {
        self.protocols
            .insert(id.to_owned(), parse_versions(versions));
        self
    }

    /// 登记一个命名空间包及其可用版本（链式）。
    #[must_use]
    pub fn with_package(mut self, identifier: &str, versions: &[&str]) -> Self {
        self.packages
            .insert(identifier.to_owned(), parse_versions(versions));
        self
    }
}

fn parse_versions(versions: &[&str]) -> Vec<SemVer> {
    versions
        .iter()
        .filter_map(|v| SemVer::parse(v).ok())
        .collect()
}

fn outcome(map: &BTreeMap<String, Vec<SemVer>>, key: &str) -> ResolutionOutcome {
    map.get(key).map_or(ResolutionOutcome::Unknown, |vs| {
        ResolutionOutcome::Known(vs.clone())
    })
}

impl Registry for InMemoryRegistry {
    fn lookup_module(&self, id: &str) -> ResolutionOutcome {
        outcome(&self.modules, id)
    }
    fn lookup_protocol(&self, id: &str) -> ResolutionOutcome {
        outcome(&self.protocols, id)
    }
    fn lookup_package(&self, identifier: &str) -> ResolutionOutcome {
        outcome(&self.packages, identifier)
    }
}

// ----------------------- H2 静态已签名注册表后端 -----------------------

/// 单条注册表信任锚（与 FayID / CAP D-5 信任根模型对齐）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTrustAnchor {
    /// 公钥引用标识。
    pub key_id: String,
    /// 绑定的规范 FayID（如 `org_ifay_registry_root`）。
    pub signer_fay_id: String,
    /// 64 字符十六进制 Ed25519 公钥。
    pub public_key_hex: String,
    /// 是否允许签发核心受治理索引。
    #[serde(default = "default_true")]
    pub trusted_index_signer: bool,
}

const fn default_true() -> bool {
    true
}

/// 注册表信任根集合。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTrustStore {
    /// 已登记的可信公钥锚列表。
    pub anchors: Vec<RegistryTrustAnchor>,
}

impl RegistryTrustStore {
    /// 创建空信任库。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            anchors: Vec::new(),
        }
    }

    /// 链式追加一条信任锚。
    #[must_use]
    pub fn with_anchor(mut self, anchor: RegistryTrustAnchor) -> Self {
        self.anchors.push(anchor);
        self
    }

    /// 从 JSON 字符串解析信任锚集合。
    pub fn from_json_str(json: &str) -> Result<Self, FaygerError> {
        serde_json::from_str(json).map_err(|e| {
            registry_error(
                ErrorCode::FmRegistryInvalidIndex,
                "InvalidTrustStoreJson",
                &e.to_string(),
            )
        })
    }
}

/// 静态索引中的单个包发布版本记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticPackageRelease {
    /// SemVer 版本号。
    pub version: String,
    /// 包制品的 64 字符小写十六进制 SHA-256 摘要。
    pub artifact_sha256: String,
}

/// 静态索引中的单个 `@scope/name` 包条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticPackageEntry {
    /// 可用发布列表。
    pub releases: Vec<StaticPackageRelease>,
    /// 发布者规范 FayID（核心作用域须为 `ifay_*` 或 `org_ifay*`；组织治理须为 `org_*`/`cofay_*`/`ifay_*`）。
    #[serde(default)]
    pub signer_fay_id: Option<String>,
    /// 包级来源签名是否已通过发布流水线核验。
    #[serde(default = "default_true")]
    pub provenance_verified: bool,
}

/// 静态注册表索引的顶层 Ed25519 签名信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryIndexSignature {
    /// 对应信任锚的 `key_id`。
    pub key_id: String,
    /// 签发者 FayID。
    pub signer_fay_id: String,
    /// 固定为 `"ed25519"`。
    pub algorithm: String,
    /// 剥离 `signature` 后对象经 RFC 8785 JCS 规范化的 SHA-256（小写 hex）。
    pub payload_jcs_sha256: String,
    /// `ifay.fayger.registry-index.v1:` 拼接 JCS 字节上的 Ed25519 签名（128 字符 hex）。
    pub signature_hex: String,
}

/// 静态注册表索引文档（`fayger-registry-index/1.0`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticRegistryIndex {
    /// 固定为 [`REGISTRY_INDEX_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 索引生成时间戳（ISO 8601）。
    pub generated_at: String,
    /// 模块可用版本表。
    #[serde(default)]
    pub modules: BTreeMap<String, Vec<String>>,
    /// 协议可用版本表。
    #[serde(default)]
    pub protocols: BTreeMap<String, Vec<String>>,
    /// 命名空间包登记表。
    #[serde(default)]
    pub packages: BTreeMap<String, StaticPackageEntry>,
    /// 顶层索引签名。
    #[serde(default)]
    pub signature: Option<RegistryIndexSignature>,
}

#[derive(Debug, Clone)]
struct VerifiedPackageState {
    outcome: ResolutionOutcome,
    governance: PackageGovernance,
}

/// 静态已签名组件注册表后端（H2）。
#[derive(Debug, Clone)]
pub struct StaticIndexRegistry {
    mode: RegistrySourceMode,
    modules: BTreeMap<String, Vec<SemVer>>,
    protocols: BTreeMap<String, Vec<SemVer>>,
    packages: BTreeMap<String, VerifiedPackageState>,
}

impl StaticIndexRegistry {
    /// 构造离线且无本地缓存的降级注册表（所有查询返回 [`ResolutionOutcome::Indeterminate`]）。
    #[must_use]
    pub const fn offline_unavailable() -> Self {
        Self {
            mode: RegistrySourceMode::OfflineUnavailable,
            modules: BTreeMap::new(),
            protocols: BTreeMap::new(),
            packages: BTreeMap::new(),
        }
    }

    /// 验证一份在线静态索引 JSON 并构造 [`RegistrySourceMode::OnlineVerified`] 注册表。
    pub fn from_online_json(
        index_json: &str,
        trust_store: &RegistryTrustStore,
    ) -> Result<Self, FaygerError> {
        Self::parse_and_verify(index_json, trust_store, RegistrySourceMode::OnlineVerified)
    }

    /// 验证一份本地缓存索引 JSON 并构造 [`RegistrySourceMode::CachedVerified`] 注册表。
    pub fn from_cached_json(
        cached_json: &str,
        trust_store: &RegistryTrustStore,
    ) -> Result<Self, FaygerError> {
        Self::parse_and_verify(cached_json, trust_store, RegistrySourceMode::CachedVerified)
    }

    /// 在线优先、离线按已验签本地缓存确定性降级（ADR 0005 · 裁定 OQ-3）：
    ///
    /// - 若 `online_json` 提供：必须通过信任锚验签，产出 `OnlineVerified`；
    /// - 若 `online_json` 缺失（离线）且 `cached_json` 提供：验证缓存签名，产出 `CachedVerified`；
    /// - 若两者均未提供：产出 `OfflineUnavailable`（所有查询返回 `Indeterminate`）。
    pub fn from_online_or_cache(
        online_json: Option<&str>,
        cached_json: Option<&str>,
        trust_store: &RegistryTrustStore,
    ) -> Result<Self, FaygerError> {
        if let Some(online) = online_json {
            return Self::from_online_json(online, trust_store);
        }
        if let Some(cached) = cached_json {
            return Self::from_cached_json(cached, trust_store);
        }
        Ok(Self::offline_unavailable())
    }

    fn parse_and_verify(
        raw_json: &str,
        trust_store: &RegistryTrustStore,
        mode: RegistrySourceMode,
    ) -> Result<Self, FaygerError> {
        let raw_value: Value = serde_json::from_str(raw_json).map_err(|e| {
            registry_error(
                ErrorCode::FmRegistryInvalidIndex,
                "InvalidJson",
                &e.to_string(),
            )
        })?;
        verify_index_signature(&raw_value, trust_store)?;

        let index: StaticRegistryIndex = serde_json::from_value(raw_value).map_err(|e| {
            registry_error(
                ErrorCode::FmRegistryInvalidIndex,
                "SchemaMismatch",
                &e.to_string(),
            )
        })?;
        if index.schema_version != REGISTRY_INDEX_SCHEMA_VERSION {
            return Err(registry_error(
                ErrorCode::FmRegistryInvalidIndex,
                "UnsupportedSchemaVersion",
                &index.schema_version,
            ));
        }

        let modules = parse_version_map(&index.modules, "modules")?;
        let protocols = parse_version_map(&index.protocols, "protocols")?;
        let mut packages = BTreeMap::new();
        for (pkg_id_str, entry) in &index.packages {
            let pkg_id = PackageId::parse(pkg_id_str)?;
            let state = evaluate_package_entry(&pkg_id, entry)?;
            packages.insert(pkg_id_str.clone(), state);
        }

        Ok(Self {
            mode,
            modules,
            protocols,
            packages,
        })
    }
}

impl Registry for StaticIndexRegistry {
    fn lookup_module(&self, id: &str) -> ResolutionOutcome {
        if self.mode == RegistrySourceMode::OfflineUnavailable {
            return ResolutionOutcome::Indeterminate;
        }
        outcome(&self.modules, id)
    }

    fn lookup_protocol(&self, id: &str) -> ResolutionOutcome {
        if self.mode == RegistrySourceMode::OfflineUnavailable {
            return ResolutionOutcome::Indeterminate;
        }
        outcome(&self.protocols, id)
    }

    fn lookup_package(&self, identifier: &str) -> ResolutionOutcome {
        if self.mode == RegistrySourceMode::OfflineUnavailable {
            return ResolutionOutcome::Indeterminate;
        }
        self.packages
            .get(identifier)
            .map_or(ResolutionOutcome::Unknown, |s| s.outcome.clone())
    }

    fn package_governance(&self, identifier: &str) -> PackageGovernance {
        if let Some(state) = self.packages.get(identifier) {
            return state.governance.clone();
        }
        PackageId::parse(identifier).map_or(PackageGovernance::Ungoverned, |pkg| {
            if pkg.namespace().is_governed() {
                PackageGovernance::CoreGoverned
            } else {
                PackageGovernance::Ungoverned
            }
        })
    }

    fn source_mode(&self) -> RegistrySourceMode {
        self.mode
    }
}

fn parse_version_map(
    raw: &BTreeMap<String, Vec<String>>,
    section: &str,
) -> Result<BTreeMap<String, Vec<SemVer>>, FaygerError> {
    let mut out = BTreeMap::new();
    for (id, versions) in raw {
        let mut parsed = Vec::with_capacity(versions.len());
        for v in versions {
            let sv = SemVer::parse(v).map_err(|e| {
                registry_error(
                    ErrorCode::FmRegistryInvalidIndex,
                    "InvalidComponentSemVer",
                    &format!("{section}.{id}: {v} ({e})"),
                )
            })?;
            parsed.push(sv);
        }
        out.insert(id.clone(), parsed);
    }
    Ok(out)
}

fn evaluate_package_entry(
    pkg_id: &PackageId,
    entry: &StaticPackageEntry,
) -> Result<VerifiedPackageState, FaygerError> {
    let is_core = pkg_id.namespace().is_governed();
    let signer = entry.signer_fay_id.as_deref().unwrap_or("");

    let governance = if is_core {
        PackageGovernance::CoreGoverned
    } else if is_governed_entity_fay_id(signer) {
        PackageGovernance::OrganizationGoverned {
            signer_fay_id: signer.to_owned(),
        }
    } else {
        PackageGovernance::Ungoverned
    };

    if !entry.provenance_verified {
        return Ok(VerifiedPackageState {
            outcome: ResolutionOutcome::Untrusted {
                reason: "provenance_unverified".to_owned(),
            },
            governance,
        });
    }

    // 核心受治理命名空间要求发布者具备官方/组织根身份（ifay_* 或 org_ifay*）。
    if is_core && !is_core_publisher_fay_id(signer) {
        return Ok(VerifiedPackageState {
            outcome: ResolutionOutcome::Untrusted {
                reason: format!(
                    "core namespace @{} requires core publisher FayID, got '{}'",
                    pkg_id.scope(),
                    if signer.is_empty() {
                        "<missing>"
                    } else {
                        signer
                    }
                ),
            },
            governance,
        });
    }

    let mut versions = Vec::with_capacity(entry.releases.len());
    for rel in &entry.releases {
        let sv = SemVer::parse(&rel.version).map_err(|e| {
            registry_error(
                ErrorCode::FmRegistryInvalidIndex,
                "InvalidPackageSemVer",
                &format!("{}: {} ({e})", pkg_id.to_identifier(), rel.version),
            )
        })?;
        if !is_valid_sha256_hex(&rel.artifact_sha256) {
            return Ok(VerifiedPackageState {
                outcome: ResolutionOutcome::Untrusted {
                    reason: format!("invalid artifact_sha256 for version {}", rel.version),
                },
                governance,
            });
        }
        versions.push(sv);
    }

    Ok(VerifiedPackageState {
        outcome: ResolutionOutcome::Known(versions),
        governance,
    })
}

fn is_core_publisher_fay_id(fay_id: &str) -> bool {
    fay_id.starts_with("ifay_") || fay_id.starts_with("org_ifay")
}

fn is_governed_entity_fay_id(fay_id: &str) -> bool {
    (fay_id.starts_with("org_") || fay_id.starts_with("cofay_") || fay_id.starts_with("ifay_"))
        && fay_id.len() >= 8
}

fn is_valid_sha256_hex(hex_str: &str) -> bool {
    hex_str.len() == 64
        && hex_str
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 对不含顶层 `signature` 的静态索引 JSON Value 进行签名并写入 `signature` 信封（供测试与索引发布工具使用）。
pub fn sign_registry_index_value(
    index_value: &mut Value,
    key_id: &str,
    signer_fay_id: &str,
    signing_key: &SigningKey,
) -> Result<(), FaygerError> {
    let obj = index_value.as_object_mut().ok_or_else(|| {
        registry_error(
            ErrorCode::FmRegistryInvalidIndex,
            "IndexRootNotObject",
            "registry index root must be a JSON object",
        )
    })?;
    obj.remove("signature");
    let stripped = Value::Object(obj.clone());
    let jcs_bytes = jcs_canonicalize_bytes(&stripped)?;
    let payload_jcs_sha256 = hex_encode(&Sha256::digest(&jcs_bytes));

    let mut message = Vec::with_capacity(REGISTRY_INDEX_SIGNING_DOMAIN.len() + jcs_bytes.len());
    message.extend_from_slice(REGISTRY_INDEX_SIGNING_DOMAIN);
    message.extend_from_slice(&jcs_bytes);
    let sig: Signature = signing_key.sign(&message);

    let envelope = RegistryIndexSignature {
        key_id: key_id.to_owned(),
        signer_fay_id: signer_fay_id.to_owned(),
        algorithm: "ed25519".to_owned(),
        payload_jcs_sha256,
        signature_hex: hex_encode(&sig.to_bytes()),
    };
    let sig_val = serde_json::to_value(envelope).map_err(|e| {
        registry_error(
            ErrorCode::FmRegistryInvalidIndex,
            "SignatureSerializeFailed",
            &e.to_string(),
        )
    })?;
    if let Some(map) = index_value.as_object_mut() {
        map.insert("signature".to_owned(), sig_val);
    }
    Ok(())
}

fn verify_index_signature(
    raw_value: &Value,
    trust_store: &RegistryTrustStore,
) -> Result<(), FaygerError> {
    let obj = raw_value.as_object().ok_or_else(|| {
        registry_error(
            ErrorCode::FmRegistryInvalidIndex,
            "IndexRootNotObject",
            "registry index root must be a JSON object",
        )
    })?;
    let sig_val = obj.get("signature").ok_or_else(|| {
        registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "MissingSignature",
            "registry index missing top-level signature",
        )
    })?;
    let sig_env: RegistryIndexSignature = serde_json::from_value(sig_val.clone()).map_err(|e| {
        registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "MalformedSignatureEnvelope",
            &e.to_string(),
        )
    })?;
    if sig_env.algorithm != "ed25519" {
        return Err(registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "UnsupportedAlgorithm",
            &sig_env.algorithm,
        ));
    }

    let anchor = trust_store
        .anchors
        .iter()
        .find(|a| a.key_id == sig_env.key_id && a.trusted_index_signer)
        .ok_or_else(|| {
            registry_error(
                ErrorCode::FmRegistrySignatureFail,
                "UnknownOrUntrustedKey",
                &sig_env.key_id,
            )
        })?;
    if anchor.signer_fay_id != sig_env.signer_fay_id {
        return Err(registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "SignerFayIdMismatch",
            &format!(
                "expected '{}', got '{}'",
                anchor.signer_fay_id, sig_env.signer_fay_id
            ),
        ));
    }

    let mut stripped_map = obj.clone();
    stripped_map.remove("signature");
    let jcs_bytes = jcs_canonicalize_bytes(&Value::Object(stripped_map))?;
    let computed_sha256 = hex_encode(&Sha256::digest(&jcs_bytes));
    if computed_sha256 != sig_env.payload_jcs_sha256 {
        return Err(registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "PayloadDigestMismatch",
            &format!(
                "expected '{}', computed '{computed_sha256}'",
                sig_env.payload_jcs_sha256
            ),
        ));
    }

    let pub_bytes: [u8; 32] = decode_fixed_hex(&anchor.public_key_hex, "public_key_hex")?;
    let sig_bytes: [u8; 64] = decode_fixed_hex(&sig_env.signature_hex, "signature_hex")?;
    let verifying_key = VerifyingKey::from_bytes(&pub_bytes).map_err(|e| {
        registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "InvalidPublicKey",
            &e.to_string(),
        )
    })?;
    let signature = Signature::from_bytes(&sig_bytes);

    let mut message = Vec::with_capacity(REGISTRY_INDEX_SIGNING_DOMAIN.len() + jcs_bytes.len());
    message.extend_from_slice(REGISTRY_INDEX_SIGNING_DOMAIN);
    message.extend_from_slice(&jcs_bytes);
    verifying_key
        .verify_strict(&message, &signature)
        .map_err(|e| {
            registry_error(
                ErrorCode::FmRegistrySignatureFail,
                "InvalidSignature",
                &e.to_string(),
            )
        })
}

fn jcs_canonicalize_bytes(value: &Value) -> Result<Vec<u8>, FaygerError> {
    let mut out = Vec::new();
    write_jcs_value(value, &mut out)?;
    Ok(out)
}

fn write_jcs_value(value: &Value, out: &mut Vec<u8>) -> Result<(), FaygerError> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(num) => {
            if num.is_f64() && !(num.is_i64() || num.is_u64()) {
                return Err(registry_error(
                    ErrorCode::FmRegistryInvalidIndex,
                    "NonIntegerNumberInJcs",
                    &num.to_string(),
                ));
            }
            out.extend_from_slice(num.to_string().as_bytes());
        }
        Value::String(s) => {
            let encoded = serde_json::to_string(s).map_err(|e| {
                registry_error(
                    ErrorCode::FmRegistryInvalidIndex,
                    "JcsStringEscapeFailed",
                    &e.to_string(),
                )
            })?;
            out.extend_from_slice(encoded.as_bytes());
        }
        Value::Array(items) => {
            out.push(b'[');
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(b',');
                }
                write_jcs_value(item, out)?;
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|(ka, _), (kb, _)| {
                let ua: Vec<u16> = ka.encode_utf16().collect();
                let ub: Vec<u16> = kb.encode_utf16().collect();
                ua.cmp(&ub)
            });
            out.push(b'{');
            for (idx, (k, v)) in entries.into_iter().enumerate() {
                if idx > 0 {
                    out.push(b',');
                }
                let key_encoded = serde_json::to_string(k).map_err(|e| {
                    registry_error(
                        ErrorCode::FmRegistryInvalidIndex,
                        "JcsKeyEscapeFailed",
                        &e.to_string(),
                    )
                })?;
                out.extend_from_slice(key_encoded.as_bytes());
                out.push(b':');
                write_jcs_value(v, out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn decode_fixed_hex<const N: usize>(hex_str: &str, field: &str) -> Result<[u8; N], FaygerError> {
    if hex_str.len() != N * 2 {
        return Err(registry_error(
            ErrorCode::FmRegistrySignatureFail,
            "InvalidHexLength",
            &format!(
                "{field}: expected {} hex chars, got {}",
                N * 2,
                hex_str.len()
            ),
        ));
    }
    let mut out = [0u8; N];
    let bytes = hex_str.as_bytes();
    for (idx, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[idx * 2]).ok_or_else(|| {
            registry_error(ErrorCode::FmRegistrySignatureFail, "InvalidHexChar", field)
        })?;
        let lo = hex_nibble(bytes[idx * 2 + 1]).ok_or_else(|| {
            registry_error(ErrorCode::FmRegistrySignatureFail, "InvalidHexChar", field)
        })?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn registry_error(code: ErrorCode, reason: &str, detail: &str) -> FaygerError {
    FaygerError::new(
        code,
        Layer::Assembler,
        "component registry index verification failed",
    )
    .with_context("reason", ContextValue::string(reason))
    .with_context("detail", ContextValue::string(detail))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        hex_encode, sign_registry_index_value, InMemoryRegistry, PackageGovernance, Registry,
        RegistrySourceMode, RegistryTrustAnchor, RegistryTrustStore, ResolutionOutcome,
        StaticIndexRegistry, StubRegistry, REGISTRY_INDEX_SCHEMA_VERSION,
    };
    use ed25519_dalek::SigningKey;
    use fayger_buf::manifest::SemVer;
    use fayger_error::ErrorCode;
    use serde_json::json;

    fn sample_signed_index() -> (String, RegistryTrustStore) {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk_hex = hex_encode(sk.verifying_key().as_bytes());
        let store = RegistryTrustStore::new().with_anchor(RegistryTrustAnchor {
            key_id: "root-2026".to_owned(),
            signer_fay_id: "org_ifay_registry_root".to_owned(),
            public_key_hex: pk_hex,
            trusted_index_signer: true,
        });
        let digest = "a".repeat(64);
        let mut idx = json!({
            "schema_version": REGISTRY_INDEX_SCHEMA_VERSION,
            "generated_at": "2026-11-16T00:00:00Z",
            "modules": {
                "device_driver_hub": ["1.0.0", "1.4.0", "1.5.0-alpha.1", "2.0.0"],
                "sensor": ["1.1.0"]
            },
            "protocols": {
                "cap_protocol": ["1.0.0"],
                "faying_protocol": ["1.0.0", "1.1.0"]
            },
            "packages": {
                "@drone-drivers/dji-fc": {
                    "releases": [{"version": "3.2.1", "artifact_sha256": digest}],
                    "signer_fay_id": "org_ifay_drivers",
                    "provenance_verified": true
                },
                "@acme/custom-sensor": {
                    "releases": [{"version": "0.4.0", "artifact_sha256": digest}],
                    "signer_fay_id": "org_acme_robotics",
                    "provenance_verified": true
                },
                "@hobby/unsigned-toy": {
                    "releases": [{"version": "0.1.0", "artifact_sha256": digest}],
                    "provenance_verified": true
                },
                "@drone-drivers/forged-fc": {
                    "releases": [{"version": "1.0.0", "artifact_sha256": digest}],
                    "signer_fay_id": "user_random_actor",
                    "provenance_verified": true
                }
            }
        });
        sign_registry_index_value(&mut idx, "root-2026", "org_ifay_registry_root", &sk)
            .expect("sign index");
        (serde_json::to_string_pretty(&idx).expect("json"), store)
    }

    #[test]
    fn stub_registry_returns_stub_for_everything() {
        let r = StubRegistry;
        assert_eq!(r.lookup_module("sensor"), ResolutionOutcome::Stub);
        assert_eq!(r.lookup_protocol("cap_protocol"), ResolutionOutcome::Stub);
        assert_eq!(
            r.lookup_package("@drone-drivers/dji-fc"),
            ResolutionOutcome::Stub
        );
        assert_eq!(r.source_mode(), RegistrySourceMode::Stub);
    }

    #[test]
    fn in_memory_known_and_unknown() {
        let r = InMemoryRegistry::new()
            .with_module("sensor", &["1.0.0", "1.2.0"])
            .with_package("@drone-drivers/dji-fc", &["3.0.0"]);
        assert_eq!(
            r.lookup_module("sensor"),
            ResolutionOutcome::Known(vec![
                SemVer::parse("1.0.0").expect("v"),
                SemVer::parse("1.2.0").expect("v"),
            ])
        );
        assert_eq!(r.lookup_module("missing"), ResolutionOutcome::Unknown);
        assert!(matches!(
            r.lookup_package("@drone-drivers/dji-fc"),
            ResolutionOutcome::Known(_)
        ));
    }

    #[test]
    fn static_index_verifies_signature_and_enforces_governance_and_provenance() {
        let (json_str, store) = sample_signed_index();
        let reg = StaticIndexRegistry::from_online_json(&json_str, &store).expect("verified");
        assert_eq!(reg.source_mode(), RegistrySourceMode::OnlineVerified);

        // Core governed + valid core publisher -> Known + CoreGoverned
        assert_eq!(
            reg.package_governance("@drone-drivers/dji-fc"),
            PackageGovernance::CoreGoverned
        );
        assert!(matches!(
            reg.lookup_package("@drone-drivers/dji-fc"),
            ResolutionOutcome::Known(_)
        ));

        // Third-party bound to org_* -> OrganizationGoverned
        assert_eq!(
            reg.package_governance("@acme/custom-sensor"),
            PackageGovernance::OrganizationGoverned {
                signer_fay_id: "org_acme_robotics".to_owned()
            }
        );

        // Third-party without org/cofay/ifay -> Ungoverned
        assert_eq!(
            reg.package_governance("@hobby/unsigned-toy"),
            PackageGovernance::Ungoverned
        );

        // Core namespace claimed by non-core publisher -> Untrusted!
        assert!(matches!(
            reg.lookup_package("@drone-drivers/forged-fc"),
            ResolutionOutcome::Untrusted { .. }
        ));
    }

    #[test]
    fn static_index_rejects_tampered_payload() {
        let (json_str, store) = sample_signed_index();
        let tampered = json_str.replace("\"1.1.0\"", "\"9.9.9\"");
        let err = StaticIndexRegistry::from_online_json(&tampered, &store).expect_err("tampered");
        assert_eq!(err.code, ErrorCode::FmRegistrySignatureFail);
    }

    #[test]
    fn offline_cache_fallback_and_unavailable_indeterminate() {
        let (json_str, store) = sample_signed_index();
        let cached =
            StaticIndexRegistry::from_online_or_cache(None, Some(&json_str), &store).expect("ok");
        assert_eq!(cached.source_mode(), RegistrySourceMode::CachedVerified);
        assert!(matches!(
            cached.lookup_module("sensor"),
            ResolutionOutcome::Known(_)
        ));

        let offline = StaticIndexRegistry::from_online_or_cache(None, None, &store).expect("ok");
        assert_eq!(
            offline.source_mode(),
            RegistrySourceMode::OfflineUnavailable
        );
        assert_eq!(
            offline.lookup_module("sensor"),
            ResolutionOutcome::Indeterminate
        );
        assert_eq!(
            offline.lookup_package("@drone-drivers/dji-fc"),
            ResolutionOutcome::Indeterminate
        );
    }
}
