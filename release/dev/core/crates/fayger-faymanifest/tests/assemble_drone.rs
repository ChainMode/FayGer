//! Feature: H1-PROTO-02 (FayManifest 草案), 端到端：用蓝图「无人机控制 iFay」样例
//! 验证 `assemble` 解析 → 校验 → 依赖解析的完整链路。
//!
//! 样例取自 ifay 蓝图 §13「Complete Example: Drone Control iFay」，落地于本 crate
//! `examples/drone-controller.faymanifest.json`（只读引用 + 本地副本，叙述权威在 ifay）。

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_faymanifest::{
    assemble_str, ControlMode, DependencyKind, DependencyStatus, FayManifest, InMemoryRegistry,
    Namespace, PackageId, StubRegistry,
};

fn drone_manifest_json() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/drone-controller.faymanifest.json"
    );
    std::fs::read_to_string(path).expect("read drone example manifest")
}

#[test]
fn drone_example_parses_and_validates() {
    let m = FayManifest::from_json_str(&drone_manifest_json()).expect("parse");
    m.validate().expect("validate");
    assert_eq!(m.name, "drone-controller-ifay");
    assert_eq!(m.control_mode, ControlMode::Ego);
    assert_eq!(m.modules.len(), 4);
    assert_eq!(m.protocols.len(), 2);
    assert_eq!(m.drivers.len(), 2);
    assert!(m.ego.is_some());
}

#[test]
fn drone_example_resolves_to_phase_2_with_full_infra() {
    let report = assemble_str(&drone_manifest_json(), &StubRegistry).expect("assemble");

    // 蓝图：无人机示例 -> Phase 2 (Direct Client Takeover)。
    assert_eq!(report.resolved_phase, 2);

    // 蓝图 auto-supplement 表：基础设施六模块 + Faying 协议 + ego_model（因声明了 ego）。
    for m in [
        "fayid",
        "fayger_runtime",
        "ifay_profile",
        "permission_system",
        "security_ethics_compliance",
        "multiterminal_sync",
        "ego_model",
    ] {
        assert!(
            report.auto_supplemented_modules.iter().any(|x| x == m),
            "expected auto-supplemented module: {m}"
        );
    }
    assert!(report
        .auto_supplemented_protocols
        .iter()
        .any(|p| p == "faying_protocol"));

    // 蓝图结语：最终 12 组件 + 3 协议。生效模块 = 4 声明 + 7 自动 = 11；
    // 协议 = 2 声明 + 1 自动 = 3。（"12 组件" 含 FayGer Runtime 等基础设施，
    // 本草案把 FayGer Runtime 计入模块集，故模块数 11；差异说明见 crate 文档。）
    assert_eq!(report.effective_protocols.len(), 3);
    assert!(report.effective_modules.len() >= 11);
}

#[test]
fn drone_example_driver_and_ego_namespaces_recognized() {
    // 命名空间识别：@drone-drivers/... 与 @ego-models/...。
    let dji = PackageId::parse("@drone-drivers/dji-fc").expect("pkg");
    assert_eq!(dji.namespace(), Namespace::DroneDrivers);
    let ego = PackageId::parse("@ego-models/drone-pilot-v1").expect("pkg");
    assert_eq!(ego.namespace(), Namespace::EgoModels);

    let report = assemble_str(&drone_manifest_json(), &StubRegistry).expect("assemble");
    let driver_dep = report
        .dependencies
        .iter()
        .find(|d| {
            d.kind == DependencyKind::DriverPackage && d.identifier == "@drone-drivers/dji-fc"
        })
        .expect("driver dependency present");
    // 注册表未接入 -> 明确标注 stubbed（不误报缺失）。
    assert_eq!(driver_dep.status, DependencyStatus::Stubbed);

    let ego_dep = report
        .dependencies
        .iter()
        .find(|d| d.kind == DependencyKind::EgoModel)
        .expect("ego dependency present");
    assert_eq!(ego_dep.identifier, "@ego-models/drone-pilot-v1");
    assert_eq!(ego_dep.status, DependencyStatus::Stubbed);
}

#[test]
fn drone_example_reports_dependencies_and_gaps() {
    let report = assemble_str(&drone_manifest_json(), &StubRegistry).expect("assemble");
    // stub 注册表下没有 "缺失"（缺失 = 注册表确定不存在），但有 stub 标注。
    assert!(!report.has_missing());
    assert!(report.dependencies.iter().any(|d| d.identifier == "sensor"));
    // 阶段最小集缺口：Phase 2 缺 personal_data_heap。
    assert!(report
        .phase_gaps
        .iter()
        .any(|g| g.phase == 2 && g.missing_modules.iter().any(|m| m == "personal_data_heap")));
    // 报告可渲染为人类可读文本。
    let text = report.render();
    assert!(text.contains("FayManifest assembly report"));
    assert!(text.contains("resolvedPhase: 2"));
    assert!(text.contains("@drone-drivers/dji-fc"));
}

#[test]
fn drone_example_with_in_memory_registry_resolves_versions() {
    // 给一个内存注册表登记部分组件与版本，演示版本约束解析（^1.0.0 命中最高兼容版本）。
    let reg = InMemoryRegistry::new()
        .with_module("device_driver_hub", &["1.0.0", "1.3.0", "2.0.0"])
        .with_package("@drone-drivers/dji-fc", &["3.2.1"]);
    let report = assemble_str(&drone_manifest_json(), &reg).expect("assemble");

    let hub = report
        .dependencies
        .iter()
        .find(|d| d.identifier == "device_driver_hub")
        .expect("hub dep");
    match &hub.status {
        DependencyStatus::Resolved { version: Some(v) } => assert_eq!(v.to_string(), "1.3.0"),
        other => panic!("expected resolved 1.3.0 for ^1.0.0, got {other:?}"),
    }

    let dji = report
        .dependencies
        .iter()
        .find(|d| d.identifier == "@drone-drivers/dji-fc")
        .expect("dji dep");
    assert!(matches!(dji.status, DependencyStatus::Resolved { .. }));

    // 未登记的 sensor 在非 stub 注册表下报告为缺失。
    let sensor = report
        .dependencies
        .iter()
        .find(|d| d.identifier == "sensor")
        .expect("sensor dep");
    assert_eq!(sensor.status, DependencyStatus::Missing);
}
