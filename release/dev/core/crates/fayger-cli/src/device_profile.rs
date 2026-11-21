//! 设备运行时环境描述文件（`DeviceRuntimeProfile` · `fayger-device-profile/1.0`）与宿主资源池配置。
//!
//! 对应 MVP 规格中的两层授权链第 ① 层与终端切片附体（Partial Embodiment）环境协商：
//! - 设备所有人将本机可用资源池（[`HostResourcePoolConfig`]）授予 `FayGer`；
//! - `FayGer` 探测宿主硬件与网络/本地 `Ego` 槽位状态，生成标准化的 [`DeviceRuntimeProfile`]；
//! - 当 `iFay` / `coFay` 灵魂包挂载到当前 `Host` 时，依据 `tier` 协商局部挂载切片（[`EmbodimentPlan`]）。

use crate::ego_package::hex_sha256;
use crate::error::CliError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::num::NonZeroUsize;
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::sync::OnceLock;
use std::time::Duration;

/// `DeviceRuntimeProfile` 规范版本号。
pub const DEVICE_PROFILE_SCHEMA_VERSION: &str = "fayger-device-profile/1.0";

/// 宿主资源池配置文件 schema 版本号。
pub const HOST_POOL_SCHEMA_VERSION: &str = "fayger-host-pool/1.0";

/// 默认本地 `Ego` 三合一推理服务地址。
pub const DEFAULT_EGO_ENDPOINT: &str = "http://127.0.0.1:7980";

/// `FayGer` 宿主资源池标准能力目录（覆盖文件沙箱、系统剪贴板/通知、硬件遥测/语音/截屏、桌面应用唤起与终端进程）。
pub const KNOWN_HOST_CAPABILITIES: [&str; 12] = [
    "fs.read",
    "fs.write",
    "os.clipboard",
    "os.notify",
    "hardware.telemetry",
    "hardware.audio",
    "hardware.screen",
    "app.control",
    "process.exec",
    "net.http",
    "media.camera",
    "media.mic",
];

/// 设备所有人授予 `FayGer` 的本机宿主资源池配置（两层授权链第 ① 层）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostResourcePoolConfig {
    /// 配置文件版本（[`HOST_POOL_SCHEMA_VERSION`]）。
    pub schema_version: String,
    /// 设备所有人已向 `FayGer` 开放的宿主能力集合。
    pub granted_capabilities: BTreeSet<String>,
    /// 受控沙箱允许挂载的工作区根目录列表。
    pub workspace_roots: Vec<String>,
    /// 本地或远端 `Ego` 三合一推理服务端点。
    pub ego_endpoint: String,
    /// 可选的外部慢思考大模型服务端点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_llm_endpoint: Option<String>,
    /// 宿主网络在线开关状态。
    pub network_online: bool,
}

impl Default for HostResourcePoolConfig {
    fn default() -> Self {
        let granted_capabilities = KNOWN_HOST_CAPABILITIES
            .into_iter()
            .map(str::to_owned)
            .collect();
        Self {
            schema_version: HOST_POOL_SCHEMA_VERSION.to_owned(),
            granted_capabilities,
            workspace_roots: Vec::new(),
            ego_endpoint: DEFAULT_EGO_ENDPOINT.to_owned(),
            external_llm_endpoint: None,
            network_online: true,
        }
    }
}

impl HostResourcePoolConfig {
    /// 为指定 `state_dir` 生成带默认受控工作区路径的宿主资源池配置。
    #[must_use]
    pub fn default_for_state_dir(state_dir: &Path) -> Self {
        let mut cfg = Self::default();
        let sandboxes = state_dir.join("sandboxes");
        cfg.workspace_roots = vec![sandboxes.display().to_string()];
        cfg
    }

    /// 从 `<state_dir>/host_pool.json` 加载配置；若不存在则返回默认配置。
    pub fn load_or_default(state_dir: &Path) -> Result<Self, CliError> {
        let file_path = state_dir.join("host_pool.json");
        if !file_path.exists() {
            return Ok(Self::default_for_state_dir(state_dir));
        }
        let raw = fs::read_to_string(&file_path).map_err(|err| CliError::Io {
            path: file_path.display().to_string(),
            detail: err.to_string(),
        })?;
        serde_json::from_str(&raw).map_err(|err| CliError::Io {
            path: file_path.display().to_string(),
            detail: format!("invalid host_pool.json: {err}"),
        })
    }

