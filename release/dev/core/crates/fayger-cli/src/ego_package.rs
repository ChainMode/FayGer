//! 已签名 `Ego_Package` 双层验签、按需 Cell SHA-256 核验与保真度策略准入门控（Wave 2）。
//!
//! 跨仓密码学与结构契约（对齐 `Ego` ADR `0002` 与 `FayID` `ifay.ego.package-signing.v1`）：
//! 1. **Tier 2 清单签名**：剥离 `manifest.json` 顶层 `"signature"` 键后，对剩余 JSON 对象按
//!    RFC 8785 JCS 规范化得到 `jcs_bytes`；核验 `sha256(jcs_bytes) == signature.payload_jcs_sha256`，
//!    并在域分离负载 `b"ifay.ego.package-signing.v1:" || jcs_bytes` 上执行 Ed25519 `verify_strict`。
//! 2. **身份绑定**：`signature.signer_fay_id` 必须等于目标 `FayManifest` 声明的 `fayId`（`ifay_...`），
//!    且 `signature.public_key_hex` 必须匹配受信任的派生子公钥。
//! 3. **Tier 1 Cell 按需摘要校验**：对请求挂载的每个 Cell 相对路径，校验其无路径越界后读取真实字节，
//!    计算 `sha256:<64-lowercase-hex>` 并与 `manifest.integrity.cell_digests` 逐项比对。
//! 4. **保真度策略准入**：按 [`FidelityGatePolicy`] 评估 `manifest.fidelity_report`（`#[serde(deny_unknown_fields)]`
//!    保证不夹带任何原始对话样本或探针文本）。

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// `Ego_Package` 签名域分离标识（`FayID` / `Ego` Wave 2 冻结常量）。
pub const EGO_SIGNING_DOMAIN: &str = "ifay.ego.package-signing.v1";

const EGO_SIGNING_PREFIX: &[u8] = b"ifay.ego.package-signing.v1:";

/// 脱敏数值化人格保真度报告（严禁包含任何原始对话或探针文本）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FidelityReport {
    /// 报告结构版本（`"1.0"`）。
    pub schema_version: String,
    /// 评估状态：`"measured"` 或 `"insufficient_data"`。
    pub status: String,
    /// 本地留出集样本条数。
    pub holdout_sample_count: u64,
    /// 留出集交叉熵损失（可选）。
    pub holdout_loss: Option<f64>,
    /// 留出集困惑度（可选）。
    pub holdout_perplexity: Option<f64>,
    /// 跨 Cell 行为一致性得分 `[0.0, 1.0]`（可选）。
    pub cell_consistency_score: Option<f64>,
    /// 本地人类原型配对探针胜率 `[0.0, 1.0]`（可选）。
    pub probe_win_rate: Option<f64>,
    /// 相对上一签名包的统计漂移（可选）。
    pub drift_from_baseline: Option<f64>,
}

/// `FayGer` 侧对 `Ego_Package` 的保真度准入策略。
#[derive(Debug, Clone, PartialEq)]
pub struct FidelityGatePolicy {
    /// 是否允许 `status == "insufficient_data"` 的包通过准入。
    pub allow_insufficient_data: bool,
    /// 最低跨 Cell 行为一致性阈值（若设置）。
    pub min_cell_consistency: Option<f64>,
    /// 最高留出集困惑度上限（若设置）。
    pub max_holdout_perplexity: Option<f64>,
    /// 最低人类原型探针胜率阈值（若设置）。
    pub min_probe_win_rate: Option<f64>,
}

impl Default for FidelityGatePolicy {
    fn default() -> Self {
        Self::production_default()
    }
}

impl FidelityGatePolicy {
    /// 生产默认准入阈值：要求已完成测量，且一致性 `>= 0.70`、困惑度 `<= 50.0`。
    #[must_use]
    pub const fn production_default() -> Self {
        Self {
            allow_insufficient_data: false,
            min_cell_consistency: Some(0.70),
            max_holdout_perplexity: Some(50.0),
            min_probe_win_rate: None,
        }
    }

