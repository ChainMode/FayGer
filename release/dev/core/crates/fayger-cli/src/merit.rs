//! `GMChain` 公誉链与 `MeriToken`（美誉值 `μ`）本地口袋与回环 JSON-RPC 2.0 协同引擎。
//!
//! 核心宪章不变量：
//! 1. **严格非金融（Non-Monetary）**：`0` 手续费、`0` 转账，不可买卖或质押生息；
//! 2. **双轨定点模型（`SCALE = 1_000_000`）**：
//!    - `cur_merit_raw`：当前实时美誉值（微单位 `10^-6 μ`）；
//!    - `min_merit_floor_raw`：历史贡献保底值（初始自然常数 `e = 2_718_281`，单调只增不减）；
//!    - `consumable_merit_raw = max(0, cur_merit_raw - min_merit_floor_raw)`：可消费活跃池；
//! 3. **优先权消费底值保护**：行使投票优先权（`voting_priority`）、公共服务优先权
//!    （`cofay_service_priority`）或宿主算力优先调度（`compute_slot_priority`）仅扣减
//!    `consumable_merit_raw`，`min_merit_floor_raw` 绝对保持不变；超额请求立即拒绝；
//! 4. **双模无缝衔接**：当本机回环 `gmc-node`（默认 `http://127.0.0.1:8546/rpc`）在线时走真实
//!    JSON-RPC 2.0 同步（`gmchain_live_rpc`）；离线时无缝回退至同构确定性定点计算（`local_pocket_fallback`）。

use crate::ego_package::{encode_hex, hex_sha256, jcs_canonicalize};
use crate::error::CliError;
use crate::wallet::{fresh_entropy_32, unix_now_secs};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// `MeriToken` 定点精度：`1 μ = 1_000_000` 微单位。
pub const MERIT_SCALE: i128 = 1_000_000;

/// 新挂载 `Fay` 的默认初始实时美誉值：`5.00 μ`（`5_000_000`）。
pub const INITIAL_CUR_MERIT_RAW: i128 = 5_000_000;

/// 宪章自然常数底值 `e ≈ 2.718281 μ`（`2_718_281`，单调只增不减）。
pub const MIN_MERIT_E_RAW: i128 = 2_718_281;

/// 新挂载 `Fay` 的默认初始可消费活跃池：`5_000_000 - 2_718_281 = 2_281_719`（`2.281719 μ`）。
pub const INITIAL_CONSUMABLE_MERIT_RAW: i128 = INITIAL_CUR_MERIT_RAW - MIN_MERIT_E_RAW;

/// 默认本机回环 `GMChain` RPC 端点。
pub const DEFAULT_GMCHAIN_ENDPOINT: &str = "http://127.0.0.1:8546";

/// 分级美誉凭证出示信封 schema 版本。
pub const MERIT_CREDENTIAL_SCHEMA_VERSION: &str = "gmchain-merit-credential/1.0";

/// 在线连接 `gmc-node` 的同步模式标签。
pub const SYNC_MODE_LIVE_RPC: &str = "gmchain_live_rpc";

/// 本机离线确定性回退的同步模式标签。
pub const SYNC_MODE_LOCAL_FALLBACK: &str = "local_pocket_fallback";

/// 合法优先权消费类别集合。
pub const VALID_PRIORITY_KINDS: [&str; 3] = [
    "voting_priority",
    "cofay_service_priority",
    "compute_slot_priority",
];

/// 合法隐私分级凭证档位集合。
pub const VALID_PRIVACY_TIERS: [&str; 3] = ["L1_PUBLIC", "L2_BAND", "L3_ZK_BLINDED"];

/// 回环 RPC 连接与读写超时（`1500ms`）。
const RPC_SOCKET_TIMEOUT: Duration = Duration::from_millis(1500);

/// 供 `PersistedEngineData` `#[serde(default)]` 使用的默认端点构造函数。
#[must_use]
pub fn default_gmchain_endpoint_string() -> String {
    DEFAULT_GMCHAIN_ENDPOINT.to_owned()
}

/// 兼容 `serde_json::Value` 与文本 JSON 的 `i128` 序列化模块（避免 `serde_json::to_value` 溢出错误，同时兼容数字与字符串输入）。
mod serde_i128_compat {
    use super::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(value: &i128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Ok(narrow) = i64::try_from(*value) {
            serializer.serialize_i64(narrow)
        } else {
            serializer.serialize_str(&value.to_string())
        }
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<i128, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Signed(i64),
            Unsigned(u64),
            Text(String),
        }

        match Repr::deserialize(deserializer)? {
            Repr::Signed(v) => Ok(i128::from(v)),
            Repr::Unsigned(v) => Ok(i128::from(v)),
            Repr::Text(s) => s.trim().parse::<i128>().map_err(serde::de::Error::custom),
        }
    }
}

/// 单条美誉值变动流水（铸造入账或优先权消费消减）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeritLedgerEntry {
    /// 流水单号（`batch_...` 或 `csm_...`）。
    pub record_id: String,
    /// 流水类别：`"contribution_added"` 或 `"priority_consumed"`。
    pub entry_kind: String,
    /// 标题（入账）或消费原因（消减）。
    pub title: String,
    /// 领域链标识（如 `gmc-open-science`）或优先权类别（如 `voting_priority`）。
    pub domain_or_priority: String,
    /// 可选关联治理提案编号（如 `prop-001`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    /// 可选贡献衰减周期天数（如 `30` / `90` / `180`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizon_days: Option<u64>,
    /// 本次变动微单位（入账为正，消费消减为负）。
    #[serde(with = "serde_i128_compat")]
    pub delta_mu_raw: i128,
    /// 人类可读变动量文本（如 `"+40.00 μ"` 或 `"-12.00 μ"`）。
    pub delta_formatted: String,
    /// 变动前 `cur_merit_raw`。
    #[serde(with = "serde_i128_compat")]
    pub before_cur_merit_raw: i128,
    /// 变动后 `cur_merit_raw`。
    #[serde(with = "serde_i128_compat")]
    pub after_cur_merit_raw: i128,
    /// 变动后 `min_merit_floor_raw`（优先权消减时严格保持不变）。
    #[serde(with = "serde_i128_compat")]
    pub min_merit_floor_raw: i128,
    /// 变动后 `consumable_merit_raw`。
    #[serde(with = "serde_i128_compat")]
    pub consumable_merit_raw: i128,
    /// 账本来源（`"gmchain_live_rpc"` 或 `"local_pocket_fallback"`）。
    pub source: String,
    /// 记录时区块高度。
    pub block_height: u64,
    /// 记录时间戳（Unix 秒）。
    pub timestamp_unix: u64,
}