    /// 将宿主资源池配置持久化至 `<state_dir>/host_pool.json`。
    pub fn save(&self, state_dir: &Path) -> Result<(), CliError> {
        fs::create_dir_all(state_dir).map_err(|err| CliError::Io {
            path: state_dir.display().to_string(),
            detail: err.to_string(),
        })?;
        let file_path = state_dir.join("host_pool.json");
        let pretty = serde_json::to_string_pretty(self).map_err(|err| CliError::Io {
            path: file_path.display().to_string(),
            detail: err.to_string(),
        })?;
        fs::write(&file_path, pretty).map_err(|err| CliError::Io {
            path: file_path.display().to_string(),
            detail: err.to_string(),
        })
    }
}

/// `DeviceRuntimeProfile` 中的 `Ego` 快慢双系统槽位状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgoSlotStatus {
    /// 配置的 `Ego` 三合一服务端点。
    pub endpoint: String,
    /// 当前是否可连通在线 `Ego` 服务。
    pub online: bool,
    /// 运行模式：`"ego_remote_3in1"` 或 `"local_deterministic_fallback"`。
    pub mode: String,
    /// 支持的标准三合一阶段。
    pub supported_stages: Vec<String>,
    /// 当前设备算力档位建议的最高 `Ego` 参数层切片等级。
    pub max_slice_tier: String,
}

/// 终端 `FayGer` 提交的《设备运行时环境描述文件》（`fayger-device-profile/1.0`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRuntimeProfile {
    /// 固定为 [`DEVICE_PROFILE_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 宿主设备稳定匿名指纹标识（`host_<16-hex>`）。
    pub host_id: String,
    /// 操作系统类别（如 `macos` / `linux` / `windows`）。
    pub os: String,
    /// 指令集架构（如 `aarch64` / `x86_64`）。
    pub arch: String,
    /// 逻辑 CPU 核心数。
    pub cpu_cores: usize,
    /// 宿主物理内存容量（MB）。
    pub memory_mb: u64,
    /// 当前状态目录所在卷可用存储空间（MB）。
    pub free_storage_mb: u64,
    /// 网络在线状态。
    pub network_online: bool,
    /// 综合硬件档位（`desktop-full` / `server-standard` / `edge-constrained` / `thin-remote`）。
    pub tier: String,
    /// 当前宿主资源池授予 `FayGer` 的能力列表（有序）。
    pub granted_capabilities: Vec<String>,
    /// 本地 `Ego` 三合一推理服务槽位状态。
    pub ego_slot: EgoSlotStatus,
}

impl DeviceRuntimeProfile {
    /// 渲染人类可读的设备运行时环境摘要文本。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "FayGer Device Runtime Profile ({})", self.schema_version);
        let _ = writeln!(s, "  host_id:              {}", self.host_id);
        let _ = writeln!(s, "  os / arch:            {} / {}", self.os, self.arch);
        let _ = writeln!(s, "  cpu_cores:            {}", self.cpu_cores);
        let _ = writeln!(s, "  memory_mb:            {}", self.memory_mb);
        let _ = writeln!(s, "  free_storage_mb:      {}", self.free_storage_mb);
        let _ = writeln!(s, "  network_online:       {}", self.network_online);
        let _ = writeln!(s, "  tier:                 {}", self.tier);
        let caps = if self.granted_capabilities.is_empty() {
            "(none)".to_owned()
        } else {
            self.granted_capabilities.join(", ")
        };
        let _ = writeln!(s, "  granted_capabilities: {caps}");
        let _ = writeln!(
            s,
            "  ego_slot:             {} [{}] (online={})",
            self.ego_slot.endpoint, self.ego_slot.mode, self.ego_slot.online
        );
        s
    }
}

/// 终端切片附体（Partial Embodiment）挂载协商结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbodimentPlan {
    /// 协商时的宿主设备档位。
    pub device_tier: String,
    /// 附体形态：`full_embodiment` / `partial_slice` / `thin_remote`。
    pub embodiment_mode: String,
    /// 按需挂载到本机的局部切片列表。
    pub mounted_slices: Vec<String>,
    /// 保留在主体（Primary Replicas）按需远端调用的切片列表。
    pub remote_deferred_slices: Vec<String>,
}