    /// 宽松离线策略（允许 `insufficient_data`）。
    #[must_use]
    pub const fn permissive_offline() -> Self {
        Self {
            allow_insufficient_data: true,
            min_cell_consistency: None,
            max_holdout_perplexity: None,
            min_probe_win_rate: None,
        }
    }

    /// 评估一份 [`FidelityReport`] 是否满足当前准入策略。
    ///
    /// # Errors
    /// 不满足阈值或状态要求时返回 `FM_VALIDATION_FAILED`（`reason = EgoFidelityGateRejected`）。
    pub fn evaluate(&self, report: &FidelityReport) -> Result<(), FaygerError> {
        if report.schema_version != "1.0" {
            return Err(fidelity_rejected("UnsupportedFidelitySchemaVersion"));
        }
        match report.status.as_str() {
            "insufficient_data" => {
                if !self.allow_insufficient_data {
                    return Err(fidelity_rejected("InsufficientFidelityData"));
                }
                return Ok(());
            }
            "measured" => {}
            _ => return Err(fidelity_rejected("InvalidFidelityStatus")),
        }

        if let Some(min_cons) = self.min_cell_consistency {
            let actual = report
                .cell_consistency_score
                .ok_or_else(|| fidelity_rejected("MissingCellConsistencyScore"))?;
            if actual < min_cons {
                return Err(fidelity_rejected("CellConsistencyBelowThreshold"));
            }
        }
        if let Some(max_ppl) = self.max_holdout_perplexity {
            let actual = report
                .holdout_perplexity
                .ok_or_else(|| fidelity_rejected("MissingHoldoutPerplexity"))?;
            if actual > max_ppl {
                return Err(fidelity_rejected("HoldoutPerplexityAboveThreshold"));
            }
        }
        if let Some(min_probe) = self.min_probe_win_rate {
            let actual = report
                .probe_win_rate
                .ok_or_else(|| fidelity_rejected("MissingProbeWinRate"))?;
            if actual < min_probe {
                return Err(fidelity_rejected("ProbeWinRateBelowThreshold"));
            }
        }
        Ok(())
    }
}

/// Tier 1 完整性摘要声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgoIntegrity {
    /// 结构版本（`"1.0"`）。
    pub schema_version: String,
    /// 基座模型权重摘要（`"sha256:<hex>"` 或 `"unverified_offline"`）。
    pub base_model_digest: String,
    /// 相对于包根目录的 POSIX 路径 → `"sha256:<64-lowercase-hex>"` 映射。
    pub cell_digests: BTreeMap<String, String>,
}

/// Tier 2 Ed25519 签名信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgoSignature {
    /// 结构版本（`"1.0"`）。
    pub schema_version: String,
    /// 签名算法（固定为 `"Ed25519"`）。
    pub alg: String,
    /// 签名域分离标签（固定为 [`EGO_SIGNING_DOMAIN`]）。
    pub domain: String,
    /// 签名主体规范化 `iFay` 标识（`"ifay_..."`）。
    pub signer_fay_id: String,
    /// 派生子密钥引用标识。
    pub key_id: String,
    /// 32 字节 Ed25519 公钥的小写十六进制（64 字符）。
    pub public_key_hex: String,
    /// JCS 规范化无签名清单的 SHA-256 小写十六进制（64 字符）。
    pub payload_jcs_sha256: String,
    /// 64 字节 Ed25519 签名的小写十六进制（128 字符）。
    pub sig_hex: String,
}

/// 通过双层验签与保真度准入后的已挂载 `Ego_Package` 摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedEgoMount {
    /// 经核验的签名者 `fay_id`。
    pub signer_fay_id: String,
    /// 签名子密钥标识。
    pub key_id: String,
    /// 基座模型名称。
    pub base_model: String,
    /// 基座模型摘要标注。
    pub base_model_digest: String,
    /// 本次按需核验通过的 Cell 相对路径 → 实测摘要。
    pub mounted_cells: BTreeMap<String, String>,
    /// 保真度报告状态（`"measured"` / `"insufficient_data"`）。
    pub fidelity_status: String,
    /// 清单 JCS SHA-256 摘要。
    pub payload_jcs_sha256: String,
}

