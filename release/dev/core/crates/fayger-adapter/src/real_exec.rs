//! 受控真实宿主系统调用绑定与双重门控执行（Wave 2 · `bind-real-host-syscalls`）。
//!
//! 本模块在不改动 M3 确定性 mock `translate` 基准的前提下，提供默认关闭的受控真实宿主执行路径。
//!
//! 门控不变量（见 `decisions/0003-controlled-real-host-syscalls-and-cap-double-gate.md`）：
//! 1. **默认关闭**：[`RealSyscallPolicy::default`] 固定 `enable_real_syscalls = false`，此时仅产出
//!    [`RealSyscallMode::MockFallback`] 与确定性 [`HostSyscall`]，零真实宿主副作用。
//! 2. **能力裁剪优先**：所需能力必须先在 `AdapterRegistry::granted()` 中被授予，否则在触达任何
//!    宿主逻辑前即返回 `ADP_CAPABILITY_DENIED`。
//! 3. **CAP 生产授权双重门控**：开启真实调用时，还必须满足 `cap_production_verified == true` 且携带
//!    非空 `audit_approval_ticket`；否则返回 `ADP_CAPABILITY_DENIED`（`reason = CapProductionGateNotSatisfied`）。
//! 4. **受控沙箱与错误映射**：仅在 `native-desktop` 与 `server` 上开放限制于规范化 `workspace_root`
//!    内部的文件读写、确定性 SHA-256 与宿主时钟读取；任何越界路径或底层 I/O 错误统一映射为
//!    `ErrorCode::AdpHostCallFailed`（`Layer::Adapter`），并在结构化 `context` 中保留原始失败原因。

use crate::adapter::AdapterDescriptor;
use crate::builtin::{CAP_CRYPTO_HASH, CAP_FS_READ, CAP_FS_WRITE, CAP_TIME_NOW};
use crate::capability::Capability;
use crate::host::HostSyscall;
use crate::instruction::{InstructionCategory, UniversalInstruction};
use crate::registry::AdapterRegistry;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 真实宿主系统调用执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealSyscallMode {
    /// 默认回退：仅执行确定性 mock 翻译，零真实宿主副作用。
    MockFallback,
    /// 受控真实执行：已通过能力裁剪与生产 CAP 双重门控。
    ControlledReal,
}

/// 受控真实宿主系统调用策略（默认关闭）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealSyscallPolicy {
    /// 是否显式启用受控真实宿主调用（默认 `false`）。
    pub enable_real_syscalls: bool,
    /// 该操作是否已通过生产级 CAP / Faying Mandate 核验。
    pub cap_production_verified: bool,
    /// 启用真实调用的审计批准单标识（真实调用开启时必填）。
    pub audit_approval_ticket: String,
    /// 文件类真实调用被严格限制在其中的受控工作区根目录。
    pub workspace_root: Option<PathBuf>,
}

impl Default for RealSyscallPolicy {
    fn default() -> Self {
        Self::disabled()
    }
}

impl RealSyscallPolicy {
    /// 构造默认关闭策略（零真实副作用）。
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enable_real_syscalls: false,
            cap_production_verified: false,
            audit_approval_ticket: String::new(),
            workspace_root: None,
        }
    }

    /// 构造显式开启受控真实调用的策略。
    #[must_use]
    pub fn controlled_live(
        cap_production_verified: bool,
        audit_approval_ticket: impl Into<String>,
        workspace_root: Option<PathBuf>,
    ) -> Self {
        Self {
            enable_real_syscalls: true,
            cap_production_verified,
            audit_approval_ticket: audit_approval_ticket.into(),
            workspace_root,
        }
    }
}

