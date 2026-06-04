//! 端到端桥接运行：把一个 BuF 经真实适配层在所选宿主上跑完生命周期（M10）。

use crate::adapter::BridgeAdapter;
use crate::mapping::InstructionMapping;
use fayger_adapter::{
    AdapterRegistry, BrowserAdapter, HostEnvironment, HostPolicy, HostSyscall, InAppAdapter,
    NativeDesktopAdapter, ServerAdapter,
};
use fayger_buf::object::BuFObject;
use fayger_error::FaygerError;
use fayger_runtime::{LifecycleState, Runtime, RuntimeInterface};

/// 桥接运行报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeReport {
    /// 运行结束时的生命周期状态（正常为 `Terminated`）。
    pub final_state: LifecycleState,
    /// 派发的指令数（等于 section 数）。
    pub dispatched: usize,
    /// 成功翻译产生的宿主调用列表（按派发顺序）。
    pub syscalls: Vec<HostSyscall>,
    /// 失败的派发数（被适配层拒绝的指令数）。
    pub failures: usize,
    /// 被选中适配器的 `platform_id`。
    pub platform_id: String,
}

/// 注册四类内置适配器（稳定顺序：NativeDesktop / Server / Browser / InApp）。
#[must_use]
pub fn register_builtin_adapters() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    registry.register(Box::new(NativeDesktopAdapter::new()));
    registry.register(Box::new(ServerAdapter::new()));
    registry.register(Box::new(BrowserAdapter::new()));
    registry.register(Box::new(InAppAdapter::new()));
    registry
}

/// 端到端桥接运行：注册内置适配器 → 按 `host` 选择 → 在 `Runtime<BridgeAdapter>` 上跑完生命周期。
///
/// # Errors
/// 无匹配适配器时返回 `ADP_NO_MATCHING_PLATFORM`（`source_layer = Adapter`）；运行层步骤
/// 失败时透出对应 `FaygerError`。
pub fn run_bridged(
    obj: BuFObject,
    host: &HostEnvironment,
    policy: &HostPolicy,
    mapping: InstructionMapping,
) -> Result<BridgeReport, FaygerError> {
    let mut registry = register_builtin_adapters();
    registry.select(host, policy)?;
    // 记录被选中适配器的 platform_id：派发一条探针即可从结果取得；这里改为从 host 推断不可靠，
    // 故运行后从首个成功 syscall 取；若无成功 syscall 则回退到对注册表的查询。
    let bridge = BridgeAdapter::new(registry, mapping);

    let mut rt = Runtime::new(bridge);
    let id = rt.load(obj)?;
    rt.init(id)?;
    rt.start(id)?;
    while rt.step(id)? {}
    rt.terminate(id)?;

    let final_state = rt.state(id)?;
    let outcomes = rt.adapter().outcomes();
    let dispatched = outcomes.len();
    let mut syscalls: Vec<HostSyscall> = Vec::new();
    let mut failures = 0usize;
    for outcome in outcomes {
        match outcome {
            Ok(sc) => syscalls.push(sc.clone()),
            Err(_) => failures += 1,
        }
    }
    // platform_id：优先取自首个成功 syscall；否则对一条探针指令再翻译查询适配器。
    let platform_id = syscalls.first().map_or_else(
        || selected_platform_id(rt.adapter()),
        |sc| sc.platform_id.clone(),
    );

    Ok(BridgeReport {
        final_state,
        dispatched,
        syscalls,
        failures,
        platform_id,
    })
}

