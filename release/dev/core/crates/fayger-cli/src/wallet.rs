//! 内置 `FayID` 身份与 `Auth Exchange` 凭据保险箱（`fayger-fayid-vault/1.0`）。
//!
//! 实现 MVP 规格 §2.1 的核心身份与凭据代理原语：
//! - 像创建加密钱包一样生成助记词与主种子，派生根 `Human ID`（对外永不暴露明文，仅暴露掩码与承诺值）；
//! - 按需签发不可关联动态码（`Dynamic Code` · `dync_...`），支持跨平台防关联核验；
//! - 确定性派生个人数字灵魂标识 `iFay ID`（`ifay_...`，默认去中心化全网隐身）与公共席位 `coFay ID`
//!   （`cofay_...` + 公开 [`RoleCharter`] `1.0`）；
//! - `Auth Exchange`：将传统 API Key / OAuth Token / 账密封装保存在本机 `0600` 权限保险箱中，对外仅兑换有时效、
//!   限作用域的已签名 [`AuthExchangeGrant`] 授权票据。

use crate::ego_package::{encode_hex, hex_sha256, jcs_canonicalize};
use crate::error::CliError;
use ed25519_dalek::{Signer, SigningKey};
use fayger_error::{ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::hash_map::RandomState;
use std::fmt::Write as _;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 本地 `FayID` 保险箱存储 schema 版本。
pub const WALLET_VAULT_SCHEMA_VERSION: &str = "fayger-fayid-vault/1.0";

/// `coFay` 公开岗位章程（`RoleCharter`）schema 版本。
pub const ROLE_CHARTER_SCHEMA_VERSION: &str = "cofay-role-charter/1.0";

/// `Auth Exchange` 兑换出的限时授权票据 schema 版本。
pub const AUTH_GRANT_SCHEMA_VERSION: &str = "fayid-auth-grant/1.0";

const VAULT_FILENAME: &str = "wallet_vault.json";

const RFC4648_LOWER_BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

const MNEMONIC_WORDS: [&str; 64] = [
    "anchor", "atlas", "aurora", "beacon", "breeze", "canyon", "cedar", "cipher", "cobalt",
    "comet", "coral", "cosmos", "crystal", "dawn", "delta", "echo", "ember", "epoch", "falcon",
    "fjord", "flux", "forest", "galaxy", "glacier", "granite", "harbor", "harmony", "haven",
    "helix", "horizon", "indigo", "island", "jade", "lantern", "lumen", "marble", "meadow",
    "merit", "meteor", "mirror", "nebula", "nexus", "nimbus", "north", "oasis", "obsidian", "onyx",
    "orbit", "origin", "phoenix", "pioneer", "prism", "pulse", "quartz", "radiant", "river",
    "saffron", "shield", "silver", "solstice", "spark", "summit", "velvet", "zenith",
];

static ENTROPY_COUNTER: AtomicU64 = AtomicU64::new(1);

/// 初始化钱包后一次性返回的创建回执（仅初始化瞬间展示助记词）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletInitReceipt {
    /// 保险箱 schema 版本。
    pub schema_version: String,
    /// 钱包标签名称。
    pub label: String,
    /// 一次性展示的 12 词恢复助记词（后续 `wallet show` 不再回显）。
    pub recovery_mnemonic_once: String,
    /// 脱敏掩码形式的 `Human ID`（如 `hid_abcd...2345`，绝不暴露完整明文）。
    pub human_id_masked: String,
    /// `Human ID` 的密码学承诺值（`sha256:<64-hex>`）。
    pub human_id_commitment: String,
    /// `Auth Exchange` 票据签发公钥（64 字符十六进制）。
    pub exchange_authority_pubkey_hex: String,
    /// 创建时间戳（Unix 秒）。
    pub created_at_unix: u64,
}