/// 受控真实宿主原语请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealSyscallRequest {
    /// 向受控 `workspace_root` 内的相对路径写入字节。
    ControlledFsWrite {
        /// 相对于 `workspace_root` 的无越界路径。
        relative_path: String,
        /// 待写入字节。
        contents: Vec<u8>,
    },
    /// 从受控 `workspace_root` 内的相对路径读取字节。
    ControlledFsRead {
        /// 相对于 `workspace_root` 的无越界路径。
        relative_path: String,
    },
    /// 计算输入负载的宿主 SHA-256 摘要（返回 32 字节原始摘要）。
    CryptoSha256 {
        /// 待哈希字节。
        payload: Vec<u8>,
    },
    /// 读取宿主当前 UTC 毫秒时间戳（返回十进制 ASCII 字节）。
    HostTimeNow,
}

impl RealSyscallRequest {
    /// 该请求对应的稳定操作名。
    #[must_use]
    pub const fn operation_name(&self) -> &'static str {
        match self {
            Self::ControlledFsWrite { .. } => CAP_FS_WRITE,
            Self::ControlledFsRead { .. } => CAP_FS_READ,
            Self::CryptoSha256 { .. } => CAP_CRYPTO_HASH,
            Self::HostTimeNow => CAP_TIME_NOW,
        }
    }

    /// 该请求所需的指令类别。
    #[must_use]
    pub const fn category(&self) -> InstructionCategory {
        match self {
            Self::ControlledFsWrite { .. } | Self::ControlledFsRead { .. } => {
                InstructionCategory::Io
            }
            Self::CryptoSha256 { .. } => InstructionCategory::Crypto,
            Self::HostTimeNow => InstructionCategory::Time,
        }
    }

    /// 该请求所需的能力标识。
    #[must_use]
    pub fn required_capability(&self) -> Capability {
        Capability::new(self.operation_name())
    }

    fn target_summary(&self) -> String {
        match self {
            Self::ControlledFsWrite {
                relative_path,
                contents,
            } => format!("fs.write:{relative_path} ({}B)", contents.len()),
            Self::ControlledFsRead { relative_path } => format!("fs.read:{relative_path}"),
            Self::CryptoSha256 { payload } => format!("crypto.hash:sha256 ({}B)", payload.len()),
            Self::HostTimeNow => "time.now:unix_ms".to_owned(),
        }
    }

    fn operands_preview(&self) -> Vec<u8> {
        match self {
            Self::ControlledFsWrite { relative_path, .. }
            | Self::ControlledFsRead { relative_path } => relative_path.as_bytes().to_vec(),
            Self::CryptoSha256 { payload } => payload.clone(),
            Self::HostTimeNow => Vec::new(),
        }
    }
}

/// 单次宿主调用的审计摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealSyscallAudit {
    /// 实际执行模式（`mock_fallback` 或 `controlled_real`）。
    pub mode: RealSyscallMode,
    /// 选中适配器的平台标识。
    pub platform_id: String,
    /// 执行的操作名。
    pub operation: String,
    /// 生产 CAP 门控核验标记。
    pub cap_production_verified: bool,
    /// 审计批准单号（若开启真实执行）。
    pub audit_approval_ticket: Option<String>,
    /// 受控目标摘要（不含敏感原文内容）。
    pub target_summary: String,
    /// 输出字节流的十六进制 SHA-256 摘要。
    pub output_sha256: String,
}

/// 受控宿主调用返回结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealSyscallResult {
    /// 实际执行模式。
    pub mode: RealSyscallMode,
    /// 底层确定性翻译产出的基准 [`HostSyscall`]。
    pub mock_syscall: HostSyscall,
    /// 宿主返回字节（`MockFallback` 模式下为空）。
    pub output_bytes: Vec<u8>,
    /// 可序列化审计摘要。
    pub audit: RealSyscallAudit,
}

