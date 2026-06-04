//! M3 端到端：构造宿主画像 → 选中内置适配器 → 能力裁剪 → 派发一组指令并断言。
//!
//! Validates: Requirements 10.2, 10.5

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_adapter::{
    AdapterRegistry, BrowserAdapter, Capability, HostEnvironment, HostPolicy, InAppAdapter,
    InstructionCategory, NativeDesktopAdapter, PlatformAdapter, ServerAdapter,
    UniversalInstruction,
};
use fayger_error::ErrorCode;
use std::collections::BTreeSet;

fn caps(items: &[&str]) -> BTreeSet<Capability> {
    items.iter().map(|s| Capability::new(*s)).collect()
}

fn full_registry() -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(Box::new(NativeDesktopAdapter::new()));
    r.register(Box::new(ServerAdapter::new()));
    r.register(Box::new(BrowserAdapter::new()));
    r.register(Box::new(InAppAdapter::new()));
    r
}

#[test]
fn browser_host_selects_browser_and_negotiates() {
    // 浏览器画像：sandbox=browser，可用 net.http / time.now（无 proc.spawn）。
    let host = HostEnvironment::new(
        "browser",
        "browser",
        "wasm32",
        "browser",
        caps(&["net.http", "time.now"]),
    );
    // 策略允许 net.http / time.now / proc.spawn（但 available 无 proc.spawn）。
    let policy = HostPolicy::allow_all(["net.http", "time.now", "proc.spawn"]);

    let mut registry = full_registry();
    registry.select(&host, &policy).expect("select browser");
    assert!(registry.is_serviceable());

    // granted = browser.supported ∩ available({net.http,time.now}) ∩ policy = {net.http, time.now}
    assert_eq!(registry.granted(), &caps(&["net.http", "time.now"]));

    // BuF 必需 net.http → 通过
    registry
        .check_required(&caps(&["net.http"]))
        .expect("net.http granted");
    // BuF 必需 proc.spawn → 缺失拒绝
    let denied = registry
        .check_required(&caps(&["proc.spawn"]))
        .expect_err("proc.spawn missing");
    assert_eq!(denied.code, ErrorCode::AdpCapabilityDenied);

    // 派发一条受支持且能力满足的 Net 指令 → HostSyscall
    let net_key = BrowserAdapter::new()
        .descriptor()
        .supported_instructions
        .into_iter()
        .find(|k| k.category == InstructionCategory::Net)
        .expect("browser supports Net");
    let net_instr = UniversalInstruction::new(
        net_key.category,
        net_key.opcode,
        vec![0xab],
        caps(&["net.http"]),
    );
    let syscall = registry.dispatch(&net_instr).expect("dispatch net");
    assert_eq!(syscall.platform_id, "browser");
    assert_eq!(syscall.args, vec![0xab]);

    // 派发一条要求 proc.spawn 的指令 → ADP_CAPABILITY_DENIED（能力先于支持判定）
    let proc_instr =
        UniversalInstruction::new(InstructionCategory::Proc, 0, vec![], caps(&["proc.spawn"]));
    let err = registry.dispatch(&proc_instr).expect_err("proc denied");
    assert_eq!(err.code, ErrorCode::AdpCapabilityDenied);

    // 派发一条无能力要求但类别不被 Browser 支持的 Proc 指令 → ADP_UNSUPPORTED_INSTRUCTION
    let proc_no_cap =
        UniversalInstruction::new(InstructionCategory::Proc, 0, vec![], BTreeSet::new());
    let err2 = registry
        .dispatch(&proc_no_cap)
        .expect_err("proc unsupported");
    assert_eq!(err2.code, ErrorCode::AdpUnsupportedInstruction);
}

#[test]
fn no_matching_platform_blocks_service() {
    let mut registry = full_registry();
    let host = HostEnvironment::new("toaster", "toaster-os", "z80", "none", BTreeSet::new());
    let err = registry
        .select(&host, &HostPolicy::default())
        .expect_err("no adapter for toaster");
    assert_eq!(err.code, ErrorCode::AdpNoMatchingPlatform);
    assert!(!registry.is_serviceable());

    // 未可服务时派发 → ADP_NO_MATCHING_PLATFORM
    let instr = UniversalInstruction::new(InstructionCategory::Host, 7, vec![], BTreeSet::new());
    let err2 = registry.dispatch(&instr).expect_err("not serviceable");
    assert_eq!(err2.code, ErrorCode::AdpNoMatchingPlatform);
}