/// 对磁盘上的 `Ego_Package` 执行双层密码学验签、按需 Cell SHA-256 核验与保真度门控。
///
/// 若 `cells_to_mount` 为空，则默认核验 `integrity.cell_digests` 中声明的全部 Cell。
///
/// # Errors
/// - 文件不可读返回 `LDR_SOURCE_READ_FAILED`；
/// - JSON 结构非法返回 `LDR_PARSE_FAIL`；
/// - 签名缺失、公钥不匹配、`signer_fay_id` 不一致或 Ed25519 验签失败返回 `LDR_SIGNATURE_FAIL`；
/// - 清单 JCS 摘要或 Cell `.safetensors` SHA-256 不匹配返回 `LDR_DIGEST_MISMATCH`；
/// - 保真度报告未通过策略返回 `FM_VALIDATION_FAILED`。
#[allow(clippy::too_many_lines)]
pub fn verify_and_mount_ego_package(
    package_dir: &Path,
    expected_fay_id: &str,
    trusted_public_key_hex: &str,
    cells_to_mount: &[String],
    fidelity_policy: &FidelityGatePolicy,
) -> Result<VerifiedEgoMount, FaygerError> {
    let manifest_path = package_dir.join("manifest.json");
    let raw_manifest = fs::read_to_string(&manifest_path).map_err(|err| {
        FaygerError::new(
            ErrorCode::LdrSourceReadFailed,
            Layer::Loader,
            "failed to read Ego_Package manifest.json",
        )
        .with_context(
            "path",
            ContextValue::string(manifest_path.display().to_string()),
        )
        .with_context("detail", ContextValue::string(err.to_string()))
    })?;

    let mut manifest_val: Value = serde_json::from_str(&raw_manifest).map_err(|err| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "Ego_Package manifest.json is not valid JSON",
        )
        .with_context("reason", ContextValue::string("EgoManifestJsonInvalid"))
        .with_context("detail", ContextValue::string(err.to_string()))
    })?;

    let obj = manifest_val.as_object_mut().ok_or_else(|| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "Ego_Package manifest.json root must be a JSON object",
        )
        .with_context("reason", ContextValue::string("EgoManifestNotObject"))
    })?;

    let sig_val = obj.remove("signature").ok_or_else(|| {
        sig_fail(
            "MissingSignature",
            "Ego_Package manifest.json is missing signature",
        )
    })?;
    let sig: EgoSignature = serde_json::from_value(sig_val).map_err(|err| {
        sig_fail(
            "InvalidSignatureEnvelope",
            format!("invalid Ego_Package signature block: {err}"),
        )
    })?;

    verify_signature_metadata(&sig, expected_fay_id, trusted_public_key_hex)?;

    let jcs_string = jcs_canonicalize(&Value::Object(obj.clone()))?;
    let jcs_bytes = jcs_string.as_bytes();
    let computed_jcs_sha256 = hex_sha256(jcs_bytes);
    if !computed_jcs_sha256.eq_ignore_ascii_case(&sig.payload_jcs_sha256) {
        return Err(FaygerError::new(
            ErrorCode::LdrDigestMismatch,
            Layer::Loader,
            "Ego_Package manifest payload_jcs_sha256 mismatch",
        )
        .with_context("reason", ContextValue::string("ManifestDigest"))
        .with_context("expected", ContextValue::string(sig.payload_jcs_sha256))
        .with_context("actual", ContextValue::string(computed_jcs_sha256)));
    }

    verify_ed25519_domain_signature(&sig.public_key_hex, &sig.sig_hex, jcs_bytes)?;

    let base_model = obj
        .get("base_model")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            FaygerError::new(
                ErrorCode::LdrMissingRequiredField,
                Layer::Loader,
                "Ego_Package manifest missing base_model",
            )
        })?
        .to_owned();

    let integrity_val = obj.get("integrity").cloned().ok_or_else(|| {
        FaygerError::new(
            ErrorCode::LdrMissingRequiredField,
            Layer::Loader,
            "Ego_Package manifest missing integrity block",
        )
    })?;
    let integrity: EgoIntegrity = serde_json::from_value(integrity_val).map_err(|err| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "invalid Ego_Package integrity block",
        )
        .with_context("detail", ContextValue::string(err.to_string()))
    })?;

    let mounted_cells = verify_cells_on_demand(package_dir, &integrity, cells_to_mount)?;

    let fidelity_val = obj
        .get("fidelity_report")
        .cloned()
        .ok_or_else(|| fidelity_rejected("MissingFidelityReport"))?;
    let fidelity: FidelityReport = serde_json::from_value(fidelity_val).map_err(|err| {
        fidelity_rejected("InvalidOrTaintedFidelityReport")
            .with_context("detail", ContextValue::string(err.to_string()))
    })?;
    fidelity_policy.evaluate(&fidelity)?;

    Ok(VerifiedEgoMount {
        signer_fay_id: sig.signer_fay_id,
        key_id: sig.key_id,
        base_model,
        base_model_digest: integrity.base_model_digest,
        mounted_cells,
        fidelity_status: fidelity.status,
        payload_jcs_sha256: computed_jcs_sha256,
    })
}

