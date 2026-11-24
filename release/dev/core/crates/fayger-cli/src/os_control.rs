//! 原生操作系统权限探针、合法系统设置引导与跨平台宿主硬件/系统/桌面软件受控执行器（`os_control`）。
//!
//! 安全与合规铁律：
//! 1. **绝不绕过 OS 安全门禁**：严格调用宿主操作系统合法原生机制（macOS `pbcopy`/`pbpaste`、`osascript`、
//!    `pmset`、`say`、`screencapture`、`open`、`x-apple.systempreferences:...`；Linux `wl-clipboard`/`xclip`、
//!    `notify-send`、`/sys/class/power_supply`、`xdg-open`；Windows PowerShell / `ms-settings:`）；
//! 2. **零命令注入（Zero Shell Injection）**：全模块严禁 `sh -c` 或字符串拼接脚本；`osascript` 一律通过
//!    `on run argv` 传参，`ProcessExec` 与 `AppControl` 一律执行严格白名单与字符集校验。

use crate::device_profile::{
    detect_device_runtime_profile, HostResourcePoolConfig, KNOWN_HOST_CAPABILITIES,
};
use crate::ego_package::hex_sha256;
use crate::error::CliError;
use crate::icp_ego::CtxAnnotation;
use crate::wallet::unix_now_secs;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};

/// 允许通过 `ProcessExec` 执行的受控只读宿主检视命令白名单。
pub const ALLOWED_PROCESS_EXEC_COMMANDS: [&str; 10] = [
    "workspace.ls",
    "workspace.digest",
    "workspace.inspect",
    "uname",
    "whoami",
    "date",
    "uptime",
    "df",
    "sw_vers",
    "ls",
];

/// 单项宿主操作系统权限与原生工具就绪状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsPermissionItem {
    /// 宿主能力标识（如 `"os.clipboard"` / `"hardware.screen"` / `"app.control"`）。
    pub capability: String,
    /// 风险分级：`"normal"`（`Faying` 态直通）或 `"high_cap_approval"`（每次需 `Human Prime` `CAP` 审批）。
    pub risk_tier: String,
    /// 当前系统就绪状态：`"ready"` | `"needs_os_grant"` | `"unsupported"`。
    pub status: String,
    /// 当前是否已纳入本机 `HostResourcePool` 授权池。
    pub in_host_pool: bool,
    /// 底层操作系统原生机制说明。
    pub os_mechanism: String,
    /// 可选的系统隐私设置深链接（如 macOS `x-apple.systempreferences:...` 或 Windows `ms-settings:...`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_uri: Option<String>,
    /// 诊断详情说明。
    pub detail: String,
}

/// `POST /api/v1/os/permissions/request` 执行回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsPermissionRequestReceipt {
    /// 目标能力标识。
    pub capability: String,
    /// 触发后的探测状态。
    pub status: String,
    /// 本次执行的合法系统触发动作说明。
    pub triggered_action: String,
    /// 是否打开了系统隐私设置面板。
    pub settings_opened: bool,
    /// 是否已同步加入 `HostResourcePool`。
    pub granted_to_host_pool: bool,
    /// 关联系统设置 URI。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_uri: Option<String>,
    /// 人类可读反馈详情。
    pub detail: String,
}