/// 引擎内持久化的单个 `Fay` 实例 `MeriToken` 口袋状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeriTokenPocketState {
    /// 可选规范化链上 `canonical_fayid`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_fayid: Option<String>,
    /// 当前实时美誉值微单位（`SCALE = 1_000_000`）。
    #[serde(with = "serde_i128_compat")]
    pub cur_merit_raw: i128,
    /// 历史贡献保底值微单位（初始 `e = 2_718_281`，单调不减）。
    #[serde(with = "serde_i128_compat")]
    pub min_merit_floor_raw: i128,
    /// 可消费活跃美誉值微单位 `max(0, cur_merit_raw - min_merit_floor_raw)`。
    #[serde(with = "serde_i128_compat")]
    pub consumable_merit_raw: i128,
    /// 最近同步来源（`"gmchain_live_rpc"` 或 `"local_pocket_fallback"`）。
    pub sync_mode: String,
    /// 最近区块高度。
    pub last_block_height: u64,
    /// 最近状态根十六进制摘要。
    pub last_state_root_hex: String,
    /// 最近铸造与优先权消费流水记录。
    #[serde(default)]
    pub history: Vec<MeritLedgerEntry>,
}

impl Default for MeriTokenPocketState {
    fn default() -> Self {
        Self::initial_for("genesis_local_pocket")
    }
}

impl MeriTokenPocketState {
    /// 为指定 `fay_id` 初始化标准宪章口袋（`cur=5.00 μ`, `min=2.72 μ`, `consumable=2.28 μ`）。
    #[must_use]
    pub fn initial_for(fay_id: &str) -> Self {
        let root = compute_local_state_root(fay_id, INITIAL_CUR_MERIT_RAW, MIN_MERIT_E_RAW, 0);
        Self {
            canonical_fayid: None,
            cur_merit_raw: INITIAL_CUR_MERIT_RAW,
            min_merit_floor_raw: MIN_MERIT_E_RAW,
            consumable_merit_raw: INITIAL_CONSUMABLE_MERIT_RAW,
            sync_mode: SYNC_MODE_LOCAL_FALLBACK.to_owned(),
            last_block_height: 0,
            last_state_root_hex: root,
            history: Vec::new(),
        }
    }

    /// 导出供 `FayInstanceView`、REST API 与 Web 控制台消费的 [`MeritStandingView`]。
    #[must_use]
    pub fn to_standing_view(&self, fay_id: &str, gmchain_endpoint: &str) -> MeritStandingView {
        let floor = self.min_merit_floor_raw.max(MIN_MERIT_E_RAW);
        let consumable = (self.cur_merit_raw - floor).max(0);
        MeritStandingView {
            fay_id: fay_id.to_owned(),
            canonical_fayid: self
                .canonical_fayid
                .clone()
                .unwrap_or_else(|| fay_id.to_owned()),
            scale: MERIT_SCALE,
            cur_merit_raw: self.cur_merit_raw,
            consumable_merit_raw: consumable,
            min_merit_floor_raw: floor,
            cur_merit_mu: raw_to_mu_f64(self.cur_merit_raw),
            consumable_merit_mu: raw_to_mu_f64(consumable),
            min_merit_floor_mu: raw_to_mu_f64(floor),
            cur_merit_formatted: format_merit_2dp(self.cur_merit_raw),
            consumable_merit_formatted: format_merit_2dp(consumable),
            min_merit_floor_formatted: format_merit_2dp(floor),
            sync_mode: self.sync_mode.clone(),
            source: self.sync_mode.clone(),
            gmchain_endpoint: gmchain_endpoint.to_owned(),
            last_block_height: self.last_block_height,
            last_state_root_hex: self.last_state_root_hex.clone(),
            non_monetary: true,
            history: self.history.clone(),
        }
    }
}

/// 对外呈现的 `Fay` 美誉值全貌视图（挂载于 `FayInstanceView.merit` 与 `/api/v1/fays/<id>/merit`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeritStandingView {
    /// 实例 `fay_id`。
    pub fay_id: String,
    /// 公誉链规范化 `canonical_fayid`。
    pub canonical_fayid: String,
    /// 定点比例尺（固定 `1_000_000`）。
    #[serde(with = "serde_i128_compat")]
    pub scale: i128,
    /// 当前美誉值微单位。
    #[serde(with = "serde_i128_compat")]
    pub cur_merit_raw: i128,
    /// 可消费活跃美誉值微单位（`max(0, cur_merit_raw - min_merit_floor_raw)`）。
    #[serde(with = "serde_i128_compat")]
    pub consumable_merit_raw: i128,
    /// 历史贡献底值微单位（单调不减，初始 `e = 2_718_281`）。
    #[serde(with = "serde_i128_compat")]
    pub min_merit_floor_raw: i128,
    /// 当前美誉值（`μ` 浮点展示值）。
    pub cur_merit_mu: f64,
    /// 可消费活跃美誉值（`μ` 浮点展示值）。
    pub consumable_merit_mu: f64,
    /// 历史贡献底值（`μ` 浮点展示值）。
    pub min_merit_floor_mu: f64,
    /// 两位小数格式化 `curMerit`（如 `"5.00"`）。
    pub cur_merit_formatted: String,
    /// 两位小数格式化可消费额（如 `"2.28"`）。
    pub consumable_merit_formatted: String,
    /// 两位小数格式化底值（如 `"2.72"`）。
    pub min_merit_floor_formatted: String,
    /// 同步模式：`"gmchain_live_rpc"` 或 `"local_pocket_fallback"`。
    pub sync_mode: String,
    /// 账本来源别名（与 `sync_mode` 一致）。
    pub source: String,
    /// 当前配置的回环 `GMChain` 端点。
    pub gmchain_endpoint: String,
    /// 最新区块高度。
    pub last_block_height: u64,
    /// 最新状态根十六进制。
    pub last_state_root_hex: String,
    /// 宪章非金融属性标记（恒为 `true`：`0` 手续费、`0` 转账）。
    pub non_monetary: bool,
    /// 最近铸造与优先权消费流水列表。
    pub history: Vec<MeritLedgerEntry>,
}