/// 按 RFC 8785 (JCS) 规范化一个 JSON [`Value`]。
///
/// - 对象键按 UTF-16 代码单元升序排列；
/// - 整数值浮点数（如 `1.0` / `0.0`）按 ECMAScript `Number::toString` 规范输出为无小数点整数表示。
pub fn jcs_canonicalize(value: &Value) -> Result<String, FaygerError> {
    match value {
        Value::Null => Ok("null".to_owned()),
        Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_owned()),
        Value::Number(n) => canonicalize_number(n),
        Value::String(s) => serde_json::to_string(s).map_err(|err| {
            FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "JCS string serialization failed",
            )
            .with_context("detail", ContextValue::string(err.to_string()))
        }),
        Value::Array(items) => {
            let mut rendered = Vec::with_capacity(items.len());
            for item in items {
                rendered.push(jcs_canonicalize(item)?);
            }
            Ok(format!("[{}]", rendered.join(",")))
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
            let mut rendered = Vec::with_capacity(entries.len());
            for (k, v) in entries {
                let key_json = serde_json::to_string(k).map_err(|err| {
                    FaygerError::new(
                        ErrorCode::LdrParseFail,
                        Layer::Loader,
                        "JCS key serialization failed",
                    )
                    .with_context("detail", ContextValue::string(err.to_string()))
                })?;
                rendered.push(format!("{key_json}:{}", jcs_canonicalize(v)?));
            }
            Ok(format!("{{{}}}", rendered.join(",")))
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn canonicalize_number(n: &serde_json::Number) -> Result<String, FaygerError> {
    if let Some(i) = n.as_i64() {
        return Ok(i.to_string());
    }
    if let Some(u) = n.as_u64() {
        return Ok(u.to_string());
    }
    let f = n.as_f64().ok_or_else(|| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "non-finite number is invalid in RFC 8785 JCS",
        )
    })?;
    if !f.is_finite() {
        return Err(FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "NaN/Infinity is invalid in RFC 8785 JCS",
        ));
    }
    if f == 0.0 {
        return Ok("0".to_owned());
    }
    if f.fract() == 0.0 && (-9_007_199_254_740_991.0..=9_007_199_254_740_991.0).contains(&f) {
        return Ok((f as i64).to_string());
    }
    serde_json::to_string(n).map_err(|err| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "JCS float serialization failed",
        )
        .with_context("detail", ContextValue::string(err.to_string()))
    })
}

