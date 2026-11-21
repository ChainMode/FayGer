//! `FayGer` 多租户守护引擎核心状态机（`FayGerEngine`）。
//!
//! 统一编排 Batch 1 主干闭环的全部运行期不变量：
//! 1. **两层授权链实时交集**：`Effective = BuF.requested ∩ HostPool.granted ∩ PerFayPolicy.allowed`；
//! 2. **显式 `Faying` 上岗状态机与 `G0` 红色急刹**：所有加载的 `iFay` / `coFay` 默认处于 `ROGUE`
//!    停机态（严禁一切外部宿主调用），仅在 `Human Prime` 拨动上岗开关后进入限时带心跳的 `FAYING` 态；
//!    点击 `G0` 急刹或心跳超时立即退回 `ROGUE` 并作废全部挂起审批；
//! 3. **内置 `CAP` 宿主驱动守门员**：常规沙箱读写在门控通过后真实落盘并写入 SHA-256 哈希链审计；
//!    高风险操作（`process.exec`、`media.camera`、高敏写入）生成 `ICP` `.ctx` `CAP Approval Card` 挂起待批；
//! 4. **本地 `TP`（`Telepathy`）双 Fay 共享认知空间**：两个已挂载 `Fay` 在授权会话内直接共享结构化记忆、
//!    视图状态与任务上下文，全程防篡改哈希链留痕。

use crate::commands::default_policy;
use crate::device_profile::{
    detect_device_runtime_profile, negotiate_embodiment_slices, DeviceRuntimeProfile,
    EmbodimentPlan, HostResourcePoolConfig,
};
use crate::ego_package::{encode_hex, hex_sha256, jcs_canonicalize};
use crate::error::CliError;
use crate::host_profile::detect_host_environment;
use crate::icp_ego::{
    message_mentions_merit, render_ctx_terminal, run_ego_three_in_one, CtxAnnotation, CtxEnvelope,
    CtxKeyValue, ICP_CTX_SCHEMA_VERSION,
};
use crate::live_ifay::{
    verify_audit_chain, AuditChainBuilder, HashChainedAuditEntry, GENESIS_PREV_HASH,
};
use crate::merit::{
    build_merit_credential, compute_local_state_root, default_gmchain_endpoint_string,
    format_merit_2dp, normalize_loopback_gmchain_endpoint, probe_gmchain_online,
    rpc_consume_priority, rpc_record_contribution, rpc_register_external_fay,
    validate_priority_kind, MeriTokenPocketState, MeritAddInput, MeritAddReceipt, MeritConfigView,
    MeritConsumeInput, MeritConsumeReceipt, MeritCredentialPresentation, MeritLedgerEntry,
    MeritStandingView, DEFAULT_GMCHAIN_ENDPOINT, MERIT_SCALE, MIN_MERIT_E_RAW, SYNC_MODE_LIVE_RPC,
    SYNC_MODE_LOCAL_FALLBACK,
};
use crate::soul::{create_soul_buf_bytes, extract_soul_from_buf, SoulKind, SoulPackageMetadata};
use crate::wallet::{
    fresh_entropy_32, local_attestation_human_token, show_wallet, unix_now_secs, WalletPublicView,
};
use fayger_adapter::{
    execute_controlled_syscall, AdapterRegistry, BrowserAdapter, HostPolicy, InAppAdapter,
    NativeDesktopAdapter, RealSyscallPolicy, RealSyscallRequest, ServerAdapter,
};
use fayger_error::{ErrorCode, FaygerError, Layer};
use fayger_loader::load;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// 引擎持久化快照 schema 版本。
pub const ENGINE_STATE_SCHEMA_VERSION: &str = "fayger-engine-state/1.0";

/// `Fay` 实例上岗监护生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FayingState {
    /// 脱缰/停机隔离态（默认初始态或急刹后状态，严格拦截一切 D 维宿主外部动作）。
    Rogue,
    /// 已获 `Human Prime` 显式上岗授权并保持有效心跳的受监护运行态。
    Faying,
}

impl FayingState {
    /// 稳定大写字符串标签。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rogue => "ROGUE",
            Self::Faying => "FAYING",
        }
    }
}

/// 激活 `Faying` 上岗态时签发的限时委托与心跳记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveFayingMandate {
    /// 委托单标识（`mnd_<12-hex>`）。
    pub mandate_id: String,
    /// 会话授权等级（如 `"G2"` / `"G1"`）。
    pub grade: String,
    /// 签发时间（Unix 秒）。
    pub issued_at_unix: u64,
    /// 委托到期时间（Unix 秒）。
    pub expires_at_unix: u64,
    /// 最近一次有效心跳时间（Unix 秒）。
    pub last_heartbeat_unix: u64,
    /// 心跳超时阈值（秒）；超时未续期自动退回 `ROGUE`。
    pub heartbeat_timeout_secs: u64,
}

/// 对话历史中的单轮记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTurnRecord {
    /// 用户输入消息或指令描述。
    pub user_input: String,
    /// `Fay` 返回的 `ICP` `.ctx` 结构化响应信封。
    pub response_ctx: CtxEnvelope,
    /// 终端自适应纯文本降级渲染结果。
    pub terminal_text: String,
}

/// 引擎内部持久化的单个已加载 `Fay` 实例记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFayInstance {
    /// 实例在当前 `FayGer` 内的槽位编号。
    pub instance_id: String,
    /// `.buf` 文件保存路径。
    pub buf_path: String,
    /// 提取自 `.buf` 的灵魂包元数据。
    pub metadata: SoulPackageMetadata,
    /// `BuF` 清单声明请求的能力集合。
    pub requested_capabilities: BTreeSet<String>,
    /// `FayGer` 管理员为该实例配置的策略允许能力集合（两层授权链第 ② 层）。
    pub policy_capabilities: BTreeSet<String>,
    /// 当前上岗状态（初始必为 `ROGUE`）。
    pub faying_state: FayingState,
    /// 当前生效的 `Faying` 委托（`ROGUE` 态下为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_mandate: Option<ActiveFayingMandate>,
    /// 最近一次退回 `ROGUE` 的原因说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rogue_reason: Option<String>,
    /// 终端切片附体挂载协商方案。
    pub embodiment: EmbodimentPlan,
    /// 待回流同步回对等主体（Primary Replicas）的增量条数。
    pub pending_sync_deltas: u64,
    /// 专属隔离受控沙箱目录。
    pub sandbox_dir: String,
    /// `ICP` `.ctx` 历史对话列表。
    pub chat_history: Vec<ChatTurnRecord>,
    /// 加载时间（Unix 秒）。
    pub loaded_at_unix: u64,
    /// `MeriToken`（`美誉值`, `μ`）账户与优先权流水口袋（向后兼容旧快照自动初始化）。
    #[serde(default)]
    pub merit_pocket: MeriTokenPocketState,
}

/// 对外（REST API 与 Web 控制台）呈现的单个 `Fay` 实例全貌（含实时交集计算结果）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FayInstanceView {
    /// 实例槽位编号。
    pub instance_id: String,
    /// 规范化 `fay_id`（`ifay_...` 或 `cofay_...`）。
    pub fay_id: String,
    /// 显示名称。
    pub display_name: String,
    /// 类别（`ifay` / `cofay`）。
    pub kind: SoulKind,
    /// `.buf` 路径。
    pub buf_path: String,
    /// `BuF` 声明请求的能力列表。
    pub requested_capabilities: Vec<String>,
    /// `FayGer` 管理员分配给该实例的策略能力列表。
    pub policy_capabilities: Vec<String>,
    /// 实时交集：`BuF.requested ∩ HostPool.granted ∩ PerFayPolicy.allowed`！
    pub effective_capabilities: Vec<String>,
    /// 当前上岗状态（`ROGUE` 或 `FAYING`）。
    pub faying_state: FayingState,
    /// 当前生效委托详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_mandate: Option<ActiveFayingMandate>,
    /// 最近退回 `ROGUE` 的原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rogue_reason: Option<String>,
    /// 终端切片附体方案。
    pub embodiment: EmbodimentPlan,
    /// 待回流主体的增量数。
    pub pending_sync_deltas: u64,
    /// 受控沙箱路径。
    pub sandbox_dir: String,
    /// 灵魂元数据（含 `iFay` 六维画像或 `coFay` `RoleCharter`）。
    pub metadata: SoulPackageMetadata,
    /// 最近对话轮次。
    pub chat_history: Vec<ChatTurnRecord>,
    /// 实时 `MeriToken`（美誉值 `μ`）与可消费优先权状态全貌。
    #[serde(default)]
    pub merit: MeritStandingView,
}

/// 请求 `CAP` 守门员评估并执行的宿主驱动动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RequestedHostAction {
    /// 读取受控沙箱内文件（`fs.read`，常规风险）。
    FsRead {
        /// 相对于实例受控沙箱根目录的路径。
        path: String,
    },
    /// 写入受控沙箱内文件（`fs.write`，普通笔记为常规/敏感，配置文件或显式标记为高风险）。
    FsWrite {
        /// 相对于实例受控沙箱根目录的路径。
        path: String,
        /// 待写入 UTF-8 文本。
        content: String,
        /// 是否显式标记为高风险需人工确认。
        #[serde(default)]
        high_risk: bool,
    },
    /// 在受控沙箱内执行白名单工作区检视命令（`process.exec`，固定高风险需 `Human Prime` 审批）。
    ProcessExec {
        /// 受控命令标识（如 `workspace.ls`、`workspace.digest`、`workspace.inspect`）。
        command: String,
    },
    /// 采集受控摄像头快照（`media.camera`，固定高风险需 `Human Prime` 审批）。
    CameraSnapshot {
        /// 快照场景标签。
        label: String,
    },
}

impl RequestedHostAction {
    /// 该动作所需的宿主能力标识。
    #[must_use]
    pub const fn required_capability(&self) -> &'static str {
        match self {
            Self::FsRead { .. } => "fs.read",
            Self::FsWrite { .. } => "fs.write",
            Self::ProcessExec { .. } => "process.exec",
            Self::CameraSnapshot { .. } => "media.camera",
        }
    }

    /// 该动作的风险等级是否为高风险（需挂起等待 `Human Prime` 显式点击 Allow）。
    #[must_use]
    pub fn requires_human_confirmation(&self) -> bool {
        match self {
            Self::FsRead { .. } => false,
            Self::FsWrite {
                path, high_risk, ..
            } => {
                let ext = Path::new(path).extension();
                *high_risk
                    || ext.is_some_and(|e| {
                        e.eq_ignore_ascii_case("sh")
                            || e.eq_ignore_ascii_case("toml")
                            || e.eq_ignore_ascii_case("env")
                    })
                    || path.starts_with("config/")
            }
            Self::ProcessExec { .. } | Self::CameraSnapshot { .. } => true,
        }
    }

    /// 操作目标简短摘要。
    #[must_use]
    pub fn target_summary(&self) -> String {
        match self {
            Self::FsRead { path } => format!("sandbox://{path}"),
            Self::FsWrite { path, content, .. } => {
                format!("sandbox://{path} ({} bytes)", content.len())
            }
            Self::ProcessExec { command } => format!("exec://{command}"),
            Self::CameraSnapshot { label } => format!("camera://{label}"),
        }
    }
}

/// 高风险 `CAP` 待审批单记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapApprovalItem {
    /// 审批单唯一编号（`apr_<12-hex>`）。
    pub approval_id: String,
    /// 发起请求的 `fay_id`。
    pub fay_id: String,
    /// 待执行宿主动作。
    pub action: RequestedHostAction,
    /// 所需能力标识。
    pub required_capability: String,
    /// 风险等级（`"high"`）。
    pub criticality: String,
    /// 参数 JCS SHA-256 绑定摘要（防篡改）。
    pub params_sha256: String,
    /// 状态：`"pending"` / `"approved"` / `"denied"` / `"revoked_g0"`。
    pub status: String,
    /// 创建时间（Unix 秒）。
    pub created_at_unix: u64,
    /// 裁决时间（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at_unix: Option<u64>,
    /// 执行输出摘要（审批通过后填入）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_summary: Option<String>,
}

