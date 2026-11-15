//! 端到端受控活体 `iFay` 组装与真实任务执行闭环（Wave 2 · `live_ifay`）。
//!
//! 本模块把 `FayGer` 各子系统串成首个端到端真实受控执行流水线：
//! 1. `FayManifest` 装配与依赖/协议声明核验；
//! 2. 生产级 `Faying Mandate` + `CAP 1.0` 签名驱动核验与受控真实执行门控（`production_verified = true`）；
//! 3. `BuF` 加载链摘要与 CRC32 校验，提取结构化任务段 `fayger-task-ir/1.0`；
//! 4. 已签名 `Ego_Package` 双层验签（RFC 8785 JCS + Ed25519 + 按需 Cell SHA-256 + `FidelityGatePolicy`）；
//! 5. 宿主适配器选择、自描述宿主日志通道（JSON Lines）挂载、运行层生命周期驱动与受控沙箱文件读写执行；
//! 6. 逐阶段 SHA-256 防篡改哈希链审计输出（`prev_hash -> entry_hash`）。

use crate::commands::{default_policy, pack_bytes};
use crate::ego_package::{
    encode_hex, hex_sha256, jcs_canonicalize, sign_ego_manifest_value,
    verify_and_mount_ego_package, FidelityGatePolicy, VerifiedEgoMount,
};
use crate::error::CliError;
use crate::host_profile::detect_host_environment;
use ed25519_dalek::{Signer, SigningKey};
use fayger_adapter::{
    execute_controlled_syscall, AdapterRegistry, BrowserAdapter, HostPolicy, InAppAdapter,
    NativeDesktopAdapter, RealSyscallAudit, RealSyscallPolicy, RealSyscallRequest, ServerAdapter,
};
use fayger_cap::{
    canonical_json, evaluate_production_command, AccessMode, CapGrant, ConsequenceIrreversibility,
    Criticality, DomainCategory, FayingGrade, GrantMinGrade, MandateBindingModel,
    ProductionAuditRecord, ProductionDisposition, ProductionEvaluationRequest,
    ProductionMandateEvidence, ProductionOperationDescriptor, ProductionResourceDescriptor,
    ProductionSafeStateDescriptor, RealExecutionGateConfig, SignedDriverManifest, TrustAnchorStore,
};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use fayger_faymanifest::{assemble_str, FayManifest, StubRegistry};
use fayger_loader::load;
use fayger_observability::{EventDraft, EventLevel, HostLogRecord, HostLogSink, HostLogStats};
use fayger_runtime::{LifecycleState, MockAdapter, Runtime, RuntimeInterface};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 结构化确定性任务段 schema 版本。
pub const TASK_IR_SCHEMA_VERSION: &str = "fayger-task-ir/1.0";

/// 端到端受控活体 `iFay` 审计报告 schema 版本。
pub const LIVE_IFAY_AUDIT_SCHEMA_VERSION: &str = "fayger-live-ifay-audit/1.0";

/// 创世审计哈希前驱（64 个 `'0'`）。
pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// 封装在 `BuF` `code` 段中的确定性结构化任务程序。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProgram {
    /// 固定为 [`TASK_IR_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 任务唯一标识。
    pub task_id: String,
    /// 执行该任务需按需校验并挂载的 `Ego_Package` Cell 相对路径列表。
    pub required_cells: Vec<String>,
    /// 按顺序执行的受控任务步骤。
    pub steps: Vec<TaskStep>,
}

/// 单条确定性受控任务步骤。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TaskStep {
    /// 基于已核验挂载的 `Ego` Cell 产出确定性人格化响应摘要（不访问外部云端模型）。
    EgoReply {
        /// 引用的已核验 Cell 相对路径。
        cell: String,
        /// 任务提示标签（不含隐私原文）。
        prompt_tag: String,
        /// 确定性人格化输出负载。
        deterministic_reply: String,
    },
    /// 在受控 `workspace_root` 内写入 UTF-8 文件。
    ControlledFsWrite {
        /// 相对于 `workspace_root` 的受控路径。
        relative_path: String,
        /// 待写入 UTF-8 内容。
        utf8_content: String,
    },
    /// 从受控 `workspace_root` 内读取文件并可选核对期望 SHA-256。
    ControlledFsRead {
        /// 相对于 `workspace_root` 的受控路径。
        relative_path: String,
        /// 可选的期望小写十六进制 SHA-256。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_sha256: Option<String>,
    },
    /// 对给定 UTF-8 负载执行受控宿主 SHA-256 计算。
    CryptoSha256 {
        /// 待计算摘要的 UTF-8 字符串。
        utf8_payload: String,
    },
}