/// 非阻塞在默认浏览器打开本地 `FayGer Dashboard` 地址（仅供 `fayger up --open` 调用）。
pub fn open_browser_nonblocking(url: &str) {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://127.0.0.1")
        && !trimmed.starts_with("http://localhost")
        && !trimmed.starts_with("http://[")
    {
        return;
    }
    let os = std::env::consts::OS;
    let mut cmd = if os == "macos" {
        let mut c = ProcessCommand::new("/usr/bin/open");
        c.arg(trimmed);
        c
    } else if os == "windows" {
        let mut c = ProcessCommand::new("cmd");
        c.args(["/C", "start", "", trimmed]);
        c
    } else {
        let mut c = ProcessCommand::new("xdg-open");
        c.arg(trimmed);
        c
    };
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// 探测当前操作系统上全部标准宿主能力的原生工具与权限就绪状态。
#[must_use]
pub fn probe_os_permissions(host_pool: &HostResourcePoolConfig) -> Vec<OsPermissionItem> {
    KNOWN_HOST_CAPABILITIES
        .iter()
        .map(|cap| probe_single_capability(cap, host_pool.granted_capabilities.contains(*cap)))
        .collect()
}

#[allow(clippy::too_many_lines)]
fn probe_single_capability(cap: &str, in_host_pool: bool) -> OsPermissionItem {
    let os = std::env::consts::OS;
    match cap {
        "fs.read" | "fs.write" => OsPermissionItem {
            capability: cap.to_owned(),
            risk_tier: "normal".to_owned(),
            status: "ready".to_owned(),
            in_host_pool,
            os_mechanism: "FayGer Isolated Sandbox Directory".to_owned(),
            settings_uri: None,
            detail: "Scoped strictly inside per-Fay sandbox directory".to_owned(),
        },
        "os.clipboard" => {
            let (status, mech, detail) = if os == "macos" {
                let ok =
                    Path::new("/usr/bin/pbpaste").exists() && Path::new("/usr/bin/pbcopy").exists();
                (
                    if ok { "ready" } else { "unsupported" },
                    "macOS Pasteboard (pbcopy / pbpaste)",
                    "Native macOS pasteboard utilities available",
                )
            } else if os == "windows" {
                (
                    "ready",
                    "Windows PowerShell Clipboard (Get-Clipboard / Set-Clipboard)",
                    "Native Windows clipboard cmdlets available",
                )
            } else if which_bin("wl-paste") || which_bin("xclip") {
                (
                    "ready",
                    "Linux Wayland/X11 Clipboard (wl-clipboard / xclip)",
                    "Clipboard utility detected on PATH",
                )
            } else {
                (
                    "needs_os_grant",
                    "Linux Wayland/X11 Clipboard (sandbox fallback active)",
                    "Install wl-clipboard or xclip for desktop clipboard sync; sandbox fallback ready",
                )
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "normal".to_owned(),
                status: status.to_owned(),
                in_host_pool,
                os_mechanism: mech.to_owned(),
                settings_uri: None,
                detail: detail.to_owned(),
            }
        }
        "os.notify" => {
            let (status, mech, uri) = if os == "macos" {
                (
                    if Path::new("/usr/bin/osascript").exists() {
                        "ready"
                    } else {
                        "unsupported"
                    },
                    "macOS Notification Center (osascript)",
                    Some(
                        "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
                            .to_owned(),
                    ),
                )
            } else if os == "windows" {
                (
                    "ready",
                    "Windows Notification System",
                    Some("ms-settings:notifications".to_owned()),
                )
            } else if which_bin("notify-send") {
                ("ready", "freedesktop notify-send", None)
            } else {
                (
                    "needs_os_grant",
                    "Linux notify-send (receipt fallback active)",
                    None,
                )
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "normal".to_owned(),
                status: status.to_owned(),
                in_host_pool,
                os_mechanism: mech.to_owned(),
                settings_uri: uri,
                detail: "Desktop toast notification channel".to_owned(),
            }
        }
        "hardware.telemetry" => {
            let mech = if os == "macos" {
                "macOS IOKit Power / sysctl (pmset -g batt)"
            } else if os == "linux" {
                "Linux /sys/class/power_supply & /proc/meminfo"
            } else {
                "Windows System Power & Hardware Telemetry"
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "normal".to_owned(),
                status: "ready".to_owned(),
                in_host_pool,
                os_mechanism: mech.to_owned(),
                settings_uri: None,
                detail: "Live battery, AC state, CPU, memory & storage telemetry".to_owned(),
            }
        }
        "hardware.actuate" => OsPermissionItem {
            capability: cap.to_owned(),
            risk_tier: "normal".to_owned(),
            status: "ready".to_owned(),
            in_host_pool,
            os_mechanism: "FHBP/1.0 Hardware Embodiment & Physical Safety Envelope".to_owned(),
            settings_uri: None,
            detail:
                "Embodied Drone, RC Car, Robot Arm, 5-Axis CNC, Cinema Jib & C99 FHBP/1.0 nodes"
                    .to_owned(),
        },
        "hardware.audio" => {
            let (status, mech) = if os == "macos" {
                (
                    if Path::new("/usr/bin/say").exists() {
                        "ready"
                    } else {
                        "unsupported"
                    },
                    "macOS Speech Synthesis (/usr/bin/say)",
                )
            } else if os == "windows" {
                ("ready", "Windows System.Speech.Synthesis")
            } else if which_bin("spd-say") || which_bin("espeak") {
                ("ready", "Linux Speech Dispatcher (spd-say / espeak)")
            } else {
                (
                    "needs_os_grant",
                    "Linux TTS (install speech-dispatcher / espeak)",
                )
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "normal".to_owned(),
                status: status.to_owned(),
                in_host_pool,
                os_mechanism: mech.to_owned(),
                settings_uri: None,
                detail: "Bounded speaker voice synthesis (<= 240 chars)".to_owned(),
            }
        }
        "hardware.screen" => {
            let (status, mech, uri, detail) = if os == "macos" {
                (
                    "needs_os_grant",
                    "macOS TCC ScreenCapture (/usr/sbin/screencapture)",
                    Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture".to_owned()),
                    "Requires macOS Screen Recording TCC permission for terminal/host process + CAP approval",
                )
            } else if os == "windows" {
                (
                    "ready",
                    "Windows Graphics CopyFromScreen",
                    Some("ms-settings:privacy".to_owned()),
                    "Gated by Human Prime CAP approval on every capture",
                )
            } else {
                (
                    if which_bin("gnome-screenshot") || which_bin("import") {
                        "ready"
                    } else {
                        "needs_os_grant"
                    },
                    "Linux Desktop Screen Capture",
                    None,
                    "Gated by Human Prime CAP approval on every capture",
                )
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "high_cap_approval".to_owned(),
                status: status.to_owned(),
                in_host_pool,
                os_mechanism: mech.to_owned(),
                settings_uri: uri,
                detail: detail.to_owned(),
            }
        }
        "app.control" => {
            let (mech, uri) = if os == "macos" {
                (
                    "macOS LaunchServices (/usr/bin/open -a)",
                    Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation".to_owned()),
                )
            } else if os == "windows" {
                (
                    "Windows Shell Start",
                    Some("ms-settings:privacy".to_owned()),
                )
            } else {
                ("Linux xdg-open Launcher", None)
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "high_cap_approval".to_owned(),
                status: "ready".to_owned(),
                in_host_pool,
                os_mechanism: mech.to_owned(),
                settings_uri: uri,
                detail:
                    "Whitelisted desktop application launcher gated by CAP Human Prime approval"
                        .to_owned(),
            }
        }
        "process.exec" => OsPermissionItem {
            capability: cap.to_owned(),
            risk_tier: "high_cap_approval".to_owned(),
            status: "ready".to_owned(),
            in_host_pool,
            os_mechanism: "FayGer Allowlisted Read-Only OS Inspector".to_owned(),
            settings_uri: None,
            detail: format!("Allowlist: {}", ALLOWED_PROCESS_EXEC_COMMANDS.join(", ")),
        },
        "net.http" => OsPermissionItem {
            capability: cap.to_owned(),
            risk_tier: "normal".to_owned(),
            status: "ready".to_owned(),
            in_host_pool,
            os_mechanism: "Rust TcpStream + System curl HTTPS Bridge".to_owned(),
            settings_uri: None,
            detail: "Used for local Ollama/GMChain RPC and HTTPS External LLM APIs".to_owned(),
        },
        "media.camera" => {
            let uri = if os == "macos" {
                Some(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera"
                        .to_owned(),
                )
            } else if os == "windows" {
                Some("ms-settings:privacy-webcam".to_owned())
            } else {
                None
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "high_cap_approval".to_owned(),
                status: if os == "macos" {
                    "needs_os_grant".to_owned()
                } else {
                    "ready".to_owned()
                },
                in_host_pool,
                os_mechanism: "Controlled Camera Frame Capture".to_owned(),
                settings_uri: uri,
                detail: "Gated by OS Camera privacy consent + CAP Human Prime approval".to_owned(),
            }
        }
        _ => {
            let uri = if os == "macos" {
                Some(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
                        .to_owned(),
                )
            } else if os == "windows" {
                Some("ms-settings:privacy-microphone".to_owned())
            } else {
                None
            };
            OsPermissionItem {
                capability: cap.to_owned(),
                risk_tier: "high_cap_approval".to_owned(),
                status: if os == "macos" {
                    "needs_os_grant".to_owned()
                } else {
                    "ready".to_owned()
                },
                in_host_pool,
                os_mechanism: "OS Audio Input Permission".to_owned(),
                settings_uri: uri,
                detail: "Gated by OS Microphone privacy consent + CAP approval".to_owned(),
            }
        }
    }
}

/// 执行合法的操作系统权限探测/触发，并在 `open_settings == true` 时唤起原生系统隐私设置页。
pub fn request_os_permission_native(
    capability: &str,
    open_settings: bool,
    in_host_pool: bool,
) -> Result<OsPermissionRequestReceipt, CliError> {
    let clean = capability.trim();
    if !KNOWN_HOST_CAPABILITIES.contains(&clean) {
        return Err(CliError::Usage(format!(
            "unknown host capability '{clean}' (expected one of: {})",
            KNOWN_HOST_CAPABILITIES.join(", ")
        )));
    }

    let probed = probe_single_capability(clean, in_host_pool);
    let os = std::env::consts::OS;
    let headless_test = std::env::var("FAYGER_HEADLESS_PERMISSION_TEST")
        .ok()
        .as_deref()
        == Some("1");

    let mut triggered_action = format!("Verified {} ({})", clean, probed.os_mechanism);
    let mut final_status = probed.status.clone();

    if !headless_test {
        match clean {
            "os.clipboard" if os == "macos" => {
                if let Ok(out) = ProcessCommand::new("/usr/bin/pbpaste").output() {
                    if out.status.success() {
                        "ready".clone_into(&mut final_status);
                        triggered_action = format!(
                            "Probed macOS pasteboard via pbpaste ({} bytes readable)",
                            out.stdout.len()
                        );
                    }
                }
            }
            "os.notify" if os == "macos" => {
                let res = ProcessCommand::new("/usr/bin/osascript")
                    .args([
                        "-e",
                        "on run argv",
                        "-e",
                        "display notification (item 2 of argv) with title (item 1 of argv)",
                        "-e",
                        "end run",
                        "--",
                        "FayGer OS Permission",
                        "FayGer native notification channel verified.",
                    ])
                    .output();
                if res.is_ok_and(|o| o.status.success()) {
                    "ready".clone_into(&mut final_status);
                    "Dispatched verification banner to macOS Notification Center"
                        .clone_into(&mut triggered_action);
                }
            }
            "hardware.telemetry" if os == "macos" => {
                if let Ok(out) = ProcessCommand::new("/usr/bin/pmset")
                    .args(["-g", "batt"])
                    .output()
                {
                    if out.status.success() {
                        "ready".clone_into(&mut final_status);
                        let first_line = String::from_utf8_lossy(&out.stdout)
                            .lines()
                            .next()
                            .unwrap_or("ok")
                            .trim()
                            .to_owned();
                        triggered_action = format!("Probed macOS power telemetry: {first_line}");
                    }
                }
            }
            _ => {}
        }
    }

    let mut settings_opened = false;
    if open_settings && !headless_test {
        if let Some(ref uri) = probed.settings_uri {
            if os == "macos" {
                settings_opened = ProcessCommand::new("/usr/bin/open")
                    .arg(uri)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .is_ok();
            } else if os == "windows" {
                settings_opened = ProcessCommand::new("cmd")
                    .args(["/C", "start", "", uri])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .is_ok();
            }
        }
    }

    Ok(OsPermissionRequestReceipt {
        capability: clean.to_owned(),
        status: final_status,
        triggered_action,
        settings_opened,
        granted_to_host_pool: in_host_pool,
        settings_uri: probed.settings_uri,
        detail: probed.detail,
    })
}

/// 严格校验 `AppControl` 的目标应用名称与可选 URL，阻断一切路径穿越或命令注入字符。
pub fn validate_app_control_params(
    app_name: &str,
    target_url: Option<&str>,
) -> Result<String, CliError> {
    let clean = app_name.trim();
    if clean.is_empty() || clean.len() > 48 {
        return Err(CliError::Usage(
            "app_name must be 1..=48 characters (e.g. 'Calculator', 'Notes', 'Safari')".to_owned(),
        ));
    }
    if clean.contains("..")
        || !clean
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | '-' | '_'))
    {
        return Err(CliError::Usage(format!(
            "app_name '{clean}' rejected: only ASCII letters, digits, spaces, '.', '-', and '_' are allowed"
        )));
    }
    if let Some(u) = target_url.map(str::trim).filter(|s| !s.is_empty()) {
        if (!u.starts_with("https://") && !u.starts_with("http://"))
            || u.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(CliError::Usage(format!(
                "target_url '{u}' rejected: must be a clean http(s):// URL without whitespace"
            )));
        }
    }
    Ok(clean.to_owned())
}