/// `Telepathy` (`TP`) 共享认知空间内的单条结构化认知对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TpCognitiveEntry {
    /// 条目编号（`tpe_<10-hex>`）。
    pub entry_id: String,
    /// 会话内递增版本号。
    pub revision: u64,
    /// 写入方 `fay_id`。
    pub author_fay_id: String,
    /// 认知类别：`memory_fact` / `view_state` / `task_context` / `decision_artifact`。
    pub category: String,
    /// 语义键名。
    pub key: String,
    /// 无损结构化 JSON 负载（消除提示词传话失真）。
    pub structured_value: Value,
    /// 结构化负载 JCS SHA-256 摘要。
    pub value_sha256: String,
    /// 写入时间（Unix 秒）。
    pub created_at_unix: u64,
}

/// 本地双 `Fay` `Telepathy` (`TP`) 共享认知会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TpSharedSession {
    /// 会话唯一编号（`tp_<12-hex>`）。
    pub session_id: String,
    /// 发起方 `fay_id`。
    pub initiator_fay_id: String,
    /// 协作对端 `fay_id`。
    pub peer_fay_id: String,
    /// 协作主题。
    pub topic: String,
    /// 双方开放的认知与操作权限范围（如 `["read_memory", "write_shared_context", "propose_task"]`）。
    pub permissions: Vec<String>,
    /// 状态：`"active"` 或 `"closed"`。
    pub status: String,
    /// 当前修订号。
    pub revision: u64,
    /// 共享认知条目列表。
    pub shared_entries: Vec<TpCognitiveEntry>,
    /// 创建时间（Unix 秒）。
    pub created_at_unix: u64,
}

/// Web 控制台首屏与轮询聚合快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    /// `FayGer` 引擎版本。
    pub fayger_version: String,
    /// 当前设备运行时环境描述文件。
    pub device_profile: DeviceRuntimeProfile,
    /// 当前宿主资源池配置。
    pub host_pool: HostResourcePoolConfig,
    /// `FayID` 保险箱脱敏公开视图。
    pub wallet: WalletPublicView,
    /// 全部已挂载 `Fay` 实例实时视图。
    pub fays: Vec<FayInstanceView>,
    /// `CAP` 审批单列表（最新在前）。
    pub cap_approvals: Vec<CapApprovalItem>,
    /// `TP` 心灵感应共享会话列表。
    pub tp_sessions: Vec<TpSharedSession>,
    /// 防篡改 SHA-256 审计链全量校验结果。
    pub audit_chain_verified: bool,
    /// 当前审计链头哈希。
    pub chain_head_hash: String,
    /// 最近审计分录列表。
    pub audit_entries: Vec<HashChainedAuditEntry>,
    /// 当前本机回环 `GMChain` / `MeriToken` 协同配置状态。
    pub merit_config: MeritConfigView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEngineData {
    schema_version: String,
    next_instance_seq: u64,
    #[serde(default = "default_gmchain_endpoint_string")]
    gmchain_endpoint: String,
    fays: BTreeMap<String, StoredFayInstance>,
    cap_approvals: BTreeMap<String, CapApprovalItem>,
    tp_sessions: BTreeMap<String, TpSharedSession>,
    audit_entries: Vec<HashChainedAuditEntry>,
}

impl Default for PersistedEngineData {
    fn default() -> Self {
        Self {
            schema_version: ENGINE_STATE_SCHEMA_VERSION.to_owned(),
            next_instance_seq: 1,
            gmchain_endpoint: DEFAULT_GMCHAIN_ENDPOINT.to_owned(),
            fays: BTreeMap::new(),
            cap_approvals: BTreeMap::new(),
            tp_sessions: BTreeMap::new(),
            audit_entries: Vec::new(),
        }
    }
}

/// `FayGer` 多租户守护引擎核心。
#[derive(Debug)]
pub struct FayGerEngine {
    state_dir: PathBuf,
    host_pool: HostResourcePoolConfig,
    data: PersistedEngineData,
}

impl FayGerEngine {
    /// 从 `state_dir` 打开或初始化 `FayGerEngine`。
    pub fn open(state_dir: &Path) -> Result<Self, CliError> {
        fs::create_dir_all(state_dir.join("souls")).map_err(|err| CliError::Io {
            path: state_dir.display().to_string(),
            detail: err.to_string(),
        })?;
        fs::create_dir_all(state_dir.join("sandboxes")).map_err(|err| CliError::Io {
            path: state_dir.display().to_string(),
            detail: err.to_string(),
        })?;

        let host_pool = HostResourcePoolConfig::load_or_default(state_dir)?;
        host_pool.save(state_dir)?;

        let state_file = state_dir.join("engine_state.json");
        let data = if state_file.exists() {
            let raw = fs::read_to_string(&state_file).map_err(|err| CliError::Io {
                path: state_file.display().to_string(),
                detail: err.to_string(),
            })?;
            serde_json::from_str(&raw).map_err(|err| CliError::Io {
                path: state_file.display().to_string(),
                detail: format!("invalid engine_state.json: {err}"),
            })?
        } else {
            PersistedEngineData::default()
        };

        let mut engine = Self {
            state_dir: state_dir.to_path_buf(),
            host_pool,
            data,
        };
        if engine.data.audit_entries.is_empty() {
            engine.append_audit(
                "engine_genesis",
                json!({
                    "state_dir": state_dir.display().to_string(),
                    "host_pool_capabilities": engine.host_pool.granted_capabilities,
                }),
            )?;
            engine.save()?;
        }
        Ok(engine)
    }

    /// 返回引擎状态目录。
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// 返回当前宿主资源池配置。
    #[must_use]
    pub const fn host_pool(&self) -> &HostResourcePoolConfig {
        &self.host_pool
    }

    /// 获取当前设备运行时环境描述文件（`fayger-device-profile/1.0`）。
    #[must_use]
    pub fn device_profile(&self, probe_ego_network: bool) -> DeviceRuntimeProfile {
        detect_device_runtime_profile(&self.host_pool, probe_ego_network)
    }

    /// 更新设备所有人授予 `FayGer` 的宿主资源池（两层授权链第 ① 层）。
    pub fn update_host_pool(
        &mut self,
        granted_capabilities: Option<BTreeSet<String>>,
        ego_endpoint: Option<String>,
        external_llm_endpoint: Option<Option<String>>,
        network_online: Option<bool>,
    ) -> Result<HostResourcePoolConfig, CliError> {
        if let Some(caps) = granted_capabilities {
            self.host_pool.granted_capabilities = caps;
        }
        if let Some(ep) = ego_endpoint {
            let trimmed = ep.trim();
            if !trimmed.is_empty() {
                trimmed.clone_into(&mut self.host_pool.ego_endpoint);
            }
        }
        if let Some(ext) = external_llm_endpoint {
            self.host_pool.external_llm_endpoint = ext.filter(|s| !s.trim().is_empty());
        }
        if let Some(online) = network_online {
            self.host_pool.network_online = online;
        }
        self.host_pool.save(&self.state_dir)?;
        self.append_audit(
            "host_pool_updated",
            json!({
                "granted_capabilities": self.host_pool.granted_capabilities,
                "ego_endpoint": self.host_pool.ego_endpoint,
                "network_online": self.host_pool.network_online,
            }),
        )?;
        self.save()?;
        Ok(self.host_pool.clone())
    }

    /// 一键创建并挂载新的 `iFay` 或 `coFay` `.buf` 实例（初始状态固定为 `ROGUE`）。
    pub fn create_and_load_soul(
        &mut self,
        kind: SoulKind,
        name: &str,
        requested_caps: Option<&[String]>,
    ) -> Result<FayInstanceView, CliError> {
        let (buf_bytes, meta) = create_soul_buf_bytes(&self.state_dir, kind, name, requested_caps)?;
        let buf_path = self
            .state_dir
            .join("souls")
            .join(format!("{}.buf", meta.fay_id));
        fs::write(&buf_path, &buf_bytes).map_err(|err| CliError::Io {
            path: buf_path.display().to_string(),
            detail: err.to_string(),
        })?;
        self.mount_buf_bytes(&buf_bytes, &buf_path.display().to_string(), name)
    }

    /// 从磁盘 `.buf` 路径校验并挂载 `Fay` 实例（初始状态固定为 `ROGUE`）。
    pub fn load_buf_from_path(&mut self, path: &str) -> Result<FayInstanceView, CliError> {
        let bytes = fs::read(path).map_err(|err| CliError::Io {
            path: path.to_owned(),
            detail: err.to_string(),
        })?;
        let fallback_name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("loaded-fay");
        self.mount_buf_bytes(&bytes, path, fallback_name)
    }

    /// 从内存 `.buf` 字节流校验并挂载 `Fay` 实例（初始状态固定为 `ROGUE`）。
    pub fn mount_buf_bytes(
        &mut self,
        buf_bytes: &[u8],
        source_label: &str,
        fallback_name: &str,
    ) -> Result<FayInstanceView, CliError> {
        let obj = load(buf_bytes, &default_policy())?;
        let meta = extract_soul_from_buf(&obj, fallback_name);
        let requested_set: BTreeSet<String> = meta.requested_capabilities.iter().cloned().collect();
        // 默认为新挂载的实例赋予与请求集合一致的初始策略集合（但仍受 HostPool 交集与 ROGUE 状态严格约束）。
        let initial_policy = requested_set.clone();

        let profile = self.device_profile(false);
        let embodiment = negotiate_embodiment_slices(&profile, meta.kind == SoulKind::Cofay);

        let sandbox_path = self.state_dir.join("sandboxes").join(&meta.fay_id);
        fs::create_dir_all(&sandbox_path).map_err(|err| CliError::Io {
            path: sandbox_path.display().to_string(),
            detail: err.to_string(),
        })?;

        let seq = self.data.next_instance_seq;
        self.data.next_instance_seq = seq.saturating_add(1);
        let instance_id = format!("inst_{seq:04}");
        let fay_id = meta.fay_id.clone();

        let mut merit_pocket = MeriTokenPocketState::initial_for(&fay_id);
        if let Ok(Some(reg)) = rpc_register_external_fay(
            &self.data.gmchain_endpoint,
            &fay_id,
            &meta.display_name,
            meta.kind.as_str(),
        ) {
            merit_pocket.canonical_fayid = reg.canonical_fayid;
            merit_pocket.cur_merit_raw = reg.cur_merit_raw;
            merit_pocket.min_merit_floor_raw = reg.min_merit_floor_raw;
            merit_pocket.consumable_merit_raw = reg.consumable_merit_raw;
            merit_pocket.last_block_height = reg.block_height;
            merit_pocket.last_state_root_hex = reg.state_root_hex;
            SYNC_MODE_LIVE_RPC.clone_into(&mut merit_pocket.sync_mode);
        }

        let stored = StoredFayInstance {
            instance_id,
            buf_path: source_label.to_owned(),
            metadata: meta,
            requested_capabilities: requested_set.clone(),
            policy_capabilities: initial_policy,
            faying_state: FayingState::Rogue,
            active_mandate: None,
            last_rogue_reason: Some("Initial load starts detached in ROGUE state".to_owned()),
            embodiment: embodiment.clone(),
            pending_sync_deltas: 0,
            sandbox_dir: sandbox_path.display().to_string(),
            chat_history: Vec::new(),
            loaded_at_unix: unix_now_secs(),
            merit_pocket,
        };
        let view = self.to_instance_view(&stored);
        self.data.fays.insert(fay_id.clone(), stored);

        self.append_audit(
            "fay_mounted_rogue",
            json!({
                "fay_id": fay_id,
                "faying_state": "ROGUE",
                "requested_capabilities": requested_set,
                "embodiment_mode": embodiment.embodiment_mode,
            }),
        )?;
        self.save()?;
        Ok(view)
    }