/// 防篡改 SHA-256 哈希链中的单条审计分录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashChainedAuditEntry {
    /// 递增序号（从 0 开始）。
    pub seq: u64,
    /// 流水线阶段标识。
    pub stage: String,
    /// 结构化阶段审计摘要。
    pub summary: Value,
    /// 前一条分录的 `entry_hash`（`seq == 0` 时为 [`GENESIS_PREV_HASH`]）。
    pub prev_hash: String,
    /// 当前分录 `{seq, stage, summary, prev_hash}` 经 JCS 规范化后的 SHA-256。
    pub entry_hash: String,
}

/// 构造并追加防篡改哈希链审计分录的构建器。
#[derive(Debug, Default)]
pub struct AuditChainBuilder {
    entries: Vec<HashChainedAuditEntry>,
}

impl AuditChainBuilder {
    /// 新建空的审计链构建器。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 追加一条阶段审计分录并返回其计算出的 `entry_hash`。
    pub fn append(
        &mut self,
        stage: impl Into<String>,
        summary: Value,
    ) -> Result<String, FaygerError> {
        let seq = self.entries.len() as u64;
        let prev_hash = self.entries.last().map_or_else(
            || GENESIS_PREV_HASH.to_owned(),
            |last| last.entry_hash.clone(),
        );
        let stage_str = stage.into();
        let preimage = json!({
            "prev_hash": prev_hash,
            "seq": seq,
            "stage": stage_str,
            "summary": summary,
        });
        let canonical = jcs_canonicalize(&preimage)?;
        let entry_hash = hex_sha256(canonical.as_bytes());
        self.entries.push(HashChainedAuditEntry {
            seq,
            stage: stage_str,
            summary,
            prev_hash,
            entry_hash: entry_hash.clone(),
        });
        Ok(entry_hash)
    }

    /// 消费构建器并返回完整分录链。
    #[must_use]
    pub fn into_entries(self) -> Vec<HashChainedAuditEntry> {
        self.entries
    }
}

/// 逐项重算并校验一条 [`HashChainedAuditEntry`] 哈希链的防篡改完整性。
#[must_use]
pub fn verify_audit_chain(entries: &[HashChainedAuditEntry]) -> bool {
    let mut expected_prev = GENESIS_PREV_HASH;
    for (idx, entry) in entries.iter().enumerate() {
        if entry.seq != idx as u64 || entry.prev_hash != expected_prev {
            return false;
        }
        let preimage = json!({
            "prev_hash": entry.prev_hash,
            "seq": entry.seq,
            "stage": entry.stage,
            "summary": entry.summary,
        });
        let Ok(canonical) = jcs_canonicalize(&preimage) else {
            return false;
        };
        let computed = hex_sha256(canonical.as_bytes());
        if computed != entry.entry_hash {
            return false;
        }
        expected_prev = &entry.entry_hash;
    }
    true
}

/// 端到端受控活体 `iFay` 执行输入集。
pub struct LiveIfayInputs<'a> {
    /// `FayManifest` JSON 文本。
    pub faymanifest_json: &'a str,
    /// 生产级 CAP 1.0 `ProductionEvaluationRequest` JSON 文本（内含已签名驱动清单与 Mandate）。
    pub cap_request_json: &'a str,
    /// CAP 信任锚公钥映射 JSON 文本（`{"<key_id>": "<64-hex-ed25519-pubkey>"}`）。
    pub trust_anchors_json: &'a str,
    /// 磁盘上的已签名 `Ego_Package` 目录路径。
    pub ego_package_dir: &'a Path,
    /// 受信任的 `iFay` 派生 `Ego` 签名子公钥（64 字符十六进制）。
    pub ego_public_key_hex: &'a str,
    /// 封装了 `fayger-task-ir/1.0` 的 `BuF` 原始字节流。
    pub buf_bytes: &'a [u8],
    /// 受控真实文件操作被严格限制在其中的工作区目录。
    pub workspace_root: &'a Path,
    /// 显式开启受控真实执行的审计批准单号。
    pub audit_approval_ticket: &'a str,
    /// `Ego` 保真度准入策略。
    pub fidelity_policy: &'a FidelityGatePolicy,
}

/// 端到端受控活体 `iFay` 执行产物。
#[derive(Debug, Clone)]
pub struct LiveIfayOutcome {
    /// 应用名称。
    pub app_name: String,
    /// 行动主体 `fay_id`（`ifay_...`）。
    pub fay_id: String,
    /// 选中适配器的平台标识。
    pub platform_id: String,
    /// 宿主日志通道标识。
    pub host_log_channel: String,
    /// 生产 CAP 评估审计记录。
    pub cap_audit: ProductionAuditRecord,
    /// 已验签并挂载的 `Ego_Package` 摘要。
    pub verified_ego: VerifiedEgoMount,
    /// 运行层终态。
    pub final_lifecycle_state: LifecycleState,
    /// 逐步受控宿主调用审计列表。
    pub syscall_audits: Vec<RealSyscallAudit>,
    /// 采集到的宿主日志通道 JSON Lines 条数与计量。
    pub host_log_stats: HostLogStats,
    /// 采集到的宿主日志 JSON Lines 内容。
    pub host_log_lines: Vec<String>,
    /// 完整 SHA-256 防篡改哈希链分录。
    pub audit_chain: Vec<HashChainedAuditEntry>,
    /// 审计链头哈希（最末一条 `entry_hash`）。
    pub chain_head_hash: String,
}

