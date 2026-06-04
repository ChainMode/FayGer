//! 适配器注册表：选择、能力门槛与指令派发。
//!
//! [`AdapterRegistry`] 管理一组 [`PlatformAdapter`]，依 [`HostEnvironment`] 选中匹配者
//! 进入可服务状态，并在选中时基于宿主可用能力与策略算出 `granted`。派发时对每条指令
//! 做能力检查与支持判定。

use crate::adapter::PlatformAdapter;
use crate::capability::{Capability, HostPolicy};
use crate::host::{HostEnvironment, HostSyscall};
use crate::instruction::UniversalInstruction;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use std::collections::BTreeSet;

/// 适配器注册表。
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn PlatformAdapter>>,
    selected: Option<usize>,
    granted: BTreeSet<Capability>,
}

impl AdapterRegistry {
    /// 构造空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
            selected: None,
            granted: BTreeSet::new(),
        }
    }

    /// 注册一个适配器（按注册顺序参与选择）。
    pub fn register(&mut self, adapter: Box<dyn PlatformAdapter>) {
        self.adapters.push(adapter);
    }

    /// 依宿主环境选择适配器并进入可服务状态。
    ///
    /// 选中后计算 `granted = adapter.supported_capabilities ∩ host.available_capabilities ∩ policy.allowed`。
    ///
    /// # Errors
    /// 无任何适配器 `matches(host)` 为真时返回 `ADP_NO_MATCHING_PLATFORM`。
    pub fn select(
        &mut self,
        host: &HostEnvironment,
        policy: &HostPolicy,
    ) -> Result<(), FaygerError> {
        let idx = self.adapters.iter().position(|a| a.matches(host));
        let Some(idx) = idx else {
            return Err(FaygerError::new(
                ErrorCode::AdpNoMatchingPlatform,
                Layer::Adapter,
                "no registered adapter matches the host environment",
            )
            .with_context(
                "platform_id",
                ContextValue::string(host.platform_id.clone()),
            ));
        };
        let supported = self.adapters[idx].descriptor().supported_capabilities;
        // granted = supported ∩ available ∩ policy.allowed
        self.granted = supported
            .iter()
            .filter(|c| host.available_capabilities.contains(*c) && policy.allowed.contains(*c))
            .cloned()
            .collect();
        self.selected = Some(idx);
        Ok(())
    }

    /// 是否已进入可服务状态。
    #[must_use]
    pub const fn is_serviceable(&self) -> bool {
        self.selected.is_some()
    }

    /// 当前授予的能力集合（select 后有效）。
    #[must_use]
    pub const fn granted(&self) -> &BTreeSet<Capability> {
        &self.granted
    }

    /// 校验一组 BuF 必需能力是否被授予。
    ///
    /// # Errors
    /// `required \ granted ≠ ∅` 时返回 `ADP_CAPABILITY_DENIED`，`context.missing` 为差集。
    pub fn check_required(&self, required: &BTreeSet<Capability>) -> Result<(), FaygerError> {
        let missing: BTreeSet<Capability> = required.difference(&self.granted).cloned().collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(capability_denied(&missing))
        }
    }

    /// 派发一条指令：未可服务 → `ADP_NO_MATCHING_PLATFORM`；能力不足 →
    /// `ADP_CAPABILITY_DENIED`；指令不被支持 → `ADP_UNSUPPORTED_INSTRUCTION`；否则翻译。
    ///
    /// # Errors
    /// 见上述三类错误码。
    pub fn dispatch(&self, instr: &UniversalInstruction) -> Result<HostSyscall, FaygerError> {
        let Some(idx) = self.selected else {
            return Err(FaygerError::new(
                ErrorCode::AdpNoMatchingPlatform,
                Layer::Adapter,
                "adapter registry is not serviceable; call select first",
            ));
        };
        // 能力检查：指令所需能力必须全部在 granted 中。
        let missing: BTreeSet<Capability> = instr
            .capabilities_required
            .difference(&self.granted)
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(capability_denied(&missing));
        }
        // 支持判定 + 翻译（适配器内部判定 supported_instructions）。
        self.adapters[idx].translate(instr)
    }
}