impl Default for MeritStandingView {
    fn default() -> Self {
        MeriTokenPocketState::default()
            .to_standing_view("genesis_local_pocket", DEFAULT_GMCHAIN_ENDPOINT)
    }
}

/// `GET/POST /api/v1/merit/config` 返回的 `GMChain` 连接与同步配置视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeritConfigView {
    /// 规范化回环基础端点（如 `"http://127.0.0.1:8546"`）。
    pub gmchain_endpoint: String,
    /// 完整 JSON-RPC 2.0 URL（`"<gmchain_endpoint>/rpc"`）。
    pub rpc_url: String,
    /// 当前回环 `gmc-node` 是否在线可达。
    pub online: bool,
    /// 当前同步模式（`"gmchain_live_rpc"` 或 `"local_pocket_fallback"`）。
    pub sync_mode: String,
    /// 本次已同步的挂载 `Fay` 实例数量。
    pub synced_fays: usize,
}

/// 添加美誉值（`POST /api/v1/fays/<instance_id>/merit/add`）请求参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeritAddInput {
    /// 贡献事项标题。
    pub title: String,
    /// 所属领域链标识（如 `"gmc-open-science"` / `"gmc-public-infra"`）。
    pub domain_chain_id: String,
    /// 申请入账美誉值整数额度（`+Δμ > 0`）。
    pub amount_mu: i64,
    /// 衰减周期天数（如 `30` / `90` / `180`）。
    pub horizon_days: u64,
    /// 贡献核验摘要说明。
    pub summary: String,
}

/// 添加美誉值入账成功回执。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeritAddReceipt {
    /// 生成的贡献批次编号（`batch_...`）。
    pub batch_id: String,
    /// 本次新增微单位（`amount_mu * 1_000_000`）。
    #[serde(with = "serde_i128_compat")]
    pub added_mu_raw: i128,
    /// 区块高度。
    pub block_height: u64,
    /// 状态根摘要。
    pub state_root_hex: String,
    /// 入账后的完整美誉值状态视图（平铺展开全部核心指标）。
    #[serde(flatten)]
    pub standing: MeritStandingView,
}

/// 消费美誉值行使优先权（`POST /api/v1/fays/<instance_id>/merit/consume`）请求参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeritConsumeInput {
    /// 优先权类别：`voting_priority` | `cofay_service_priority` | `compute_slot_priority`。
    pub priority_kind: String,
    /// 精确消减微单位（`> 0`）。
    pub amount_raw: i128,
    /// 用于发送给 `gmc_consumePriority` 的规范化十进制字符串（如 `"12"` 或 `"12.5"`）。
    pub amount_mu_wire: String,
    /// 行使优先权事由说明。
    pub reason: String,
    /// 可选关联治理提案编号（如 `"prop-001"`）。
    pub proposal_id: Option<String>,
}

/// 消费美誉值成功回执。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeritConsumeReceipt {
    /// 优先权消费回执编号（`csm_...`）。
    pub consumption_id: String,
    /// 行使的优先权类别。
    pub priority_kind: String,
    /// 可选关联提案编号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    /// 用途说明。
    pub reason: String,
    /// 实际消减微单位。
    #[serde(with = "serde_i128_compat")]
    pub consumed_mu_raw: i128,
    /// 消减前 `cur_merit_raw`。
    #[serde(with = "serde_i128_compat")]
    pub before_cur_merit_raw: i128,
    /// 消减后 `cur_merit_raw`。
    #[serde(with = "serde_i128_compat")]
    pub after_cur_merit_raw: i128,
    /// 区块高度。
    pub block_height: u64,
    /// 状态根摘要。
    pub state_root_hex: String,
    /// 消减后的完整美誉值状态视图（含严格保持不变的 `min_merit_floor_raw`）。
    #[serde(flatten)]
    pub standing: MeritStandingView,
}

/// 分级隐私美誉凭证出示信封（`gmchain-merit-credential/1.0`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeritCredentialPresentation {
    /// 固定为 [`MERIT_CREDENTIAL_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 凭证唯一标识（`mcred_...`）。
    pub credential_id: String,
    /// 隐私披露等级：`"L1_PUBLIC"` | `"L2_BAND"` | `"L3_ZK_BLINDED"`。
    pub privacy_tier: String,
    /// 签发时间戳（Unix 秒）。
    pub issued_at_unix: u64,
    /// 宪章非金融标记（恒为 `true`）。
    pub non_monetary: bool,
    /// 同步模式。
    pub sync_mode: String,
    /// 锚定区块高度。
    pub block_height: u64,
    /// 锚定公誉链状态根。
    pub state_root_hex: String,
    /// 按所选隐私档位裁剪后的披露负载。
    pub disclosure: Value,
    /// 对凭证主体经 RFC 8785 JCS 规范化后的 SHA-256 密码学承诺摘要。
    pub commitment_sha256: String,
}

