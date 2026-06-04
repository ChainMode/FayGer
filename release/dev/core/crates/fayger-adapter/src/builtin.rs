//! 四类内置 Platform_Adapter：NativeDesktop / Server / Browser / InApp。
//!
//! 每类适配器用内部 `BuiltinAdapter` 实例化，差异仅在描述符（能力矩阵、支持指令）
//! 与匹配的宿主画像。`translate` / `normalize` 复用 [`crate::adapter`] 的 helper。
//!
//! M3 的能力矩阵见设计文档；`translate` 产出确定性 mock HostSyscall，不触达真实宿主。

use crate::adapter::{
    normalize_default, translate_via_descriptor, AdapterDescriptor, PlatformAdapter,
};
use crate::capability::Capability;
use crate::host::{HostEnvironment, HostEvent, HostSyscall};
use crate::instruction::{
    InstructionCategory, InstructionKey, UniversalEvent, UniversalInstruction,
};
use fayger_error::FaygerError;
use std::collections::BTreeSet;

// ----------------------- 稳定能力常量 -----------------------

/// 文件读能力。
pub const CAP_FS_READ: &str = "fs.read";
/// 文件写能力。
pub const CAP_FS_WRITE: &str = "fs.write";
/// HTTP 网络能力。
pub const CAP_NET_HTTP: &str = "net.http";
/// DOM / UI 能力。
pub const CAP_UI_DOM: &str = "ui.dom";
/// 时钟能力。
pub const CAP_TIME_NOW: &str = "time.now";
/// 随机数能力。
pub const CAP_RANDOM_BYTES: &str = "random.bytes";
/// 加密哈希能力。
pub const CAP_CRYPTO_HASH: &str = "crypto.hash";
/// 进程创建能力。
pub const CAP_PROC_SPAWN: &str = "proc.spawn";
/// 宿主调用能力。
pub const CAP_HOST_CALL: &str = "host.call";

fn caps(items: &[&str]) -> BTreeSet<Capability> {
    items.iter().map(|s| Capability::new(*s)).collect()
}

/// 每个能力对应一条代表性指令 key（category, opcode）。opcode 用类内递增编号。
fn keys_for(categories: &[InstructionCategory]) -> BTreeSet<InstructionKey> {
    categories
        .iter()
        .enumerate()
        .map(|(i, &c)| InstructionKey::new(c, u16::try_from(i).unwrap_or(0)))
        .collect()
}

/// 内置适配器的统一实现。
#[derive(Debug, Clone)]
pub(crate) struct BuiltinAdapter {
    descriptor: AdapterDescriptor,
    /// 本适配器负责的 os_kind 集合（platform_id 为空时的回退启发式）。
    match_os_kinds: Vec<String>,
    /// 本适配器负责的 sandbox_kind 集合（回退启发式）。
    match_sandboxes: Vec<String>,
}

impl BuiltinAdapter {
    fn matches_impl(&self, host: &HostEnvironment) -> bool {
        // 优先比对 platform_id（消除桌面/服务器歧义）。
        if !host.platform_id.is_empty() {
            return host.platform_id == self.descriptor.platform_id;
        }
        // 回退：os_kind 或 sandbox_kind 命中。
        self.match_os_kinds.iter().any(|k| k == &host.os_kind)
            || self.match_sandboxes.iter().any(|s| s == &host.sandbox_kind)
    }
}

impl PlatformAdapter for BuiltinAdapter {
    fn descriptor(&self) -> AdapterDescriptor {
        self.descriptor.clone()
    }
    fn matches(&self, host: &HostEnvironment) -> bool {
        self.matches_impl(host)
    }
    fn translate(&self, instr: &UniversalInstruction) -> Result<HostSyscall, FaygerError> {
        translate_via_descriptor(&self.descriptor, instr)
    }
    fn normalize(&self, event: &HostEvent) -> Result<UniversalEvent, FaygerError> {
        Ok(normalize_default(event))
    }
}

// ----------------------- 四类内置适配器 -----------------------