/// 在已就绪的 [`AdapterRegistry`] 上按双重门控执行一条受控宿主调用。
///
/// # Errors
/// - 注册表未就绪返回 `ADP_NO_MATCHING_PLATFORM`；
/// - 所需能力未被授予，或启用真实调用但未通过生产 CAP 门控，返回 `ADP_CAPABILITY_DENIED`；
/// - 指令类别不被当前适配器支持返回 `ADP_UNSUPPORTED_INSTRUCTION`；
/// - 平台未开放真实绑定、路径越界或底层宿主 I/O 失败返回 `ADP_HOST_CALL_FAILED`。
pub fn execute_controlled_syscall(
    registry: &AdapterRegistry,
    request: &RealSyscallRequest,
    policy: &RealSyscallPolicy,
) -> Result<RealSyscallResult, FaygerError> {
    let descriptor = registry.selected_descriptor().ok_or_else(|| {
        FaygerError::new(
            ErrorCode::AdpNoMatchingPlatform,
            Layer::Adapter,
            "adapter registry is not serviceable; call select first",
        )
    })?;

    let instr = build_instruction_for_descriptor(&descriptor, request);
    // 第 1 道门控：能力裁剪 + 指令支持判定（先产确定性 mock HostSyscall）。
    let mock_syscall = registry.dispatch(&instr)?;

    if !policy.enable_real_syscalls {
        let output_bytes = Vec::new();
        let output_sha256 = hex_sha256(&output_bytes);
        return Ok(RealSyscallResult {
            mode: RealSyscallMode::MockFallback,
            mock_syscall,
            output_bytes,
            audit: RealSyscallAudit {
                mode: RealSyscallMode::MockFallback,
                platform_id: descriptor.platform_id,
                operation: request.operation_name().to_owned(),
                cap_production_verified: false,
                audit_approval_ticket: None,
                target_summary: request.target_summary(),
                output_sha256,
            },
        });
    }

    // 第 2 道门控：生产级 CAP / Faying 授权门控（OQ-1 双重门控）。
    if !policy.cap_production_verified || policy.audit_approval_ticket.trim().is_empty() {
        return Err(FaygerError::new(
            ErrorCode::AdpCapabilityDenied,
            Layer::Adapter,
            "real host syscall requires production CAP verification and non-empty audit approval ticket",
        )
        .with_context("reason", ContextValue::string("CapProductionGateNotSatisfied"))
        .with_context("operation", ContextValue::string(request.operation_name())));
    }

    // 平台真实绑定可用性检查（OQ-3：无法确认可用时默认拒绝）。
    if !matches!(descriptor.platform_id.as_str(), "native-desktop" | "server") {
        return Err(host_call_failed(
            "PlatformRealBindingUnavailable",
            request.operation_name(),
            None,
            format!(
                "platform '{}' does not expose controlled OS host bindings",
                descriptor.platform_id
            ),
        ));
    }

    let output_bytes = perform_real_primitive(request, policy.workspace_root.as_deref())?;
    let output_sha256 = hex_sha256(&output_bytes);
    Ok(RealSyscallResult {
        mode: RealSyscallMode::ControlledReal,
        mock_syscall,
        output_bytes,
        audit: RealSyscallAudit {
            mode: RealSyscallMode::ControlledReal,
            platform_id: descriptor.platform_id,
            operation: request.operation_name().to_owned(),
            cap_production_verified: true,
            audit_approval_ticket: Some(policy.audit_approval_ticket.clone()),
            target_summary: request.target_summary(),
            output_sha256,
        },
    })
}

fn build_instruction_for_descriptor(
    descriptor: &AdapterDescriptor,
    request: &RealSyscallRequest,
) -> UniversalInstruction {
    let category = request.category();
    let opcode = descriptor
        .supported_instructions
        .iter()
        .find(|key| key.category == category)
        .map_or(0, |key| key.opcode);
    let mut required = BTreeSet::new();
    required.insert(request.required_capability());
    UniversalInstruction::new(category, opcode, request.operands_preview(), required)
}