/// 校验并规范化回环 `GMChain` HTTP 端点（仅允许 `127.0.0.1`、`localhost` 或 `[::1]`）。
pub fn normalize_loopback_gmchain_endpoint(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim();
    let Some(without_scheme) = trimmed.strip_prefix("http://") else {
        return Err(CliError::Usage(format!(
            "invalid gmchain_endpoint '{trimmed}': must start with 'http://' on loopback (127.0.0.1 or localhost)"
        )));
    };
    let stripped = without_scheme
        .trim_end_matches('/')
        .strip_suffix("/rpc")
        .unwrap_or_else(|| without_scheme.trim_end_matches('/'))
        .trim_end_matches('/');

    if stripped.is_empty() || stripped.contains('/') || stripped.contains('@') {
        return Err(CliError::Usage(format!(
            "invalid gmchain_endpoint '{trimmed}': unexpected URL path or authority"
        )));
    }

    let (host, port_opt) = if let Some(rest) = stripped.strip_prefix('[') {
        let Some((ipv6_host, after_bracket)) = rest.split_once(']') else {
            return Err(CliError::Usage(format!(
                "invalid IPv6 gmchain_endpoint '{trimmed}'"
            )));
        };
        let port = if let Some(p_str) = after_bracket.strip_prefix(':') {
            Some(parse_port(p_str, trimmed)?)
        } else if after_bracket.is_empty() {
            None
        } else {
            return Err(CliError::Usage(format!(
                "invalid authority in gmchain_endpoint '{trimmed}'"
            )));
        };
        (format!("[{ipv6_host}]"), port)
    } else if let Some((h, p_str)) = stripped.rsplit_once(':') {
        (h.to_owned(), Some(parse_port(p_str, trimmed)?))
    } else {
        (stripped.to_owned(), None)
    };

    let is_loopback =
        host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost") || host == "[::1]";
    if !is_loopback {
        return Err(CliError::Usage(format!(
            "gmchain_endpoint '{trimmed}' rejected: restricted to loopback '127.0.0.1' or 'localhost'"
        )));
    }

    let normalized_host = if host.eq_ignore_ascii_case("localhost") {
        "localhost".to_owned()
    } else {
        host
    };
    let port = port_opt.unwrap_or(8546);
    Ok(format!("http://{normalized_host}:{port}"))
}

fn parse_port(p_str: &str, full: &str) -> Result<u16, CliError> {
    let port = p_str.parse::<u16>().map_err(|_| {
        CliError::Usage(format!(
            "invalid port '{p_str}' in gmchain_endpoint '{full}'"
        ))
    })?;
    if port == 0 {
        return Err(CliError::Usage(format!(
            "port cannot be 0 in gmchain_endpoint '{full}'"
        )));
    }
    Ok(port)
}

/// 将微单位（`SCALE = 1_000_000`）四舍五入格式化为两位小数字符串（如 `"5.00"`、`"2.72"`）。
#[must_use]
pub fn format_merit_2dp(raw: i128) -> String {
    let sign = if raw < 0 { "-" } else { "" };
    let abs = raw.unsigned_abs();
    let centi = (abs + 5_000) / 10_000;
    let whole = centi / 100;
    let frac = centi % 100;
    format!("{sign}{whole}.{frac:02}")
}

/// 将微单位格式化为精确无尾随冗余零的十进制字符串（如 `12_000_000 -> "12"`，`12_500_000 -> "12.5"`）。
#[must_use]
pub fn format_merit_clean_decimal(raw: i128) -> String {
    let sign = if raw < 0 { "-" } else { "" };
    let abs = raw.unsigned_abs();
    let whole = abs / (MERIT_SCALE as u128);
    let frac = abs % (MERIT_SCALE as u128);
    if frac == 0 {
        format!("{sign}{whole}")
    } else {
        let frac_str = format!("{frac:06}");
        let trimmed = frac_str.trim_end_matches('0');
        format!("{sign}{whole}.{trimmed}")
    }
}

/// 将微单位转换为 `f64` `μ` 展示值。
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn raw_to_mu_f64(raw: i128) -> f64 {
    (raw as f64) / (MERIT_SCALE as f64)
}

/// 从 JSON 数字或十进制字符串解析正数 `amount_mu`（返回 `(raw, clean_wire_string)`）。
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub fn parse_positive_mu_to_raw(val: &Value) -> Result<(i128, String), CliError> {
    if let Some(i) = val.as_i64() {
        if i <= 0 {
            return Err(CliError::Usage(
                "amount_mu must be strictly positive (> 0)".to_owned(),
            ));
        }
        let raw = i128::from(i).saturating_mul(MERIT_SCALE);
        return Ok((raw, i.to_string()));
    }
    if let Some(u) = val.as_u64() {
        if u == 0 {
            return Err(CliError::Usage(
                "amount_mu must be strictly positive (> 0)".to_owned(),
            ));
        }
        let raw = i128::from(u).saturating_mul(MERIT_SCALE);
        return Ok((raw, u.to_string()));
    }
    if let Some(f) = val.as_f64() {
        if !f.is_finite() || f <= 0.0 {
            return Err(CliError::Usage(
                "amount_mu must be a finite positive number (> 0)".to_owned(),
            ));
        }
        let scaled = (f * (MERIT_SCALE as f64)).round();
        if scaled < 1.0 || scaled > (i64::MAX as f64) {
            return Err(CliError::Usage(
                "amount_mu is out of valid range".to_owned(),
            ));
        }
        let raw = scaled as i128;
        return Ok((raw, format_merit_clean_decimal(raw)));
    }
    if let Some(s) = val.as_str() {
        return parse_decimal_mu_str(s);
    }
    Err(CliError::Usage(
        "missing or invalid 'amount_mu' (expected positive number or decimal string)".to_owned(),
    ))
}

fn parse_decimal_mu_str(input: &str) -> Result<(i128, String), CliError> {
    let trimmed = input.trim().trim_end_matches('μ').trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(CliError::Usage(format!(
            "amount_mu '{input}' must be strictly positive (> 0)"
        )));
    }
    let clean = trimmed.strip_prefix('+').unwrap_or(trimmed);
    let (whole_part, frac_part) = clean.split_once('.').unwrap_or((clean, ""));
    if whole_part.is_empty() && frac_part.is_empty() {
        return Err(CliError::Usage(format!("invalid amount_mu '{input}'")));
    }
    let whole: u64 = if whole_part.is_empty() {
        0
    } else {
        whole_part
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("invalid amount_mu '{input}'")))?
    };
    if frac_part.len() > 6 || !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(CliError::Usage(format!(
            "invalid amount_mu '{input}' (up to 6 decimal places supported)"
        )));
    }
    let mut padded = frac_part.to_owned();
    while padded.len() < 6 {
        padded.push('0');
    }
    let frac: u64 = if padded.is_empty() {
        0
    } else {
        padded
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("invalid amount_mu '{input}'")))?
    };
    let raw = i128::from(whole)
        .saturating_mul(MERIT_SCALE)
        .saturating_add(i128::from(frac));
    if raw <= 0 {
        return Err(CliError::Usage(
            "amount_mu must be strictly positive (> 0)".to_owned(),
        ));
    }
    Ok((raw, format_merit_clean_decimal(raw)))
}