    /// 更新管理员对某个已加载 `Fay` 的分配策略（两层授权链第 ② 层），实时重算 `BuF ∩ HostPool ∩ Policy`。
    pub fn update_fay_policy(
        &mut self,
        fay_id: &str,
        allowed_capabilities: &BTreeSet<String>,
    ) -> Result<FayInstanceView, CliError> {
        self.enforce_liveness()?;
        {
            let stored = self.get_fay_mut(fay_id)?;
            stored.policy_capabilities.clone_from(allowed_capabilities);
        }
        let view = self.to_instance_view(self.get_fay(fay_id)?);

        self.append_audit(
            "fay_policy_updated",
            json!({
                "fay_id": view.fay_id,
                "policy_capabilities": allowed_capabilities,
                "effective_capabilities": view.effective_capabilities,
            }),
        )?;
        self.save()?;
        Ok(view)
    }

    /// 拨动显式 `Faying` 上岗开关：签发限时带心跳的 `ActiveFayingMandate`，状态由 `ROGUE -> FAYING`。
    pub fn activate_faying(
        &mut self,
        fay_id: &str,
        grade: Option<&str>,
        ttl_secs: Option<u64>,
        heartbeat_timeout_secs: Option<u64>,
    ) -> Result<FayInstanceView, CliError> {
        self.enforce_liveness()?;
        let now = unix_now_secs();
        let clean_grade = match grade.map(str::trim) {
            Some("G1") => "G1",
            Some("G3") => "G3",
            Some("G4") => "G4",
            _ => "G2",
        };
        let ttl = ttl_secs.unwrap_or(3600).clamp(5, 86_400);
        let hb_timeout = heartbeat_timeout_secs.unwrap_or(300).clamp(2, 3600);
        let entropy = fresh_entropy_32();
        let mandate_id = format!("mnd_{}", &encode_hex(&entropy)[..12]);

        let mandate = ActiveFayingMandate {
            mandate_id: mandate_id.clone(),
            grade: clean_grade.to_owned(),
            issued_at_unix: now,
            expires_at_unix: now.saturating_add(ttl),
            last_heartbeat_unix: now,
            heartbeat_timeout_secs: hb_timeout,
        };

        {
            let stored = self.get_fay_mut(fay_id)?;
            stored.faying_state = FayingState::Faying;
            stored.active_mandate = Some(mandate);
            stored.last_rogue_reason = None;
        }
        let view = self.to_instance_view(self.get_fay(fay_id)?);

        self.append_audit(
            "faying_activated",
            json!({
                "fay_id": view.fay_id,
                "mandate_id": mandate_id,
                "grade": clean_grade,
                "ttl_secs": ttl,
                "heartbeat_timeout_secs": hb_timeout,
            }),
        )?;
        self.save()?;
        Ok(view)
    }

    /// 刷新已上岗 `Fay` 的监护心跳。
    pub fn record_heartbeat(&mut self, fay_id: &str) -> Result<FayInstanceView, CliError> {
        self.enforce_liveness()?;
        let now = unix_now_secs();
        {
            let stored = self.get_fay_mut(fay_id)?;
            let Some(mandate) = stored.active_mandate.as_mut() else {
                return Err(CliError::Failure(FaygerError::new(
                    ErrorCode::AdpCapabilityDenied,
                    Layer::Runtime,
                    "cannot renew heartbeat while Fay is detached in ROGUE state",
                )));
            };
            mandate.last_heartbeat_unix = now;
        }
        let view = self.to_instance_view(self.get_fay(fay_id)?);
        self.save()?;
        Ok(view)
    }

    /// 触发红色 `G0` 紧急制动（`Red Emergency Brake`）：立即将实例退回 `ROGUE`，吊销委托并作废挂起审批！
    pub fn trigger_emergency_brake(
        &mut self,
        fay_id: &str,
        reason: Option<&str>,
    ) -> Result<FayInstanceView, CliError> {
        let canonical_id = self.resolve_fay_key(fay_id)?;
        let now = unix_now_secs();
        let brake_reason = reason
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Human Prime triggered G0 Red Emergency Brake");

        let revoked_mandate_id = {
            let stored = self.get_fay_mut(&canonical_id)?;
            let prev_id = stored.active_mandate.take().map(|m| m.mandate_id);
            stored.faying_state = FayingState::Rogue;
            stored.last_rogue_reason = Some(brake_reason.to_owned());
            for turn in &mut stored.chat_history {
                for ann in &mut turn.response_ctx.annotations {
                    if let CtxAnnotation::CapApprovalCard { status, .. } = ann {
                        if status == "pending" {
                            "revoked_g0".clone_into(status);
                        }
                    }
                }
            }
            prev_id
        };

        let mut revoked_approvals = Vec::new();
        for item in self.data.cap_approvals.values_mut() {
            if item.fay_id == canonical_id && item.status == "pending" {
                "revoked_g0".clone_into(&mut item.status);
                item.decided_at_unix = Some(now);
                revoked_approvals.push(item.approval_id.clone());
            }
        }

        self.append_audit(
            "g0_emergency_brake",
            json!({
                "fay_id": canonical_id,
                "new_state": "ROGUE",
                "revoked_mandate_id": revoked_mandate_id,
                "revoked_pending_approvals": revoked_approvals,
                "reason": brake_reason,
            }),
        )?;
        self.save()?;
        let stored = self.get_fay(&canonical_id)?;
        Ok(self.to_instance_view(stored))
    }

    /// 向指定 `Fay` 发送纯对话消息（委托至 [`Self::chat_or_act`]）。
    pub fn chat_with_fay(
        &mut self,
        fay_id: &str,
        message: &str,
    ) -> Result<ChatTurnRecord, CliError> {
        self.chat_or_act(fay_id, message, None)
    }

    /// 向指定 `Fay` 发送对话消息或宿主驱动指令，经 `Ego` 三合一与 `CAP` 守门员处理后返回 `.ctx` 信封。
    #[allow(clippy::too_many_lines)]
    pub fn chat_or_act(
        &mut self,
        fay_id: &str,
        message: &str,
        explicit_action: Option<RequestedHostAction>,
    ) -> Result<ChatTurnRecord, CliError> {
        self.enforce_liveness()?;
        let canonical_id = self.resolve_fay_key(fay_id)?;
        let clean_msg = message.trim();
        let inferred_action = explicit_action.or_else(|| parse_slash_host_action(clean_msg));
        let mentions_merit = inferred_action.is_none() && message_mentions_merit(clean_msg);
        if mentions_merit {
            let _ = self.sync_single_fay_merit_best_effort(&canonical_id);
        }

        let stored_snapshot = self.get_fay(&canonical_id)?.clone();
        let effective_caps = self.compute_effective_capabilities(&stored_snapshot);
        let ego_endpoint = self.host_pool.ego_endpoint.clone();
        let is_cofay = stored_snapshot.metadata.kind == SoulKind::Cofay;

        let (draft_reply, mut annotations, audit_stage, audit_detail) = if let Some(action) =
            inferred_action
        {
            self.evaluate_and_run_host_action(&stored_snapshot, &effective_caps, &action)?
        } else {
            let tp_items = self.collect_tp_context_for_fay(&canonical_id);
            let mut rows = vec![
                CtxKeyValue {
                    key: "faying_state".to_owned(),
                    value: stored_snapshot.faying_state.as_str().to_owned(),
                },
                CtxKeyValue {
                    key: "effective_capabilities".to_owned(),
                    value: if effective_caps.is_empty() {
                        "(none)".to_owned()
                    } else {
                        effective_caps.join(", ")
                    },
                },
                CtxKeyValue {
                    key: "embodiment_mode".to_owned(),
                    value: stored_snapshot.embodiment.embodiment_mode.clone(),
                },
                CtxKeyValue {
                    key: "tp_shared_entries".to_owned(),
                    value: tp_items.len().to_string(),
                },
            ];
            for (k, v) in tp_items.iter().take(3) {
                rows.push(CtxKeyValue {
                    key: format!("tp:{k}"),
                    value: v.clone(),
                });
            }
            let standing = stored_snapshot
                .merit_pocket
                .to_standing_view(&canonical_id, &self.data.gmchain_endpoint);
            let mut draft = format!(
                "[{}] Received: \"{}\". Effective capabilities (BuF ∩ HostPool ∩ Policy): [{}]. Shared TP facts visible: {}.",
                stored_snapshot.metadata.display_name,
                clean_msg,
                if effective_caps.is_empty() {
                    "none".to_owned()
                } else {
                    effective_caps.join(", ")
                },
                tp_items.len()
            );
            let mut anns = vec![
                CtxAnnotation::StatusBadge {
                    label: "Faying Lifecycle".to_owned(),
                    value: stored_snapshot.faying_state.as_str().to_owned(),
                    tone: if stored_snapshot.faying_state == FayingState::Faying {
                        "ok".to_owned()
                    } else {
                        "warn".to_owned()
                    },
                },
                CtxAnnotation::KvTable {
                    title: "Runtime Context & Capability Intersection".to_owned(),
                    rows,
                },
            ];
            if mentions_merit {
                let _ = write!(
                    draft,
                    " MeriToken standing: curMerit={} μ, consumable={} μ (curMerit - minMerit), minMerit floor={} μ [{}].",
                    standing.cur_merit_formatted,
                    standing.consumable_merit_formatted,
                    standing.min_merit_floor_formatted,
                    standing.sync_mode
                );
                anns.push(CtxAnnotation::MeritStandingCard {
                    fay_id: canonical_id.clone(),
                    cur_merit_mu: format!("{} μ", standing.cur_merit_formatted),
                    consumable_merit_mu: format!("{} μ", standing.consumable_merit_formatted),
                    min_merit_floor_mu: format!("{} μ", standing.min_merit_floor_formatted),
                    sync_mode: standing.sync_mode,
                    block_height: standing.last_block_height,
                    state_root_hex: standing.last_state_root_hex,
                });
            }
            (
                draft,
                anns,
                "icp_dialogue_turn",
                json!({
                    "fay_id": canonical_id,
                    "faying_state": stored_snapshot.faying_state.as_str(),
                    "tp_context_count": tp_items.len(),
                    "merit_card_attached": mentions_merit,
                }),
            )
        };

        let ego_out = run_ego_three_in_one(
            &ego_endpoint,
            &canonical_id,
            &stored_snapshot.metadata.display_name,
            is_cofay,
            clean_msg,
            &effective_caps,
            &draft_reply,
        );

        let msg_entropy = fresh_entropy_32();
        let env = CtxEnvelope {
            schema_version: ICP_CTX_SCHEMA_VERSION.to_owned(),
            message_id: format!("ctx_{}", &encode_hex(&msg_entropy)[..12]),
            timestamp_unix: unix_now_secs(),
            fay_id: canonical_id.clone(),
            fay_kind: stored_snapshot.metadata.kind.as_str().to_owned(),
            faying_state: stored_snapshot.faying_state.as_str().to_owned(),
            summary_text: ego_out.guarded_reply,
            ego_trace: ego_out.trace,
            annotations: std::mem::take(&mut annotations),
        };
        let terminal_text = render_ctx_terminal(&env);
        let turn = ChatTurnRecord {
            user_input: if clean_msg.is_empty() {
                "(structured host action)".to_owned()
            } else {
                clean_msg.to_owned()
            },
            response_ctx: env,
            terminal_text,
        };

        {
            let stored_mut = self.get_fay_mut(&canonical_id)?;
            stored_mut.chat_history.push(turn.clone());
        }
        self.append_audit(audit_stage, audit_detail)?;
        self.save()?;
        Ok(turn)
    }