fn verify_signature_metadata(
    sig: &EgoSignature,
    expected_fay_id: &str,
    trusted_public_key_hex: &str,
) -> Result<(), FaygerError> {
    if sig.alg != "Ed25519" {
        return Err(sig_fail(
            "UnsupportedAlgorithm",
            format!("unsupported Ego signature algorithm '{}'", sig.alg),
        ));
    }
    if sig.domain != EGO_SIGNING_DOMAIN {
        return Err(sig_fail(
            "InvalidDomain",
            format!("unexpected Ego signing domain '{}'", sig.domain),
        ));
    }
    if !is_canonical_ifay_id(&sig.signer_fay_id) {
        return Err(sig_fail(
            "InvalidSignerFayIdFormat",
            format!(
                "Ego signer_fay_id '{}' is not canonical ifay_<base32>",
                sig.signer_fay_id
            ),
        ));
    }
    if sig.signer_fay_id != expected_fay_id {
        return Err(sig_fail(
            "SignerFayIdMismatch",
            format!(
                "Ego signer_fay_id '{}' does not match expected '{}'",
                sig.signer_fay_id, expected_fay_id
            ),
        ));
    }
    if !sig
        .public_key_hex
        .eq_ignore_ascii_case(trusted_public_key_hex)
    {
        return Err(sig_fail(
            "UnknownKey",
            "Ego public_key_hex is not trusted for this iFay",
        ));
    }
    Ok(())
}

fn verify_ed25519_domain_signature(
    public_key_hex: &str,
    sig_hex: &str,
    jcs_bytes: &[u8],
) -> Result<(), FaygerError> {
    let pk_bytes = decode_hex_exact::<32>(public_key_hex)
        .ok_or_else(|| sig_fail("InvalidPublicKey", "public_key_hex must be 32-byte hex"))?;
    let sig_bytes = decode_hex_exact::<64>(sig_hex)
        .ok_or_else(|| sig_fail("InvalidSignature", "sig_hex must be 64-byte hex"))?;

    let verifying_key = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|err| sig_fail("InvalidPublicKey", err.to_string()))?;
    let signature = Signature::from_bytes(&sig_bytes);

    let mut message = Vec::with_capacity(EGO_SIGNING_PREFIX.len() + jcs_bytes.len());
    message.extend_from_slice(EGO_SIGNING_PREFIX);
    message.extend_from_slice(jcs_bytes);

    verifying_key
        .verify_strict(&message, &signature)
        .map_err(|err| sig_fail("InvalidSignature", err.to_string()))
}

fn verify_cells_on_demand(
    package_dir: &Path,
    integrity: &EgoIntegrity,
    cells_to_mount: &[String],
) -> Result<BTreeMap<String, String>, FaygerError> {
    if integrity.cell_digests.is_empty() {
        return Err(FaygerError::new(
            ErrorCode::LdrMissingRequiredField,
            Layer::Loader,
            "Ego_Package integrity.cell_digests must not be empty",
        ));
    }
    let targets: Vec<String> = if cells_to_mount.is_empty() {
        integrity.cell_digests.keys().cloned().collect()
    } else {
        cells_to_mount.to_vec()
    };

    let mut verified = BTreeMap::new();
    for rel_path in targets {
        let expected_digest = integrity.cell_digests.get(&rel_path).ok_or_else(|| {
            FaygerError::new(
                ErrorCode::LdrDigestMismatch,
                Layer::Loader,
                "requested Ego Cell is absent from integrity.cell_digests",
            )
            .with_context("reason", ContextValue::string("MissingCellDigest"))
            .with_context("cell", ContextValue::string(rel_path.clone()))
        })?;

        let safe_path = safe_join_cell_path(package_dir, &rel_path)?;
        let bytes = fs::read(&safe_path).map_err(|err| {
            FaygerError::new(
                ErrorCode::LdrSourceReadFailed,
                Layer::Loader,
                "failed to read Ego Cell weights file",
            )
            .with_context("cell", ContextValue::string(rel_path.clone()))
            .with_context("detail", ContextValue::string(err.to_string()))
        })?;

        let actual_digest = format!("sha256:{}", hex_sha256(&bytes));
        if !actual_digest.eq_ignore_ascii_case(expected_digest) {
            return Err(FaygerError::new(
                ErrorCode::LdrDigestMismatch,
                Layer::Loader,
                "Ego Cell .safetensors SHA-256 digest mismatch",
            )
            .with_context("reason", ContextValue::string("SectionDigest"))
            .with_context("cell", ContextValue::string(rel_path))
            .with_context("expected", ContextValue::string(expected_digest.clone()))
            .with_context("actual", ContextValue::string(actual_digest)));
        }
        verified.insert(rel_path, actual_digest);
    }
    Ok(verified)
}