/// 校验并规范化优先权类别。
pub fn validate_priority_kind(raw: &str) -> Result<String, CliError> {
    let clean = raw.trim();
    if VALID_PRIORITY_KINDS.contains(&clean) {
        Ok(clean.to_owned())
    } else {
        Err(CliError::Usage(format!(
            "invalid priority_kind '{clean}' (expected one of: {})",
            VALID_PRIORITY_KINDS.join(", ")
        )))
    }
}

/// 校验并规范化隐私分级档位。
pub fn validate_privacy_tier(raw: &str) -> Result<String, CliError> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "L1_PUBLIC" | "L1" => Ok("L1_PUBLIC".to_owned()),
        "L2_BAND" | "L2" => Ok("L2_BAND".to_owned()),
        "L3_ZK_BLINDED" | "L3" => Ok("L3_ZK_BLINDED".to_owned()),
        other => Err(CliError::Usage(format!(
            "invalid privacy_tier '{other}' (expected one of: {})",
            VALID_PRIVACY_TIERS.join(", ")
        ))),
    }
}

/// 计算本地回退模式下的确定性状态根 SHA-256。
#[must_use]
pub fn compute_local_state_root(
    fay_id: &str,
    cur_merit_raw: i128,
    min_merit_floor_raw: i128,
    block_height: u64,
) -> String {
    let preimage = format!(
        "gmchain-local-pocket/1.0:{fay_id}:{cur_merit_raw}:{min_merit_floor_raw}:{block_height}"
    );
    hex_sha256(preimage.as_bytes())
}

/// 构造 `gmchain-merit-credential/1.0` 分级美誉出示凭证（含 RFC 8785 JCS SHA-256 承诺）。
pub fn build_merit_credential(
    standing: &MeritStandingView,
    display_name: &str,
    privacy_tier: &str,
) -> Result<MeritCredentialPresentation, CliError> {
    let tier = validate_privacy_tier(privacy_tier)?;
    let now = unix_now_secs();
    let entropy = fresh_entropy_32();
    let credential_id = format!("mcred_{}", &encode_hex(&entropy)[..12]);

    let disclosure = match tier.as_str() {
        "L1_PUBLIC" => json!({
            "fay_id": standing.fay_id,
            "canonical_fayid": standing.canonical_fayid,
            "display_name": display_name,
            "cur_merit_raw": standing.cur_merit_raw,
            "cur_merit_mu": standing.cur_merit_formatted,
            "consumable_merit_raw": standing.consumable_merit_raw,
            "consumable_merit_mu": standing.consumable_merit_formatted,
            "min_merit_floor_raw": standing.min_merit_floor_raw,
            "min_merit_floor_mu": standing.min_merit_floor_formatted,
            "recent_ledger_count": standing.history.len(),
        }),
        "L2_BAND" => {
            let masked_id = mask_identifier(&standing.fay_id);
            let cur_band = classify_merit_band(standing.cur_merit_raw);
            let consumable_band = classify_consumable_band(standing.consumable_merit_raw);
            json!({
                "fay_id_masked": masked_id,
                "standing_band": cur_band,
                "consumable_priority_band": consumable_band,
                "constitutional_floor_attested": standing.min_merit_floor_raw >= MIN_MERIT_E_RAW,
                "exact_balance_redacted": true,
            })
        }
        _ => {
            let blind_nonce = encode_hex(&entropy);
            let blinded_handle = format!(
                "zkfay_{}",
                &hex_sha256(format!("handle:{}:{blind_nonce}", standing.fay_id).as_bytes())[..24]
            );
            let balance_commitment = hex_sha256(
                format!(
                    "pedersen-sim:{}:{}:{}:{blind_nonce}",
                    standing.fay_id, standing.cur_merit_raw, standing.min_merit_floor_raw
                )
                .as_bytes(),
            );
            json!({
                "blinded_subject_handle": blinded_handle,
                "balance_commitment_sha256": balance_commitment,
                "predicate": "curMerit >= minMerit >= e",
                "predicate_verified": standing.cur_merit_raw >= standing.min_merit_floor_raw
                    && standing.min_merit_floor_raw >= MIN_MERIT_E_RAW,
                "has_consumable_priority": standing.consumable_merit_raw > 0,
                "unlinkable": true,
            })
        }
    };

    let preimage = json!({
        "schema_version": MERIT_CREDENTIAL_SCHEMA_VERSION,
        "credential_id": credential_id,
        "privacy_tier": tier,
        "issued_at_unix": now,
        "non_monetary": true,
        "sync_mode": standing.sync_mode,
        "block_height": standing.last_block_height,
        "state_root_hex": standing.last_state_root_hex,
        "disclosure": disclosure,
    });
    let canonical = jcs_canonicalize(&preimage)?;
    let commitment_sha256 = hex_sha256(canonical.as_bytes());

    Ok(MeritCredentialPresentation {
        schema_version: MERIT_CREDENTIAL_SCHEMA_VERSION.to_owned(),
        credential_id,
        privacy_tier: tier,
        issued_at_unix: now,
        non_monetary: true,
        sync_mode: standing.sync_mode.clone(),
        block_height: standing.last_block_height,
        state_root_hex: standing.last_state_root_hex.clone(),
        disclosure,
        commitment_sha256,
    })
}

fn mask_identifier(id: &str) -> String {
    if id.len() <= 12 {
        return format!("{id}***");
    }
    format!("{}...{}", &id[..9], &id[id.len() - 4..])
}

const fn classify_merit_band(cur_raw: i128) -> &'static str {
    if cur_raw >= 100 * MERIT_SCALE {
        "S3_FELLOW (>= 100 μ)"
    } else if cur_raw >= 20 * MERIT_SCALE {
        "S2_STEWARD (20–100 μ)"
    } else if cur_raw >= 5 * MERIT_SCALE {
        "S1_CONTRIBUTOR (5–20 μ)"
    } else {
        "S0_SEED (< 5 μ)"
    }
}