    /// `Human Prime` 对挂起的高风险 `CAP` 审批单作出批准（`approve = true`）或拒绝（`approve = false`）裁决。
    #[allow(clippy::too_many_lines)]
    pub fn decide_cap_approval(
        &mut self,
        approval_id: &str,
        approve: bool,
    ) -> Result<CapApprovalItem, CliError> {
        self.enforce_liveness()?;
        let item_snapshot = self
            .data
            .cap_approvals
            .get(approval_id)
            .cloned()
            .ok_or_else(|| {
                CliError::Failure(FaygerError::new(
                    ErrorCode::FmValidationFailed,
                    Layer::Assembler,
                    format!("CAP approval '{approval_id}' not found"),
                ))
            })?;

        if item_snapshot.status != "pending" {
            return Err(CliError::Failure(FaygerError::new(
                ErrorCode::AdpCapabilityDenied,
                Layer::Adapter,
                format!(
                    "CAP approval '{approval_id}' is no longer pending (status={})",
                    item_snapshot.status
                ),
            )));
        }

        let now = unix_now_secs();
        let fay_id = item_snapshot.fay_id.clone();

        if !approve {
            if let Some(slot) = self.data.cap_approvals.get_mut(approval_id) {
                "denied".clone_into(&mut slot.status);
                slot.decided_at_unix = Some(now);
                slot.execution_summary = Some("Denied by Human Prime".to_owned());
            }
            self.update_approval_card_status_in_history(&fay_id, approval_id, "denied");
            self.append_audit(
                "cap_denied_by_human_prime",
                json!({
                    "approval_id": approval_id,
                    "fay_id": fay_id,
                    "operation": item_snapshot.required_capability,
                }),
            )?;
            self.save()?;
            return Ok(self.data.cap_approvals[approval_id].clone());
        }

        // 批准分支：再次核验实例当前仍处于 FAYING 态且能力交集仍有效！
        let stored_fay = self.get_fay(&fay_id)?.clone();
        if stored_fay.faying_state != FayingState::Faying {
            if let Some(slot) = self.data.cap_approvals.get_mut(approval_id) {
                "revoked_g0".clone_into(&mut slot.status);
                slot.decided_at_unix = Some(now);
            }
            self.save()?;
            return Err(CliError::Failure(FaygerError::new(
                ErrorCode::AdpCapabilityDenied,
                Layer::Adapter,
                "cannot approve CAP action: Fay is detached in ROGUE state",
            )));
        }
        let effective_caps = self.compute_effective_capabilities(&stored_fay);
        if !effective_caps
            .iter()
            .any(|c| c == &item_snapshot.required_capability)
        {
            return Err(CliError::Failure(FaygerError::new(
                ErrorCode::AdpCapabilityDenied,
                Layer::Adapter,
                "cannot approve CAP action: capability removed from BuF ∩ HostPool ∩ Policy intersection",
            )));
        }

        let human_token = local_attestation_human_token(&self.state_dir)?;
        let ticket_id = format!("CAP-CONFIRMED-{approval_id}");
        let (exec_summary, artifact_ann, output_sha256) = execute_sandboxed_action(
            Path::new(&stored_fay.sandbox_dir),
            &item_snapshot.action,
            &ticket_id,
        )?;

        if let Some(slot) = self.data.cap_approvals.get_mut(approval_id) {
            "approved".clone_into(&mut slot.status);
            slot.decided_at_unix = Some(now);
            slot.execution_summary = Some(exec_summary.clone());
        }
        self.update_approval_card_status_in_history(&fay_id, approval_id, "approved");

        let confirm_env = CtxEnvelope {
            schema_version: ICP_CTX_SCHEMA_VERSION.to_owned(),
            message_id: format!("ctx_apr_{}", &approval_id[4..]),
            timestamp_unix: now,
            fay_id: fay_id.clone(),
            fay_kind: stored_fay.metadata.kind.as_str().to_owned(),
            faying_state: "FAYING".to_owned(),
            summary_text: format!("Human Prime approved {approval_id}: {exec_summary}"),
            ego_trace: crate::icp_ego::EgoPipelineTrace {
                mode: "cap_confirmed_execution".to_owned(),
                intuition_intent: "human_prime_confirmation".to_owned(),
                intuition_confidence: 1.0,
                dynamic_context_summary: format!("Attested by {human_token}"),
                guard_rewrite_applied: false,
            },
            annotations: vec![
                CtxAnnotation::StatusBadge {
                    label: "CAP High-Risk Gate".to_owned(),
                    value: "APPROVED_AND_EXECUTED".to_owned(),
                    tone: "ok".to_owned(),
                },
                artifact_ann,
            ],
        };
        let term = render_ctx_terminal(&confirm_env);
        {
            let fay_mut = self.get_fay_mut(&fay_id)?;
            fay_mut.pending_sync_deltas = fay_mut.pending_sync_deltas.saturating_add(1);
            fay_mut.chat_history.push(ChatTurnRecord {
                user_input: format!("[Human Prime Approval: {approval_id}]"),
                response_ctx: confirm_env,
                terminal_text: term,
            });
        }

        self.append_audit(
            "cap_approved_controlled_execution",
            json!({
                "approval_id": approval_id,
                "fay_id": fay_id,
                "human_prime_attestation": human_token,
                "params_sha256": item_snapshot.params_sha256,
                "output_sha256": output_sha256,
            }),
        )?;
        self.save()?;
        Ok(self.data.cap_approvals[approval_id].clone())
    }

    /// 创建本地双 `Fay` `Telepathy` (`TP`) 共享认知空间会话。
    pub fn create_tp_session(
        &mut self,
        initiator_fay_id: &str,
        peer_fay_id: &str,
        topic: &str,
        permissions: Vec<String>,
    ) -> Result<TpSharedSession, CliError> {
        let init_clean = initiator_fay_id.trim();
        let peer_clean = peer_fay_id.trim();
        if init_clean.is_empty() || peer_clean.is_empty() || init_clean == peer_clean {
            return Err(CliError::Usage(
                "TP session requires two distinct loaded Fay IDs".to_owned(),
            ));
        }
        let _ = self.get_fay(init_clean)?;
        let _ = self.get_fay(peer_clean)?;

        let entropy = fresh_entropy_32();
        let session_id = format!("tp_{}", &encode_hex(&entropy)[..12]);
        let clean_topic = if topic.trim().is_empty() {
            "Shared Cognitive Workspace".to_owned()
        } else {
            topic.trim().to_owned()
        };
        let clean_perms = if permissions.is_empty() {
            vec![
                "read_memory".to_owned(),
                "write_shared_context".to_owned(),
                "propose_task".to_owned(),
            ]
        } else {
            permissions
        };

        let session = TpSharedSession {
            session_id: session_id.clone(),
            initiator_fay_id: init_clean.to_owned(),
            peer_fay_id: peer_clean.to_owned(),
            topic: clean_topic.clone(),
            permissions: clean_perms.clone(),
            status: "active".to_owned(),
            revision: 0,
            shared_entries: Vec::new(),
            created_at_unix: unix_now_secs(),
        };
        self.data
            .tp_sessions
            .insert(session_id.clone(), session.clone());
        self.append_audit(
            "tp_session_opened",
            json!({
                "session_id": session_id,
                "initiator_fay_id": init_clean,
                "peer_fay_id": peer_clean,
                "topic": clean_topic,
                "permissions": clean_perms,
            }),
        )?;
        self.save()?;
        Ok(session)
    }

    /// 在已授权的 `TP` 会话内直接共享一条结构化认知条目（记忆/视图状态/任务意图）。
    pub fn share_tp_entry(
        &mut self,
        session_id: &str,
        author_fay_id: &str,
        category: &str,
        key: &str,
        structured_value: Value,
    ) -> Result<TpCognitiveEntry, CliError> {
        let clean_key = key.trim();
        if clean_key.is_empty() {
            return Err(CliError::Usage(
                "TP shared entry requires non-empty key".to_owned(),
            ));
        }
        let canonical = jcs_canonicalize(&structured_value)?;
        let value_sha256 = hex_sha256(canonical.as_bytes());
        let entropy = fresh_entropy_32();
        let entry_id = format!("tpe_{}", &encode_hex(&entropy)[..10]);
        let clean_cat = if category.trim().is_empty() {
            "memory_fact".to_owned()
        } else {
            category.trim().to_owned()
        };

        let entry = {
            let session = self.data.tp_sessions.get_mut(session_id).ok_or_else(|| {
                CliError::Failure(FaygerError::new(
                    ErrorCode::FmValidationFailed,
                    Layer::Assembler,
                    format!("TP session '{session_id}' not found"),
                ))
            })?;
            if session.status != "active" {
                return Err(CliError::Failure(FaygerError::new(
                    ErrorCode::AdpCapabilityDenied,
                    Layer::Runtime,
                    "TP session is already closed",
                )));
            }
            if author_fay_id != session.initiator_fay_id && author_fay_id != session.peer_fay_id {
                return Err(CliError::Failure(FaygerError::new(
                    ErrorCode::AdpCapabilityDenied,
                    Layer::Runtime,
                    format!("Fay '{author_fay_id}' is not an authorized participant of TP session '{session_id}'"),
                )));
            }
            session.revision = session.revision.saturating_add(1);
            let created = TpCognitiveEntry {
                entry_id,
                revision: session.revision,
                author_fay_id: author_fay_id.to_owned(),
                category: clean_cat.clone(),
                key: clean_key.to_owned(),
                structured_value,
                value_sha256: value_sha256.clone(),
                created_at_unix: unix_now_secs(),
            };
            session.shared_entries.push(created.clone());
            created
        };

        self.append_audit(
            "tp_cognitive_shared",
            json!({
                "session_id": session_id,
                "entry_id": entry.entry_id,
                "revision": entry.revision,
                "author_fay_id": author_fay_id,
                "category": clean_cat,
                "key": clean_key,
                "value_sha256": value_sha256,
            }),
        )?;
        self.save()?;
        Ok(entry)
    }

    /// 关闭一个 `TP` 共享认知会话。
    pub fn close_tp_session(&mut self, session_id: &str) -> Result<TpSharedSession, CliError> {
        let updated = {
            let session = self.data.tp_sessions.get_mut(session_id).ok_or_else(|| {
                CliError::Failure(FaygerError::new(
                    ErrorCode::FmValidationFailed,
                    Layer::Assembler,
                    format!("TP session '{session_id}' not found"),
                ))
            })?;
            "closed".clone_into(&mut session.status);
            session.clone()
        };
        self.append_audit(
            "tp_session_closed",
            json!({
                "session_id": session_id,
                "final_revision": updated.revision,
            }),
        )?;
        self.save()?;
        Ok(updated)
    }

    /// 列出全部已加载 `Fay` 实例视图。
    pub fn list_fays(&mut self) -> Result<Vec<FayInstanceView>, CliError> {
        self.enforce_liveness()?;
        Ok(self
            .data
            .fays
            .values()
            .map(|s| self.to_instance_view(s))
            .collect())
    }

    /// 获取单个 `Fay` 实例视图。
    pub fn get_fay_view(&mut self, fay_id: &str) -> Result<FayInstanceView, CliError> {
        self.enforce_liveness()?;
        let stored = self.get_fay(fay_id)?;
        Ok(self.to_instance_view(stored))
    }

    /// 列出全部 `CAP` 审批单（按创建时间倒序）。
    #[must_use]
    pub fn list_cap_approvals(&self) -> Vec<CapApprovalItem> {
        let mut list: Vec<CapApprovalItem> = self.data.cap_approvals.values().cloned().collect();
        list.sort_by_key(|b| std::cmp::Reverse(b.created_at_unix));
        list
    }

    /// 列出全部 `TP` 会话。
    #[must_use]
    pub fn list_tp_sessions(&self) -> Vec<TpSharedSession> {
        self.data.tp_sessions.values().cloned().collect()
    }

    /// 返回审计哈希链分录及自校验状态。
    #[must_use]
    pub fn audit_log(&self) -> (&[HashChainedAuditEntry], bool) {
        (
            &self.data.audit_entries,
            verify_audit_chain(&self.data.audit_entries),
        )
    }

    /// 返回当前 `GMChain` / `MeriToken` 端点配置与在线探测状态。
    #[must_use]
    pub fn merit_config(&self) -> MeritConfigView {
        let online = probe_gmchain_online(&self.data.gmchain_endpoint);
        MeritConfigView {
            gmchain_endpoint: self.data.gmchain_endpoint.clone(),
            rpc_url: format!("{}/rpc", self.data.gmchain_endpoint),
            online,
            sync_mode: if online {
                SYNC_MODE_LIVE_RPC.to_owned()
            } else {
                SYNC_MODE_LOCAL_FALLBACK.to_owned()
            },
            synced_fays: self.data.fays.len(),
        }
    }