impl LiveIfayOutcome {
    /// 渲染人类可读摘要与机器可读审计信封 JSON。
    pub fn render(&self) -> Result<String, CliError> {
        let envelope = json!({
            "schema_version": LIVE_IFAY_AUDIT_SCHEMA_VERSION,
            "fayger_version": env!("CARGO_PKG_VERSION"),
            "release_channel": "prerelease",
            "app_name": self.app_name,
            "fay_id": self.fay_id,
            "platform_id": self.platform_id,
            "host_log_channel": self.host_log_channel,
            "cap_production_audit": self.cap_audit,
            "ego_mount": self.verified_ego,
            "final_lifecycle_state": format!("{:?}", self.final_lifecycle_state),
            "syscall_audits": self.syscall_audits,
            "host_log_stats": {
                "emitted_count": self.host_log_stats.emitted_count,
                "dropped_overflow_count": self.host_log_stats.dropped_overflow_count,
                "write_failure_count": self.host_log_stats.write_failure_count,
                "buffered_count": self.host_log_stats.buffered_count,
            },
            "audit_chain_verified": verify_audit_chain(&self.audit_chain),
            "chain_head_hash": self.chain_head_hash,
            "audit_chain": self.audit_chain,
        });
        let pretty = serde_json::to_string_pretty(&envelope).map_err(|err| {
            CliError::Failure(
                FaygerError::new(
                    ErrorCode::FmValidationFailed,
                    Layer::Assembler,
                    "failed to serialize live iFay audit envelope",
                )
                .with_context("detail", ContextValue::string(err.to_string())),
            )
        })?;

        let mut text = String::new();
        let _ = writeln!(text, "Live iFay controlled execution report");
        let _ = writeln!(text, "  app:                 {}", self.app_name);
        let _ = writeln!(text, "  fay_id:              {}", self.fay_id);
        let _ = writeln!(text, "  platform_id:         {}", self.platform_id);
        let _ = writeln!(text, "  host_log_channel:    {}", self.host_log_channel);
        let _ = writeln!(
            text,
            "  production_verified: {}",
            self.cap_audit.production_verified
        );
        let _ = writeln!(
            text,
            "  cap_disposition:     {:?}",
            self.cap_audit.disposition
        );
        let _ = writeln!(
            text,
            "  ego_mounted_cells:   {}",
            self.verified_ego.mounted_cells.len()
        );
        let _ = writeln!(text, "  syscalls_executed:   {}", self.syscall_audits.len());
        let _ = writeln!(
            text,
            "  host_logs_emitted:   {}",
            self.host_log_stats.emitted_count
        );
        let _ = writeln!(text, "  chain_head_hash:     {}", self.chain_head_hash);
        let _ = writeln!(text, "\nAudit envelope (JSON)\n{pretty}");
        Ok(text)
    }
}

#[derive(Clone, Default)]
struct SharedJsonlCollector {
    lines: Rc<RefCell<Vec<String>>>,
}

impl HostLogSink for SharedJsonlCollector {
    fn write_record(&mut self, _record: &HostLogRecord, json_line: &str) -> Result<(), String> {
        self.lines.borrow_mut().push(json_line.to_owned());
        Ok(())
    }
}