const fn classify_consumable_band(consumable_raw: i128) -> &'static str {
    if consumable_raw >= 25 * MERIT_SCALE {
        "HIGH_ACTIVE_POOL (>= 25 μ)"
    } else if consumable_raw >= 5 * MERIT_SCALE {
        "MEDIUM_ACTIVE_POOL (5–25 μ)"
    } else if consumable_raw > 0 {
        "LIGHT_ACTIVE_POOL (0–5 μ)"
    } else {
        "FLOOR_LOCKED (0 μ)"
    }
}

/// 回环 JSON-RPC 2.0 传输调用结果。
#[derive(Debug, Clone)]
pub enum RpcCallOutcome {
    /// 成功收到 `gmc-node` 返回的 `result` 字段。
    Connected(Value),
    /// 本机回环端点未启动或不可达（自动触发本地确定性回退）。
    Offline(String),
    /// `gmc-node` 在线但显式返回了业务层 JSON-RPC `error` 拒绝。
    RpcRejected {
        /// JSON-RPC 错误码。
        code: i64,
        /// 错误消息。
        message: String,
    },
}

/// `gmc_registerExternalFay` 解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcRegisteredFay {
    /// 链上规范化 `canonical_fayid`。
    pub canonical_fayid: Option<String>,
    /// 当前美誉值微单位。
    pub cur_merit_raw: i128,
    /// 可消费美誉值微单位。
    pub consumable_merit_raw: i128,
    /// 历史底值微单位。
    pub min_merit_floor_raw: i128,
    /// 区块高度。
    pub block_height: u64,
    /// 状态根十六进制。
    pub state_root_hex: String,
}

/// `gmc_recordContribution` 解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcRecordedContribution {
    /// 贡献批次编号。
    pub batch_id: String,
    /// 更新后当前美誉值微单位。
    pub cur_merit_raw: i128,
    /// 更新后可消费美誉值微单位。
    pub consumable_merit_raw: i128,
    /// 更新后历史底值微单位。
    pub min_merit_floor_raw: i128,
    /// 区块高度。
    pub block_height: u64,
    /// 状态根十六进制。
    pub state_root_hex: String,
}

/// `gmc_consumePriority` 解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcConsumedPriority {
    /// 优先权消费单号。
    pub consumption_id: String,
    /// 本次消减微单位。
    pub consumed_mu_raw: i128,
    /// 消减前美誉值微单位。
    pub before_cur_merit_raw: i128,
    /// 消减后美誉值微单位。
    pub after_cur_merit_raw: i128,
    /// 消减后可消费美誉值微单位。
    pub consumable_merit_raw: i128,
    /// 消减后底值微单位。
    pub min_merit_floor_raw: i128,
    /// 区块高度。
    pub block_height: u64,
    /// 状态根十六进制。
    pub state_root_hex: String,
}

/// 调用 `gmc_registerExternalFay` 同步或注册外部 `Fay`。
pub fn rpc_register_external_fay(
    base_endpoint: &str,
    fay_id: &str,
    display_name: &str,
    kind: &str,
) -> Result<Option<RpcRegisteredFay>, CliError> {
    let params = json!({
        "fay_id": fay_id,
        "display_name": display_name,
        "kind": kind,
        "role_label": "FayGer Digital Soul",
    });
    match call_gmchain_json_rpc(base_endpoint, "gmc_registerExternalFay", &params) {
        RpcCallOutcome::Offline(_) => Ok(None),
        RpcCallOutcome::RpcRejected { code, message } => Err(CliError::Usage(format!(
            "GMChain RPC gmc_registerExternalFay error ({code}): {message}"
        ))),
        RpcCallOutcome::Connected(res) => {
            let cur = extract_i128(&res, "cur_merit_raw").unwrap_or(INITIAL_CUR_MERIT_RAW);
            let floor = extract_i128(&res, "min_merit_floor_raw")
                .unwrap_or(MIN_MERIT_E_RAW)
                .max(MIN_MERIT_E_RAW);
            let consumable =
                extract_i128(&res, "consumable_merit_raw").unwrap_or_else(|| (cur - floor).max(0));
            let height = res.get("block_height").and_then(Value::as_u64).unwrap_or(0);
            let state_root_hex = res
                .get("state_root_hex")
                .and_then(Value::as_str)
                .map_or_else(
                    || compute_local_state_root(fay_id, cur, floor, height),
                    str::to_owned,
                );
            let canonical_fayid = res
                .get("canonical_fayid")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Ok(Some(RpcRegisteredFay {
                canonical_fayid,
                cur_merit_raw: cur,
                consumable_merit_raw: consumable,
                min_merit_floor_raw: floor,
                block_height: height,
                state_root_hex,
            }))
        }
    }
}

/// 调用 `gmc_recordContribution` 在公誉链铸造记录贡献批次。
pub fn rpc_record_contribution(
    base_endpoint: &str,
    fay_id: &str,
    display_name: &str,
    kind: &str,
    input: &MeritAddInput,
) -> Result<Option<RpcRecordedContribution>, CliError> {
    let params = json!({
        "fay_id": fay_id,
        "display_name": display_name,
        "kind": kind,
        "title": input.title,
        "domain_chain_id": input.domain_chain_id,
        "amount_mu": input.amount_mu,
        "horizon_days": input.horizon_days,
        "summary": input.summary,
    });
    match call_gmchain_json_rpc(base_endpoint, "gmc_recordContribution", &params) {
        RpcCallOutcome::Offline(_) => Ok(None),
        RpcCallOutcome::RpcRejected { code, message } => Err(CliError::Usage(format!(
            "GMChain RPC gmc_recordContribution rejected ({code}): {message}"
        ))),
        RpcCallOutcome::Connected(res) => {
            let batch_id = res.get("batch_id").and_then(Value::as_str).map_or_else(
                || format!("batch_{}", &encode_hex(&fresh_entropy_32())[..12]),
                str::to_owned,
            );
            let cur = extract_i128(&res, "cur_merit_raw")
                .ok_or_else(|| CliError::Usage("GMChain RPC missing 'cur_merit_raw'".to_owned()))?;
            let floor = extract_i128(&res, "min_merit_floor_raw")
                .unwrap_or(MIN_MERIT_E_RAW)
                .max(MIN_MERIT_E_RAW);
            let consumable =
                extract_i128(&res, "consumable_merit_raw").unwrap_or_else(|| (cur - floor).max(0));
            let height = res.get("block_height").and_then(Value::as_u64).unwrap_or(0);
            let state_root_hex = res
                .get("state_root_hex")
                .and_then(Value::as_str)
                .map_or_else(
                    || compute_local_state_root(fay_id, cur, floor, height),
                    str::to_owned,
                );
            Ok(Some(RpcRecordedContribution {
                batch_id,
                cur_merit_raw: cur,
                consumable_merit_raw: consumable,
                min_merit_floor_raw: floor,
                block_height: height,
                state_root_hex,
            }))
        }
    }
}