    /// 更新回环 `GMChain` JSON-RPC 端点（强制仅允许 `127.0.0.1` / `localhost`）并触发全部已挂载 `Fay` 同步。
    pub fn update_merit_config(
        &mut self,
        gmchain_endpoint: Option<&str>,
    ) -> Result<MeritConfigView, CliError> {
        self.enforce_liveness()?;
        if let Some(raw_ep) = gmchain_endpoint.map(str::trim).filter(|s| !s.is_empty()) {
            let normalized = normalize_loopback_gmchain_endpoint(raw_ep)?;
            self.data.gmchain_endpoint = normalized;
        }
        let ids: Vec<String> = self.data.fays.keys().cloned().collect();
        let mut live_synced = 0usize;
        for id in &ids {
            if self.sync_single_fay_merit_best_effort(id) {
                live_synced = live_synced.saturating_add(1);
            }
        }
        let online = live_synced > 0 || probe_gmchain_online(&self.data.gmchain_endpoint);
        let sync_mode = if online {
            SYNC_MODE_LIVE_RPC.to_owned()
        } else {
            SYNC_MODE_LOCAL_FALLBACK.to_owned()
        };
        self.append_audit(
            "merit.config_updated",
            json!({
                "gmchain_endpoint": self.data.gmchain_endpoint,
                "online": online,
                "synced_fays": ids.len(),
            }),
        )?;
        self.save()?;
        Ok(MeritConfigView {
            gmchain_endpoint: self.data.gmchain_endpoint.clone(),
            rpc_url: format!("{}/rpc", self.data.gmchain_endpoint),
            online,
            sync_mode,
            synced_fays: ids.len(),
        })
    }

    /// 查询并（在 `GMChain` 在线时）实时同步单个 `Fay` 的 `MeriToken` 账户视图（`ROGUE` 与 `FAYING` 均允许查看）。
    pub fn get_fay_merit(&mut self, id_or_instance: &str) -> Result<MeritStandingView, CliError> {
        self.enforce_liveness()?;
        let canonical_id = self.resolve_fay_key(id_or_instance)?;
        let changed = self.sync_single_fay_merit_best_effort(&canonical_id);
        if changed {
            self.save()?;
        }
        let stored = self.get_fay(&canonical_id)?;
        Ok(stored
            .merit_pocket
            .to_standing_view(&canonical_id, &self.data.gmchain_endpoint))
    }

    /// 为处于 `FAYING` 受监护状态的 `Fay` 提交协作贡献见证并入账 `MeriToken`（`ROGUE` 状态严格 `403` 拦截）。
    #[allow(clippy::too_many_lines)]
    pub fn add_fay_merit(
        &mut self,
        id_or_instance: &str,
        input: &MeritAddInput,
    ) -> Result<MeritAddReceipt, CliError> {
        self.enforce_liveness()?;
        let canonical_id = self.resolve_fay_key(id_or_instance)?;
        let stored_snapshot = self.get_fay(&canonical_id)?.clone();

        if stored_snapshot.faying_state != FayingState::Faying {
            return Err(CliError::Failure(FaygerError::new(
                ErrorCode::AdpCapabilityDenied,
                Layer::Runtime,
                format!(
                    "Fay '{canonical_id}' is in ROGUE state; Human Prime must switch Faying ON before adding MeriToken contributions"
                ),
            )));
        }

        let clean_title = input.title.trim();
        if clean_title.is_empty() {
            return Err(CliError::Usage(
                "merit/add requires non-empty 'title'".to_owned(),
            ));
        }
        if input.amount_mu <= 0 {
            return Err(CliError::Usage(
                "merit/add requires positive 'amount_mu' (> 0)".to_owned(),
            ));
        }
        let clean_domain = if input.domain_chain_id.trim().is_empty() {
            "gmc-open-science".to_owned()
        } else {
            input.domain_chain_id.trim().to_owned()
        };
        let clean_horizon = input.horizon_days.max(1);
        let clean_summary = if input.summary.trim().is_empty() {
            clean_title.to_owned()
        } else {
            input.summary.trim().to_owned()
        };
        let normalized_input = MeritAddInput {
            title: clean_title.to_owned(),
            domain_chain_id: clean_domain.clone(),
            amount_mu: input.amount_mu,
            horizon_days: clean_horizon,
            summary: clean_summary,
        };

        let added_mu_raw = i128::from(normalized_input.amount_mu).saturating_mul(MERIT_SCALE);
        let before_cur = stored_snapshot.merit_pocket.cur_merit_raw;

        let rpc_res = rpc_record_contribution(
            &self.data.gmchain_endpoint,
            &canonical_id,
            &stored_snapshot.metadata.display_name,
            stored_snapshot.metadata.kind.as_str(),
            &normalized_input,
        )?;

        let (batch_id, after_cur, after_floor, after_consumable, block_height, state_root_hex, src) =
            if let Some(live) = rpc_res {
                let floor = live
                    .min_merit_floor_raw
                    .max(stored_snapshot.merit_pocket.min_merit_floor_raw)
                    .max(MIN_MERIT_E_RAW);
                let consumable = (live.cur_merit_raw - floor).max(0);
                (
                    live.batch_id,
                    live.cur_merit_raw,
                    floor,
                    consumable,
                    live.block_height,
                    live.state_root_hex,
                    SYNC_MODE_LIVE_RPC.to_owned(),
                )
            } else {
                let next_cur = before_cur.saturating_add(added_mu_raw);
                let floor_accrual = added_mu_raw / 10;
                let next_floor = stored_snapshot
                    .merit_pocket
                    .min_merit_floor_raw
                    .max(MIN_MERIT_E_RAW)
                    .saturating_add(floor_accrual);
                let next_consumable = (next_cur - next_floor).max(0);
                let next_height = stored_snapshot
                    .merit_pocket
                    .last_block_height
                    .saturating_add(1);
                let next_root =
                    compute_local_state_root(&canonical_id, next_cur, next_floor, next_height);
                let bid = format!("batch_{}", &encode_hex(&fresh_entropy_32())[..12]);
                (
                    bid,
                    next_cur,
                    next_floor,
                    next_consumable,
                    next_height,
                    next_root,
                    SYNC_MODE_LOCAL_FALLBACK.to_owned(),
                )
            };

        let ledger_item = MeritLedgerEntry {
            record_id: batch_id.clone(),
            entry_kind: "contribution_added".to_owned(),
            title: normalized_input.title.clone(),
            domain_or_priority: clean_domain.clone(),
            proposal_id: None,
            horizon_days: Some(clean_horizon),
            delta_mu_raw: added_mu_raw,
            delta_formatted: format!("+{} μ", format_merit_2dp(added_mu_raw)),
            before_cur_merit_raw: before_cur,
            after_cur_merit_raw: after_cur,
            min_merit_floor_raw: after_floor,
            consumable_merit_raw: after_consumable,
            source: src.clone(),
            block_height,
            timestamp_unix: unix_now_secs(),
        };

        let standing = {
            let endpoint = self.data.gmchain_endpoint.clone();
            let stored_mut = self.get_fay_mut(&canonical_id)?;
            stored_mut.pending_sync_deltas = stored_mut.pending_sync_deltas.saturating_add(1);
            stored_mut.merit_pocket.cur_merit_raw = after_cur;
            stored_mut.merit_pocket.min_merit_floor_raw = after_floor;
            stored_mut.merit_pocket.consumable_merit_raw = after_consumable;
            stored_mut.merit_pocket.last_block_height = block_height;
            stored_mut
                .merit_pocket
                .last_state_root_hex
                .clone_from(&state_root_hex);
            stored_mut.merit_pocket.sync_mode.clone_from(&src);
            stored_mut.merit_pocket.history.push(ledger_item);
            stored_mut
                .merit_pocket
                .to_standing_view(&canonical_id, &endpoint)
        };

        self.append_audit(
            "merit.contribution_added",
            json!({
                "fay_id": canonical_id,
                "batch_id": batch_id,
                "domain_chain_id": clean_domain,
                "added_mu_raw": added_mu_raw,
                "cur_merit_raw": after_cur,
                "min_merit_floor_raw": after_floor,
                "consumable_merit_raw": after_consumable,
                "source": src,
                "block_height": block_height,
                "state_root_hex": state_root_hex,
            }),
        )?;
        self.save()?;

        Ok(MeritAddReceipt {
            batch_id,
            added_mu_raw,
            block_height,
            state_root_hex,
            standing,
        })
    }

    /// 为处于 `FAYING` 受监护状态的 `Fay` 行使优先权并消减可消费活跃美誉值（严格保护 `minMerit` 底值不变！）。
    #[allow(clippy::too_many_lines)]
    pub fn consume_fay_merit(
        &mut self,
        id_or_instance: &str,
        input: &MeritConsumeInput,
    ) -> Result<MeritConsumeReceipt, CliError> {
        self.enforce_liveness()?;
        let canonical_id = self.resolve_fay_key(id_or_instance)?;

        {
            let stored = self.get_fay(&canonical_id)?;
            if stored.faying_state != FayingState::Faying {
                return Err(CliError::Failure(FaygerError::new(
                    ErrorCode::AdpCapabilityDenied,
                    Layer::Runtime,
                    format!(
                        "Fay '{canonical_id}' is in ROGUE state; Human Prime must switch Faying ON before exercising MeriToken priority rights"
                    ),
                )));
            }
        }

        let priority_kind = validate_priority_kind(&input.priority_kind)?;
        if input.amount_raw <= 0 {
            return Err(CliError::Usage(
                "merit/consume requires positive 'amount_mu' (> 0)".to_owned(),
            ));
        }

        // 若本地尚未与在线公誉链对齐且请求额超过当前本地缓存，先执行一次链上同步核实最新余额。
        {
            let pocket = &self.get_fay(&canonical_id)?.merit_pocket;
            let cur_consumable = (pocket.cur_merit_raw - pocket.min_merit_floor_raw).max(0);
            if input.amount_raw > cur_consumable && pocket.sync_mode != SYNC_MODE_LIVE_RPC {
                let _ = self.sync_single_fay_merit_best_effort(&canonical_id);
            }
        }

        let stored_snapshot = self.get_fay(&canonical_id)?.clone();
        let preserved_floor = stored_snapshot
            .merit_pocket
            .min_merit_floor_raw
            .max(MIN_MERIT_E_RAW);
        let available_consumable =
            (stored_snapshot.merit_pocket.cur_merit_raw - preserved_floor).max(0);

        if input.amount_raw > available_consumable {
            return Err(CliError::Usage(format!(
                "requested priority consumption {} μ exceeds consumable active pool {} μ (curMerit={} μ - minMerit floor={} μ); constitutional minMerit floor is strictly protected",
                format_merit_2dp(input.amount_raw),
                format_merit_2dp(available_consumable),
                format_merit_2dp(stored_snapshot.merit_pocket.cur_merit_raw),
                format_merit_2dp(preserved_floor),
            )));
        }

        let clean_reason = if input.reason.trim().is_empty() {
            format!("Exercise {priority_kind}")
        } else {
            input.reason.trim().to_owned()
        };
        let clean_proposal = input
            .proposal_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        let normalized_req = MeritConsumeInput {
            priority_kind: priority_kind.clone(),
            amount_raw: input.amount_raw,
            amount_mu_wire: input.amount_mu_wire.clone(),
            reason: clean_reason.clone(),
            proposal_id: clean_proposal.clone(),
        };

        let rpc_res =
            rpc_consume_priority(&self.data.gmchain_endpoint, &canonical_id, &normalized_req)?;

        let (
            consumption_id,
            consumed_mu_raw,
            before_cur,
            after_cur,
            after_floor,
            after_consumable,
            block_height,
            state_root_hex,
            src,
        ) = if let Some(live) = rpc_res {
            // 宪章底值铁律：消费操作绝不允许令 minMerit 下降！
            let floor = live.min_merit_floor_raw.max(preserved_floor);
            if live.after_cur_merit_raw < floor {
                return Err(CliError::Usage(
                    "GMChain RPC response violated minMerit constitutional floor".to_owned(),
                ));
            }
            let consumable = (live.after_cur_merit_raw - floor).max(0);
            (
                live.consumption_id,
                live.consumed_mu_raw,
                live.before_cur_merit_raw,
                live.after_cur_merit_raw,
                floor,
                consumable,
                live.block_height,
                live.state_root_hex,
                SYNC_MODE_LIVE_RPC.to_owned(),
            )
        } else {
            let before = stored_snapshot.merit_pocket.cur_merit_raw;
            let after = before - input.amount_raw;
            let next_consumable = (after - preserved_floor).max(0);
            let next_height = stored_snapshot
                .merit_pocket
                .last_block_height
                .saturating_add(1);
            let next_root =
                compute_local_state_root(&canonical_id, after, preserved_floor, next_height);
            let cid = format!("csm_{}", &encode_hex(&fresh_entropy_32())[..12]);
            (
                cid,
                input.amount_raw,
                before,
                after,
                preserved_floor,
                next_consumable,
                next_height,
                next_root,
                SYNC_MODE_LOCAL_FALLBACK.to_owned(),
            )
        };

        let ledger_item = MeritLedgerEntry {
            record_id: consumption_id.clone(),
            entry_kind: "priority_consumed".to_owned(),
            title: clean_reason.clone(),
            domain_or_priority: priority_kind.clone(),
            proposal_id: clean_proposal.clone(),
            horizon_days: None,
            delta_mu_raw: -consumed_mu_raw,
            delta_formatted: format!("-{} μ", format_merit_2dp(consumed_mu_raw)),
            before_cur_merit_raw: before_cur,
            after_cur_merit_raw: after_cur,
            min_merit_floor_raw: after_floor,
            consumable_merit_raw: after_consumable,
            source: src.clone(),
            block_height,
            timestamp_unix: unix_now_secs(),
        };

        let standing = {
            let endpoint = self.data.gmchain_endpoint.clone();
            let stored_mut = self.get_fay_mut(&canonical_id)?;
            stored_mut.pending_sync_deltas = stored_mut.pending_sync_deltas.saturating_add(1);
            stored_mut.merit_pocket.cur_merit_raw = after_cur;
            stored_mut.merit_pocket.min_merit_floor_raw = after_floor;
            stored_mut.merit_pocket.consumable_merit_raw = after_consumable;
            stored_mut.merit_pocket.last_block_height = block_height;
            stored_mut
                .merit_pocket
                .last_state_root_hex
                .clone_from(&state_root_hex);
            stored_mut.merit_pocket.sync_mode.clone_from(&src);
            stored_mut.merit_pocket.history.push(ledger_item);
            stored_mut
                .merit_pocket
                .to_standing_view(&canonical_id, &endpoint)
        };

        self.append_audit(
            "merit.priority_consumed",
            json!({
                "fay_id": canonical_id,
                "consumption_id": consumption_id,
                "priority_kind": priority_kind,
                "proposal_id": clean_proposal,
                "consumed_mu_raw": consumed_mu_raw,
                "before_cur_merit_raw": before_cur,
                "after_cur_merit_raw": after_cur,
                "min_merit_floor_raw": after_floor,
                "consumable_merit_raw": after_consumable,
                "source": src,
                "block_height": block_height,
                "state_root_hex": state_root_hex,
            }),
        )?;
        self.save()?;

        Ok(MeritConsumeReceipt {
            consumption_id,
            priority_kind,
            proposal_id: clean_proposal,
            reason: clean_reason,
            consumed_mu_raw,
            before_cur_merit_raw: before_cur,
            after_cur_merit_raw: after_cur,
            block_height,
            state_root_hex,
            standing,
        })
    }