/// 依据当前宿主硬件与 [`HostResourcePoolConfig`] 构造 [`DeviceRuntimeProfile`]。
#[must_use]
pub fn detect_device_runtime_profile(
    pool: &HostResourcePoolConfig,
    probe_ego_network: bool,
) -> DeviceRuntimeProfile {
    let os = std::env::consts::OS.to_owned();
    let arch = std::env::consts::ARCH.to_owned();
    let cpu_cores = std::thread::available_parallelism().map_or(4, NonZeroUsize::get);
    let (memory_mb, free_storage_mb) = cached_hardware_watermarks();
    let host_name = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "local-fayger-host".to_owned());
    let fingerprint_seed = format!("fayger-host-v1:{os}:{arch}:{cpu_cores}:{host_name}");
    let digest = hex_sha256(fingerprint_seed.as_bytes());
    let host_id = format!("host_{}", &digest[..16]);

    let (tier, max_slice_tier) = classify_device_tier(&os, cpu_cores, memory_mb);
    let ego_online = if probe_ego_network {
        probe_http_endpoint_reachable(&pool.ego_endpoint, Duration::from_millis(80))
    } else {
        false
    };
    let ego_mode = if ego_online {
        "ego_remote_3in1".to_owned()
    } else {
        "local_deterministic_fallback".to_owned()
    };

    DeviceRuntimeProfile {
        schema_version: DEVICE_PROFILE_SCHEMA_VERSION.to_owned(),
        host_id,
        os,
        arch,
        cpu_cores,
        memory_mb,
        free_storage_mb,
        network_online: pool.network_online,
        tier,
        granted_capabilities: pool.granted_capabilities.iter().cloned().collect(),
        ego_slot: EgoSlotStatus {
            endpoint: pool.ego_endpoint.clone(),
            online: ego_online,
            mode: ego_mode,
            supported_stages: vec![
                "intuition".to_owned(),
                "dynamic-context".to_owned(),
                "guard-rewrite".to_owned(),
            ],
            max_slice_tier,
        },
    }
}

/// 根据设备环境描述文件协商 `iFay` / `coFay` 的终端切片附体方案（纯函数）。
#[must_use]
pub fn negotiate_embodiment_slices(
    profile: &DeviceRuntimeProfile,
    is_cofay: bool,
) -> EmbodimentPlan {
    if is_cofay {
        return EmbodimentPlan {
            device_tier: profile.tier.clone(),
            embodiment_mode: "public_role_seat".to_owned(),
            mounted_slices: vec![
                "identity.cofay".to_owned(),
                "role_charter.v1".to_owned(),
                "skill.public_seat".to_owned(),
                "cap.driver_bindings".to_owned(),
            ],
            remote_deferred_slices: Vec::new(),
        };
    }

    match profile.tier.as_str() {
        "desktop-full" | "server-standard" => EmbodimentPlan {
            device_tier: profile.tier.clone(),
            embodiment_mode: "full_embodiment".to_owned(),
            mounted_slices: vec![
                "identity.fayid".to_owned(),
                "ego.system1_core".to_owned(),
                "ego.lora_active".to_owned(),
                "thought.hot_memory".to_owned(),
                "skill.workspace_drivers".to_owned(),
                "hardware.cap_gatekeeper".to_owned(),
            ],
            remote_deferred_slices: vec!["thought.cold_archive_replica".to_owned()],
        },
        "edge-constrained" => EmbodimentPlan {
            device_tier: profile.tier.clone(),
            embodiment_mode: "partial_slice".to_owned(),
            mounted_slices: vec![
                "identity.fayid".to_owned(),
                "ego.micro_distilled".to_owned(),
                "skill.edge_minimal".to_owned(),
            ],
            remote_deferred_slices: vec![
                "ego.full_weights".to_owned(),
                "thought.long_horizon_graph".to_owned(),
            ],
        },
        _ => EmbodimentPlan {
            device_tier: profile.tier.clone(),
            embodiment_mode: "thin_remote".to_owned(),
            mounted_slices: vec!["identity.session_stub".to_owned()],
            remote_deferred_slices: vec![
                "ego.remote_3in1".to_owned(),
                "thought.primary_replica".to_owned(),
                "skill.primary_replica".to_owned(),
            ],
        },
    }
}

/// CLI `fayger profile [--json] [--state-dir <dir>]` 执行入口。
pub fn profile_command(json_output: bool, state_dir: Option<&Path>) -> Result<String, CliError> {
    let pool = match state_dir {
        Some(dir) => HostResourcePoolConfig::load_or_default(dir)?,
        None => HostResourcePoolConfig::default(),
    };
    let profile = detect_device_runtime_profile(&pool, true);
    if json_output {
        serde_json::to_string_pretty(&profile).map_err(|err| CliError::Io {
            path: "<profile-json>".to_owned(),
            detail: err.to_string(),
        })
    } else {
        Ok(profile.render_text())
    }
}

/// 探测 HTTP 端点 `http://host:port/...` 是否可快速建立 TCP 连接。
#[must_use]
pub fn probe_http_endpoint_reachable(endpoint: &str, timeout: Duration) -> bool {
    let Some(host_port) = parse_http_host_port(endpoint) else {
        return false;
    };
    let Ok(mut addrs) = host_port.to_socket_addrs() else {
        return false;
    };
    let _ = SocketAddr::ip;
    addrs.any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
}