/// 构造 `ADP_CAPABILITY_DENIED` 错误，`missing` 为未满足的能力集合（有序列表）。
fn capability_denied(missing: &BTreeSet<Capability>) -> FaygerError {
    let list = missing
        .iter()
        .map(|c| ContextValue::string(c.as_str()))
        .collect();
    FaygerError::new(
        ErrorCode::AdpCapabilityDenied,
        Layer::Adapter,
        "required capabilities are not granted on the current host",
    )
    .with_context("missing", ContextValue::List(list))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::AdapterRegistry;
    use crate::adapter::PlatformAdapter;
    use crate::builtin::{
        BrowserAdapter, NativeDesktopAdapter, ServerAdapter, CAP_NET_HTTP, CAP_PROC_SPAWN,
    };
    use crate::capability::{Capability, HostPolicy};
    use crate::host::HostEnvironment;
    use crate::instruction::{InstructionCategory, InstructionKey, UniversalInstruction};
    use fayger_error::ErrorCode;
    use std::collections::BTreeSet;

    fn caps(items: &[&str]) -> BTreeSet<Capability> {
        items.iter().map(|s| Capability::new(*s)).collect()
    }

    fn browser_host(available: &[&str]) -> HostEnvironment {
        HostEnvironment::new("browser", "browser", "wasm32", "browser", caps(available))
    }

    fn full_registry() -> AdapterRegistry {
        let mut r = AdapterRegistry::new();
        r.register(Box::new(NativeDesktopAdapter::new()));
        r.register(Box::new(ServerAdapter::new()));
        r.register(Box::new(BrowserAdapter::new()));
        r
    }

    #[test]
    fn select_no_match_errors() {
        let mut r = full_registry();
        let host = HostEnvironment::new("unknown-platform", "exotic", "x", "none", BTreeSet::new());
        let err = r
            .select(&host, &HostPolicy::default())
            .expect_err("no match");
        assert_eq!(err.code, ErrorCode::AdpNoMatchingPlatform);
        assert!(!r.is_serviceable());
    }

    #[test]
    fn select_match_enters_serviceable_and_computes_granted() {
        let mut r = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        let policy = HostPolicy::allow_all([CAP_NET_HTTP, CAP_PROC_SPAWN]);
        r.select(&host, &policy).expect("select");
        assert!(r.is_serviceable());
        // granted = browser.supported ∩ available({net.http}) ∩ policy({net.http,proc.spawn}) = {net.http}
        assert_eq!(r.granted(), &caps(&[CAP_NET_HTTP]));
    }

    #[test]
    fn select_is_deterministic() {
        let mut r1 = full_registry();
        let mut r2 = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        let policy = HostPolicy::allow_all([CAP_NET_HTTP]);
        r1.select(&host, &policy).unwrap();
        r2.select(&host, &policy).unwrap();
        assert_eq!(r1.granted(), r2.granted());
    }

    #[test]
    fn check_required_passes_when_subset() {
        let mut r = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        r.select(&host, &HostPolicy::allow_all([CAP_NET_HTTP]))
            .unwrap();
        r.check_required(&caps(&[CAP_NET_HTTP])).expect("granted");
    }

    #[test]
    fn check_required_fails_with_missing() {
        let mut r = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        r.select(&host, &HostPolicy::allow_all([CAP_NET_HTTP]))
            .unwrap();
        let err = r
            .check_required(&caps(&[CAP_PROC_SPAWN]))
            .expect_err("missing");
        assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
        assert!(err.context.contains_key("missing"));
    }

    #[test]
    fn dispatch_not_serviceable_errors() {
        let r = full_registry();
        let instr = UniversalInstruction::new(InstructionCategory::Net, 1, vec![], BTreeSet::new());
        let err = r.dispatch(&instr).expect_err("not serviceable");
        assert_eq!(err.code, ErrorCode::AdpNoMatchingPlatform);
    }

    #[test]
    fn dispatch_capability_denied() {
        let mut r = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        r.select(&host, &HostPolicy::allow_all([CAP_NET_HTTP]))
            .unwrap();
        // 指令要求 proc.spawn，但未授予
        let instr =
            UniversalInstruction::new(InstructionCategory::Net, 1, vec![], caps(&[CAP_PROC_SPAWN]));
        let err = r.dispatch(&instr).expect_err("denied");
        assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
    }

    #[test]
    fn dispatch_unsupported_instruction() {
        let mut r = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        r.select(&host, &HostPolicy::allow_all([CAP_NET_HTTP]))
            .unwrap();
        // Proc 类别在 Browser 适配器不支持；指令无能力要求（避免先被能力拦截）。
        let instr =
            UniversalInstruction::new(InstructionCategory::Proc, 0, vec![], BTreeSet::new());
        let err = r.dispatch(&instr).expect_err("unsupported");
        assert_eq!(err.code, ErrorCode::AdpUnsupportedInstruction);
    }

    #[test]
    fn dispatch_supported_yields_syscall() {
        let mut r = full_registry();
        let host = browser_host(&[CAP_NET_HTTP]);
        r.select(&host, &HostPolicy::allow_all([CAP_NET_HTTP]))
            .unwrap();
        // Net 类别 opcode 1 是 Browser 支持的指令之一（keys_for 中 Net 的 opcode）。
        // 这里用无能力要求的 Net 指令，opcode 取 Browser supported 中的 Net key。
        let net_key = BrowserAdapter::new()
            .descriptor()
            .supported_instructions
            .into_iter()
            .find(|k| k.category == InstructionCategory::Net)
            .expect("browser supports a Net instruction");
        let instr = UniversalInstruction::new(
            net_key.category,
            net_key.opcode,
            vec![1, 2],
            BTreeSet::new(),
        );
        let sc = r.dispatch(&instr).expect("syscall");
        assert_eq!(sc.platform_id, "browser");
        assert_eq!(sc.args, vec![1, 2]);
        let _ = InstructionKey::new(InstructionCategory::Net, 0); // 触及 InstructionKey 使用
    }
}