    /// 生成 `L1_PUBLIC` / `L2_BAND` / `L3_ZK_BLINDED` 分级美誉出示凭证并写入防篡改审计链。
    pub fn present_fay_merit_credential(
        &mut self,
        id_or_instance: &str,
        privacy_tier: &str,
    ) -> Result<MeritCredentialPresentation, CliError> {
        self.enforce_liveness()?;
        let canonical_id = self.resolve_fay_key(id_or_instance)?;
        let _ = self.sync_single_fay_merit_best_effort(&canonical_id);

        let (standing, display_name) = {
            let stored = self.get_fay(&canonical_id)?;
            (
                stored
                    .merit_pocket
                    .to_standing_view(&canonical_id, &self.data.gmchain_endpoint),
                stored.metadata.display_name.clone(),
            )
        };
        let cred = build_merit_credential(&standing, &display_name, privacy_tier)?;

        self.append_audit(
            "merit.credential_presented",
            json!({
                "fay_id": canonical_id,
                "credential_id": cred.credential_id,
                "privacy_tier": cred.privacy_tier,
                "commitment_sha256": cred.commitment_sha256,
                "block_height": cred.block_height,
            }),
        )?;
        self.save()?;
        Ok(cred)
    }

    fn sync_single_fay_merit_best_effort(&mut self, canonical_fay_id: &str) -> bool {
        let Some(stored) = self.data.fays.get(canonical_fay_id) else {
            return false;
        };
        let display_name = stored.metadata.display_name.clone();
        let kind_str = stored.metadata.kind.as_str().to_owned();
        let endpoint = self.data.gmchain_endpoint.clone();

        if let Ok(Some(reg)) =
            rpc_register_external_fay(&endpoint, canonical_fay_id, &display_name, &kind_str)
        {
            if let Some(slot) = self.data.fays.get_mut(canonical_fay_id) {
                slot.merit_pocket.canonical_fayid = reg.canonical_fayid;
                slot.merit_pocket.cur_merit_raw = reg.cur_merit_raw;
                slot.merit_pocket.min_merit_floor_raw = reg
                    .min_merit_floor_raw
                    .max(slot.merit_pocket.min_merit_floor_raw)
                    .max(MIN_MERIT_E_RAW);
                slot.merit_pocket.consumable_merit_raw = (slot.merit_pocket.cur_merit_raw
                    - slot.merit_pocket.min_merit_floor_raw)
                    .max(0);
                slot.merit_pocket.last_block_height = reg.block_height;
                slot.merit_pocket.last_state_root_hex = reg.state_root_hex;
                SYNC_MODE_LIVE_RPC.clone_into(&mut slot.merit_pocket.sync_mode);
            }
            true
        } else {
            if let Some(slot) = self.data.fays.get_mut(canonical_fay_id) {
                SYNC_MODE_LOCAL_FALLBACK.clone_into(&mut slot.merit_pocket.sync_mode);
            }
            false
        }
    }

    /// 组装供控制台渲染的全量 [`DashboardSnapshot`]。
    pub fn snapshot(&mut self, probe_ego_network: bool) -> Result<DashboardSnapshot, CliError> {
        self.enforce_liveness()?;
        let wallet = show_wallet(&self.state_dir)?;
        let fays: Vec<FayInstanceView> = self
            .data
            .fays
            .values()
            .map(|s| self.to_instance_view(s))
            .collect();
        let cap_approvals = self.list_cap_approvals();
        let tp_sessions = self.list_tp_sessions();
        let verified = verify_audit_chain(&self.data.audit_entries);
        let head = self
            .data
            .audit_entries
            .last()
            .map_or_else(|| GENESIS_PREV_HASH.to_owned(), |e| e.entry_hash.clone());
        let merit_config = if probe_ego_network {
            self.merit_config()
        } else {
            MeritConfigView {
                gmchain_endpoint: self.data.gmchain_endpoint.clone(),
                rpc_url: format!("{}/rpc", self.data.gmchain_endpoint),
                online: false,
                sync_mode: SYNC_MODE_LOCAL_FALLBACK.to_owned(),
                synced_fays: fays.len(),
            }
        };

        Ok(DashboardSnapshot {
            fayger_version: crate::FAYGER_VERSION.to_owned(),
            device_profile: self.device_profile(probe_ego_network),
            host_pool: self.host_pool.clone(),
            wallet,
            fays,
            cap_approvals,
            tp_sessions,
            audit_chain_verified: verified,
            chain_head_hash: head,
            audit_entries: self.data.audit_entries.clone(),
            merit_config,
        })
    }

