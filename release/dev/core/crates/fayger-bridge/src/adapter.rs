//! 桥接适配器：实现运行层 `Adapter` trait，把派发的指令经适配层翻译/裁剪/翻译为 HostSyscall（M10）。
//!
//! [`BridgeAdapter`] 是运行层与适配层的接缝实现：运行层在 `step` 时调用
//! `Adapter::dispatch`，桥接把运行层指令翻译为适配层指令并经 [`AdapterRegistry`] 派发，
//! 把每次结果记录为 [`DispatchOutcome`]。运行结束后由 [`crate::run::run_bridged`] 读回汇总。

use crate::mapping::{to_adapter_instruction, InstructionMapping};
use fayger_adapter::{AdapterRegistry, HostSyscall};
use fayger_error::FaygerError;
use fayger_runtime::{Adapter, UniversalInstruction as RtInstruction};

/// 一次桥接派发的结果：成功的 [`HostSyscall`] 或适配层 `ADP_*` 错误。
pub type DispatchOutcome = Result<HostSyscall, FaygerError>;

/// 桥接适配器：持有一个已选中的 [`AdapterRegistry`] 与一个 [`InstructionMapping`]。
///
/// 实现运行层 [`Adapter`] trait；每收到一条运行层指令即翻译并经注册表派发，记录结果。
pub struct BridgeAdapter {
    registry: AdapterRegistry,
    mapping: InstructionMapping,
    outcomes: Vec<DispatchOutcome>,
}

impl BridgeAdapter {
    /// 用一个**已 `select`** 的注册表与映射构造。
    #[must_use]
    pub const fn new(registry: AdapterRegistry, mapping: InstructionMapping) -> Self {
        Self {
            registry,
            mapping,
            outcomes: Vec::new(),
        }
    }

    /// 按派发顺序记录的全部派发结果。
    pub fn outcomes(&self) -> &[DispatchOutcome] {
        &self.outcomes
    }

    /// 不可变借用底层注册表（用于查询 granted 等）。
    #[must_use]
    pub const fn registry(&self) -> &AdapterRegistry {
        &self.registry
    }
}

impl std::fmt::Debug for BridgeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeAdapter")
            .field("mapping", &self.mapping)
            .field("outcomes", &self.outcomes.len())
            .finish_non_exhaustive()
    }
}

impl Adapter for BridgeAdapter {
    fn dispatch(&mut self, instr: &RtInstruction) {
        let outcome = dispatch_one(&self.registry, &self.mapping, instr);
        self.outcomes.push(outcome);
    }
}

/// 单条桥接派发（纯，给定一个已 `select` 的注册表与映射）。
///
/// 翻译运行层指令为适配层指令并经 `registry.dispatch` 派发。能力裁剪与支持判定语义与
/// 适配层完全一致（能力检查先于支持判定）。
pub fn dispatch_one(
    registry: &AdapterRegistry,
    mapping: &InstructionMapping,
    instr: &RtInstruction,
) -> DispatchOutcome {
    let adp = to_adapter_instruction(instr, mapping);
    registry.dispatch(&adp)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{dispatch_one, BridgeAdapter};
    use crate::mapping::InstructionMapping;
    use fayger_adapter::builtin::{CAP_HOST_CALL, CAP_PROC_SPAWN};
    use fayger_adapter::{
        AdapterRegistry, BrowserAdapter, Capability, HostEnvironment, HostPolicy,
        NativeDesktopAdapter,
    };
    use fayger_error::ErrorCode;
    use fayger_runtime::{Adapter, UniversalInstruction as RtInstruction};
    use std::collections::BTreeSet;

    fn caps(items: &[&str]) -> BTreeSet<Capability> {
        items.iter().map(|s| Capability::new(*s)).collect()
    }

    fn desktop_registry() -> AdapterRegistry {
        let mut r = AdapterRegistry::new();
        r.register(Box::new(NativeDesktopAdapter::new()));
        let host = HostEnvironment::new(
            "native-desktop",
            "linux",
            "x86_64",
            "none",
            caps(&[CAP_HOST_CALL, CAP_PROC_SPAWN]),
        );
        r.select(
            &host,
            &HostPolicy::allow_all([CAP_HOST_CALL, CAP_PROC_SPAWN]),
        )
        .expect("select");
        r
    }

    #[test]
    fn dispatch_one_ok_yields_syscall() {
        let r = desktop_registry();
        let outcome = dispatch_one(
            &r,
            &InstructionMapping::canonical_io(),
            &RtInstruction::new(0, 0, vec![1, 2, 3]),
        );
        let sc = outcome.expect("ok");
        assert_eq!(sc.platform_id, "native-desktop");
        assert_eq!(sc.args, vec![1, 2, 3]);
    }

    #[test]
    fn dispatch_one_capability_denied() {
        let r = desktop_registry();
        // 要求一个未授予的能力（host 只给了 host.call/proc.spawn）。
        let mapping = InstructionMapping::canonical_io().requiring(["fs.write"]);
        let outcome = dispatch_one(&r, &mapping, &RtInstruction::new(0, 0, vec![]));
        let err = outcome.expect_err("denied");
        assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);
    }

    #[test]
    fn bridge_adapter_records_each_outcome() {
        let mut bridge = BridgeAdapter::new(desktop_registry(), InstructionMapping::canonical_io());
        bridge.dispatch(&RtInstruction::new(0, 0, vec![1]));
        bridge.dispatch(&RtInstruction::new(0, 0, vec![2, 3]));
        assert_eq!(bridge.outcomes().len(), 2);
        assert!(bridge.outcomes().iter().all(Result::is_ok));
    }

    #[test]
    fn unsupported_instruction_on_browser() {
        // Browser 不支持 Proc 类别；映射到 Proc 且无能力要求（避免先被能力拦截）。
        let mut r = AdapterRegistry::new();
        r.register(Box::new(BrowserAdapter::new()));
        let host = HostEnvironment::new("browser", "browser", "wasm32", "browser", BTreeSet::new());
        r.select(&host, &HostPolicy::default()).expect("select");
        let mapping = InstructionMapping {
            category: Some(fayger_adapter::InstructionCategory::Proc),
            opcode: Some(0),
            capabilities_required: BTreeSet::new(),
        };
        let outcome = dispatch_one(&r, &mapping, &RtInstruction::new(0, 0, vec![]));
        let err = outcome.expect_err("unsupported");
        assert_eq!(err.code, ErrorCode::AdpUnsupportedInstruction);
    }
}