/// 从 `http://host:port/path` 提取 `host:port`。
#[must_use]
pub fn parse_http_host_port(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim();
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))?;
    let authority = rest.split('/').next()?.trim();
    if authority.is_empty() {
        return None;
    }
    if authority.contains(':') {
        Some(authority.to_owned())
    } else if trimmed.starts_with("https://") {
        Some(format!("{authority}:443"))
    } else {
        Some(format!("{authority}:80"))
    }
}

fn classify_device_tier(os: &str, cpu_cores: usize, memory_mb: u64) -> (String, String) {
    if cpu_cores >= 4 && memory_mb >= 8192 {
        let tier = if os == "linux" && std::env::var_os("DISPLAY").is_none() {
            "server-standard"
        } else {
            "desktop-full"
        };
        (tier.to_owned(), "full".to_owned())
    } else if cpu_cores >= 2 && memory_mb >= 2048 {
        ("edge-constrained".to_owned(), "micro-lora".to_owned())
    } else {
        ("thin-remote".to_owned(), "remote-only".to_owned())
    }
}

fn cached_hardware_watermarks() -> (u64, u64) {
    static HARDWARE_CACHE: OnceLock<(u64, u64)> = OnceLock::new();
    *HARDWARE_CACHE.get_or_init(|| (detect_memory_mb(), detect_free_storage_mb()))
}

fn detect_memory_mb() -> u64 {
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb_str = rest.trim().trim_end_matches("kB").trim();
                if let Ok(kb) = kb_str.parse::<u64>() {
                    return (kb / 1024).max(512);
                }
            }
        }
    }
    if std::env::consts::OS == "macos" {
        if let Ok(out) = ProcessCommand::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if out.status.success() {
                if let Ok(txt) = String::from_utf8(out.stdout) {
                    if let Ok(bytes) = txt.trim().parse::<u64>() {
                        return (bytes / (1024 * 1024)).max(512);
                    }
                }
            }
        }
    }
    16_384
}

fn detect_free_storage_mb() -> u64 {
    if matches!(std::env::consts::OS, "macos" | "linux") {
        if let Ok(out) = ProcessCommand::new("df").args(["-k", "."]).output() {
            if out.status.success() {
                if let Ok(txt) = String::from_utf8(out.stdout) {
                    if let Some(data_line) = txt.lines().nth(1) {
                        let cols: Vec<&str> = data_line.split_whitespace().collect();
                        if let Some(avail_kb_str) = cols.get(3) {
                            if let Ok(avail_kb) = avail_kb_str.parse::<u64>() {
                                return (avail_kb / 1024).max(256);
                            }
                        }
                    }
                }
            }
        }
    }
    65_536
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        detect_device_runtime_profile, negotiate_embodiment_slices, parse_http_host_port,
        HostResourcePoolConfig, DEVICE_PROFILE_SCHEMA_VERSION,
    };

    #[test]
    fn profile_contains_canonical_schema_and_capabilities() {
        let pool = HostResourcePoolConfig::default();
        let profile = detect_device_runtime_profile(&pool, false);
        assert_eq!(profile.schema_version, DEVICE_PROFILE_SCHEMA_VERSION);
        assert!(profile.host_id.starts_with("host_"));
        assert!(profile.cpu_cores >= 1);
        assert!(profile.memory_mb >= 512);
        assert!(profile.granted_capabilities.contains(&"fs.read".to_owned()));
        assert_eq!(profile.ego_slot.mode, "local_deterministic_fallback");
    }

    #[test]
    fn embodiment_negotiation_adapts_to_tier_and_soul_kind() {
        let pool = HostResourcePoolConfig::default();
        let mut profile = detect_device_runtime_profile(&pool, false);
        profile.tier = "edge-constrained".to_owned();
        let edge_plan = negotiate_embodiment_slices(&profile, false);
        assert_eq!(edge_plan.embodiment_mode, "partial_slice");
        assert!(edge_plan
            .remote_deferred_slices
            .contains(&"ego.full_weights".to_owned()));

        let cofay_plan = negotiate_embodiment_slices(&profile, true);
        assert_eq!(cofay_plan.embodiment_mode, "public_role_seat");
    }

    #[test]
    fn parses_http_host_port_variants() {
        assert_eq!(
            parse_http_host_port("http://127.0.0.1:7980/v1/intuition").as_deref(),
            Some("127.0.0.1:7980")
        );
        assert_eq!(
            parse_http_host_port("http://localhost").as_deref(),
            Some("localhost:80")
        );
        assert!(parse_http_host_port("not-a-url").is_none());
    }
}