/// 运行完整的端到端受控活体 `iFay` 组装与真实任务执行闭环。
#[allow(clippy::too_many_lines)]
pub fn execute_live_ifay(inputs: &LiveIfayInputs<'_>) -> Result<LiveIfayOutcome, CliError> {
    let mut chain = AuditChainBuilder::new();

    // 1. FayManifest 装配与关键声明核验
    let assembly_report = assemble_str(inputs.faymanifest_json, &StubRegistry)?;
    let faymanifest = FayManifest::from_json_str(inputs.faymanifest_json)?;
    faymanifest.validate()?;
    let fay_id = faymanifest
        .fay_id
        .as_deref()
        .ok_or_else(|| live_validation_err("FayIdRequiredForLiveExecution", "fayId"))?;
    if !faymanifest.protocols.iter().any(|p| p.id == "cap_protocol") {
        return Err(live_validation_err("CapProtocolNotDeclared", "protocols").into());
    }
    if faymanifest.ego.is_none() {
        return Err(live_validation_err("EgoComponentRequiredForLiveIfay", "ego").into());
    }

    let cap_req: ProductionEvaluationRequest = serde_json::from_str(inputs.cap_request_json)
        .map_err(|err| {
            FaygerError::new(
                ErrorCode::FmParseFail,
                Layer::Assembler,
                "invalid CAP ProductionEvaluationRequest JSON",
            )
            .with_context("detail", ContextValue::string(err.to_string()))
        })?;
    if cap_req.fay_id != fay_id {
        return Err(live_validation_err("CapRequestFayIdMismatch", "fay_id").into());
    }
    if !faymanifest
        .drivers
        .iter()
        .any(|d| d.driver_package == cap_req.manifest.driver_id)
    {
        return Err(
            live_validation_err("DriverPackageNotDeclared", "drivers[].driverPackage").into(),
        );
    }

    chain.append(
        "faymanifest_assembly",
        json!({
            "app_name": faymanifest.name,
            "fay_id": fay_id,
            "resolved_phase": assembly_report.resolved_phase,
            "driver_id": cap_req.manifest.driver_id,
        }),
    )?;

    // 2. 生产级 CAP 1.0 + Faying Mandate 评估与真实执行门控
    let anchors_map: BTreeMap<String, String> = serde_json::from_str(inputs.trust_anchors_json)
        .map_err(|err| {
            FaygerError::new(
                ErrorCode::FmParseFail,
                Layer::Assembler,
                "invalid CAP trust anchors JSON map",
            )
            .with_context("detail", ContextValue::string(err.to_string()))
        })?;
    let mut anchors = TrustAnchorStore::new();
    for (key_id, pk_hex) in &anchors_map {
        anchors.insert_ed25519_hex(key_id.clone(), pk_hex)?;
    }
    let gate_cfg = RealExecutionGateConfig::enable_with_ticket(inputs.audit_approval_ticket)?;
    let cap_audit = evaluate_production_command(&cap_req, &anchors, &gate_cfg);
    if !cap_audit.production_verified
        || cap_audit.disposition != ProductionDisposition::AllowControlledExecution
    {
        return Err(FaygerError::new(
            ErrorCode::AdpCapabilityDenied,
            Layer::Adapter,
            format!(
                "CAP production gate denied live execution: {}",
                cap_audit.reason
            ),
        )
        .with_context("reason", ContextValue::string("CapLiveExecutionDenied"))
        .into());
    }

    chain.append(
        "cap_production_gate",
        json!({
            "request_id": cap_audit.request_id,
            "production_verified": cap_audit.production_verified,
            "disposition": cap_audit.disposition,
            "ticket": cap_audit.real_execution_ticket,
        }),
    )?;

    // 3. BuF 加载校验与结构化任务解析
    let buf_obj = load(inputs.buf_bytes, &default_policy())?;
    let entry_id = buf_obj.manifest.entry.as_str().to_owned();
    let task_section = buf_obj
        .sections
        .iter()
        .find(|s| s.id == entry_id)
        .or_else(|| buf_obj.sections.first())
        .ok_or_else(|| {
            FaygerError::new(
                ErrorCode::LdrMissingRequiredField,
                Layer::Loader,
                "task BuF contains no executable section",
            )
        })?;
    let task_program: TaskProgram =
        serde_json::from_slice(&task_section.payload).map_err(|err| {
            FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "BuF task section is not valid fayger-task-ir/1.0 JSON",
            )
            .with_context("detail", ContextValue::string(err.to_string()))
        })?;
    if task_program.schema_version != TASK_IR_SCHEMA_VERSION {
        return Err(FaygerError::new(
            ErrorCode::LdrSchemaUnsupported,
            Layer::Loader,
            "unsupported BuF task IR schema_version",
        )
        .with_context(
            "schema_version",
            ContextValue::string(task_program.schema_version),
        )
        .into());
    }

    chain.append(
        "buf_load",
        json!({
            "entry": entry_id,
            "task_id": task_program.task_id,
            "step_count": task_program.steps.len(),
        }),
    )?;

    // 4. 已签名 Ego_Package 双层密码学验签与保真度策略准入
    let verified_ego = verify_and_mount_ego_package(
        inputs.ego_package_dir,
        fay_id,
        inputs.ego_public_key_hex,
        &task_program.required_cells,
        inputs.fidelity_policy,
    )?;

    chain.append(
        "ego_package_mount",
        json!({
            "signer_fay_id": verified_ego.signer_fay_id,
            "key_id": verified_ego.key_id,
            "payload_jcs_sha256": verified_ego.payload_jcs_sha256,
            "mounted_cells": verified_ego.mounted_cells,
            "fidelity_status": verified_ego.fidelity_status,
        }),
    )?;

    // 5. 宿主适配器选择、宿主日志通道挂载、运行层生命周期与逐步受控原语执行
    let host_env = detect_host_environment();
    let host_policy = HostPolicy::allow_all(
        host_env
            .available_capabilities
            .iter()
            .map(|c| c.as_str().to_owned()),
    );
    let mut adapter_registry = build_full_adapter_registry();
    adapter_registry.select(&host_env, &host_policy)?;
    let selected_desc = adapter_registry.selected_descriptor().ok_or_else(|| {
        FaygerError::new(
            ErrorCode::AdpNoMatchingPlatform,
            Layer::Adapter,
            "no adapter selected",
        )
    })?;

    let mut rt = Runtime::new(MockAdapter::new());
    let log_collector = SharedJsonlCollector::default();
    let attached_log = adapter_registry.attach_host_log_channel(
        rt.events_mut(),
        Box::new(log_collector.clone()),
        256,
        true,
    );

    let instance_id = rt.load(buf_obj)?;
    rt.init(instance_id)?;
    rt.start(instance_id)?;
    while rt.step(instance_id)? {}

    let syscall_policy = RealSyscallPolicy::controlled_live(
        true,
        inputs.audit_approval_ticket,
        Some(inputs.workspace_root.to_path_buf()),
    );
    let mut syscall_audits = Vec::with_capacity(task_program.steps.len());

    for (step_idx, step) in task_program.steps.iter().enumerate() {
        let audit = execute_task_step(
            step_idx,
            step,
            &verified_ego,
            &adapter_registry,
            &syscall_policy,
            rt.events_mut(),
            instance_id.0,
        )?;
        chain.append(
            "task_step",
            json!({
                "step_index": step_idx,
                "operation": audit.operation,
                "target_summary": audit.target_summary,
                "output_sha256": audit.output_sha256,
            }),
        )?;
        syscall_audits.push(audit);
    }

    rt.terminate(instance_id)?;
    let final_lifecycle_state = rt.state(instance_id)?;

    let host_log_stats = attached_log.map_or(
        HostLogStats {
            emitted_count: 0,
            dropped_overflow_count: 0,
            write_failure_count: 0,
            buffered_count: 0,
        },
        |(_, fwd)| fwd.stats(),
    );
    let host_log_lines = log_collector.lines.borrow().clone();

    let chain_head_hash = chain.append(
        "lifecycle_complete",
        json!({
            "final_state": format!("{final_lifecycle_state:?}"),
            "syscalls_executed": syscall_audits.len(),
            "host_logs_emitted": host_log_stats.emitted_count,
        }),
    )?;
    let audit_chain = chain.into_entries();
    if !verify_audit_chain(&audit_chain) {
        return Err(FaygerError::new(
            ErrorCode::RtInstanceFailed,
            Layer::Runtime,
            "audit hash chain self-verification failed",
        )
        .into());
    }

    Ok(LiveIfayOutcome {
        app_name: faymanifest.name,
        fay_id: fay_id.to_owned(),
        platform_id: selected_desc.platform_id,
        host_log_channel: selected_desc.log_channel.kind.as_str().to_owned(),
        cap_audit,
        verified_ego,
        final_lifecycle_state,
        syscall_audits,
        host_log_stats,
        host_log_lines,
        audit_chain,
        chain_head_hash,
    })
}