/// 在无成功 syscall 时，用一条探针指令询问被选中适配器的 platform_id。
///
/// 探针用规范映射（`Io.op0`，无能力要求）——所有内置适配器都支持 `Io.op0`，
/// 且无能力门槛，故只要已可服务即能成功翻译，从而取得 platform_id。
fn selected_platform_id(bridge: &BridgeAdapter) -> String {
    let probe = fayger_runtime::UniversalInstruction::new(0, 0, Vec::new());
    crate::adapter::dispatch_one(
        bridge.registry(),
        &InstructionMapping::canonical_io(),
        &probe,
    )
    .map(|sc| sc.platform_id)
    .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::run_bridged;
    use crate::mapping::InstructionMapping;
    use fayger_adapter::builtin::{
        CAP_CRYPTO_HASH, CAP_FS_READ, CAP_FS_WRITE, CAP_HOST_CALL, CAP_NET_HTTP, CAP_PROC_SPAWN,
        CAP_RANDOM_BYTES, CAP_TIME_NOW, CAP_UI_DOM,
    };
    use fayger_adapter::{Capability, HostEnvironment, HostPolicy};
    use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
    use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
    use fayger_buf::object::{BuFObject, Section};
    use fayger_error::ErrorCode;
    use fayger_runtime::LifecycleState;
    use std::collections::{BTreeMap, BTreeSet};

    fn desktop_caps() -> BTreeSet<Capability> {
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

    fn desktop_host() -> HostEnvironment {
        HostEnvironment::new("native-desktop", "linux", "x86_64", "none", desktop_caps())
    }

    fn desktop_policy() -> HostPolicy {
        HostPolicy::allow_all([
            CAP_FS_READ,
            CAP_FS_WRITE,
            CAP_NET_HTTP,
            CAP_UI_DOM,
            CAP_TIME_NOW,
            CAP_RANDOM_BYTES,
            CAP_CRYPTO_HASH,
            CAP_PROC_SPAWN,
            CAP_HOST_CALL,
        ])
    }

    fn object_with(payloads: &[(&str, Vec<u8>)]) -> BuFObject {
        let descriptors = payloads
            .iter()
            .map(|(id, p)| SectionDescriptor {
                id: (*id).to_owned(),
                kind: SectionKind::Code,
                digest: Digest::placeholder(),
                length: p.len() as u64,
                visibility: SectionVisibility::Required,
                profile_constraints: ProfileConstraints::default(),
            })
            .collect();
        let sections = payloads
            .iter()
            .map(|(id, p)| Section::new(*id, p.clone()))
            .collect();
        BuFObject {
            manifest: BuFManifest {
                schema_version: SemVer::parse("1.0.0").expect("ok"),
                runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
                deprecation_notice: None,
                entry: EntryPoint::new("main"),
                runtime: None,
                capabilities: BTreeSet::new(),
                quotas: None,
                sections: descriptors,
                signature: None,
                extensions: BTreeMap::new(),
            },
            sections,
            raw_header: fayger_buf::format::Header {
                format_version: fayger_buf::format::FORMAT_VERSION_M0,
                manifest_offset: 0,
                manifest_length: 0,
                section_index_offset: 0,
                section_index_length: 0,
            },
            raw_trailer: fayger_buf::format::Trailer::placeholder(0),
        }
    }

    #[test]
    fn desktop_canonical_run_terminates_with_syscalls() {
        let obj = object_with(&[("a", vec![1, 2]), ("b", vec![3])]);
        let report = run_bridged(
            obj,
            &desktop_host(),
            &desktop_policy(),
            InstructionMapping::canonical_io(),
        )
        .expect("run");
        assert_eq!(report.final_state, LifecycleState::Terminated);
        assert_eq!(report.dispatched, 2);
        assert_eq!(report.syscalls.len(), 2);
        assert_eq!(report.failures, 0);
        assert_eq!(report.platform_id, "native-desktop");
        // args 等于段长小端（运行层派生程序 operands = payload 长度）。
        assert_eq!(report.syscalls[0].args, 2u32.to_le_bytes().to_vec());
        assert_eq!(report.syscalls[1].args, 1u32.to_le_bytes().to_vec());
    }

    #[test]
    fn no_matching_adapter_errors() {
        let host = HostEnvironment::new("", "plan9", "x86_64", "unknown", BTreeSet::new());
        let err = run_bridged(
            object_with(&[("a", vec![1])]),
            &host,
            &HostPolicy::default(),
            InstructionMapping::canonical_io(),
        )
        .expect_err("no match");
        assert_eq!(err.code, ErrorCode::AdpNoMatchingPlatform);
    }

    #[test]
    fn capability_denied_counts_as_failure() {
        // 桌面宿主，但映射要求一个未授予能力 → 每条派发都被拒。
        let obj = object_with(&[("a", vec![1]), ("b", vec![2])]);
        let host = HostEnvironment::new(
            "native-desktop",
            "linux",
            "x86_64",
            "none",
            std::iter::once(Capability::new(CAP_HOST_CALL)).collect(),
        );
        let policy = HostPolicy::allow_all([CAP_HOST_CALL]);
        let mapping = InstructionMapping::canonical_io().requiring(["proc.spawn"]);
        let report = run_bridged(obj, &host, &policy, mapping).expect("run");
        assert_eq!(report.final_state, LifecycleState::Terminated);
        assert_eq!(report.dispatched, 2);
        assert_eq!(report.failures, 2);
        assert!(report.syscalls.is_empty());
        // 无成功 syscall，platform_id 经探针取得仍为 native-desktop（探针无能力要求）。
        assert_eq!(report.platform_id, "native-desktop");
    }
}