    /// 检查所有处于 `FAYING` 态的实例是否委托过期或心跳超时；超时者立即自动打回 `ROGUE`！
    pub fn enforce_liveness(&mut self) -> Result<(), CliError> {
        let now = unix_now_secs();
        let mut timed_out_ids = Vec::new();
        for (fay_id, stored) in &mut self.data.fays {
            if stored.faying_state == FayingState::Faying {
                let expired = stored.active_mandate.as_ref().is_none_or(|m| {
                    now >= m.expires_at_unix
                        || now.saturating_sub(m.last_heartbeat_unix) > m.heartbeat_timeout_secs
                });
                if expired {
                    stored.faying_state = FayingState::Rogue;
                    stored.active_mandate = None;
                    stored.last_rogue_reason = Some(
                        "Faying mandate expired or heartbeat timed out -> auto-reverted to ROGUE"
                            .to_owned(),
                    );
                    timed_out_ids.push(fay_id.clone());
                }
            }
        }
        if !timed_out_ids.is_empty() {
            for fid in &timed_out_ids {
                for apr in self.data.cap_approvals.values_mut() {
                    if &apr.fay_id == fid && apr.status == "pending" {
                        "revoked_g0".clone_into(&mut apr.status);
                        apr.decided_at_unix = Some(now);
                    }
                }
                self.append_audit(
                    "faying_heartbeat_timeout_rogue",
                    json!({
                        "fay_id": fid,
                        "new_state": "ROGUE",
                    }),
                )?;
            }
            self.save()?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_and_run_host_action(
        &mut self,
        stored: &StoredFayInstance,
        effective_caps: &[String],
        action: &RequestedHostAction,
    ) -> Result<(String, Vec<CtxAnnotation>, &'static str, Value), CliError> {
        let req_cap = action.required_capability();
        let target = action.target_summary();

        // 第 1 道门：Rogue Fay 停机隔离守卫！
        if stored.faying_state == FayingState::Rogue {
            let detail = format!(
                "Fay '{}' is currently in ROGUE (detached) state. Flip the Faying Activation Switch before invoking '{}'.",
                stored.metadata.fay_id, req_cap
            );
            return Ok((
                detail.clone(),
                vec![
                    CtxAnnotation::StatusBadge {
                        label: "CAP Gatekeeper".to_owned(),
                        value: "DENY_ROGUE_FAY".to_owned(),
                        tone: "danger".to_owned(),
                    },
                    CtxAnnotation::AlertBanner {
                        level: "error".to_owned(),
                        title: "Rogue Fay Guard Blocked Host Operation".to_owned(),
                        detail,
                    },
                ],
                "cap_denied_rogue_fay",
                json!({
                    "fay_id": stored.metadata.fay_id,
                    "operation": req_cap,
                    "target": target,
                    "disposition": "DENY_ROGUE_FAY",
                }),
            ));
        }

        // 第 2 道门：BuF ∩ HostPool ∩ PerFayPolicy 两层授权链实时交集校验！
        if !effective_caps.iter().any(|c| c == req_cap) {
            let in_buf = stored.requested_capabilities.contains(req_cap);
            let in_pool = self.host_pool.granted_capabilities.contains(req_cap);
            let in_policy = stored.policy_capabilities.contains(req_cap);
            let msg = format!(
                "Capability '{req_cap}' denied by BuF ∩ HostPool ∩ Policy intersection (BuF={in_buf}, HostPool={in_pool}, PerFayPolicy={in_policy})."
            );
            return Ok((
                msg.clone(),
                vec![
                    CtxAnnotation::StatusBadge {
                        label: "CAP Gatekeeper".to_owned(),
                        value: "DENY_CAPABILITY_INTERSECTION".to_owned(),
                        tone: "danger".to_owned(),
                    },
                    CtxAnnotation::AlertBanner {
                        level: "warning".to_owned(),
                        title: "Capability Outside Effective Intersection".to_owned(),
                        detail: msg,
                    },
                    CtxAnnotation::KvTable {
                        title: "Two-Layer Capability Chain Check".to_owned(),
                        rows: vec![
                            CtxKeyValue {
                                key: "Required Capability".to_owned(),
                                value: req_cap.to_owned(),
                            },
                            CtxKeyValue {
                                key: "BuF Manifest Requested".to_owned(),
                                value: in_buf.to_string(),
                            },
                            CtxKeyValue {
                                key: "FayGer Host Pool".to_owned(),
                                value: in_pool.to_string(),
                            },
                            CtxKeyValue {
                                key: "Per-Fay Admin Policy".to_owned(),
                                value: in_policy.to_string(),
                            },
                        ],
                    },
                ],
                "cap_denied_capability_intersection",
                json!({
                    "fay_id": stored.metadata.fay_id,
                    "operation": req_cap,
                    "in_buf": in_buf,
                    "in_host_pool": in_pool,
                    "in_fay_policy": in_policy,
                }),
            ));
        }

        // 第 3 道门：按风险等级分流（常规沙箱执行 vs 高风险生成 ICP CAP Approval Card 挂起待批）
        if action.requires_human_confirmation() {
            let action_val = serde_json::to_value(action).unwrap_or_else(|_| json!({}));
            let canonical_params = jcs_canonicalize(&action_val)?;
            let params_sha256 = hex_sha256(canonical_params.as_bytes());
            let entropy = fresh_entropy_32();
            let approval_id = format!("apr_{}", &encode_hex(&entropy)[..12]);
            let reason = format!(
                "High-criticality host driver '{req_cap}' on '{target}' requires explicit Human Prime confirmation."
            );

            let item = CapApprovalItem {
                approval_id: approval_id.clone(),
                fay_id: stored.metadata.fay_id.clone(),
                action: action.clone(),
                required_capability: req_cap.to_owned(),
                criticality: "high".to_owned(),
                params_sha256: params_sha256.clone(),
                status: "pending".to_owned(),
                created_at_unix: unix_now_secs(),
                decided_at_unix: None,
                execution_summary: None,
            };
            self.data.cap_approvals.insert(approval_id.clone(), item);

            return Ok((
                format!("Operation '{req_cap}' held by CAP Gatekeeper pending Human Prime approval ({approval_id})."),
                vec![
                    CtxAnnotation::StatusBadge {
                        label: "CAP Gatekeeper".to_owned(),
                        value: "HOLD_FOR_HUMAN_CONFIRMATION".to_owned(),
                        tone: "warn".to_owned(),
                    },
                    CtxAnnotation::CapApprovalCard {
                        approval_id: approval_id.clone(),
                        fay_id: stored.metadata.fay_id.clone(),
                        operation: req_cap.to_owned(),
                        target,
                        criticality: "high".to_owned(),
                        reason,
                        status: "pending".to_owned(),
                    },
                ],
                "cap_hold_for_human_confirmation",
                json!({
                    "approval_id": approval_id,
                    "fay_id": stored.metadata.fay_id,
                    "operation": req_cap,
                    "params_sha256": params_sha256,
                }),
            ));
        }

        // 常规受控沙箱执行
        let mandate_ticket = stored
            .active_mandate
            .as_ref()
            .map_or("FAYING-ROUTINE-TICKET", |m| m.mandate_id.as_str());
        let (summary, artifact_ann, output_sha256) =
            execute_sandboxed_action(Path::new(&stored.sandbox_dir), action, mandate_ticket)?;

        if let Some(fay_mut) = self.data.fays.get_mut(&stored.metadata.fay_id) {
            fay_mut.pending_sync_deltas = fay_mut.pending_sync_deltas.saturating_add(1);
        }

        Ok((
            summary,
            vec![
                CtxAnnotation::StatusBadge {
                    label: "CAP Gatekeeper".to_owned(),
                    value: "ALLOW_CONTROLLED".to_owned(),
                    tone: "ok".to_owned(),
                },
                artifact_ann,
            ],
            "cap_allow_controlled",
            json!({
                "fay_id": stored.metadata.fay_id,
                "operation": req_cap,
                "target": target,
                "mandate_ticket": mandate_ticket,
                "output_sha256": output_sha256,
            }),
        ))
    }

    fn compute_effective_capabilities(&self, stored: &StoredFayInstance) -> Vec<String> {
        stored
            .requested_capabilities
            .iter()
            .filter(|cap| {
                self.host_pool.granted_capabilities.contains(*cap)
                    && stored.policy_capabilities.contains(*cap)
            })
            .cloned()
            .collect()
    }

    fn to_instance_view(&self, stored: &StoredFayInstance) -> FayInstanceView {
        FayInstanceView {
            instance_id: stored.instance_id.clone(),
            fay_id: stored.metadata.fay_id.clone(),
            display_name: stored.metadata.display_name.clone(),
            kind: stored.metadata.kind,
            buf_path: stored.buf_path.clone(),
            requested_capabilities: stored.requested_capabilities.iter().cloned().collect(),
            policy_capabilities: stored.policy_capabilities.iter().cloned().collect(),
            effective_capabilities: self.compute_effective_capabilities(stored),
            faying_state: stored.faying_state,
            active_mandate: stored.active_mandate.clone(),
            last_rogue_reason: stored.last_rogue_reason.clone(),
            embodiment: stored.embodiment.clone(),
            pending_sync_deltas: stored.pending_sync_deltas,
            sandbox_dir: stored.sandbox_dir.clone(),
            metadata: stored.metadata.clone(),
            chat_history: stored.chat_history.clone(),
            merit: stored
                .merit_pocket
                .to_standing_view(&stored.metadata.fay_id, &self.data.gmchain_endpoint),
        }
    }

    fn collect_tp_context_for_fay(&self, fay_id: &str) -> Vec<(String, String)> {
        let mut items = Vec::new();
        for session in self.data.tp_sessions.values() {
            if session.status == "active"
                && (session.initiator_fay_id == fay_id || session.peer_fay_id == fay_id)
            {
                for entry in &session.shared_entries {
                    items.push((
                        entry.key.clone(),
                        serde_json::to_string(&entry.structured_value)
                            .unwrap_or_else(|_| "{}".to_owned()),
                    ));
                }
            }
        }
        items
    }

    fn update_approval_card_status_in_history(
        &mut self,
        fay_id: &str,
        approval_id: &str,
        new_status: &str,
    ) {
        if let Some(fay) = self.data.fays.get_mut(fay_id) {
            for turn in &mut fay.chat_history {
                for ann in &mut turn.response_ctx.annotations {
                    if let CtxAnnotation::CapApprovalCard {
                        approval_id: id,
                        status,
                        ..
                    } = ann
                    {
                        if id == approval_id {
                            new_status.clone_into(status);
                        }
                    }
                }
            }
        }
    }

    fn resolve_fay_key(&self, id_or_instance: &str) -> Result<String, CliError> {
        let trimmed = id_or_instance.trim();
        if self.data.fays.contains_key(trimmed) {
            return Ok(trimmed.to_owned());
        }
        if let Some((fay_id, _)) = self
            .data
            .fays
            .iter()
            .find(|(_, stored)| stored.instance_id == trimmed)
        {
            return Ok(fay_id.clone());
        }
        Err(CliError::Failure(FaygerError::new(
            ErrorCode::FmValidationFailed,
            Layer::Runtime,
            format!("Fay instance '{trimmed}' is not loaded in FayGer"),
        )))
    }

    fn get_fay(&self, id_or_instance: &str) -> Result<&StoredFayInstance, CliError> {
        let key = self.resolve_fay_key(id_or_instance)?;
        self.data.fays.get(&key).ok_or_else(|| {
            CliError::Failure(FaygerError::new(
                ErrorCode::FmValidationFailed,
                Layer::Runtime,
                format!("Fay instance '{id_or_instance}' is not loaded in FayGer"),
            ))
        })
    }

    fn get_fay_mut(&mut self, id_or_instance: &str) -> Result<&mut StoredFayInstance, CliError> {
        let key = self.resolve_fay_key(id_or_instance)?;
        self.data.fays.get_mut(&key).ok_or_else(|| {
            CliError::Failure(FaygerError::new(
                ErrorCode::FmValidationFailed,
                Layer::Runtime,
                format!("Fay instance '{id_or_instance}' is not loaded in FayGer"),
            ))
        })
    }

    fn append_audit(&mut self, stage: &str, summary: Value) -> Result<String, CliError> {
        let seq = self.data.audit_entries.len() as u64;
        let prev_hash = self
            .data
            .audit_entries
            .last()
            .map_or_else(|| GENESIS_PREV_HASH.to_owned(), |e| e.entry_hash.clone());
        let preimage = json!({
            "prev_hash": prev_hash,
            "seq": seq,
            "stage": stage,
            "summary": summary,
        });
        let canonical = jcs_canonicalize(&preimage)?;
        let entry_hash = hex_sha256(canonical.as_bytes());
        self.data.audit_entries.push(HashChainedAuditEntry {
            seq,
            stage: stage.to_owned(),
            summary,
            prev_hash,
            entry_hash: entry_hash.clone(),
        });
        let _ = AuditChainBuilder::new;
        Ok(entry_hash)
    }

    fn save(&self) -> Result<(), CliError> {
        let state_file = self.state_dir.join("engine_state.json");
        let pretty = serde_json::to_string_pretty(&self.data).map_err(|err| CliError::Io {
            path: state_file.display().to_string(),
            detail: err.to_string(),
        })?;
        fs::write(&state_file, pretty).map_err(|err| CliError::Io {
            path: state_file.display().to_string(),
            detail: err.to_string(),
        })
    }
}

/// 解析聊天消息中的快捷宿主命令（如 `/write note.txt hello`、`/read note.txt`、`/exec workspace.ls`、`/camera desk`）。
#[must_use]
pub fn parse_slash_host_action(message: &str) -> Option<RequestedHostAction> {
    let trimmed = message.trim();
    if let Some(rest) = trimmed.strip_prefix("/read ") {
        let path = rest.trim();
        if !path.is_empty() {
            return Some(RequestedHostAction::FsRead {
                path: path.to_owned(),
            });
        }
    }
    if let Some(rest) = trimmed.strip_prefix("/write ") {
        let (path, content) = rest.trim().split_once(' ')?;
        return Some(RequestedHostAction::FsWrite {
            path: path.trim().to_owned(),
            content: content.to_owned(),
            high_risk: false,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/write-high ") {
        let (path, content) = rest.trim().split_once(' ')?;
        return Some(RequestedHostAction::FsWrite {
            path: path.trim().to_owned(),
            content: content.to_owned(),
            high_risk: true,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/exec ") {
        let cmd = rest.trim();
        if !cmd.is_empty() {
            return Some(RequestedHostAction::ProcessExec {
                command: cmd.to_owned(),
            });
        }
    }
    if let Some(rest) = trimmed.strip_prefix("/camera") {
        let label = rest.trim();
        return Some(RequestedHostAction::CameraSnapshot {
            label: if label.is_empty() {
                "front_desk_preview".to_owned()
            } else {
                label.to_owned()
            },
        });
    }
    None
}

#[allow(clippy::too_many_lines)]
fn execute_sandboxed_action(
    sandbox_dir: &Path,
    action: &RequestedHostAction,
    approval_ticket: &str,
) -> Result<(String, CtxAnnotation, String), CliError> {
    fs::create_dir_all(sandbox_dir).map_err(|err| CliError::Io {
        path: sandbox_dir.display().to_string(),
        detail: err.to_string(),
    })?;

    let host_env = detect_host_environment();
    let host_policy = HostPolicy::allow_all(
        host_env
            .available_capabilities
            .iter()
            .map(|c| c.as_str().to_owned()),
    );
    let mut reg = AdapterRegistry::new();
    reg.register(Box::new(NativeDesktopAdapter::new()));
    reg.register(Box::new(ServerAdapter::new()));
    reg.register(Box::new(BrowserAdapter::new()));
    reg.register(Box::new(InAppAdapter::new()));
    reg.select(&host_env, &host_policy)?;

    let syscall_policy =
        RealSyscallPolicy::controlled_live(true, approval_ticket, Some(sandbox_dir.to_path_buf()));

    match action {
        RequestedHostAction::FsRead { path } => {
            let res = execute_controlled_syscall(
                &reg,
                &RealSyscallRequest::ControlledFsRead {
                    relative_path: path.clone(),
                },
                &syscall_policy,
            )?;
            let content_str = String::from_utf8_lossy(&res.output_bytes).into_owned();
            let preview = truncate_preview(&content_str, 240);
            let sha = res.audit.output_sha256;
            Ok((
                format!(
                    "Read {} bytes from sandbox '{path}'.",
                    res.output_bytes.len()
                ),
                CtxAnnotation::ArtifactPreview {
                    title: "Controlled Sandbox Read".to_owned(),
                    path: path.clone(),
                    content_preview: preview,
                    sha256: sha.clone(),
                },
                sha,
            ))
        }
        RequestedHostAction::FsWrite { path, content, .. } => {
            let res = execute_controlled_syscall(
                &reg,
                &RealSyscallRequest::ControlledFsWrite {
                    relative_path: path.clone(),
                    contents: content.as_bytes().to_vec(),
                },
                &syscall_policy,
            )?;
            let content_sha = hex_sha256(content.as_bytes());
            let _ = res.audit.output_sha256;
            Ok((
                format!("Wrote {} bytes to sandbox '{path}'.", content.len()),
                CtxAnnotation::ArtifactPreview {
                    title: "Controlled Sandbox Write".to_owned(),
                    path: path.clone(),
                    content_preview: truncate_preview(content, 240),
                    sha256: content_sha.clone(),
                },
                content_sha,
            ))
        }
        RequestedHostAction::ProcessExec { command } => {
            let mut entries_list = Vec::new();
            if let Ok(rd) = fs::read_dir(sandbox_dir) {
                for entry in rd.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        entries_list.push(name.to_owned());
                    }
                }
            }
            entries_list.sort();
            let report = format!(
                "controlled_exec={command}\nsandbox_entries={}\nfiles=[{}]\ntimestamp={}\n",
                entries_list.len(),
                entries_list.join(", "),
                unix_now_secs()
            );
            let log_rel = "exec_reports/last_exec.log".to_owned();
            let _ = execute_controlled_syscall(
                &reg,
                &RealSyscallRequest::ControlledFsWrite {
                    relative_path: log_rel.clone(),
                    contents: report.as_bytes().to_vec(),
                },
                &syscall_policy,
            )?;
            let sha = hex_sha256(report.as_bytes());
            Ok((
                format!(
                    "Executed controlled sandbox command '{command}' ({} files visible).",
                    entries_list.len()
                ),
                CtxAnnotation::ArtifactPreview {
                    title: format!("Sandboxed Exec Output ({command})"),
                    path: log_rel,
                    content_preview: report.trim().to_owned(),
                    sha256: sha.clone(),
                },
                sha,
            ))
        }
        RequestedHostAction::CameraSnapshot { label } => {
            let now = unix_now_secs();
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"320\" height=\"180\"><rect width=\"100%\" height=\"100%\" fill=\"#0f172a\"/><text x=\"20\" y=\"80\" fill=\"#38bdf8\" font-family=\"monospace\" font-size=\"14\">FayGer Controlled Camera Frame</text><text x=\"20\" y=\"110\" fill=\"#94a3b8\" font-family=\"monospace\" font-size=\"12\">label={label} ts={now}</text></svg>"
            );
            let rel_path = format!("captures/camera_{now}.svg");
            let _ = execute_controlled_syscall(
                &reg,
                &RealSyscallRequest::ControlledFsWrite {
                    relative_path: rel_path.clone(),
                    contents: svg.as_bytes().to_vec(),
                },
                &syscall_policy,
            )?;
            let sha = hex_sha256(svg.as_bytes());
            Ok((
                format!("Captured controlled camera frame '{rel_path}' (label={label})."),
                CtxAnnotation::ArtifactPreview {
                    title: format!("Controlled Camera Snapshot ({label})"),
                    path: rel_path,
                    content_preview: format!("SVG frame ({} bytes) · label={label}", svg.len()),
                    sha256: sha.clone(),
                },
                sha,
            ))
        }
    }
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        text.to_owned()
    } else {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{FayGerEngine, FayingState, RequestedHostAction};
    use crate::soul::SoulKind;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn enforces_rogue_guard_capability_intersection_brake_cap_approval_and_tp() {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fayger-engine-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut engine = FayGerEngine::open(&dir).expect("open engine");

        // 1. 创建 iFay 与 coFay，初始必为 ROGUE
        let ifay = engine
            .create_and_load_soul(SoulKind::Ifay, "Alice", None)
            .expect("create ifay");
        let cofay = engine
            .create_and_load_soul(SoulKind::Cofay, "ProjectSeat", None)
            .expect("create cofay");
        assert_eq!(ifay.faying_state, FayingState::Rogue);

        // 2. ROGUE 状态下外部文件写入被严格拦截
        let rogue_turn = engine
            .chat_or_act(&ifay.fay_id, "/write memo.txt secret", None)
            .expect("rogue chat");
        assert!(rogue_turn.terminal_text.contains("DENY_ROGUE_FAY"));

        // 3. 拨动 Faying 上岗拨杆 -> 进入 FAYING 态
        let active = engine
            .activate_faying(&ifay.fay_id, Some("G2"), Some(600), Some(120))
            .expect("activate");
        assert_eq!(active.faying_state, FayingState::Faying);

        // 4. 常规受控写入与读取成功
        let write_turn = engine
            .chat_or_act(&ifay.fay_id, "/write memo.txt hello-mvp", None)
            .expect("write ok");
        assert!(write_turn.terminal_text.contains("ALLOW_CONTROLLED"));
        let read_turn = engine
            .chat_or_act(&ifay.fay_id, "/read memo.txt", None)
            .expect("read ok");
        assert!(read_turn.terminal_text.contains("hello-mvp"));

        // 5. 缩减策略使 process.exec 不在交集中 -> 被两层授权链拦截
        let restricted: BTreeSet<String> = ["fs.read".to_owned(), "fs.write".to_owned()]
            .into_iter()
            .collect();
        let updated = engine
            .update_fay_policy(&ifay.fay_id, &restricted)
            .expect("policy");
        assert!(!updated
            .effective_capabilities
            .contains(&"process.exec".to_owned()));
        let denied_exec = engine
            .chat_or_act(&ifay.fay_id, "/exec workspace.ls", None)
            .expect("denied exec");
        assert!(denied_exec
            .terminal_text
            .contains("DENY_CAPABILITY_INTERSECTION"));

        // 6. 恢复 process.exec 策略 -> 高风险操作进入 HOLD_FOR_HUMAN_CONFIRMATION -> 批准后执行
        let with_exec: BTreeSet<String> = [
            "fs.read".to_owned(),
            "fs.write".to_owned(),
            "process.exec".to_owned(),
        ]
        .into_iter()
        .collect();
        engine
            .update_fay_policy(&ifay.fay_id, &with_exec)
            .expect("restore exec");
        let hold_turn = engine
            .chat_or_act(
                &ifay.fay_id,
                "",
                Some(RequestedHostAction::ProcessExec {
                    command: "workspace.ls".to_owned(),
                }),
            )
            .expect("hold exec");
        assert!(hold_turn
            .terminal_text
            .contains("HOLD_FOR_HUMAN_CONFIRMATION"));
        let pending = engine.list_cap_approvals();
        assert_eq!(pending.len(), 1);
        let approved = engine
            .decide_cap_approval(&pending[0].approval_id, true)
            .expect("approve");
        assert_eq!(approved.status, "approved");

        // 7. 触发 G0 红色急刹 -> 立即退回 ROGUE 并拦截后续宿主操作
        let braked = engine
            .trigger_emergency_brake(&ifay.fay_id, Some("Operator clicked Red Brake"))
            .expect("brake");
        assert_eq!(braked.faying_state, FayingState::Rogue);
        let blocked_after_brake = engine
            .chat_or_act(&ifay.fay_id, "/read memo.txt", None)
            .expect("blocked");
        assert!(blocked_after_brake.terminal_text.contains("DENY_ROGUE_FAY"));

        // 8. 本地双 Fay Telepathy (TP) 共享认知空间协作
        let tp = engine
            .create_tp_session(
                &ifay.fay_id,
                &cofay.fay_id,
                "Sprint Release Alignment",
                vec!["read_memory".to_owned(), "write_shared_context".to_owned()],
            )
            .expect("create tp");
        let shared = engine
            .share_tp_entry(
                &tp.session_id,
                &ifay.fay_id,
                "task_context",
                "release_target",
                json!({"version": "0.3.0", "status": "ready"}),
            )
            .expect("share tp");
        assert_eq!(shared.revision, 1);

        let (_, chain_ok) = engine.audit_log();
        assert!(chain_ok, "SHA-256 audit chain must remain tamper-evident");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enforces_meritoken_faying_guard_floor_protection_and_icp_ctx_card() {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fayger-merit-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut engine = FayGerEngine::open(&dir).expect("open engine");

        // Point gmchain_endpoint to a guaranteed closed loopback port so local fallback is deterministic.
        let closed_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let closed_port = closed_listener.local_addr().expect("addr").port();
        drop(closed_listener);
        engine
            .update_merit_config(Some(&format!("http://127.0.0.1:{closed_port}")))
            .expect("update merit config");

        let ifay = engine
            .create_and_load_soul(SoulKind::Ifay, "MeritAlice", None)
            .expect("create ifay");
        assert_eq!(ifay.merit.cur_merit_raw, 5_000_000);
        assert_eq!(ifay.merit.min_merit_floor_raw, 2_718_281);
        assert_eq!(ifay.merit.consumable_merit_raw, 2_281_719);

        // ROGUE allows viewing merit via both fay_id and instance_id, but rejects add & consume.
        let standing_in_rogue = engine
            .get_fay_merit(&ifay.instance_id)
            .expect("view in rogue");
        assert_eq!(standing_in_rogue.cur_merit_raw, 5_000_000);
        assert!(engine
            .add_fay_merit(
                &ifay.instance_id,
                &crate::merit::MeritAddInput {
                    title: "Blocked in Rogue".to_owned(),
                    domain_chain_id: "gmc-open-science".to_owned(),
                    amount_mu: 40,
                    horizon_days: 90,
                    summary: "Should fail".to_owned(),
                }
            )
            .is_err());

        // Switch Faying ON -> add +40 μ -> consume -12 μ voting_priority
        engine
            .activate_faying(&ifay.instance_id, Some("G2"), Some(600), Some(120))
            .expect("activate faying");
        let added = engine
            .add_fay_merit(
                &ifay.instance_id,
                &crate::merit::MeritAddInput {
                    title: "Open Protocol Verification".to_owned(),
                    domain_chain_id: "gmc-open-science".to_owned(),
                    amount_mu: 40,
                    horizon_days: 90,
                    summary: "Verified dataset".to_owned(),
                },
            )
            .expect("add merit");
        assert_eq!(added.standing.cur_merit_raw, 45_000_000);
        let floor_before_consume = added.standing.min_merit_floor_raw;

        let consumed = engine
            .consume_fay_merit(
                &ifay.instance_id,
                &crate::merit::MeritConsumeInput {
                    priority_kind: "voting_priority".to_owned(),
                    amount_raw: 12_000_000,
                    amount_mu_wire: "12".to_owned(),
                    reason: "Weighted vote on prop-001".to_owned(),
                    proposal_id: Some("prop-001".to_owned()),
                },
            )
            .expect("consume merit");
        assert_eq!(consumed.after_cur_merit_raw, 33_000_000);
        assert_eq!(consumed.standing.min_merit_floor_raw, floor_before_consume);

        // Over-consuming past (curMerit - minMerit) is rejected!
        assert!(engine
            .consume_fay_merit(
                &ifay.instance_id,
                &crate::merit::MeritConsumeInput {
                    priority_kind: "voting_priority".to_owned(),
                    amount_raw: 31_000_000,
                    amount_mu_wire: "31".to_owned(),
                    reason: "Attempt to dip into minMerit floor".to_owned(),
                    proposal_id: None,
                },
            )
            .is_err());

        // Chat mentioning 美誉 / μ attaches MeritStandingCard in ICP .ctx
        let chat = engine
            .chat_with_fay(&ifay.fay_id, "查询我的美誉值与优先权额度 μ")
            .expect("chat with merit");
        assert!(chat.terminal_text.contains("MERITOKEN STANDING"));
        assert!(chat.terminal_text.contains("33.00 μ"));

        let (_, verified) = engine.audit_log();
        assert!(verified);
        let _ = fs::remove_dir_all(&dir);
    }
}