/// 原生桌面适配器（Linux / macOS / Windows，能力全集）。
#[derive(Debug, Clone)]
pub struct NativeDesktopAdapter(BuiltinAdapter);

impl Default for NativeDesktopAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeDesktopAdapter {
    /// 构造原生桌面适配器。
    #[must_use]
    pub fn new() -> Self {
        let supported_capabilities = caps(&[
            CAP_FS_READ,
            CAP_FS_WRITE,
            CAP_NET_HTTP,
            CAP_UI_DOM,
            CAP_TIME_NOW,
            CAP_RANDOM_BYTES,
            CAP_CRYPTO_HASH,
            CAP_PROC_SPAWN,
            CAP_HOST_CALL,
        ]);
        let supported_instructions = keys_for(&InstructionCategory::all());
        Self(BuiltinAdapter {
            descriptor: AdapterDescriptor {
                platform_id: "native-desktop".into(),
                supported_capabilities,
                supported_instructions,
            },
            match_os_kinds: vec!["linux".into(), "macos".into(), "windows".into()],
            match_sandboxes: vec!["none".into()],
        })
    }
}

/// 服务器适配器（无 GUI，禁用 ui.*）。
#[derive(Debug, Clone)]
pub struct ServerAdapter(BuiltinAdapter);

impl Default for ServerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerAdapter {
    /// 构造服务器适配器。
    #[must_use]
    pub fn new() -> Self {
        let supported_capabilities = caps(&[
            CAP_FS_READ,
            CAP_FS_WRITE,
            CAP_NET_HTTP,
            CAP_TIME_NOW,
            CAP_RANDOM_BYTES,
            CAP_CRYPTO_HASH,
            CAP_PROC_SPAWN,
            CAP_HOST_CALL,
        ]);
        // 除 Ui 外的 7 类。
        let supported_instructions = keys_for(&[
            InstructionCategory::Io,
            InstructionCategory::Net,
            InstructionCategory::Time,
            InstructionCategory::Random,
            InstructionCategory::Crypto,
            InstructionCategory::Proc,
            InstructionCategory::Host,
        ]);
        Self(BuiltinAdapter {
            descriptor: AdapterDescriptor {
                platform_id: "server".into(),
                supported_capabilities,
                supported_instructions,
            },
            match_os_kinds: vec![],
            match_sandboxes: vec![],
        })
    }
}

/// 浏览器适配器（沙箱：禁用 proc.*，io/net 受限）。
#[derive(Debug, Clone)]
pub struct BrowserAdapter(BuiltinAdapter);

impl Default for BrowserAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserAdapter {
    /// 构造浏览器适配器。
    #[must_use]
    pub fn new() -> Self {
        // 受限：无 fs.write、无 proc.spawn；保留 fs.read（OPFS 等）、net.http（fetch）。
        let supported_capabilities = caps(&[
            CAP_FS_READ,
            CAP_NET_HTTP,
            CAP_UI_DOM,
            CAP_TIME_NOW,
            CAP_RANDOM_BYTES,
            CAP_CRYPTO_HASH,
            CAP_HOST_CALL,
        ]);
        // 无 Proc 类别。
        let supported_instructions = keys_for(&[
            InstructionCategory::Io,
            InstructionCategory::Net,
            InstructionCategory::Ui,
            InstructionCategory::Time,
            InstructionCategory::Random,
            InstructionCategory::Crypto,
            InstructionCategory::Host,
        ]);
        Self(BuiltinAdapter {
            descriptor: AdapterDescriptor {
                platform_id: "browser".into(),
                supported_capabilities,
                supported_instructions,
            },
            match_os_kinds: vec!["browser".into()],
            match_sandboxes: vec!["browser".into()],
        })
    }
}

/// 宿主 In-App 适配器（最严格，默认仅 host.call）。
#[derive(Debug, Clone)]
pub struct InAppAdapter(BuiltinAdapter);

impl Default for InAppAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl InAppAdapter {
    /// 构造默认 In-App 适配器（仅 `host.call`）。
    #[must_use]
    pub fn new() -> Self {
        Self::with_injected(caps(&[CAP_HOST_CALL]))
    }