/// 严格校验 `ProcessExec` 命令标识是否在只读安全白名单内。
pub fn validate_process_exec_command(command: &str) -> Result<String, CliError> {
    let clean = command.trim();
    if ALLOWED_PROCESS_EXEC_COMMANDS.contains(&clean) {
        Ok(clean.to_owned())
    } else {
        Err(CliError::Usage(format!(
            "process.exec command '{clean}' rejected: allowed commands are [{}]",
            ALLOWED_PROCESS_EXEC_COMMANDS.join(", ")
        )))
    }
}

/// 执行 `os.clipboard` 写入（同步写入系统剪贴板与沙箱镜像备份）。
pub fn exec_clipboard_write(
    sandbox_dir: &Path,
    text: &str,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let clip_dir = sandbox_dir.join("clipboard");
    fs::create_dir_all(&clip_dir).map_err(|err| CliError::Io {
        path: clip_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let mirror_path = clip_dir.join("latest_clipboard.txt");
    fs::write(&mirror_path, text.as_bytes()).map_err(|err| CliError::Io {
        path: mirror_path.display().to_string(),
        detail: err.to_string(),
    })?;

    let mut native_synced = false;
    let sandbox_only = std::env::var("FAYGER_SANDBOX_CLIPBOARD_ONLY")
        .ok()
        .as_deref()
        == Some("1");
    if !sandbox_only {
        if std::env::consts::OS == "macos" {
            if let Ok(mut child) = ProcessCommand::new("/usr/bin/pbcopy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut pipe) = child.stdin.take() {
                    let _ = pipe.write_all(text.as_bytes());
                }
                native_synced = child.wait().is_ok_and(|s| s.success());
            }
        } else if std::env::consts::OS == "linux" && which_bin("wl-copy") {
            if let Ok(mut child) = ProcessCommand::new("wl-copy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut pipe) = child.stdin.take() {
                    let _ = pipe.write_all(text.as_bytes());
                }
                native_synced = child.wait().is_ok_and(|s| s.success());
            }
        }
    }

    let sha = hex_sha256(text.as_bytes());
    let channel = if native_synced {
        "native_os_pasteboard"
    } else {
        "sandbox_clipboard_mirror"
    };
    let preview: String = text.chars().take(200).collect();
    Ok((
        format!("Wrote {} bytes to clipboard ({channel}).", text.len()),
        CtxAnnotation::ArtifactPreview {
            title: format!("OS Clipboard Write ({channel})"),
            path: "clipboard/latest_clipboard.txt".to_owned(),
            content_preview: preview,
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行 `os.clipboard` 读取（优先读取系统剪贴板，无桌面会话时优雅回退至沙箱剪贴板镜像）。
pub fn exec_clipboard_read(
    sandbox_dir: &Path,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let mirror_path = sandbox_dir.join("clipboard").join("latest_clipboard.txt");
    let sandbox_only = std::env::var("FAYGER_SANDBOX_CLIPBOARD_ONLY")
        .ok()
        .as_deref()
        == Some("1");

    let mut content: Option<(String, &'static str)> = None;
    if !sandbox_only {
        if std::env::consts::OS == "macos" {
            if let Ok(out) = ProcessCommand::new("/usr/bin/pbpaste").output() {
                if out.status.success() {
                    let txt = String::from_utf8_lossy(&out.stdout).into_owned();
                    if !txt.is_empty() || !mirror_path.exists() {
                        content = Some((txt, "native_os_pasteboard"));
                    }
                }
            }
        } else if std::env::consts::OS == "windows" {
            if let Ok(out) = ProcessCommand::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard"])
                .output()
            {
                if out.status.success() {
                    let txt = String::from_utf8_lossy(&out.stdout).trim_end().to_owned();
                    if !txt.is_empty() || !mirror_path.exists() {
                        content = Some((txt, "native_os_pasteboard"));
                    }
                }
            }
        } else if which_bin("wl-paste") {
            if let Ok(out) = ProcessCommand::new("wl-paste").arg("-n").output() {
                if out.status.success() {
                    content = Some((
                        String::from_utf8_lossy(&out.stdout).into_owned(),
                        "native_os_pasteboard",
                    ));
                }
            }
        }
    }

    let (final_text, source) = if let Some(pair) = content {
        pair
    } else {
        let fallback = fs::read_to_string(&mirror_path).unwrap_or_default();
        (fallback, "sandbox_clipboard_mirror")
    };
    let sha = hex_sha256(final_text.as_bytes());
    let preview: String = final_text.chars().take(240).collect();
    Ok((
        format!("Read {} bytes from clipboard ({source}).", final_text.len()),
        CtxAnnotation::ArtifactPreview {
            title: format!("OS Clipboard Read ({source})"),
            path: "clipboard/latest_clipboard.txt".to_owned(),
            content_preview: if preview.is_empty() {
                "(clipboard empty)".to_owned()
            } else {
                preview
            },
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行 `os.notify` 原生桌面系统通知（macOS 经 `osascript on run argv` 安全传参，严防脚本注入）。
pub fn exec_os_notify(
    sandbox_dir: &Path,
    title: &str,
    body: &str,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let clean_title: String = title
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    let clean_body: String = body
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(240)
        .collect();
    let final_title = if clean_title.is_empty() {
        "FayGer Notification".to_owned()
    } else {
        clean_title
    };
    let final_body = if clean_body.is_empty() {
        "Message from supervised Fay".to_owned()
    } else {
        clean_body
    };

    let headless_test = std::env::var("FAYGER_HEADLESS_PERMISSION_TEST")
        .ok()
        .as_deref()
        == Some("1");
    let mut native_delivered = false;
    if !headless_test {
        if std::env::consts::OS == "macos" {
            native_delivered = ProcessCommand::new("/usr/bin/osascript")
                .args([
                    "-e",
                    "on run argv",
                    "-e",
                    "display notification (item 2 of argv) with title (item 1 of argv)",
                    "-e",
                    "end run",
                    "--",
                    &final_title,
                    &final_body,
                ])
                .output()
                .is_ok_and(|o| o.status.success());
        } else if std::env::consts::OS == "linux" && which_bin("notify-send") {
            native_delivered = ProcessCommand::new("notify-send")
                .args(["--", &final_title, &final_body])
                .output()
                .is_ok_and(|o| o.status.success());
        }
    }

    let notif_dir = sandbox_dir.join("notifications");
    fs::create_dir_all(&notif_dir).map_err(|err| CliError::Io {
        path: notif_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let receipt = json!({
        "title": final_title,
        "body": final_body,
        "native_delivered": native_delivered,
        "timestamp_unix": unix_now_secs(),
    });
    let receipt_str = serde_json::to_string_pretty(&receipt).unwrap_or_else(|_| "{}".to_owned());
    let rel_path = "notifications/latest_notify.json";
    fs::write(sandbox_dir.join(rel_path), receipt_str.as_bytes()).map_err(|err| CliError::Io {
        path: rel_path.to_owned(),
        detail: err.to_string(),
    })?;
    let sha = hex_sha256(receipt_str.as_bytes());
    Ok((
        format!(
            "Dispatched OS notification '{}' (native_delivered={native_delivered}).",
            receipt["title"].as_str().unwrap_or("")
        ),
        CtxAnnotation::ArtifactPreview {
            title: "OS Desktop Notification".to_owned(),
            path: rel_path.to_owned(),
            content_preview: receipt_str,
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行 `hardware.telemetry` 实时采集本机电池电量/充电状态、CPU、内存、存储与运行时长。
pub fn exec_hardware_telemetry(
    sandbox_dir: &Path,
    host_pool: &HostResourcePoolConfig,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let profile = detect_device_runtime_profile(host_pool, false);
    let battery_summary = read_live_battery_status();
    let uptime_summary = read_live_uptime();

    let report = json!({
        "os": profile.os,
        "arch": profile.arch,
        "cpu_cores": profile.cpu_cores,
        "memory_mb": profile.memory_mb,
        "free_storage_mb": profile.free_storage_mb,
        "tier": profile.tier,
        "battery": battery_summary,
        "uptime": uptime_summary,
        "timestamp_unix": unix_now_secs(),
    });
    let pretty = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned());
    let tel_dir = sandbox_dir.join("telemetry");
    fs::create_dir_all(&tel_dir).map_err(|err| CliError::Io {
        path: tel_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let rel_path = "telemetry/hardware_snapshot.json";
    fs::write(sandbox_dir.join(rel_path), pretty.as_bytes()).map_err(|err| CliError::Io {
        path: rel_path.to_owned(),
        detail: err.to_string(),
    })?;
    let sha = hex_sha256(pretty.as_bytes());
    Ok((
        format!(
            "Hardware Telemetry: {} {} ({} cores, {} MB RAM, battery: {}).",
            profile.os, profile.arch, profile.cpu_cores, profile.memory_mb, battery_summary
        ),
        CtxAnnotation::ArtifactPreview {
            title: "Live Host Hardware & Battery Telemetry".to_owned(),
            path: rel_path.to_owned(),
            content_preview: pretty,
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行 `hardware.audio` 扬声器语音播报（限长 `<= 240` 字符，支持 `FAYGER_MUTE_AUDIO=1` 静音测试）。
pub fn exec_hardware_speak(
    sandbox_dir: &Path,
    text: &str,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let clean: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(240)
        .collect();
    if clean.is_empty() {
        return Err(CliError::Usage(
            "hardware_speak requires non-empty text".to_owned(),
        ));
    }
    let muted = std::env::var("FAYGER_MUTE_AUDIO").ok().as_deref() == Some("1");
    let mut spawned = false;
    if !muted {
        if std::env::consts::OS == "macos" && Path::new("/usr/bin/say").exists() {
            spawned = ProcessCommand::new("/usr/bin/say")
                .arg(&clean)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok();
        } else if std::env::consts::OS == "linux" && which_bin("spd-say") {
            spawned = ProcessCommand::new("spd-say")
                .arg(&clean)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok();
        }
    }

    let audio_dir = sandbox_dir.join("audio");
    fs::create_dir_all(&audio_dir).map_err(|err| CliError::Io {
        path: audio_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let rel_path = "audio/last_utterance.txt";
    let record = format!(
        "utterance={clean}\naudio_dispatched={spawned}\nmuted={muted}\ntimestamp={}\n",
        unix_now_secs()
    );
    fs::write(sandbox_dir.join(rel_path), record.as_bytes()).map_err(|err| CliError::Io {
        path: rel_path.to_owned(),
        detail: err.to_string(),
    })?;
    let sha = hex_sha256(record.as_bytes());
    Ok((
        format!(
            "Speaker TTS utterance queued ({} chars, audio_dispatched={spawned}).",
            clean.chars().count()
        ),
        CtxAnnotation::ArtifactPreview {
            title: "Host Speaker TTS Utterance".to_owned(),
            path: rel_path.to_owned(),
            content_preview: record.trim().to_owned(),
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行高风险 `hardware.screen` 真实屏幕截图（需经 `CAP` 人工批准；若 TCC 未授权则记录清晰引导回执）。
pub fn exec_hardware_screen_capture(
    sandbox_dir: &Path,
    label: &str,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let safe_label: String = label
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(32)
        .collect();
    let final_label = if safe_label.is_empty() {
        "desktop".to_owned()
    } else {
        safe_label
    };

    let cap_dir = sandbox_dir.join("captures");
    fs::create_dir_all(&cap_dir).map_err(|err| CliError::Io {
        path: cap_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let rel_png = format!("captures/screen_{final_label}.png");
    let abs_png = sandbox_dir.join(&rel_png);

    let headless_test = std::env::var("FAYGER_HEADLESS_PERMISSION_TEST")
        .ok()
        .as_deref()
        == Some("1");
    let mut captured_bytes: Option<Vec<u8>> = None;
    if !headless_test
        && std::env::consts::OS == "macos"
        && Path::new("/usr/sbin/screencapture").exists()
    {
        let res = ProcessCommand::new("/usr/sbin/screencapture")
            .args(["-x", &abs_png.display().to_string()])
            .output();
        if res.is_ok_and(|o| o.status.success()) {
            if let Ok(bytes) = fs::read(&abs_png) {
                if !bytes.is_empty() {
                    captured_bytes = Some(bytes);
                }
            }
        }
    }

    if let Some(png_bytes) = captured_bytes {
        let sha = hex_sha256(&png_bytes);
        let preview = format!(
            "PNG screenshot captured ({} bytes) -> {rel_png}",
            png_bytes.len()
        );
        return Ok((
            preview.clone(),
            CtxAnnotation::ArtifactPreview {
                title: format!("Host Screen Capture ({final_label})"),
                path: rel_png,
                content_preview: preview,
                sha256: sha.clone(),
            },
            sha,
        ));
    }

    let diag = json!({
        "label": final_label,
        "status": "os_screen_permission_or_display_required",
        "guidance": "Grant Screen Recording permission in Tab 1 (Privacy_ScreenCapture) when running on an interactive desktop session.",
        "timestamp_unix": unix_now_secs(),
    });
    let diag_str = serde_json::to_string_pretty(&diag).unwrap_or_else(|_| "{}".to_owned());
    let rel_diag = format!("captures/screen_{final_label}.diagnostic.json");
    fs::write(sandbox_dir.join(&rel_diag), diag_str.as_bytes()).map_err(|err| CliError::Io {
        path: rel_diag.clone(),
        detail: err.to_string(),
    })?;
    let sha = hex_sha256(diag_str.as_bytes());
    Ok((
        format!(
            "Screen capture attempted for '{final_label}'; recorded OS TCC diagnostic receipt."
        ),
        CtxAnnotation::ArtifactPreview {
            title: format!("Host Screen Capture Diagnostic ({final_label})"),
            path: rel_diag,
            content_preview: diag_str,
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行高风险 `app.control` 桌面软件唤起（严格白名单参数校验，经 `CAP` 人工批准后运行）。
pub fn exec_app_control(
    sandbox_dir: &Path,
    app_name: &str,
    action: &str,
    target_url: Option<&str>,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let clean_app = validate_app_control_params(app_name, target_url)?;
    let clean_action = if action.trim().is_empty() {
        "launch"
    } else {
        action.trim()
    };

    let os = std::env::consts::OS;
    let outcome_detail = if clean_action == "verify_installed" {
        let found = if os == "macos" {
            Path::new(&format!("/System/Applications/{clean_app}.app")).exists()
                || Path::new(&format!("/Applications/{clean_app}.app")).exists()
                || Path::new(&format!("/System/Library/CoreServices/{clean_app}.app")).exists()
        } else {
            true
        };
        format!("verified_installed={found} for '{clean_app}' on {os}")
    } else if os == "macos" {
        let mut cmd = ProcessCommand::new("/usr/bin/open");
        cmd.args(["-a", &clean_app]);
        if let Some(u) = target_url.map(str::trim).filter(|s| !s.is_empty()) {
            cmd.arg(u);
        }
        let ok = cmd.output().is_ok_and(|o| o.status.success());
        format!("macOS open -a '{clean_app}' executed (success={ok})")
    } else if os == "windows" {
        let target = target_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&clean_app);
        let ok = ProcessCommand::new("cmd")
            .args(["/C", "start", "", target])
            .output()
            .is_ok_and(|o| o.status.success());
        format!("Windows start '{target}' executed (success={ok})")
    } else {
        let target = target_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&clean_app);
        let ok = which_bin("xdg-open")
            && ProcessCommand::new("xdg-open")
                .arg(target)
                .output()
                .is_ok_and(|o| o.status.success());
        format!("Linux launcher '{target}' executed (success={ok})")
    };

    let app_dir = sandbox_dir.join("apps");
    fs::create_dir_all(&app_dir).map_err(|err| CliError::Io {
        path: app_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let receipt = json!({
        "app_name": clean_app,
        "action": clean_action,
        "target_url": target_url,
        "outcome": outcome_detail,
        "timestamp_unix": unix_now_secs(),
    });
    let pretty = serde_json::to_string_pretty(&receipt).unwrap_or_else(|_| "{}".to_owned());
    let rel_path = "apps/last_app_control.json";
    fs::write(sandbox_dir.join(rel_path), pretty.as_bytes()).map_err(|err| CliError::Io {
        path: rel_path.to_owned(),
        detail: err.to_string(),
    })?;
    let sha = hex_sha256(pretty.as_bytes());
    Ok((
        outcome_detail,
        CtxAnnotation::ArtifactPreview {
            title: format!("Controlled App Action ({clean_app})"),
            path: rel_path.to_owned(),
            content_preview: pretty,
            sha256: sha.clone(),
        },
        sha,
    ))
}

/// 执行高风险 `process.exec` 受控只读系统检视命令（严格白名单，绝不调用 `sh -c`）。
pub fn exec_controlled_process(
    sandbox_dir: &Path,
    command: &str,
) -> Result<(String, CtxAnnotation, String), CliError> {
    let clean_cmd = validate_process_exec_command(command)?;
    let stdout_report = match clean_cmd.as_str() {
        "workspace.ls" | "workspace.digest" | "workspace.inspect" => {
            let mut entries_list = Vec::new();
            if let Ok(rd) = fs::read_dir(sandbox_dir) {
                for entry in rd.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        entries_list.push(name.to_owned());
                    }
                }
            }
            entries_list.sort();
            format!(
                "controlled_exec={clean_cmd}\nsandbox_entries={}\nfiles=[{}]\ntimestamp={}\n",
                entries_list.len(),
                entries_list.join(", "),
                unix_now_secs()
            )
        }
        "uname" => run_readonly_cmd("uname", &["-a"]),
        "whoami" => run_readonly_cmd("whoami", &[]),
        "date" => run_readonly_cmd("date", &[]),
        "uptime" => run_readonly_cmd("uptime", &[]),
        "df" => run_readonly_cmd("df", &["-h", "."]),
        "sw_vers" => {
            if std::env::consts::OS == "macos" {
                run_readonly_cmd("sw_vers", &[])
            } else {
                run_readonly_cmd("uname", &["-a"])
            }
        }
        _ => {
            let s_dir = sandbox_dir.display().to_string();
            run_readonly_cmd("ls", &["-la", &s_dir])
        }
    };

    let exec_dir = sandbox_dir.join("exec_reports");
    fs::create_dir_all(&exec_dir).map_err(|err| CliError::Io {
        path: exec_dir.display().to_string(),
        detail: err.to_string(),
    })?;
    let rel_path = "exec_reports/last_exec.log";
    fs::write(sandbox_dir.join(rel_path), stdout_report.as_bytes()).map_err(|err| {
        CliError::Io {
            path: rel_path.to_owned(),
            detail: err.to_string(),
        }
    })?;
    let sha = hex_sha256(stdout_report.as_bytes());
    let first_line = stdout_report.lines().next().unwrap_or("ok").trim();
    Ok((
        format!("Executed allowlisted host command '{clean_cmd}': {first_line}"),
        CtxAnnotation::ArtifactPreview {
            title: format!("Allowlisted Host Exec ({clean_cmd})"),
            path: rel_path.to_owned(),
            content_preview: stdout_report.trim().chars().take(480).collect(),
            sha256: sha.clone(),
        },
        sha,
    ))
}

fn run_readonly_cmd(prog: &str, args: &[&str]) -> String {
    match ProcessCommand::new(prog).args(args).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stdout.trim().is_empty() {
                format!("exit={}\n{}", out.status, stderr.trim())
            } else {
                stdout.into_owned()
            }
        }
        Err(err) => format!(
            "{prog} unavailable on this host ({}): {err}",
            std::env::consts::OS
        ),
    }
}

fn read_live_battery_status() -> String {
    if std::env::consts::OS == "macos" {
        if let Ok(out) = ProcessCommand::new("/usr/bin/pmset")
            .args(["-g", "batt"])
            .output()
        {
            if out.status.success() {
                let txt = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = txt
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                if !lines.is_empty() {
                    return lines.join(" | ");
                }
            }
        }
    } else if std::env::consts::OS == "linux" {
        for bat in ["BAT0", "BAT1"] {
            let cap_path = format!("/sys/class/power_supply/{bat}/capacity");
            let st_path = format!("/sys/class/power_supply/{bat}/status");
            if let Ok(cap) = fs::read_to_string(&cap_path) {
                let st = fs::read_to_string(&st_path).unwrap_or_else(|_| "Unknown".to_owned());
                return format!("{}% ({})", cap.trim(), st.trim());
            }
        }
    }
    "AC Power / Desktop Workstation (no battery pack reported)".to_owned()
}

fn read_live_uptime() -> String {
    if matches!(std::env::consts::OS, "macos" | "linux") {
        if let Ok(out) = ProcessCommand::new("uptime").output() {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_owned();
            }
        }
    }
    format!(
        "os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn which_bin(bin_name: &str) -> bool {
    let Some(path_os) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_os).any(|dir| dir.join(bin_name).is_file())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{probe_os_permissions, validate_app_control_params, validate_process_exec_command};
    use crate::device_profile::HostResourcePoolConfig;

    #[test]
    fn validates_app_control_and_process_exec_strictly() {
        assert_eq!(
            validate_app_control_params("Calculator", Some("https://ifay.ai")).expect("ok"),
            "Calculator"
        );
        assert!(validate_app_control_params("Calculator; rm -rf /", None).is_err());
        assert!(validate_app_control_params("../bin/sh", None).is_err());
        assert!(validate_app_control_params("Safari", Some("javascript:alert(1)")).is_err());

        assert_eq!(validate_process_exec_command("uname").expect("ok"), "uname");
        assert!(validate_process_exec_command("rm -rf /").is_err());
    }

    #[test]
    fn probes_all_host_capabilities() {
        let pool = HostResourcePoolConfig::default();
        let items = probe_os_permissions(&pool);
        assert!(items.iter().any(|i| i.capability == "os.clipboard"));
        assert!(items.iter().any(|i| i.capability == "hardware.telemetry"));
        assert!(items.iter().any(|i| i.capability == "app.control"));
    }
}