/// 派生的个人 `iFay` 身份记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedIfayRecord {
    /// 规范化 `iFay` 身份标识（`ifay_<26-base32>`）。
    pub ifay_id: String,
    /// 人类可读名称。
    pub name: String,
    /// 确定性派生序号。
    pub derivation_index: u32,
    /// Ed25519 验证公钥（64 字符十六进制）。
    pub public_key_hex: String,
    /// 发现状态：默认 `"stealth_unregistered"`（去中心化全网隐身）。
    pub discovery_mode: String,
    /// 创建时间（Unix 秒）。
    pub created_at_unix: u64,
}

/// `coFay` 公开岗位章程（`RoleCharter` 1.0，替代私人 `Ego`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleCharter {
    /// 固定为 [`ROLE_CHARTER_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 绑定的 `coFay` 席位标识。
    pub cofay_id: String,
    /// 席位显示名称。
    pub seat_name: String,
    /// 公开岗位职责头衔。
    pub role_title: String,
    /// 公开职权边界声明。
    pub public_mandate_scope: String,
    /// 当前履职托管人承诺哈希（不暴露托管人明文 `Human ID`）。
    pub custodian_commitment: String,
    /// 任期届次（换届递增）。
    pub term_epoch: u32,
    /// 是否支持换届双签过户。
    pub dual_sig_handover_supported: bool,
}

/// 派生的公共席位 `coFay` 身份记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedCofayRecord {
    /// 规范化 `coFay` 席位标识（`cofay_<26-base32>`）。
    pub cofay_id: String,
    /// 席位名称。
    pub name: String,
    /// 确定性派生序号。
    pub derivation_index: u32,
    /// Ed25519 席位公钥（64 字符十六进制）。
    pub public_key_hex: String,
    /// 绑定的公开 `RoleCharter` 1.0。
    pub role_charter: RoleCharter,
    /// 创建时间（Unix 秒）。
    pub created_at_unix: u64,
}

/// 面向外部验证方的不可关联动态码票据（`Dynamic Code` · `dync_...`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicCodeTicket {
    /// 动态码（`dync_<26-base32>`）。
    pub dynamic_code: String,
    /// 签发用途/目标场景。
    pub purpose: String,
    /// 加盐防关联证明摘要（外部凭此核验真伪但无法反推或跨站关联 `Human ID`）。
    pub unlinkable_proof_sha256: String,
    /// 签发时间（Unix 秒）。
    pub issued_at_unix: u64,
    /// 过期时间（Unix 秒）。
    pub expires_at_unix: u64,
}

/// 经 `Auth Exchange` 兑换出的限时、限作用域授权票据（绝不含原始凭据明文）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthExchangeGrant {
    /// 固定为 [`AUTH_GRANT_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 授权票据唯一标识（`ag_<16-hex>`）。
    pub grant_id: String,
    /// 外部服务/提供商标识（如 `openai` / `github` / `mcp-notion`）。
    pub provider: String,
    /// 允许使用该票据的目标 `Fay` 标识（可选，空表示当前宿主授权的任意上岗 `Fay`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_fay_id: Option<String>,
    /// 授权作用域列表。
    pub scopes: Vec<String>,
    /// 原始凭据的不可逆指纹（`sha256:<16-hex>`）。
    pub secret_fingerprint: String,
    /// 签发时间（Unix 秒）。
    pub issued_at_unix: u64,
    /// 过期时间（Unix 秒）。
    pub expires_at_unix: u64,
    /// 本机 `Auth Exchange` 签发公钥（64 字符十六进制）。
    pub authority_pubkey_hex: String,
    /// 针对票据 JCS 规范化负载的 Ed25519 签名（128 字符十六进制）。
    pub signature_hex: String,
}

/// 对外安全展示的保险箱公开视图（不含主种子、助记词、明文 `Human ID` 或原始 API Key）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletPublicView {
    /// 保险箱是否已初始化。
    pub initialized: bool,
    /// 钱包标签。
    pub label: String,
    /// 掩码后的 `Human ID`（如 `hid_abcd...wxyz`）。
    pub human_id_masked: String,
    /// `Human ID` 承诺摘要。
    pub human_id_commitment: String,
    /// `Auth Exchange` 权威公钥。
    pub exchange_authority_pubkey_hex: String,
    /// 已派生的 `iFay` 列表。
    pub derived_ifays: Vec<DerivedIfayRecord>,
    /// 已派生的 `coFay` 席位列表。
    pub derived_cofays: Vec<DerivedCofayRecord>,
    /// 近期签发的动态码记录。
    pub dynamic_codes: Vec<DynamicCodeTicket>,
    /// 已兑换的 `Authorization Grant` 列表（无明文密钥）。
    pub auth_grants: Vec<AuthExchangeGrant>,
}