/// 调用 `gmc_consumePriority` 在公誉链消减活跃美誉值并行使优先权。
pub fn rpc_consume_priority(
    base_endpoint: &str,
    fay_id: &str,
    input: &MeritConsumeInput,
) -> Result<Option<RpcConsumedPriority>, CliError> {
    let params = json!({
        "fay_id": fay_id,
        "priority_kind": input.priority_kind,
        "amount_mu": input.amount_mu_wire,
        "reason": input.reason,
        "proposal_id": input.proposal_id,
        "approve": true,
    });
    match call_gmchain_json_rpc(base_endpoint, "gmc_consumePriority", &params) {
        RpcCallOutcome::Offline(_) => Ok(None),
        RpcCallOutcome::RpcRejected { code, message } => Err(CliError::Usage(format!(
            "GMChain RPC gmc_consumePriority rejected ({code}): {message}"
        ))),
        RpcCallOutcome::Connected(res) => {
            let consumption_id = res
                .get("consumption_id")
                .and_then(Value::as_str)
                .map_or_else(
                    || format!("csm_{}", &encode_hex(&fresh_entropy_32())[..12]),
                    str::to_owned,
                );
            let consumed_mu_raw = extract_i128(&res, "consumed_mu_raw").unwrap_or(input.amount_raw);
            let after_cur = extract_i128(&res, "after_cur_merit_raw")
                .or_else(|| extract_i128(&res, "cur_merit_raw"))
                .ok_or_else(|| {
                    CliError::Usage("GMChain RPC missing 'after_cur_merit_raw'".to_owned())
                })?;
            let before_cur = extract_i128(&res, "before_cur_merit_raw")
                .unwrap_or_else(|| after_cur.saturating_add(consumed_mu_raw));
            let floor = extract_i128(&res, "min_merit_floor_raw")
                .unwrap_or(MIN_MERIT_E_RAW)
                .max(MIN_MERIT_E_RAW);
            let consumable = extract_i128(&res, "consumable_merit_raw")
                .unwrap_or_else(|| (after_cur - floor).max(0));
            let height = res.get("block_height").and_then(Value::as_u64).unwrap_or(0);
            let state_root_hex = res
                .get("state_root_hex")
                .and_then(Value::as_str)
                .map_or_else(
                    || compute_local_state_root(fay_id, after_cur, floor, height),
                    str::to_owned,
                );
            Ok(Some(RpcConsumedPriority {
                consumption_id,
                consumed_mu_raw,
                before_cur_merit_raw: before_cur,
                after_cur_merit_raw: after_cur,
                consumable_merit_raw: consumable,
                min_merit_floor_raw: floor,
                block_height: height,
                state_root_hex,
            }))
        }
    }
}

/// 快速探测回环 `GMChain` 端点是否在线可达。
#[must_use]
pub fn probe_gmchain_online(base_endpoint: &str) -> bool {
    let Ok(normalized) = normalize_loopback_gmchain_endpoint(base_endpoint) else {
        return false;
    };
    let Some(host_port) = normalized.strip_prefix("http://") else {
        return false;
    };
    let Ok(mut addrs) = host_port.to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| TcpStream::connect_timeout(&addr, RPC_SOCKET_TIMEOUT).is_ok())
}

/// 基于标准库阻塞 `std::net::TcpStream` 调用 `POST <endpoint>/rpc` JSON-RPC 2.0。
pub fn call_gmchain_json_rpc(base_endpoint: &str, method: &str, params: &Value) -> RpcCallOutcome {
    let Ok(normalized) = normalize_loopback_gmchain_endpoint(base_endpoint) else {
        return RpcCallOutcome::Offline("invalid loopback endpoint".to_owned());
    };
    let Some(host_port) = normalized.strip_prefix("http://") else {
        return RpcCallOutcome::Offline("missing http scheme".to_owned());
    };
    let Ok(addrs_iter) = host_port.to_socket_addrs() else {
        return RpcCallOutcome::Offline("failed to resolve loopback address".to_owned());
    };
    let addrs: Vec<SocketAddr> = addrs_iter.collect();
    let Some(addr) = addrs.first() else {
        return RpcCallOutcome::Offline("no socket address resolved".to_owned());
    };

    let Ok(mut stream) = TcpStream::connect_timeout(addr, RPC_SOCKET_TIMEOUT) else {
        return RpcCallOutcome::Offline(format!("connection refused on {addr}"));
    };
    let _ = stream.set_read_timeout(Some(RPC_SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(RPC_SOCKET_TIMEOUT));

    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let Ok(body_bytes) = serde_json::to_vec(&payload) else {
        return RpcCallOutcome::Offline("failed to serialize JSON-RPC body".to_owned());
    };

    let head = format!(
        "POST /rpc HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    if stream.write_all(head.as_bytes()).is_err() || stream.write_all(&body_bytes).is_err() {
        return RpcCallOutcome::Offline("socket write failed".to_owned());
    }
    let _ = stream.flush();

    let Some((status_code, resp_body)) = read_http_response_bytes(&stream) else {
        return RpcCallOutcome::Offline("invalid HTTP response from GMChain RPC".to_owned());
    };
    let Ok(rpc_val) = serde_json::from_slice::<Value>(&resp_body) else {
        return RpcCallOutcome::Offline(format!(
            "non-JSON response from GMChain RPC (HTTP {status_code})"
        ));
    };

    if let Some(err_obj) = rpc_val.get("error").filter(|v| !v.is_null()) {
        let code = err_obj
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or(-32000);
        let message = err_obj
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("GMChain JSON-RPC error")
            .to_owned();
        return RpcCallOutcome::RpcRejected { code, message };
    }
    if status_code >= 400 {
        let msg = rpc_val
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("HTTP error from GMChain RPC")
            .to_owned();
        return RpcCallOutcome::RpcRejected {
            code: i64::from(status_code),
            message: msg,
        };
    }
    if let Some(result_val) = rpc_val.get("result") {
        return RpcCallOutcome::Connected(result_val.clone());
    }
    RpcCallOutcome::Offline("JSON-RPC response missing both result and error".to_owned())
}

fn read_http_response_bytes(stream: &TcpStream) -> Option<(u16, Vec<u8>)> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).ok()? == 0 {
        return None;
    }
    let status_code = status_line.split_whitespace().nth(1)?.parse::<u16>().ok()?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse::<usize>().ok();
            } else if k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
            {
                chunked = true;
            }
        }
    }

    if let Some(len) = content_length {
        if len > 16 * 1024 * 1024 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if len > 0 {
            reader.read_exact(&mut buf).ok()?;
        }
        return Some((status_code, buf));
    }

    if chunked {
        let mut out = Vec::new();
        loop {
            let mut size_line = String::new();
            if reader.read_line(&mut size_line).ok()? == 0 {
                break;
            }
            let hex_size = size_line.trim().split(';').next().unwrap_or("0");
            let chunk_len = usize::from_str_radix(hex_size, 16).ok()?;
            if chunk_len == 0 {
                break;
            }
            if out.len().saturating_add(chunk_len) > 16 * 1024 * 1024 {
                return None;
            }
            let mut chunk = vec![0u8; chunk_len];
            reader.read_exact(&mut chunk).ok()?;
            out.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            let _ = reader.read_exact(&mut crlf);
        }
        return Some((status_code, out));
    }

    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).ok()?;
    Some((status_code, rest))
}

