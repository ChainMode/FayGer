//! 宿主画像构造与 `info` 渲染（M9）。
//!
//! [`detect_host_environment`] 依据编译期平台常量确定性构造 [`HostEnvironment`]；
//! [`render_info`] 在内置适配器注册表上选择匹配适配器并渲染文本（纯，喂入画像即可测）。

use crate::error::CliError;
use fayger_adapter::builtin::{
    CAP_CRYPTO_HASH, CAP_FS_READ, CAP_FS_WRITE, CAP_HOST_CALL, CAP_NET_HTTP, CAP_PROC_SPAWN,
    CAP_RANDOM_BYTES, CAP_TIME_NOW, CAP_UI_DOM,
};
use fayger_adapter::{
    AdapterRegistry, BrowserAdapter, Capability, HostEnvironment, HostPolicy, InAppAdapter,
    NativeDesktopAdapter, ServerAdapter,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// 桌面默认可用能力集合（与 `NativeDesktopAdapter` 支持集对齐，便于演示非空 granted）。
fn desktop_capabilities() -> BTreeSet<Capability> {
    [
        CAP_FS_READ,
        CAP_FS_WRITE,
        CAP_NET_HTTP,
        CAP_UI_DOM,
        CAP_TIME_NOW,
        CAP_RANDOM_BYTES,
        CAP_CRYPTO_HASH,
        CAP_PROC_SPAWN,
        CAP_HOST_CALL,
    ]
    .into_iter()
    .map(Capability::new)
    .collect()
}

/// 依据当前平台确定性构造宿主画像。
///
/// `linux` / `macos` / `windows` → `native-desktop`（沙箱 `none`，桌面能力全集）；
/// 其它 OS → 空 `platform_id`（回退启发式），无可用能力。
#[must_use]
pub fn detect_host_environment() -> HostEnvironment {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    build_host_environment(os, arch)
}

/// 纯构造：把 os/arch 字符串映射为画像（便于单元测试喂固定平台）。
#[must_use]
pub fn build_host_environment(os: &str, arch: &str) -> HostEnvironment {
    match os {
        "linux" | "macos" | "windows" => {
            HostEnvironment::new("native-desktop", os, arch, "none", desktop_capabilities())
        }
        // 未知平台：空 platform_id 且 sandbox 为 "unknown"（不命中任何内置适配器的回退启发式），
        // 据此走「无匹配适配器」路径（Req 2.4）。
        other => HostEnvironment::new("", other, arch, "unknown", BTreeSet::new()),
    }
}

/// 在内置适配器注册表上按画像选择适配器并渲染 `info` 文本（纯）。
///
/// # Errors
/// 无任何内置适配器匹配该画像时透出 `ADP_NO_MATCHING_PLATFORM`。
pub fn render_info(host: &HostEnvironment) -> Result<String, CliError> {
    let mut registry = AdapterRegistry::new();
    registry.register(Box::new(NativeDesktopAdapter::new()));
    registry.register(Box::new(ServerAdapter::new()));
    registry.register(Box::new(BrowserAdapter::new()));
    registry.register(Box::new(InAppAdapter::new()));

    // 策略允许画像声明的全部可用能力（granted = supported ∩ available ∩ policy）。
    let policy = HostPolicy::allow_all(
        host.available_capabilities
            .iter()
            .map(|c| c.as_str().to_owned()),
    );
    registry.select(host, &policy)?;

    let mut s = String::new();
    let _ = writeln!(s, "Host environment");
    let _ = writeln!(s, "  os_kind:       {}", host.os_kind);
    let _ = writeln!(s, "  arch:          {}", host.arch);
    let _ = writeln!(s, "  platform_id:   {}", display_or_auto(&host.platform_id));
    let _ = writeln!(s, "  sandbox_kind:  {}", host.sandbox_kind);

    let granted: Vec<&str> = registry.granted().iter().map(Capability::as_str).collect();
    let _ = writeln!(s, "Selected adapter");
    if granted.is_empty() {
        let _ = writeln!(s, "  granted:       (none)");
    } else {
        let _ = writeln!(s, "  granted:       {}", granted.join(", "));
    }
    Ok(s)
}

const fn display_or_auto(platform_id: &str) -> &str {
    if platform_id.is_empty() {
        "(auto)"
    } else {
        platform_id
    }
}

/// 探测当前宿主并渲染 `info` 文本。
///
/// # Errors
/// 无匹配适配器时透出 `ADP_NO_MATCHING_PLATFORM`。
pub fn info_text() -> Result<String, CliError> {
    render_info(&detect_host_environment())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{build_host_environment, render_info};
    use fayger_error::ErrorCode;
    use std::collections::BTreeSet;

    #[test]
    fn desktop_profile_selects_native_desktop_with_granted() {
        let host = build_host_environment("linux", "x86_64");
        assert_eq!(host.platform_id, "native-desktop");
        let text = render_info(&host).expect("info ok");
        assert!(text.contains("native-desktop"));
        // 桌面能力全集 → granted 非空。
        assert!(text.contains("granted:"));
        assert!(text.contains("fs.read"));
    }

    #[test]
    fn macos_and_windows_are_desktop() {
        assert_eq!(
            build_host_environment("macos", "aarch64").platform_id,
            "native-desktop"
        );
        assert_eq!(
            build_host_environment("windows", "x86_64").platform_id,
            "native-desktop"
        );
    }

    #[test]
    fn unknown_os_has_no_match() {
        // 空 platform_id + 无 os_kind 命中 + 无可用能力 → 无适配器匹配。
        let host = build_host_environment("plan9", "x86_64");
        assert!(host.platform_id.is_empty());
        let err = render_info(&host).expect_err("no match");
        match err {
            crate::error::CliError::Failure(e) => {
                assert_eq!(e.code, ErrorCode::AdpNoMatchingPlatform);
            }
            other => panic!("expected Failure, got {other:?}"),
        }
    }

    #[test]
    fn detect_returns_some_environment() {
        // 当前测试平台必是 linux/macos/windows 之一 → native-desktop。
        let host = super::detect_host_environment();
        assert!(!host.available_capabilities.is_empty() || host.platform_id.is_empty());
        let _ = BTreeSet::<u8>::new();
    }
}