impl WalletPublicView {
    /// 未初始化时的空公开视图。
    #[must_use]
    pub fn uninitialized() -> Self {
        Self {
            initialized: false,
            label: String::new(),
            human_id_masked: "(uninitialized)".to_owned(),
            human_id_commitment: String::new(),
            exchange_authority_pubkey_hex: String::new(),
            derived_ifays: Vec::new(),
            derived_cofays: Vec::new(),
            dynamic_codes: Vec::new(),
            auth_grants: Vec::new(),
        }
    }

    /// 渲染终端可读摘要。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut s = String::new();
        if !self.initialized {
            let _ = writeln!(
                s,
                "FayID Wallet Vault: UNINITIALIZED (run `fayger wallet init`)"
            );
            return s;
        }
        let _ = writeln!(s, "FayID Wallet & Auth Exchange Vault");
        let _ = writeln!(s, "  label:                 {}", self.label);
        let _ = writeln!(s, "  human_id (masked):     {}", self.human_id_masked);
        let _ = writeln!(s, "  human_id_commitment:   {}", self.human_id_commitment);
        let _ = writeln!(s, "  derived iFays:         {}", self.derived_ifays.len());
        for item in &self.derived_ifays {
            let _ = writeln!(
                s,
                "    - {} ({}) [{}]",
                item.name, item.ifay_id, item.discovery_mode
            );
        }
        let _ = writeln!(s, "  derived coFays:        {}", self.derived_cofays.len());
        for item in &self.derived_cofays {
            let _ = writeln!(
                s,
                "    - {} ({}) role={}",
                item.name, item.cofay_id, item.role_charter.role_title
            );
        }
        let _ = writeln!(s, "  dynamic codes issued:  {}", self.dynamic_codes.len());
        let _ = writeln!(s, "  auth exchange grants:  {}", self.auth_grants.len());
        s
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SealedCredentialEntry {
    grant: AuthExchangeGrant,
    sealed_secret_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalletVaultData {
    schema_version: String,
    label: String,
    master_seed_hex: String,
    human_id_masked: String,
    human_id_commitment: String,
    exchange_authority_pubkey_hex: String,
    created_at_unix: u64,
    next_ifay_index: u32,
    next_cofay_index: u32,
    derived_ifays: Vec<DerivedIfayRecord>,
    derived_cofays: Vec<DerivedCofayRecord>,
    dynamic_codes: Vec<DynamicCodeTicket>,
    sealed_credentials: Vec<SealedCredentialEntry>,
}

/// 将任意字节序列编码为小写无填充 RFC 4648 Base32（仅含 `a..=z` 与 `2..=7`）。
#[must_use]
pub fn encode_base32_lower_nopad(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;
    for &b in bytes {
        buffer = (buffer << 8) | u32::from(b);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1f) as usize;
            out.push(char::from(RFC4648_LOWER_BASE32[idx]));
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        out.push(char::from(RFC4648_LOWER_BASE32[idx]));
    }
    out
}

/// 从输入摘要构造带前缀的 26 字符规范化 Base32 标识（如 `ifay_...` / `cofay_...` / `hid_...`）。
#[must_use]
pub fn prefixed_base32_id(prefix: &str, material: &[u8]) -> String {
    let digest = Sha256::digest(material);
    let b32 = encode_base32_lower_nopad(&digest[..20]);
    let body = &b32[..26.min(b32.len())];
    format!("{prefix}{body}")
}

/// 初始化或读取本地 `FayID` 保险箱。若已初始化且 `force == false`，报错提示以免意外覆盖助记词。
pub fn init_wallet(
    state_dir: &Path,
    label: &str,
    seed_phrase: Option<&str>,
    force: bool,
) -> Result<WalletInitReceipt, CliError> {
    let vault_path = state_dir.join(VAULT_FILENAME);
    if vault_path.exists() && !force {
        return Err(CliError::Failure(FaygerError::new(
            ErrorCode::FmValidationFailed,
            Layer::Assembler,
            "FayID wallet vault is already initialized in state_dir (pass --force to overwrite)",
        )));
    }

    let mnemonic = seed_phrase.map_or_else(generate_mnemonic_12, |s| s.trim().to_owned());
    if mnemonic.is_empty() {
        return Err(CliError::Usage(
            "seed phrase must not be empty when provided".to_owned(),
        ));
    }

    let seed_bytes = derive_seed_bytes("fayid.master-seed.v1:", mnemonic.as_bytes());
    let master_seed_hex = encode_hex(&seed_bytes);
    let raw_human_id = prefixed_base32_id(
        "hid_",
        &derive_seed_bytes("fayid.human-id.v1:", &seed_bytes),
    );
    let human_id_masked = mask_human_id(&raw_human_id);
    let commitment_hex = hex_sha256(format!("fayid.human-commitment.v1:{raw_human_id}").as_bytes());
    let human_id_commitment = format!("sha256:{commitment_hex}");

    let auth_sk = SigningKey::from_bytes(&derive_seed_bytes(
        "fayid.auth-exchange.key.v1:",
        &seed_bytes,
    ));
    let exchange_authority_pubkey_hex = encode_hex(auth_sk.verifying_key().as_bytes());
    let now = unix_now_secs();
    let clean_label = if label.trim().is_empty() {
        "Primary Human Vault".to_owned()
    } else {
        label.trim().to_owned()
    };

    let vault = WalletVaultData {
        schema_version: WALLET_VAULT_SCHEMA_VERSION.to_owned(),
        label: clean_label.clone(),
        master_seed_hex,
        human_id_masked: human_id_masked.clone(),
        human_id_commitment: human_id_commitment.clone(),
        exchange_authority_pubkey_hex: exchange_authority_pubkey_hex.clone(),
        created_at_unix: now,
        next_ifay_index: 0,
        next_cofay_index: 0,
        derived_ifays: Vec::new(),
        derived_cofays: Vec::new(),
        dynamic_codes: Vec::new(),
        sealed_credentials: Vec::new(),
    };
    save_vault(state_dir, &vault)?;

    Ok(WalletInitReceipt {
        schema_version: WALLET_VAULT_SCHEMA_VERSION.to_owned(),
        label: clean_label,
        recovery_mnemonic_once: mnemonic,
        human_id_masked,
        human_id_commitment,
        exchange_authority_pubkey_hex,
        created_at_unix: now,
    })
}

/// 确保保险箱已初始化（若未初始化则使用默认标签自动初始化，便于一键创建灵魂或 Web 首次点击）。
pub fn ensure_wallet_initialized(state_dir: &Path) -> Result<Option<WalletInitReceipt>, CliError> {
    let vault_path = state_dir.join(VAULT_FILENAME);
    if vault_path.exists() {
        return Ok(None);
    }
    let receipt = init_wallet(state_dir, "Primary Human Vault", None, false)?;
    Ok(Some(receipt))
}

/// 读取对外脱敏的保险箱公开状态。
pub fn show_wallet(state_dir: &Path) -> Result<WalletPublicView, CliError> {
    let vault_path = state_dir.join(VAULT_FILENAME);
    if !vault_path.exists() {
        return Ok(WalletPublicView::uninitialized());
    }
    let vault = load_vault(state_dir)?;
    Ok(public_view_of(&vault))
}

/// 返回用于本机终端确认证明（`HumanConfirmationProof`）的脱敏承诺型 `hid_<base32>` 标识。
///
/// 该标识由 `human_id_commitment` 二次哈希生成，满足 `CAP` 的 `hid_<base32>` 语法校验，
/// 同时不在审计日志中泄露原始根 `Human ID`。
pub fn local_attestation_human_token(state_dir: &Path) -> Result<String, CliError> {
    let vault_path = state_dir.join(VAULT_FILENAME);
    if !vault_path.exists() {
        let _ = ensure_wallet_initialized(state_dir)?;
    }
    let vault = load_vault(state_dir)?;
    Ok(prefixed_base32_id(
        "hid_",
        vault.human_id_commitment.as_bytes(),
    ))
}

/// 在保险箱中确定性派生一个新的 `iFay` 个人数字灵魂身份（`ifay_...`）。
pub fn derive_ifay(state_dir: &Path, name: &str) -> Result<DerivedIfayRecord, CliError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CliError::Usage(
            "derive-ifay requires non-empty --name".to_owned(),
        ));
    }
    let _ = ensure_wallet_initialized(state_dir)?;
    let mut vault = load_vault(state_dir)?;
    let idx = vault.next_ifay_index;
    vault.next_ifay_index = vault.next_ifay_index.saturating_add(1);

    let sk = derive_subkey(&vault.master_seed_hex, "ifay", idx, trimmed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let ifay_id = prefixed_base32_id("ifay_", &pk_bytes);
    let record = DerivedIfayRecord {
        ifay_id,
        name: trimmed.to_owned(),
        derivation_index: idx,
        public_key_hex: encode_hex(&pk_bytes),
        discovery_mode: "stealth_unregistered".to_owned(),
        created_at_unix: unix_now_secs(),
    };
    vault.derived_ifays.push(record.clone());
    save_vault(state_dir, &vault)?;
    Ok(record)
}