    /// 用宿主显式注入的能力集合构造 In-App 适配器。
    ///
    /// 支持指令固定为 `Host` 类基础指令；能力由注入集合决定。
    #[must_use]
    pub fn with_injected(injected: BTreeSet<Capability>) -> Self {
        let supported_instructions = keys_for(&[InstructionCategory::Host]);
        Self(BuiltinAdapter {
            descriptor: AdapterDescriptor {
                platform_id: "inapp".into(),
                supported_capabilities: injected,
                supported_instructions,
            },
            match_os_kinds: vec!["inapp".into()],
            match_sandboxes: vec!["inapp".into()],
        })
    }
}

// 让四类适配器都实现 PlatformAdapter（委托给内部 BuiltinAdapter）。
macro_rules! delegate_adapter {
    ($t:ty) => {
        impl PlatformAdapter for $t {
            fn descriptor(&self) -> AdapterDescriptor {
                self.0.descriptor()
            }
            fn matches(&self, host: &HostEnvironment) -> bool {
                self.0.matches(host)
            }
            fn translate(&self, instr: &UniversalInstruction) -> Result<HostSyscall, FaygerError> {
                self.0.translate(instr)
            }
            fn normalize(&self, event: &HostEvent) -> Result<UniversalEvent, FaygerError> {
                self.0.normalize(event)
            }
        }
    };
}

delegate_adapter!(NativeDesktopAdapter);
delegate_adapter!(ServerAdapter);
delegate_adapter!(BrowserAdapter);
delegate_adapter!(InAppAdapter);

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        BrowserAdapter, InAppAdapter, NativeDesktopAdapter, ServerAdapter, CAP_HOST_CALL,
        CAP_PROC_SPAWN, CAP_UI_DOM,
    };
    use crate::adapter::PlatformAdapter;
    use crate::capability::Capability;
    use crate::host::HostEnvironment;
    use std::collections::BTreeSet;

    fn host(platform_id: &str, os: &str, sandbox: &str) -> HostEnvironment {
        HostEnvironment::new(platform_id, os, "x86_64", sandbox, BTreeSet::new())
    }

    #[test]
    fn platform_ids_are_stable() {
        assert_eq!(
            NativeDesktopAdapter::new().descriptor().platform_id,
            "native-desktop"
        );
        assert_eq!(ServerAdapter::new().descriptor().platform_id, "server");
        assert_eq!(BrowserAdapter::new().descriptor().platform_id, "browser");
        assert_eq!(InAppAdapter::new().descriptor().platform_id, "inapp");
    }

    #[test]
    fn native_desktop_has_full_capabilities() {
        let d = NativeDesktopAdapter::new().descriptor();
        assert_eq!(d.supported_capabilities.len(), 9);
        assert_eq!(d.supported_instructions.len(), 8);
    }

    #[test]
    fn server_has_no_ui() {
        let d = ServerAdapter::new().descriptor();
        assert!(!d
            .supported_capabilities
            .contains(&Capability::new(CAP_UI_DOM)));
    }

    #[test]
    fn browser_has_no_proc() {
        let d = BrowserAdapter::new().descriptor();
        assert!(!d
            .supported_capabilities
            .contains(&Capability::new(CAP_PROC_SPAWN)));
    }

    #[test]
    fn inapp_default_only_host_call() {
        let d = InAppAdapter::new().descriptor();
        assert_eq!(
            d.supported_capabilities,
            std::iter::once(Capability::new(CAP_HOST_CALL)).collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn matches_prefers_platform_id() {
        let server = ServerAdapter::new();
        // platform_id 指定 server，即便 os 是 linux，也只匹配 server 适配器
        assert!(server.matches(&host("server", "linux", "none")));
        assert!(!NativeDesktopAdapter::new().matches(&host("server", "linux", "none")));
    }

    #[test]
    fn matches_falls_back_to_os_when_platform_id_empty() {
        let native = NativeDesktopAdapter::new();
        assert!(native.matches(&host("", "linux", "none")));
        let browser = BrowserAdapter::new();
        assert!(browser.matches(&host("", "browser", "browser")));
    }
}