fn safe_join_cell_path(package_dir: &Path, rel_path: &str) -> Result<PathBuf, FaygerError> {
    let mut clean = PathBuf::new();
    for comp in Path::new(rel_path).components() {
        match comp {
            Component::Normal(seg) => clean.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(FaygerError::new(
                    ErrorCode::LdrParseFail,
                    Layer::Loader,
                    "disallowed path traversal in Ego Cell path",
                )
                .with_context("cell", ContextValue::string(rel_path)));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "empty Ego Cell path",
        ));
    }
    Ok(package_dir.join(clean))
}

/// 为演示或测试固件对无签名的 `Ego_Package` 清单对象生成合规双层 Ed25519 签名。
pub fn sign_ego_manifest_value(
    unsigned_manifest: &mut Value,
    signing_key: &SigningKey,
    signer_fay_id: &str,
    key_id: &str,
) -> Result<String, FaygerError> {
    if let Some(obj) = unsigned_manifest.as_object_mut() {
        obj.remove("signature");
    }
    let jcs = jcs_canonicalize(unsigned_manifest)?;
    let payload_jcs_sha256 = hex_sha256(jcs.as_bytes());

    let mut message = Vec::with_capacity(EGO_SIGNING_PREFIX.len() + jcs.len());
    message.extend_from_slice(EGO_SIGNING_PREFIX);
    message.extend_from_slice(jcs.as_bytes());

    let signature = signing_key.sign(&message);
    let public_key_hex = encode_hex(&signing_key.verifying_key().to_bytes());
    let sig_hex = encode_hex(&signature.to_bytes());

    let sig_block = EgoSignature {
        schema_version: "1.0".to_owned(),
        alg: "Ed25519".to_owned(),
        domain: EGO_SIGNING_DOMAIN.to_owned(),
        signer_fay_id: signer_fay_id.to_owned(),
        key_id: key_id.to_owned(),
        public_key_hex: public_key_hex.clone(),
        payload_jcs_sha256,
        sig_hex,
    };
    let sig_val = serde_json::to_value(sig_block).map_err(|err| {
        FaygerError::new(
            ErrorCode::LdrParseFail,
            Layer::Loader,
            "failed to serialize EgoSignature",
        )
        .with_context("detail", ContextValue::string(err.to_string()))
    })?;
    if let Some(obj) = unsigned_manifest.as_object_mut() {
        obj.insert("signature".to_owned(), sig_val);
    }
    Ok(public_key_hex)
}

fn is_canonical_ifay_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("ifay_") else {
        return false;
    };
    rest.len() >= 16
        && rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
}

fn sig_fail(reason: &'static str, message: impl Into<String>) -> FaygerError {
    FaygerError::new(ErrorCode::LdrSignatureFail, Layer::Loader, message)
        .with_context("reason", ContextValue::string(reason))
}