/// 在保险箱中确定性派生一个新的 `coFay` 公共岗位席位（`cofay_...` + `RoleCharter` 1.0）。
pub fn derive_cofay(
    state_dir: &Path,
    name: &str,
    role_title: &str,
    scope: &str,
) -> Result<DerivedCofayRecord, CliError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CliError::Usage(
            "derive-cofay requires non-empty --name".to_owned(),
        ));
    }
    let clean_role = if role_title.trim().is_empty() {
        format!("{trimmed} Seat Steward")
    } else {
        role_title.trim().to_owned()
    };
    let clean_scope = if scope.trim().is_empty() {
        "Public role coordination, auditable workspace execution, and TP collaboration".to_owned()
    } else {
        scope.trim().to_owned()
    };

    let _ = ensure_wallet_initialized(state_dir)?;
    let mut vault = load_vault(state_dir)?;
    let idx = vault.next_cofay_index;
    vault.next_cofay_index = vault.next_cofay_index.saturating_add(1);

    let sk = derive_subkey(&vault.master_seed_hex, "cofay", idx, trimmed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let cofay_id = prefixed_base32_id("cofay_", &pk_bytes);
    let charter = RoleCharter {
        schema_version: ROLE_CHARTER_SCHEMA_VERSION.to_owned(),
        cofay_id: cofay_id.clone(),
        seat_name: trimmed.to_owned(),
        role_title: clean_role,
        public_mandate_scope: clean_scope,
        custodian_commitment: vault.human_id_commitment.clone(),
        term_epoch: 1,
        dual_sig_handover_supported: true,
    };
    let record = DerivedCofayRecord {
        cofay_id,
        name: trimmed.to_owned(),
        derivation_index: idx,
        public_key_hex: encode_hex(&pk_bytes),
        role_charter: charter,
        created_at_unix: unix_now_secs(),
    };
    vault.derived_cofays.push(record.clone());
    save_vault(state_dir, &vault)?;
    Ok(record)
}