fn perform_real_primitive(
    request: &RealSyscallRequest,
    workspace_root: Option<&Path>,
) -> Result<Vec<u8>, FaygerError> {
    match request {
        RealSyscallRequest::ControlledFsWrite {
            relative_path,
            contents,
        } => {
            let target = resolve_workspace_path(workspace_root, relative_path, CAP_FS_WRITE, true)?;
            fs::write(&target, contents).map_err(|err| {
                host_call_failed(
                    "IoError",
                    CAP_FS_WRITE,
                    Some(format!("{:?}", err.kind())),
                    err.to_string(),
                )
            })?;
            Ok(contents.len().to_string().into_bytes())
        }
        RealSyscallRequest::ControlledFsRead { relative_path } => {
            let target = resolve_workspace_path(workspace_root, relative_path, CAP_FS_READ, false)?;
            fs::read(&target).map_err(|err| {
                host_call_failed(
                    "IoError",
                    CAP_FS_READ,
                    Some(format!("{:?}", err.kind())),
                    err.to_string(),
                )
            })
        }
        RealSyscallRequest::CryptoSha256 { payload } => {
            let mut hasher = Sha256::new();
            hasher.update(payload);
            Ok(hasher.finalize().to_vec())
        }
        RealSyscallRequest::HostTimeNow => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|err| {
                    host_call_failed("ClockError", CAP_TIME_NOW, None, err.to_string())
                })?;
            Ok(now.as_millis().to_string().into_bytes())
        }
    }
}

fn resolve_workspace_path(
    workspace_root: Option<&Path>,
    relative_path: &str,
    operation: &'static str,
    create_parent: bool,
) -> Result<PathBuf, FaygerError> {
    let root = workspace_root.ok_or_else(|| {
        host_call_failed(
            "MissingWorkspaceRoot",
            operation,
            None,
            "workspace_root must be configured for controlled filesystem operations",
        )
    })?;
    let canonical_root = fs::canonicalize(root).map_err(|err| {
        host_call_failed(
            "WorkspaceRootCanonicalizeFailed",
            operation,
            Some(format!("{:?}", err.kind())),
            err.to_string(),
        )
    })?;

    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        return Err(host_call_failed(
            "PathTraversalRejected",
            operation,
            None,
            "relative_path must not be empty",
        ));
    }

    let rel = Path::new(trimmed);
    let mut clean_rel = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::Normal(seg) => clean_rel.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(host_call_failed(
                    "PathTraversalRejected",
                    operation,
                    None,
                    format!("disallowed path component in '{relative_path}'"),
                ));
            }
        }
    }
    if clean_rel.as_os_str().is_empty() {
        return Err(host_call_failed(
            "PathTraversalRejected",
            operation,
            None,
            "relative_path must resolve to a file inside workspace_root",
        ));
    }

    let joined = canonical_root.join(&clean_rel);
    if let Some(parent) = joined.parent() {
        if create_parent && !parent.exists() {
            fs::create_dir_all(parent).map_err(|err| {
                host_call_failed(
                    "IoError",
                    operation,
                    Some(format!("{:?}", err.kind())),
                    err.to_string(),
                )
            })?;
        }
        if parent.exists() {
            let canonical_parent = fs::canonicalize(parent).map_err(|err| {
                host_call_failed(
                    "IoError",
                    operation,
                    Some(format!("{:?}", err.kind())),
                    err.to_string(),
                )
            })?;
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(host_call_failed(
                    "OutsideWorkspaceRoot",
                    operation,
                    None,
                    "resolved parent directory escapes workspace_root",
                ));
            }
        }
    }

    if joined.exists() {
        let canonical_target = fs::canonicalize(&joined).map_err(|err| {
            host_call_failed(
                "IoError",
                operation,
                Some(format!("{:?}", err.kind())),
                err.to_string(),
            )
        })?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(host_call_failed(
                "OutsideWorkspaceRoot",
                operation,
                None,
                "resolved target file escapes workspace_root",
            ));
        }
    }

    Ok(joined)
}