fn execute_task_step(
    step_idx: usize,
    step: &TaskStep,
    verified_ego: &VerifiedEgoMount,
    adapter_registry: &AdapterRegistry,
    syscall_policy: &RealSyscallPolicy,
    bus: &mut fayger_observability::EventBus,
    raw_instance_id: u64,
) -> Result<RealSyscallAudit, FaygerError> {
    match step {
        TaskStep::EgoReply {
            cell,
            prompt_tag,
            deterministic_reply,
        } => {
            let cell_digest = verified_ego.mounted_cells.get(cell).ok_or_else(|| {
                FaygerError::new(
                    ErrorCode::FmValidationFailed,
                    Layer::Assembler,
                    "task step referenced an unmounted Ego Cell",
                )
                .with_context("cell", ContextValue::string(cell.clone()))
            })?;
            let composite = format!("{cell_digest}|{prompt_tag}|{deterministic_reply}");
            let res = execute_controlled_syscall(
                adapter_registry,
                &RealSyscallRequest::CryptoSha256 {
                    payload: composite.into_bytes(),
                },
                syscall_policy,
            )?;
            bus.publish(EventDraft::adapter(
                format!("step[{step_idx}] ego_reply cell={cell} prompt={prompt_tag}"),
                EventLevel::Info,
                Some(raw_instance_id),
            ));
            Ok(res.audit)
        }
        TaskStep::ControlledFsWrite {
            relative_path,
            utf8_content,
        } => {
            let res = execute_controlled_syscall(
                adapter_registry,
                &RealSyscallRequest::ControlledFsWrite {
                    relative_path: relative_path.clone(),
                    contents: utf8_content.as_bytes().to_vec(),
                },
                syscall_policy,
            )?;
            bus.publish(EventDraft::adapter(
                format!("step[{step_idx}] fs.write path={relative_path}"),
                EventLevel::Info,
                Some(raw_instance_id),
            ));
            Ok(res.audit)
        }
        TaskStep::ControlledFsRead {
            relative_path,
            expect_sha256,
        } => {
            let res = execute_controlled_syscall(
                adapter_registry,
                &RealSyscallRequest::ControlledFsRead {
                    relative_path: relative_path.clone(),
                },
                syscall_policy,
            )?;
            if let Some(expected) = expect_sha256 {
                let actual = hex_sha256(&res.output_bytes);
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(FaygerError::new(
                        ErrorCode::AdpHostCallFailed,
                        Layer::Adapter,
                        "controlled fs.read content SHA-256 did not match expected digest",
                    )
                    .with_context("reason", ContextValue::string("ReadbackDigestMismatch"))
                    .with_context("expected", ContextValue::string(expected.clone()))
                    .with_context("actual", ContextValue::string(actual)));
                }
            }
            bus.publish(EventDraft::adapter(
                format!("step[{step_idx}] fs.read path={relative_path}"),
                EventLevel::Info,
                Some(raw_instance_id),
            ));
            Ok(res.audit)
        }
        TaskStep::CryptoSha256 { utf8_payload } => {
            let res = execute_controlled_syscall(
                adapter_registry,
                &RealSyscallRequest::CryptoSha256 {
                    payload: utf8_payload.as_bytes().to_vec(),
                },
                syscall_policy,
            )?;
            bus.publish(EventDraft::adapter(
                format!("step[{step_idx}] crypto.hash sha256"),
                EventLevel::Info,
                Some(raw_instance_id),
            ));
            Ok(res.audit)
        }
    }
}