/// 签发一枚防关联动态码（`dync_...`）。
pub fn issue_dynamic_code(
    state_dir: &Path,
    purpose: &str,
    ttl_secs: u64,
) -> Result<DynamicCodeTicket, CliError> {
    let mut vault = load_vault(state_dir)?;
    let now = unix_now_secs();
    let effective_ttl = ttl_secs.clamp(30, 86_400);
    let clean_purpose = if purpose.trim().is_empty() {
        "verify_with_fayid".to_owned()
    } else {
        purpose.trim().to_owned()
    };
    let nonce = fresh_entropy_32();
    let mut material = Vec::new();
    material.extend_from_slice(b"fayid.dynamic-code.v1:");
    material.extend_from_slice(vault.master_seed_hex.as_bytes());
    material.extend_from_slice(&nonce);
    material.extend_from_slice(clean_purpose.as_bytes());
    let dynamic_code = prefixed_base32_id("dync_", &material);

    let proof_preimage = format!(
        "fayid.dync-proof.v1:{dynamic_code}:{clean_purpose}:{}:{}",
        encode_hex(&nonce[..16]),
        now
    );
    let ticket = DynamicCodeTicket {
        dynamic_code,
        purpose: clean_purpose,
        unlinkable_proof_sha256: hex_sha256(proof_preimage.as_bytes()),
        issued_at_unix: now,
        expires_at_unix: now.saturating_add(effective_ttl),
    };
    vault.dynamic_codes.push(ticket.clone());
    save_vault(state_dir, &vault)?;
    Ok(ticket)
}