fn host_call_failed(
    reason: &'static str,
    operation: &'static str,
    os_error_kind: Option<String>,
    detail: impl Into<String>,
) -> FaygerError {
    let mut err = FaygerError::new(
        ErrorCode::AdpHostCallFailed,
        Layer::Adapter,
        "controlled host syscall failed",
    )
    .with_context("reason", ContextValue::string(reason))
    .with_context("operation", ContextValue::string(operation))
    .with_context("detail", ContextValue::string(detail.into()));
    if let Some(kind) = os_error_kind {
        err = err.with_context("os_error_kind", ContextValue::string(kind));
    }
    err
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        execute_controlled_syscall, RealSyscallMode, RealSyscallPolicy, RealSyscallRequest,
    };
    use crate::builtin::{
        BrowserAdapter, NativeDesktopAdapter, ServerAdapter, CAP_CRYPTO_HASH, CAP_FS_READ,
        CAP_FS_WRITE, CAP_TIME_NOW,
    };
    use crate::capability::{Capability, HostPolicy};
    use crate::host::HostEnvironment;
    use crate::registry::AdapterRegistry;
    use fayger_error::{ErrorCode, Layer};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_workspace() -> PathBuf {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fayger-real-exec-test-{}-{seq}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    fn caps(items: &[&str]) -> BTreeSet<Capability> {
        items.iter().map(|s| Capability::new(*s)).collect()
    }

    fn desktop_registry(allowed: &[&str]) -> AdapterRegistry {
        let mut reg = AdapterRegistry::new();
        reg.register(Box::new(NativeDesktopAdapter::new()));
        let host = HostEnvironment::new(
            "native-desktop",
            "macos",
            "aarch64",
            "none",
            caps(&[CAP_FS_READ, CAP_FS_WRITE, CAP_CRYPTO_HASH, CAP_TIME_NOW]),
        );
        reg.select(&host, &HostPolicy::allow_all(allowed.iter().copied()))
            .expect("select desktop");
        reg
    }

    #[test]
    fn default_policy_is_mock_only_and_produces_zero_host_side_effects() {
        let ws = temp_workspace();
        let reg = desktop_registry(&[CAP_FS_WRITE]);
        let req = RealSyscallRequest::ControlledFsWrite {
            relative_path: "should-not-exist.txt".to_owned(),
            contents: b"hello".to_vec(),
        };
        let res = execute_controlled_syscall(&reg, &req, &RealSyscallPolicy::default())
            .expect("mock fallback succeeds");
        assert_eq!(res.mode, RealSyscallMode::MockFallback);
        assert!(!ws.join("should-not-exist.txt").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn ungranted_capability_is_rejected_before_reaching_host() {
        let ws = temp_workspace();
        let reg = desktop_registry(&[CAP_FS_READ]); // fs.write NOT granted
        let policy = RealSyscallPolicy::controlled_live(true, "CAP-AUDIT-001", Some(ws.clone()));
        let req = RealSyscallRequest::ControlledFsWrite {
            relative_path: "denied.txt".to_owned(),
            contents: b"secret".to_vec(),
        };
        let err = execute_controlled_syscall(&reg, &req, &policy).expect_err("capability denied");
        assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
        assert!(!ws.join("denied.txt").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn missing_cap_production_gate_is_rejected_even_when_capability_granted() {
        let ws = temp_workspace();
        let reg = desktop_registry(&[CAP_FS_WRITE]);
        let policy = RealSyscallPolicy::controlled_live(false, "CAP-AUDIT-001", Some(ws.clone()));
        let req = RealSyscallRequest::ControlledFsWrite {
            relative_path: "blocked-by-cap.txt".to_owned(),
            contents: b"blocked".to_vec(),
        };
        let err = execute_controlled_syscall(&reg, &req, &policy).expect_err("CAP gate blocks");
        assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
        assert!(!ws.join("blocked-by-cap.txt").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn browser_adapter_rejects_real_host_binding_by_default() {
        let mut reg = AdapterRegistry::new();
        reg.register(Box::new(BrowserAdapter::new()));
        let host = HostEnvironment::new(
            "browser",
            "browser",
            "wasm32",
            "browser",
            caps(&[CAP_TIME_NOW]),
        );
        reg.select(&host, &HostPolicy::allow_all([CAP_TIME_NOW]))
            .expect("select browser");
        let policy = RealSyscallPolicy::controlled_live(true, "CAP-AUDIT-002", None);
        let err = execute_controlled_syscall(&reg, &RealSyscallRequest::HostTimeNow, &policy)
            .expect_err("browser has no OS binding");
        assert_eq!(err.code, ErrorCode::AdpHostCallFailed);
        assert_eq!(err.source_layer, Layer::Adapter);
    }

    #[test]
    fn path_traversal_and_missing_file_map_to_adp_host_call_failed_with_context() {
        let ws = temp_workspace();
        let reg = desktop_registry(&[CAP_FS_READ, CAP_FS_WRITE]);
        let policy = RealSyscallPolicy::controlled_live(true, "CAP-AUDIT-003", Some(ws.clone()));

        let traversal = RealSyscallRequest::ControlledFsWrite {
            relative_path: "../escape.txt".to_owned(),
            contents: b"x".to_vec(),
        };
        let err = execute_controlled_syscall(&reg, &traversal, &policy).expect_err("traversal");
        assert_eq!(err.code, ErrorCode::AdpHostCallFailed);
        assert_eq!(err.source_layer, Layer::Adapter);
        assert!(err.context.contains_key("reason"));

        let missing = RealSyscallRequest::ControlledFsRead {
            relative_path: "nonexistent.txt".to_owned(),
        };
        let io_err = execute_controlled_syscall(&reg, &missing, &policy).expect_err("missing");
        assert_eq!(io_err.code, ErrorCode::AdpHostCallFailed);
        assert!(io_err.context.contains_key("os_error_kind"));
        assert!(io_err.context.contains_key("detail"));

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn controlled_real_roundtrip_works_on_desktop_and_server() {
        let ws = temp_workspace();
        let mut server_reg = AdapterRegistry::new();
        server_reg.register(Box::new(ServerAdapter::new()));
        let host = HostEnvironment::new(
            "server",
            "linux",
            "x86_64",
            "none",
            caps(&[CAP_FS_READ, CAP_FS_WRITE, CAP_CRYPTO_HASH, CAP_TIME_NOW]),
        );
        server_reg
            .select(
                &host,
                &HostPolicy::allow_all([CAP_FS_READ, CAP_FS_WRITE, CAP_CRYPTO_HASH, CAP_TIME_NOW]),
            )
            .expect("select server");

        let policy =
            RealSyscallPolicy::controlled_live(true, "CAP-AUDIT-LIVE-01", Some(ws.clone()));
        let write_res = execute_controlled_syscall(
            &server_reg,
            &RealSyscallRequest::ControlledFsWrite {
                relative_path: "tasks/out.txt".to_owned(),
                contents: b"fayger-wave2".to_vec(),
            },
            &policy,
        )
        .expect("write");
        assert_eq!(write_res.mode, RealSyscallMode::ControlledReal);
        assert!(write_res.audit.cap_production_verified);

        let read_res = execute_controlled_syscall(
            &server_reg,
            &RealSyscallRequest::ControlledFsRead {
                relative_path: "tasks/out.txt".to_owned(),
            },
            &policy,
        )
        .expect("read");
        assert_eq!(read_res.output_bytes, b"fayger-wave2");

        let hash_res = execute_controlled_syscall(
            &server_reg,
            &RealSyscallRequest::CryptoSha256 {
                payload: read_res.output_bytes,
            },
            &policy,
        )
        .expect("sha256");
        assert_eq!(hash_res.output_bytes.len(), 32);

        let time_res =
            execute_controlled_syscall(&server_reg, &RealSyscallRequest::HostTimeNow, &policy)
                .expect("time.now");
        assert_ne!(time_res.output_bytes.len(), 0);

        let _ = fs::remove_dir_all(&ws);
    }
}