fn build_full_adapter_registry() -> AdapterRegistry {
    let mut reg = AdapterRegistry::new();
    reg.register(Box::new(NativeDesktopAdapter::new()));
    reg.register(Box::new(ServerAdapter::new()));
    reg.register(Box::new(BrowserAdapter::new()));
    reg.register(Box::new(InAppAdapter::new()));
    reg
}

static DEMO_LIVE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 运行内置的一键受控活体 `iFay` 端到端闭环演示（`fayger demo live-ifay`）。
///
/// 在系统临时目录创建真实受控沙箱，完成：
/// `FayManifest` 装配 → 生产级 CAP 1.0 验签与门控 → 已签名 `Ego_Package` 双层验签 →
/// `BuF` 封装加载 → 受控沙箱真实文件写入与读回校验 → 宿主日志通道输出 → 防篡改哈希链校验，
/// 并在返回前清理临时目录。
#[allow(clippy::too_many_lines)]
pub fn demo_live_ifay_command() -> Result<String, CliError> {
    let seq = DEMO_LIVE_SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "fayger-demo-live-ifay-{}-{seq}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let ego_dir = root.join("ego_package");
    let ws_dir = root.join("controlled_workspace");
    fs::create_dir_all(ego_dir.join("normal/adapters")).map_err(|err| CliError::Io {
        path: ego_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    fs::create_dir_all(&ws_dir).map_err(|err| CliError::Io {
        path: ws_dir.display().to_string(),
        detail: err.to_string(),
    })?;

    let result = run_demo_in_sandbox(&ego_dir, &ws_dir);
    let _ = fs::remove_dir_all(&root);
    result
}

#[allow(clippy::too_many_lines)]
fn run_demo_in_sandbox(ego_dir: &Path, ws_dir: &Path) -> Result<String, CliError> {
    let fay_id = "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f";
    let driver_id = "@ifay-drivers/workspace-guardian";
    let cap_signer_key_id = "cap-anchor-2026-01";
    let ticket = "WAVE2-LIVE-TICKET-0001";

    let faymanifest_json = json!({
        "name": "workspace-guardian-ifay",
        "version": "1.0.0",
        "description": "End-to-end live iFay controlled workspace execution slice",
        "vendor": "iFay System",
        "fayId": fay_id,
        "ego": {
            "modelSource": "@ego-models/prime-guardian-v1",
            "scenarioTags": ["workspace", "normal"]
        },
        "modules": [
            { "id": "workspace_io", "version": "^1.0.0" }
        ],
        "protocols": [
            { "id": "cap_protocol", "version": "1.0.0" },
            { "id": "faying_protocol", "version": "1.0.0" }
        ],
        "controlMode": "command",
        "drivers": [
            {
                "name": "workspace-guardian",
                "type": "host_workspace",
                "driverPackage": driver_id
            }
        ]
    })
    .to_string();

    // 生成 CAP 信任锚密钥并签名 SignedDriverManifest
    let cap_signing_key = SigningKey::from_bytes(&[0x19; 32]);
    let cap_pubkey_hex = encode_hex(&cap_signing_key.verifying_key().to_bytes());
    let mut signed_driver = SignedDriverManifest {
        manifest_id: "drv-manifest-ws-guardian-1.0".to_owned(),
        driver_id: driver_id.to_owned(),
        version: "1.0.0".to_owned(),
        issued_at: "2026-11-15T00:00:00Z".to_owned(),
        signer_key_id: cap_signer_key_id.to_owned(),
        resources: vec![ProductionResourceDescriptor {
            resource_id: "workspace_sandbox".to_owned(),
            safe_states: vec![ProductionSafeStateDescriptor {
                safe_state_id: "lock_workspace_readonly".to_owned(),
                description: Some("Freeze sandbox writes and preserve audit log.".to_owned()),
            }],
            operations: vec![ProductionOperationDescriptor {
                operation_id: "write_status_artifact".to_owned(),
                mode: AccessMode::Write,
                criticality: Criticality::Sensitive,
                consequence_irreversibility: ConsequenceIrreversibility::BoundedCompensation,
                domain_category: Some(DomainCategory::Storage),
            }],
        }],
        signature_ed25519: None,
    };
    let unsigned_driver_val = serde_json::to_value(&signed_driver).map_err(|err| {
        CliError::Failure(
            live_validation_err("DriverSerializeFailed", "manifest")
                .with_context("detail", ContextValue::string(err.to_string())),
        )
    })?;
    let driver_sig = cap_signing_key.sign(canonical_json(&unsigned_driver_val).as_bytes());
    signed_driver.signature_ed25519 = Some(encode_hex(&driver_sig.to_bytes()));

    let cap_req = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: format!("live::{fay_id}::workspace_sandbox::write_status_artifact"),
        fay_id: fay_id.to_owned(),
        resource_id: "workspace_sandbox".to_owned(),
        operation_id: "write_status_artifact".to_owned(),
        requested_mode: AccessMode::Write,
        params: json!({"target": "outbox/ifay-live-status.json"}),
        domain_category: Some(DomainCategory::Storage),
        caller_criticality_override: None,
        g0_override: false,
        now_epoch_ms: 1_794_700_000_000,
        manifest: signed_driver,
        grant: CapGrant {
            grant_id: "grant-ws-01".to_owned(),
            resource_id: "workspace_sandbox".to_owned(),
            allowed_modes: vec![AccessMode::Read, AccessMode::Write],
            min_grade: GrantMinGrade::G2,
            criticality_override: None,
            mandate_binding: ProductionMandateEvidence {
                mandate_id: "mandate-live-ws-01".to_owned(),
                fay_id: fay_id.to_owned(),
                resource_id: "workspace_sandbox".to_owned(),
                allowed_modes: vec![AccessMode::Read, AccessMode::Write],
                session_grade: FayingGrade::G1,
                binding_model: MandateBindingModel::ModelAPlusB,
                online_fresh: true,
                revoked: false,
                revocation_checked_at_ms: 1_794_699_995_000,
                max_revocation_staleness_ms: 60_000,
                expires_at_epoch_ms: 1_794_703_600_000,
                model_a_derived_proof_valid: true,
            },
        },
        human_confirmation: None,
    };
    let cap_request_json = serde_json::to_string(&cap_req).map_err(|err| {
        CliError::Failure(
            live_validation_err("CapRequestSerializeFailed", "request")
                .with_context("detail", ContextValue::string(err.to_string())),
        )
    })?;
    let trust_anchors_json = json!({
        (cap_signer_key_id): cap_pubkey_hex
    })
    .to_string();

    // 生成双层签名 Ego_Package
    let cell_rel = "normal/adapters/ego-lora.safetensors";
    let cell_bytes = b"PEFT_SAFETENSORS_WAVE2_LIVE_CELL";
    fs::write(ego_dir.join(cell_rel), cell_bytes).map_err(|err| CliError::Io {
        path: ego_dir.join(cell_rel).display().to_string(),
        detail: err.to_string(),
    })?;
    let ego_signing_key = SigningKey::from_bytes(&[0x55; 32]);
    let mut ego_manifest = json!({
        "version": "1.0",
        "created_at": "2026-11-15T12:00:00Z",
        "base_model": "Qwen/Qwen2.5-0.5B-Instruct",
        "editions": ["normal"],
        "training_backend": "mlx-lora",
        "fidelity_report": {
            "schema_version": "1.0",
            "status": "measured",
            "holdout_sample_count": 16,
            "holdout_loss": 1.08,
            "holdout_perplexity": 2.94,
            "cell_consistency_score": 0.94,
            "probe_win_rate": 0.88,
            "drift_from_baseline": 0.02
        },
        "integrity": {
            "schema_version": "1.0",
            "base_model_digest": "unverified_offline",
            "cell_digests": {
                (cell_rel): format!("sha256:{}", hex_sha256(cell_bytes))
            }
        }
    });
    let ego_pubkey_hex = sign_ego_manifest_value(
        &mut ego_manifest,
        &ego_signing_key,
        fay_id,
        "fayid-ego-subkey-v1",
    )?;
    fs::write(
        ego_dir.join("manifest.json"),
        serde_json::to_string_pretty(&ego_manifest).unwrap_or_default(),
    )
    .map_err(|err| CliError::Io {
        path: ego_dir.join("manifest.json").display().to_string(),
        detail: err.to_string(),
    })?;

    // 构造结构化任务程序并打包为密封 BuF
    let artifact_content = "{\"status\":\"ok\",\"agent\":\"workspace-guardian-ifay\"}\n";
    let expected_artifact_sha = hex_sha256(artifact_content.as_bytes());
    let program = TaskProgram {
        schema_version: TASK_IR_SCHEMA_VERSION.to_owned(),
        task_id: "task-live-ifay-001".to_owned(),
        required_cells: vec![cell_rel.to_owned()],
        steps: vec![
            TaskStep::EgoReply {
                cell: cell_rel.to_owned(),
                prompt_tag: "daily_workspace_check".to_owned(),
                deterministic_reply: "Confirmed controlled workspace status artifact generation."
                    .to_owned(),
            },
            TaskStep::ControlledFsWrite {
                relative_path: "outbox/ifay-live-status.json".to_owned(),
                utf8_content: artifact_content.to_owned(),
            },
            TaskStep::ControlledFsRead {
                relative_path: "outbox/ifay-live-status.json".to_owned(),
                expect_sha256: Some(expected_artifact_sha),
            },
        ],
    };
    let program_bytes = serde_json::to_vec(&program).map_err(|err| {
        CliError::Failure(
            live_validation_err("TaskProgramSerializeFailed", "program")
                .with_context("detail", ContextValue::string(err.to_string())),
        )
    })?;
    let buf_bytes = pack_bytes(
        "main",
        &[("main".to_owned(), "code".to_owned(), program_bytes)],
    )?;

    let outcome = execute_live_ifay(&LiveIfayInputs {
        faymanifest_json: &faymanifest_json,
        cap_request_json: &cap_request_json,
        trust_anchors_json: &trust_anchors_json,
        ego_package_dir: ego_dir,
        ego_public_key_hex: &ego_pubkey_hex,
        buf_bytes: &buf_bytes,
        workspace_root: ws_dir,
        audit_approval_ticket: ticket,
        fidelity_policy: &FidelityGatePolicy::production_default(),
    })?;

    outcome.render()
}

fn live_validation_err(reason: &'static str, field: &'static str) -> FaygerError {
    FaygerError::new(
        ErrorCode::FmValidationFailed,
        Layer::Assembler,
        "live iFay execution validation failed",
    )
    .with_context("reason", ContextValue::string(reason))
    .with_context("field", ContextValue::string(field))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{demo_live_ifay_command, verify_audit_chain, AuditChainBuilder, GENESIS_PREV_HASH};
    use serde_json::json;

    #[test]
    fn audit_hash_chain_detects_any_tampering() {
        let mut builder = AuditChainBuilder::new();
        let h0 = builder
            .append("faymanifest_assembly", json!({"ok": true}))
            .expect("h0");
        let h1 = builder
            .append("cap_production_gate", json!({"verified": true}))
            .expect("h1");
        let mut entries = builder.into_entries();
        assert_eq!(entries[0].prev_hash, GENESIS_PREV_HASH);
        assert_eq!(entries[0].entry_hash, h0);
        assert_eq!(entries[1].prev_hash, h0);
        assert_eq!(entries[1].entry_hash, h1);
        assert!(verify_audit_chain(&entries));

        entries[0].summary = json!({"ok": false});
        assert!(!verify_audit_chain(&entries));
    }

    #[test]
    fn demo_live_ifay_executes_full_authenticated_pipeline() {
        let report = demo_live_ifay_command().expect("live-ifay demo");
        assert!(report.contains("Live iFay controlled execution report"));
        assert!(report.contains("production_verified: true"));
        assert!(report.contains("AllowControlledExecution"));
        assert!(report.contains("\"audit_chain_verified\": true"));
        assert!(report.contains("syscalls_executed:   3"));
    }
}