/// `Auth Exchange`：封存传统 API Key / Token 并兑换限时、限作用域的已签名授权票据。
pub fn exchange_auth_grant(
    state_dir: &Path,
    provider: &str,
    legacy_secret: &str,
    bound_fay_id: Option<&str>,
    scopes: Vec<String>,
    ttl_secs: u64,
) -> Result<AuthExchangeGrant, CliError> {
    let clean_provider = provider.trim();
    if clean_provider.is_empty() {
        return Err(CliError::Usage(
            "exchange-grant requires non-empty --provider".to_owned(),
        ));
    }
    if legacy_secret.trim().is_empty() {
        return Err(CliError::Usage(
            "exchange-grant requires non-empty --secret".to_owned(),
        ));
    }
    let _ = ensure_wallet_initialized(state_dir)?;
    let mut vault = load_vault(state_dir)?;
    let now = unix_now_secs();
    let effective_ttl = ttl_secs.clamp(60, 604_800);
    let nonce = fresh_entropy_32();
    let grant_digest = hex_sha256(&nonce);
    let grant_id = format!("ag_{}", &grant_digest[..16]);

    let fp_full =
        hex_sha256(format!("fayid.secret-fp.v1:{clean_provider}:{legacy_secret}").as_bytes());
    let secret_fingerprint = format!("sha256:{}", &fp_full[..16]);

    let clean_scopes = if scopes.is_empty() {
        vec![format!("{clean_provider}:default")]
    } else {
        scopes
    };
    let clean_bound = bound_fay_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    // 使用主种子派生流密钥密封原始凭据，磁盘与 API 均不保存明文。
    let sealed_secret_hex =
        seal_secret_bytes(&vault.master_seed_hex, &grant_id, legacy_secret.as_bytes());

    let seed_bytes = derive_seed_bytes("fayid.rehydrate.v1:", vault.master_seed_hex.as_bytes());
    let _ = seed_bytes;
    let auth_sk = authority_signing_key(&vault.master_seed_hex);
    let authority_pubkey_hex = encode_hex(auth_sk.verifying_key().as_bytes());
    let expires_at_unix = now.saturating_add(effective_ttl);

    let payload = json!({
        "authority_pubkey_hex": authority_pubkey_hex,
        "bound_fay_id": clean_bound,
        "expires_at_unix": expires_at_unix,
        "grant_id": grant_id,
        "issued_at_unix": now,
        "provider": clean_provider,
        "schema_version": AUTH_GRANT_SCHEMA_VERSION,
        "scopes": clean_scopes,
        "secret_fingerprint": secret_fingerprint,
    });
    let canonical = jcs_canonicalize(&payload)?;
    let mut domain_msg = b"fayid.auth-exchange.v1:".to_vec();
    domain_msg.extend_from_slice(canonical.as_bytes());
    let sig = auth_sk.sign(&domain_msg);

    let grant = AuthExchangeGrant {
        schema_version: AUTH_GRANT_SCHEMA_VERSION.to_owned(),
        grant_id,
        provider: clean_provider.to_owned(),
        bound_fay_id: clean_bound,
        scopes: clean_scopes,
        secret_fingerprint,
        issued_at_unix: now,
        expires_at_unix,
        authority_pubkey_hex,
        signature_hex: encode_hex(&sig.to_bytes()),
    };
    vault.sealed_credentials.push(SealedCredentialEntry {
        grant: grant.clone(),
        sealed_secret_hex,
    });
    save_vault(state_dir, &vault)?;
    Ok(grant)
}