fn extract_i128(obj: &Value, key: &str) -> Option<i128> {
    let field = obj.get(key)?;
    if let Some(i) = field.as_i64() {
        return Some(i128::from(i));
    }
    if let Some(u) = field.as_u64() {
        return Some(i128::from(u));
    }
    field.as_str()?.trim().parse::<i128>().ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        build_merit_credential, format_merit_2dp, format_merit_clean_decimal,
        normalize_loopback_gmchain_endpoint, parse_positive_mu_to_raw, validate_priority_kind,
        MeriTokenPocketState, DEFAULT_GMCHAIN_ENDPOINT, INITIAL_CONSUMABLE_MERIT_RAW,
        INITIAL_CUR_MERIT_RAW, MERIT_CREDENTIAL_SCHEMA_VERSION, MIN_MERIT_E_RAW,
    };
    use serde_json::json;

    #[test]
    fn loopback_endpoint_validation_enforces_local_only() {
        assert_eq!(
            normalize_loopback_gmchain_endpoint("http://127.0.0.1:8546").expect("valid"),
            "http://127.0.0.1:8546"
        );
        assert_eq!(
            normalize_loopback_gmchain_endpoint("http://localhost:9546/rpc/").expect("valid"),
            "http://localhost:9546"
        );
        assert!(normalize_loopback_gmchain_endpoint("http://192.168.1.20:8546").is_err());
        assert!(normalize_loopback_gmchain_endpoint("https://gmchain.org/rpc").is_err());
    }

    #[test]
    fn fixed_point_formatting_and_parsing_are_exact() {
        let pocket = MeriTokenPocketState::initial_for("ifay_demo");
        assert_eq!(pocket.cur_merit_raw, INITIAL_CUR_MERIT_RAW);
        assert_eq!(pocket.min_merit_floor_raw, MIN_MERIT_E_RAW);
        assert_eq!(pocket.consumable_merit_raw, INITIAL_CONSUMABLE_MERIT_RAW);
        assert_eq!(format_merit_2dp(pocket.cur_merit_raw), "5.00");
        assert_eq!(format_merit_2dp(pocket.min_merit_floor_raw), "2.72");
        assert_eq!(format_merit_2dp(pocket.consumable_merit_raw), "2.28");
        assert_eq!(format_merit_clean_decimal(12_000_000), "12");
        assert_eq!(format_merit_clean_decimal(12_500_000), "12.5");

        assert_eq!(
            parse_positive_mu_to_raw(&json!(12)).expect("int"),
            (12_000_000, "12".to_owned())
        );
        assert_eq!(
            parse_positive_mu_to_raw(&json!("12.25")).expect("str"),
            (12_250_000, "12.25".to_owned())
        );
        assert!(parse_positive_mu_to_raw(&json!(0)).is_err());
        assert!(parse_positive_mu_to_raw(&json!(-5)).is_err());
        assert!(validate_priority_kind("voting_priority").is_ok());
        assert!(validate_priority_kind("buy_tokens").is_err());
    }

    #[test]
    fn privacy_credentials_l1_l2_l3_commit_with_jcs_sha256() {
        let pocket = MeriTokenPocketState::initial_for("ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f");
        let standing =
            pocket.to_standing_view("ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f", DEFAULT_GMCHAIN_ENDPOINT);

        let l1 = build_merit_credential(&standing, "Alice", "L1_PUBLIC").expect("l1");
        assert_eq!(l1.schema_version, MERIT_CREDENTIAL_SCHEMA_VERSION);
        assert_eq!(l1.privacy_tier, "L1_PUBLIC");
        assert_eq!(l1.commitment_sha256.len(), 64);
        assert_eq!(l1.disclosure["cur_merit_raw"], 5_000_000);

        let l2 = build_merit_credential(&standing, "Alice", "L2_BAND").expect("l2");
        assert_eq!(l2.privacy_tier, "L2_BAND");
        assert_eq!(l2.disclosure["exact_balance_redacted"], true);
        assert!(l2.disclosure.get("cur_merit_raw").is_none());

        let l3 = build_merit_credential(&standing, "Alice", "L3_ZK_BLINDED").expect("l3");
        assert_eq!(l3.privacy_tier, "L3_ZK_BLINDED");
        assert_eq!(l3.disclosure["unlinkable"], true);
        let l3_json = serde_json::to_string(&l3).expect("json");
        assert!(!l3_json.contains("ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f"));
    }
}
