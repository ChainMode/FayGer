//! 宿主环境画像、检测器与宿主调用/事件占位类型。
//!
//! [`HostEnvironment`] 是适配器选择与能力裁剪的输入。[`HostSyscall`] 是适配器翻译
//! Universal_Instruction 的产出（M3 为确定性 mock，不触达真实系统调用）。

use crate::capability::Capability;
use std::collections::BTreeSet;

/// 宿主环境画像。
///
/// `platform_id` 若非空，适配器 `matches` 会优先比对它（消除桌面/服务器歧义）；
/// 为空时回退到 `os_kind` / `sandbox_kind` 启发式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEnvironment {
    /// 宿主明确指定的平台标识（如 `native-desktop` / `server` / `browser` / `inapp`）；可为空。
    pub platform_id: String,
    /// 操作系统类别（如 `linux` / `macos` / `windows` / `browser` / `inapp`）。
    pub os_kind: String,
    /// 体系结构（如 `x86_64` / `aarch64` / `wasm32`）。
    pub arch: String,
    /// 沙箱类别（如 `none` / `browser` / `inapp`）。
    pub sandbox_kind: String,
    /// 宿主实际可用的能力集合。
    pub available_capabilities: BTreeSet<Capability>,
}

impl HostEnvironment {
    /// 用各字段构造一个画像。
    #[must_use]
    pub fn new(
        platform_id: impl Into<String>,
        os_kind: impl Into<String>,
        arch: impl Into<String>,
        sandbox_kind: impl Into<String>,
        available_capabilities: BTreeSet<Capability>,
    ) -> Self {
        Self {
            platform_id: platform_id.into(),
            os_kind: os_kind.into(),
            arch: arch.into(),
            sandbox_kind: sandbox_kind.into(),
            available_capabilities,
        }
    }
}

/// 宿主调用描述（适配器翻译产出）。M3 为确定性 mock 数据结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSyscall {
    /// 由哪个适配器翻译（其 `platform_id`）。
    pub platform_id: String,
    /// 调用名，形如 `io.op7`（`{category}.op{opcode}`）。
    pub name: String,
    /// 调用参数，M3 直传指令的 operands。
    pub args: Vec<u8>,
}

/// 宿主原始事件占位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEvent {
    /// 原始类别编号（0..=7 映射回 InstructionCategory，越界归 Host）。
    pub raw_category: u8,
    /// 原始事件码。
    pub raw_opcode: u16,
    /// 原始负载。
    pub raw_payload: Vec<u8>,
}

/// 宿主环境检测器。
pub trait HostEnvironmentDetector {
    /// 产出当前宿主环境画像。
    fn detect(&self) -> HostEnvironment;
}

/// 固定画像检测器：回放注入的 [`HostEnvironment`]。便于测试与确定性。
#[derive(Debug, Clone)]
pub struct FixedDetector(pub HostEnvironment);

impl HostEnvironmentDetector for FixedDetector {
    fn detect(&self) -> HostEnvironment {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{FixedDetector, HostEnvironment, HostEnvironmentDetector};
    use std::collections::BTreeSet;

    #[test]
    fn fixed_detector_replays_injected_profile() {
        let env = HostEnvironment::new("browser", "browser", "wasm32", "browser", BTreeSet::new());
        let detector = FixedDetector(env.clone());
        assert_eq!(detector.detect(), env);
    }
}