/// 获取当前 Unix 秒级时间戳。
#[must_use]
pub fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1_763_600_000, |d| d.as_secs())
}

fn public_view_of(vault: &WalletVaultData) -> WalletPublicView {
    WalletPublicView {
        initialized: true,
        label: vault.label.clone(),
        human_id_masked: vault.human_id_masked.clone(),
        human_id_commitment: vault.human_id_commitment.clone(),
        exchange_authority_pubkey_hex: vault.exchange_authority_pubkey_hex.clone(),
        derived_ifays: vault.derived_ifays.clone(),
        derived_cofays: vault.derived_cofays.clone(),
        dynamic_codes: vault.dynamic_codes.clone(),
        auth_grants: vault
            .sealed_credentials
            .iter()
            .map(|entry| entry.grant.clone())
            .collect(),
    }
}

fn load_vault(state_dir: &Path) -> Result<WalletVaultData, CliError> {
    let path = state_dir.join(VAULT_FILENAME);
    let raw = fs::read_to_string(&path).map_err(|err| CliError::Io {
        path: path.display().to_string(),
        detail: format!("FayID wallet vault not found (run `fayger wallet init` first): {err}"),
    })?;
    serde_json::from_str(&raw).map_err(|err| CliError::Io {
        path: path.display().to_string(),
        detail: format!("corrupted wallet_vault.json: {err}"),
    })
}