fn fidelity_rejected(reason: &'static str) -> FaygerError {
    FaygerError::new(
        ErrorCode::FmValidationFailed,
        Layer::Assembler,
        "Ego_Package rejected by FidelityGatePolicy",
    )
    .with_context("reason", ContextValue::string(reason))
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    encode_hex(&digest)
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn decode_hex_exact<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    let bytes = hex.as_bytes();
    for i in 0..N {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        hex_sha256, jcs_canonicalize, sign_ego_manifest_value, verify_and_mount_ego_package,
        FidelityGatePolicy,
    };
    use ed25519_dalek::SigningKey;
    use fayger_error::ErrorCode;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static PKG_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_pkg_dir() -> PathBuf {
        let seq = PKG_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fayger-ego-pkg-test-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("normal/adapters")).expect("mkdir");
        dir
    }

    #[test]
    fn jcs_sorts_keys_and_normalizes_integral_floats() {
        let val = json!({
            "b": 1.0,
            "a": {"z": 0.92, "m": 0.0}
        });
        assert_eq!(
            jcs_canonicalize(&val).expect("jcs"),
            r#"{"a":{"m":0,"z":0.92},"b":1}"#
        );
    }

    #[test]
    fn two_tier_verification_accepts_valid_package_and_rejects_tampering() {
        let pkg_dir = temp_pkg_dir();
        let weights = b"SAFETENSORS_MOCK_HEADER_AND_TENSORS";
        let cell_rel = "normal/adapters/ego-lora.safetensors";
        fs::write(pkg_dir.join(cell_rel), weights).expect("write weights");
        let cell_sha = format!("sha256:{}", hex_sha256(weights));

        let fay_id = "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f";
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);

        let mut manifest = json!({
            "version": "1.0",
            "created_at": "2026-11-15T12:00:00Z",
            "base_model": "Qwen/Qwen2.5-0.5B-Instruct",
            "editions": ["normal"],
            "training_backend": "mlx-lora",
            "fidelity_report": {
                "schema_version": "1.0",
                "status": "measured",
                "holdout_sample_count": 12,
                "holdout_loss": 1.25,
                "holdout_perplexity": 3.49,
                "cell_consistency_score": 0.91,
                "probe_win_rate": 0.85,
                "drift_from_baseline": 0.04
            },
            "integrity": {
                "schema_version": "1.0",
                "base_model_digest": "unverified_offline",
                "cell_digests": {
                    (cell_rel): cell_sha
                }
            }
        });

        let pubkey_hex =
            sign_ego_manifest_value(&mut manifest, &signing_key, fay_id, "ego-subkey-01")
                .expect("sign");
        fs::write(
            pkg_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("pretty"),
        )
        .expect("write manifest");

        let verified = verify_and_mount_ego_package(
            &pkg_dir,
            fay_id,
            &pubkey_hex,
            &[cell_rel.to_owned()],
            &FidelityGatePolicy::production_default(),
        )
        .expect("valid signed Ego_Package mounts");
        assert_eq!(verified.signer_fay_id, fay_id);
        assert_eq!(verified.mounted_cells.len(), 1);

        // 篡改 .safetensors 1 字节 → Tier 1 Cell SHA-256 拒绝
        fs::write(pkg_dir.join(cell_rel), b"TAMPERED_WEIGHTS").expect("tamper cell");
        let err = verify_and_mount_ego_package(
            &pkg_dir,
            fay_id,
            &pubkey_hex,
            &[cell_rel.to_owned()],
            &FidelityGatePolicy::production_default(),
        )
        .expect_err("tampered cell rejected");
        assert_eq!(err.code, ErrorCode::LdrDigestMismatch);

        // 恢复权重但换错期望 fay_id → Tier 2 身份绑定拒绝
        fs::write(pkg_dir.join(cell_rel), weights).expect("restore cell");
        let id_err = verify_and_mount_ego_package(
            &pkg_dir,
            "ifay_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &pubkey_hex,
            &[cell_rel.to_owned()],
            &FidelityGatePolicy::production_default(),
        )
        .expect_err("wrong fay_id rejected");
        assert_eq!(id_err.code, ErrorCode::LdrSignatureFail);

        let _ = fs::remove_dir_all(&pkg_dir);
    }

    #[test]
    fn fidelity_gate_rejects_raw_text_injection_and_insufficient_metrics() {
        let pkg_dir = temp_pkg_dir();
        let weights = b"WEIGHTS";
        let cell_rel = "normal/adapters/ego-lora.safetensors";
        fs::write(pkg_dir.join(cell_rel), weights).expect("write weights");
        let fay_id = "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f";
        let signing_key = SigningKey::from_bytes(&[0x24; 32]);

        let mut tainted = json!({
            "version": "1.0",
            "created_at": "2026-11-15T12:00:00Z",
            "base_model": "Qwen/Qwen2.5-0.5B-Instruct",
            "editions": ["normal"],
            "training_backend": "mlx-lora",
            "fidelity_report": {
                "schema_version": "1.0",
                "status": "measured",
                "holdout_sample_count": 4,
                "holdout_loss": 1.1,
                "holdout_perplexity": 3.0,
                "cell_consistency_score": 0.9,
                "probe_win_rate": null,
                "drift_from_baseline": null,
                "raw_sample_leak": "NEVER_ALLOWED"
            },
            "integrity": {
                "schema_version": "1.0",
                "base_model_digest": "unverified_offline",
                "cell_digests": {
                    (cell_rel): format!("sha256:{}", hex_sha256(weights))
                }
            }
        });
        let pubkey_hex =
            sign_ego_manifest_value(&mut tainted, &signing_key, fay_id, "k1").expect("sign");
        fs::write(
            pkg_dir.join("manifest.json"),
            serde_json::to_string(&tainted).expect("json"),
        )
        .expect("write");

        let err = verify_and_mount_ego_package(
            &pkg_dir,
            fay_id,
            &pubkey_hex,
            &[],
            &FidelityGatePolicy::production_default(),
        )
        .expect_err("unknown raw text field rejected");
        assert_eq!(err.code, ErrorCode::FmValidationFailed);

        let _ = fs::remove_dir_all(&pkg_dir);
    }

    #[test]
    fn verifies_python_ego_pod_signed_package_vector_byte_for_byte() {
        let pkg_dir = temp_pkg_dir();
        let cell_rel = "normal/adapters/ego-lora.safetensors";
        fs::write(
            pkg_dir.join(cell_rel),
            b"PEFT_SAFETENSORS_CROSS_REPO_VECTOR_V1",
        )
        .expect("write cross-repo cell bytes");

        // 由 `ego_pod.vault.signing.sign_manifest`（master_seed = "01"*32）实测生成的跨仓向量
        let python_signed_manifest = r#"{
  "version": "0.1.0",
  "created_at": "2026-11-15T10:00:00Z",
  "base_model": "qwen3.5-0.8B",
  "editions": [
    "normal"
  ],
  "training_backend": "mlx",
  "fidelity_report": {
    "schema_version": "1.0",
    "status": "measured",
    "holdout_sample_count": 4,
    "holdout_loss": 1.2345,
    "holdout_perplexity": 3.4366,
    "cell_consistency_score": 0.95,
    "probe_win_rate": null,
    "drift_from_baseline": null
  },
  "integrity": {
    "schema_version": "1.0",
    "base_model_digest": "unverified_offline",
    "cell_digests": {
      "normal/adapters/ego-lora.safetensors": "sha256:e88d2a51bd4e5ef1ec7f4f5e661e12adfc8c2f26ca5db7db723013528887e46f"
    }
  },
  "signature": {
    "schema_version": "1.0",
    "alg": "Ed25519",
    "domain": "ifay.ego.package-signing.v1",
    "signer_fay_id": "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f",
    "key_id": "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f#ego-sign-1",
    "public_key_hex": "12335bd629b624bf5ba65ee037ecba7d93101e2353c32d40c721f11b54cd1705",
    "payload_jcs_sha256": "7e31553d091d619237c80cc987ed440d258bf59f307b9408a1bf2699a2ab89a5",
    "sig_hex": "e59acc0cf9ae1c5539be49a70f8556779511a22bd4b316579292b3daeac35399280029c16b3b2a1f1060a3d2a53bd59adb9775247b351e13666167e7dc885b0d"
  }
}"#;
        fs::write(pkg_dir.join("manifest.json"), python_signed_manifest)
            .expect("write python manifest");

        let verified = verify_and_mount_ego_package(
            &pkg_dir,
            "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f",
            "12335bd629b624bf5ba65ee037ecba7d93101e2353c32d40c721f11b54cd1705",
            &[cell_rel.to_owned()],
            &FidelityGatePolicy::production_default(),
        )
        .expect("Python ego_pod signed Ego_Package must verify byte-for-byte in FayGer");

        assert_eq!(verified.base_model, "qwen3.5-0.8B");
        assert_eq!(
            verified.payload_jcs_sha256,
            "7e31553d091d619237c80cc987ed440d258bf59f307b9408a1bf2699a2ab89a5"
        );
        assert_eq!(verified.mounted_cells.len(), 1);

        let _ = fs::remove_dir_all(&pkg_dir);
    }
}