fn save_vault(state_dir: &Path, vault: &WalletVaultData) -> Result<(), CliError> {
    fs::create_dir_all(state_dir).map_err(|err| CliError::Io {
        path: state_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let path = state_dir.join(VAULT_FILENAME);
    let pretty = serde_json::to_string_pretty(vault).map_err(|err| CliError::Io {
        path: path.display().to_string(),
        detail: err.to_string(),
    })?;
    fs::write(&path, pretty).map_err(|err| CliError::Io {
        path: path.display().to_string(),
        detail: err.to_string(),
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        let _ = fs::set_permissions(&path, perms);
    }
    Ok(())
}

fn derive_seed_bytes(domain: &str, input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(input);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn derive_subkey(master_seed_hex: &str, kind: &str, index: u32, name: &str) -> SigningKey {
    let preimage = format!("fayid.subkey.v1:{kind}:{index}:{name}:{master_seed_hex}");
    let key_bytes = derive_seed_bytes("fayid.ed25519.v1:", preimage.as_bytes());
    SigningKey::from_bytes(&key_bytes)
}

fn authority_signing_key(master_seed_hex: &str) -> SigningKey {
    let mut seed_raw = [0u8; 32];
    let bytes = master_seed_hex.as_bytes();
    if bytes.len() == 64 {
        for (i, slot) in seed_raw.iter_mut().enumerate() {
            let hi = hex_val(bytes[i * 2]);
            let lo = hex_val(bytes[i * 2 + 1]);
            *slot = (hi << 4) | lo;
        }
    } else {
        seed_raw = derive_seed_bytes("fayid.fallback.v1:", bytes);
    }
    SigningKey::from_bytes(&derive_seed_bytes("fayid.auth-exchange.key.v1:", &seed_raw))
}

const fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn seal_secret_bytes(master_seed_hex: &str, grant_id: &str, plaintext: &[u8]) -> String {
    let mut sealed = Vec::with_capacity(plaintext.len());
    for (block_idx, chunk) in plaintext.chunks(32).enumerate() {
        let key_stream = derive_seed_bytes(
            "fayid.seal-stream.v1:",
            format!("{master_seed_hex}:{grant_id}:{block_idx}").as_bytes(),
        );
        for (i, &byte) in chunk.iter().enumerate() {
            sealed.push(byte ^ key_stream[i]);
        }
    }
    encode_hex(&sealed)
}

fn mask_human_id(raw_human_id: &str) -> String {
    if raw_human_id.len() <= 12 {
        return "hid_****".to_owned();
    }
    let head = &raw_human_id[..8];
    let tail = &raw_human_id[raw_human_id.len() - 4..];
    format!("{head}...{tail}")
}

fn generate_mnemonic_12() -> String {
    let entropy = fresh_entropy_32();
    let mut words = Vec::with_capacity(12);
    for slot in &entropy[..12] {
        let idx = usize::from(*slot) % MNEMONIC_WORDS.len();
        words.push(MNEMONIC_WORDS[idx]);
    }
    words.join(" ")
}

/// 生成 32 字节安全随机熵（优先读取 `/dev/urandom`，并混入高精度时钟、进程号与 RandomState）。
pub fn fresh_entropy_32() -> [u8; 32] {
    let mut os_buf = [0u8; 32];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut os_buf);
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0u128, |d| d.as_nanos());
    let seq = ENTROPY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let random_hasher_val = RandomState::new().build_hasher().finish();

    let mut hasher = Sha256::new();
    hasher.update(b"fayger.csprng.mix.v1:");
    hasher.update(os_buf);
    hasher.update(nanos.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(seq.to_le_bytes());
    hasher.update(random_hasher_val.to_le_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        derive_cofay, derive_ifay, exchange_auth_grant, init_wallet, issue_dynamic_code,
        show_wallet, ROLE_CHARTER_SCHEMA_VERSION,
    };
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fayger-wallet-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn wallet_lifecycle_never_exposes_plaintext_human_id_or_raw_secret() {
        let dir = temp_dir();
        let receipt = init_wallet(
            &dir,
            "Alice Vault",
            Some("anchor atlas aurora beacon breeze canyon cedar cipher cobalt comet coral cosmos"),
            false,
        )
        .expect("init");
        assert!(receipt.human_id_masked.starts_with("hid_"));
        assert!(receipt.human_id_masked.contains("..."));
        assert!(receipt.human_id_commitment.starts_with("sha256:"));

        let ifay = derive_ifay(&dir, "alice-personal").expect("derive ifay");
        assert!(ifay.ifay_id.starts_with("ifay_"));
        assert_eq!(ifay.discovery_mode, "stealth_unregistered");

        let cofay = derive_cofay(
            &dir,
            "release-chair",
            "Release Steward",
            "Cut signed builds",
        )
        .expect("derive cofay");
        assert!(cofay.cofay_id.starts_with("cofay_"));
        assert_eq!(
            cofay.role_charter.schema_version,
            ROLE_CHARTER_SCHEMA_VERSION
        );

        let dync = issue_dynamic_code(&dir, "forum-login", 300).expect("dync");
        assert!(dync.dynamic_code.starts_with("dync_"));

        let demo_input = "<example-placeholder-legacy-credential-9999>";
        let grant = exchange_auth_grant(
            &dir,
            "openai",
            demo_input,
            Some(&ifay.ifay_id),
            vec!["chat.completions".to_owned()],
            3600,
        )
        .expect("grant");
        assert!(grant.grant_id.starts_with("ag_"));

        let view = show_wallet(&dir).expect("show");
        let serialized_view = serde_json::to_string(&view).expect("json");
        assert!(!serialized_view.contains(demo_input));
        assert!(!serialized_view.contains("anchor atlas aurora"));

        let raw_vault_on_disk =
            fs::read_to_string(dir.join("wallet_vault.json")).expect("read vault");
        assert!(!raw_vault_on_disk.contains(demo_input));

        let _ = fs::remove_dir_all(&dir);
    }
}
