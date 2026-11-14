//! 依赖解析算法与装配报告（草案）。
//!
//! [`resolve`] 把一个已校验的 [`FayManifest`] 装配为 [`AssemblyReport`]：
//!
//! 1. **自动补充基础设施**（蓝图 §13 "Automatic Dependency Supplementation"）：无论是否声明，
//!    所有 iFay 都补充 FayID / FayGer Runtime / iFay Profile / Permission System /
//!    Security & Ethics Compliance / Multi-terminal Synchronization / Faying Protocol；
//!    若声明了 `ego`，再补充 `ego_model` 模块。
//! 2. **判定解析阶段**（Resolved Stage）：依据声明的业务组件落在蓝图各 Phase 的最小组件集
//!    （§13 "Phase-wise Minimum Component Configuration"）取最高 Phase。
//! 3. **解析依赖**：对每个生效组件 / 驱动包 / Ego 模型来源查询 [`Registry`] 并应用版本约束，
//!    给出 `Resolved / Unsatisfiable / Missing / Stubbed` 状态。
//! 4. **报告阶段最小集缺口**：对 1..=解析阶段，列出尚未满足的最小组件（信息性）。

use crate::constraint::VersionConstraint;
use crate::model::{ControlMode, FayManifest};
use crate::registry::{Registry, ResolutionOutcome};
use fayger_buf::manifest::SemVer;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

// ----------------------- 基础设施与阶段目录 -----------------------

/// 始终自动补充的基础设施模块（蓝图 §13）。
pub const INFRA_MODULES: &[&str] = &[
    "fayid",
    "fayger_runtime",
    "ifay_profile",
    "permission_system",
    "security_ethics_compliance",
    "multiterminal_sync",
];

/// 始终自动补充的基础设施协议。
pub const INFRA_PROTOCOLS: &[&str] = &["faying_protocol"];

/// 声明 `ego` 时自动补充的模块。
pub const EGO_MODULE: &str = "ego_model";

/// 各 Phase 新增的最小模块集（蓝图 §13 表）。索引 0 = Phase 1。
const PHASE_MODULES: [&[&str]; 5] = [
    &[
        "fayid",
        "credential_management",
        "first_person_tracer",
        "simulated_operation",
        "ego_model",
    ],
    &[
        "sensor",
        "device_driver_hub",
        "registered_skill",
        "invoke_skill",
        "personal_data_heap",
    ],
    &["external_knowledge"],
    &[
        "self_awareness",
        "self_driven_behavior",
        "internal_skill",
        "aligned_consciousness",
    ],
    &["gmchain", "meritoken"],
];

/// 各 Phase 新增的最小协议集（蓝图 §13 表）。索引 0 = Phase 1。
const PHASE_PROTOCOLS: [&[&str]; 5] = [
    &["faying_protocol", "interactive_dialogue_protocol"],
    &["cap_protocol", "dtp_protocol"],
    &["ssp_protocol"],
    &["telepathy_protocol"],
    &[],
];

/// Phase 的人类可读名称。
#[must_use]
pub const fn phase_name(phase: u8) -> &'static str {
    match phase {
        1 => "Simulated Human Operation",
        2 => "Direct Client Takeover",
        3 => "Virtual World Interface",
        4 => "Full Anthropomorphization",
        5 => "Value Distribution",
        _ => "Unknown",
    }
}

/// 模块 id 所属 Phase（1..=5）；不在目录中返回 `None`。
#[must_use]
pub fn module_phase(id: &str) -> Option<u8> {
    catalog_phase(&PHASE_MODULES, id)
}

/// 协议 id 所属 Phase（1..=5）；不在目录中返回 `None`。
#[must_use]
pub fn protocol_phase(id: &str) -> Option<u8> {
    catalog_phase(&PHASE_PROTOCOLS, id)
}

fn catalog_phase(table: &[&[&str]; 5], id: &str) -> Option<u8> {
    table.iter().enumerate().find_map(|(i, set)| {
        if set.contains(&id) {
            u8::try_from(i + 1).ok()
        } else {
            None
        }
    })
}

// ----------------------- 报告类型 -----------------------

/// 组件来源：开发者声明 vs 系统自动补充。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentOrigin {
    /// 由开发者在 FayManifest 中显式声明。
    Declared,
    /// 由系统自动补充（基础设施 / ego_model）。
    AutoSupplemented,
}

impl ComponentOrigin {
    const fn label(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::AutoSupplemented => "auto",
        }
    }
}

/// 依赖项种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    /// 模块。
    Module,
    /// 协议。
    Protocol,
    /// 驱动包（命名空间标识）。
    DriverPackage,
    /// Ego 模型来源（命名空间标识）。
    EgoModel,
}

impl DependencyKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Protocol => "protocol",
            Self::DriverPackage => "driver",
            Self::EgoModel => "ego",
        }
    }
}

/// 单个依赖项的解析状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyStatus {
    /// 已解析到满足约束的版本（`None` 表示已知但注册表未给版本信息）。
    Resolved {
        /// 选中的版本。
        version: Option<SemVer>,
    },
    /// 注册表已知该项，但无版本满足约束。
    Unsatisfiable {
        /// 注册表登记的可用版本。
        available: Vec<SemVer>,
    },
    /// 注册表确定不存在该项（缺失）。
    Missing,
    /// 注册表未接入该类查询（H1 占位，真实后端见 H2）。
    Stubbed,
}

impl DependencyStatus {
    fn render(&self) -> String {
        match self {
            Self::Resolved { version: Some(v) } => format!("resolved -> {v}"),
            Self::Resolved { version: None } => "resolved (registry gave no version)".to_owned(),
            Self::Unsatisfiable { available } => {
                let list: Vec<String> = available.iter().map(ToString::to_string).collect();
                format!("UNSATISFIABLE (available: {})", list.join(", "))
            }
            Self::Missing => "MISSING (not in registry)".to_owned(),
            Self::Stubbed => "stubbed (registry not connected; H2)".to_owned(),
        }
    }
}

/// 一条依赖解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyResolution {
    /// 依赖种类。
    pub kind: DependencyKind,
    /// 标识（组件 id 或 `@scope/name`）。
    pub identifier: String,
    /// 来源（声明 / 自动补充）。仅模块/协议有意义；驱动与 Ego 恒为 `Declared`。
    pub origin: ComponentOrigin,
    /// 渲染后的版本约束（若有）。
    pub constraint: Option<String>,
    /// 解析状态。
    pub status: DependencyStatus,
}

/// 某 Phase 最小组件集尚未满足的缺口（信息性）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseGap {
    /// Phase 序号。
    pub phase: u8,
    /// 缺失的模块。
    pub missing_modules: Vec<String>,
    /// 缺失的协议。
    pub missing_protocols: Vec<String>,
}

/// 装配报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyReport {
    /// 实现名称。
    pub name: String,
    /// 实现版本。
    pub version: String,
    /// 厂商。
    pub vendor: String,
    /// 控制模式。
    pub control_mode: ControlMode,
    /// 解析阶段（1..=5）。
    pub resolved_phase: u8,
    /// 声明的模块 id（排序）。
    pub declared_modules: Vec<String>,
    /// 声明的协议 id（排序）。
    pub declared_protocols: Vec<String>,
    /// 自动补充的模块 id（排序）。
    pub auto_supplemented_modules: Vec<String>,
    /// 自动补充的协议 id（排序）。
    pub auto_supplemented_protocols: Vec<String>,
    /// 生效（声明 ∪ 自动）模块 id（排序）。
    pub effective_modules: Vec<String>,
    /// 生效（声明 ∪ 自动）协议 id（排序）。
    pub effective_protocols: Vec<String>,
    /// 逐依赖解析结果。
    pub dependencies: Vec<DependencyResolution>,
    /// 阶段最小集缺口。
    pub phase_gaps: Vec<PhaseGap>,
    /// 注册表是否为占位实现（用于报告抬头）。
    pub registry_is_stub: bool,
}

impl AssemblyReport {
    /// 依赖中是否存在缺失项（注册表确定不存在）。stubbed 不计入缺失。
    #[must_use]
    pub fn has_missing(&self) -> bool {
        self.dependencies
            .iter()
            .any(|d| matches!(d.status, DependencyStatus::Missing))
    }

    /// 依赖中是否存在不可满足项（版本约束无解）。
    #[must_use]
    pub fn has_unsatisfiable(&self) -> bool {
        self.dependencies
            .iter()
            .any(|d| matches!(d.status, DependencyStatus::Unsatisfiable { .. }))
    }

    /// 渲染为可读文本（供 CLI 输出）。
    #[must_use]
    pub fn render(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "FayManifest assembly report");
        let _ = writeln!(s, "  name:          {}", self.name);
        let _ = writeln!(s, "  version:       {}", self.version);
        let _ = writeln!(s, "  vendor:        {}", self.vendor);
        let _ = writeln!(s, "  controlMode:   {}", self.control_mode.as_str());
        let _ = writeln!(
            s,
            "  resolvedPhase: {} ({})",
            self.resolved_phase,
            phase_name(self.resolved_phase)
        );
        let _ = writeln!(s);
        render_id_line(&mut s, "Declared modules", &self.declared_modules);
        render_id_line(&mut s, "Declared protocols", &self.declared_protocols);
        render_id_line(
            &mut s,
            "Auto-supplemented modules",
            &self.auto_supplemented_modules,
        );
        render_id_line(
            &mut s,
            "Auto-supplemented protocols",
            &self.auto_supplemented_protocols,
        );

        let _ = writeln!(s);
        let reg = if self.registry_is_stub {
            "stub (H2 not yet implemented)"
        } else {
            "in-memory"
        };
        let _ = writeln!(s, "Dependency resolution (registry: {reg}):");
        for d in &self.dependencies {
            let constraint = d
                .constraint
                .as_ref()
                .map_or_else(String::new, |c| format!(" {c}"));
            let _ = writeln!(
                s,
                "  [{:<8}] {:<32} ({}){}  ->  {}",
                d.kind.label(),
                d.identifier,
                d.origin.label(),
                constraint,
                d.status.render()
            );
        }

        let _ = writeln!(s);
        if self.phase_gaps.is_empty() {
            let _ = writeln!(
                s,
                "Phase minimum components: all satisfied for phases 1..={}.",
                self.resolved_phase
            );
        } else {
            let _ = writeln!(s, "Phase minimum gaps (informational):");
            for g in &self.phase_gaps {
                let mut parts: Vec<String> = Vec::new();
                if !g.missing_modules.is_empty() {
                    parts.push(format!("missing modules: {}", g.missing_modules.join(", ")));
                }
                if !g.missing_protocols.is_empty() {
                    parts.push(format!(
                        "missing protocols: {}",
                        g.missing_protocols.join(", ")
                    ));
                }
                let _ = writeln!(
                    s,
                    "  Phase {} ({}): {}",
                    g.phase,
                    phase_name(g.phase),
                    parts.join("; ")
                );
            }
        }
        s
    }
}

fn render_id_line(s: &mut String, label: &str, ids: &[String]) {
    let body = if ids.is_empty() {
        "(none)".to_owned()
    } else {
        ids.join(", ")
    };
    let _ = writeln!(s, "{label} ({}): {body}", ids.len());
}

// ----------------------- 解析算法 -----------------------

/// 把已校验的 FayManifest 解析为装配报告。
///
/// 前置条件：`manifest` 已通过 [`FayManifest::validate`]（版本约束与命名空间已合法）。
/// 本函数不返回错误——它产出一份**报告**（缺失/不可满足/stub 体现在依赖状态里）。
#[must_use]
pub fn resolve(manifest: &FayManifest, registry: &dyn Registry) -> AssemblyReport {
    let registry_is_stub = registry.is_stub();
    // 声明集合。
    let declared_modules: BTreeSet<String> =
        manifest.modules.iter().map(|m| m.id.clone()).collect();
    let declared_protocols: BTreeSet<String> =
        manifest.protocols.iter().map(|p| p.id.clone()).collect();

    // 自动补充集合（去掉已声明的，避免重复计入 auto）。
    let mut auto_modules: BTreeSet<String> =
        INFRA_MODULES.iter().map(|s| (*s).to_owned()).collect();
    if manifest.ego.is_some() {
        auto_modules.insert(EGO_MODULE.to_owned());
    }
    auto_modules.retain(|m| !declared_modules.contains(m));

    let mut auto_protocols: BTreeSet<String> =
        INFRA_PROTOCOLS.iter().map(|s| (*s).to_owned()).collect();
    auto_protocols.retain(|p| !declared_protocols.contains(p));

    // 生效集合 = 声明 ∪ 自动。
    let effective_modules: BTreeSet<String> =
        declared_modules.union(&auto_modules).cloned().collect();
    let effective_protocols: BTreeSet<String> =
        declared_protocols.union(&auto_protocols).cloned().collect();

    // 解析阶段：取声明业务组件的最高 Phase（缺省 Phase 1）。
    let resolved_phase = compute_phase(&declared_modules, &declared_protocols);

    // 约束映射（来自声明）。
    let module_constraints = constraint_map(manifest.modules.iter().map(|m| (&m.id, &m.version)));
    let protocol_constraints =
        constraint_map(manifest.protocols.iter().map(|p| (&p.id, &p.version)));

    // 逐依赖解析。
    let mut dependencies: Vec<DependencyResolution> = Vec::new();
    for id in &effective_modules {
        let constraint = module_constraints.get(id).cloned().flatten();
        let origin = origin_of(id, &declared_modules);
        dependencies.push(DependencyResolution {
            kind: DependencyKind::Module,
            identifier: id.clone(),
            origin,
            constraint: constraint.as_ref().map(VersionConstraint::render),
            status: resolve_one(registry.lookup_module(id), constraint.as_ref()),
        });
    }
    for id in &effective_protocols {
        let constraint = protocol_constraints.get(id).cloned().flatten();
        let origin = origin_of(id, &declared_protocols);
        dependencies.push(DependencyResolution {
            kind: DependencyKind::Protocol,
            identifier: id.clone(),
            origin,
            constraint: constraint.as_ref().map(VersionConstraint::render),
            status: resolve_one(registry.lookup_protocol(id), constraint.as_ref()),
        });
    }
    // 驱动包（保持声明顺序，去重）。
    let mut seen_pkgs: BTreeSet<String> = BTreeSet::new();
    for d in &manifest.drivers {
        if !seen_pkgs.insert(d.driver_package.clone()) {
            continue;
        }
        dependencies.push(DependencyResolution {
            kind: DependencyKind::DriverPackage,
            identifier: d.driver_package.clone(),
            origin: ComponentOrigin::Declared,
            constraint: None,
            status: resolve_one(registry.lookup_package(&d.driver_package), None),
        });
    }
    if let Some(ego) = &manifest.ego {
        dependencies.push(DependencyResolution {
            kind: DependencyKind::EgoModel,
            identifier: ego.model_source.clone(),
            origin: ComponentOrigin::Declared,
            constraint: None,
            status: resolve_one(registry.lookup_package(&ego.model_source), None),
        });
    }

    let phase_gaps = compute_phase_gaps(resolved_phase, &effective_modules, &effective_protocols);

    AssemblyReport {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        vendor: manifest.vendor.clone(),
        control_mode: manifest.control_mode,
        resolved_phase,
        declared_modules: declared_modules.into_iter().collect(),
        declared_protocols: declared_protocols.into_iter().collect(),
        auto_supplemented_modules: auto_modules.into_iter().collect(),
        auto_supplemented_protocols: auto_protocols.into_iter().collect(),
        effective_modules: effective_modules.into_iter().collect(),
        effective_protocols: effective_protocols.into_iter().collect(),
        dependencies,
        phase_gaps,
        registry_is_stub,
    }
}

fn origin_of(id: &str, declared: &BTreeSet<String>) -> ComponentOrigin {
    if declared.contains(id) {
        ComponentOrigin::Declared
    } else {
        ComponentOrigin::AutoSupplemented
    }
}

fn compute_phase(modules: &BTreeSet<String>, protocols: &BTreeSet<String>) -> u8 {
    let m = modules.iter().filter_map(|id| module_phase(id)).max();
    let p = protocols.iter().filter_map(|id| protocol_phase(id)).max();
    m.into_iter().chain(p).max().unwrap_or(1)
}

fn compute_phase_gaps(
    resolved_phase: u8,
    effective_modules: &BTreeSet<String>,
    effective_protocols: &BTreeSet<String>,
) -> Vec<PhaseGap> {
    let mut gaps: Vec<PhaseGap> = Vec::new();
    for phase in 1..=resolved_phase {
        let idx = (phase - 1) as usize;
        let missing_modules: Vec<String> = PHASE_MODULES[idx]
            .iter()
            .filter(|m| !effective_modules.contains(**m))
            .map(|m| (*m).to_owned())
            .collect();
        let missing_protocols: Vec<String> = PHASE_PROTOCOLS[idx]
            .iter()
            .filter(|p| !effective_protocols.contains(**p))
            .map(|p| (*p).to_owned())
            .collect();
        if !missing_modules.is_empty() || !missing_protocols.is_empty() {
            gaps.push(PhaseGap {
                phase,
                missing_modules,
                missing_protocols,
            });
        }
    }
    gaps
}

fn constraint_map<'a>(
    items: impl Iterator<Item = (&'a String, &'a Option<String>)>,
) -> BTreeMap<String, Option<VersionConstraint>> {
    items
        .map(|(id, raw)| {
            let c = raw
                .as_ref()
                .map(|s| VersionConstraint::parse(s).unwrap_or(VersionConstraint::Any));
            (id.clone(), c)
        })
        .collect()
}

fn resolve_one(
    outcome: ResolutionOutcome,
    constraint: Option<&VersionConstraint>,
) -> DependencyStatus {
    match outcome {
        ResolutionOutcome::Stub => DependencyStatus::Stubbed,
        ResolutionOutcome::Unknown => DependencyStatus::Missing,
        ResolutionOutcome::Known(versions) => {
            if versions.is_empty() {
                return DependencyStatus::Resolved { version: None };
            }
            let best = versions
                .iter()
                .filter(|&v| constraint.is_none_or(|c| c.satisfies(v)))
                .max()
                .cloned();
            best.map_or(
                DependencyStatus::Unsatisfiable {
                    available: versions,
                },
                |v| DependencyStatus::Resolved { version: Some(v) },
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{module_phase, protocol_phase, resolve, DependencyStatus};
    use crate::model::FayManifest;
    use crate::registry::{InMemoryRegistry, StubRegistry};

    fn parse(s: &str) -> FayManifest {
        let m = FayManifest::from_json_str(s).expect("parse");
        m.validate().expect("valid");
        m
    }

    const DRONE: &str = r#"{
        "name": "drone", "version": "1.0.0", "description": "d", "vendor": "v",
        "modules": [
            {"id":"device_driver_hub","version":"^1.0.0"},
            {"id":"sensor"},
            {"id":"invoke_skill"},
            {"id":"registered_skill"}
        ],
        "protocols": [{"id":"cap_protocol"},{"id":"dtp_protocol"}],
        "controlMode": "ego",
        "drivers": [{"name":"DJI","type":"device","driverPackage":"@drone-drivers/dji-fc"}],
        "ego": {"modelSource":"@ego-models/drone-pilot-v1"}
    }"#;

    #[test]
    fn phase_catalog_lookups() {
        assert_eq!(module_phase("device_driver_hub"), Some(2));
        assert_eq!(module_phase("fayid"), Some(1));
        assert_eq!(protocol_phase("cap_protocol"), Some(2));
        assert_eq!(protocol_phase("telepathy_protocol"), Some(4));
        assert_eq!(module_phase("not_a_module"), None);
    }

    #[test]
    fn drone_resolves_to_phase_2() {
        let m = parse(DRONE);
        let report = resolve(&m, &StubRegistry);
        assert_eq!(report.resolved_phase, 2);
    }

    #[test]
    fn ego_triggers_ego_model_auto_supplement() {
        let m = parse(DRONE);
        let report = resolve(&m, &StubRegistry);
        assert!(report
            .auto_supplemented_modules
            .contains(&"ego_model".to_owned()));
        // 基础设施七件套（含 faying_protocol 协议）。
        assert!(report
            .auto_supplemented_modules
            .contains(&"fayid".to_owned()));
        assert!(report
            .auto_supplemented_protocols
            .contains(&"faying_protocol".to_owned()));
    }

    #[test]
    fn no_ego_means_no_ego_model() {
        let s = r#"{ "name":"x","version":"1.0.0","description":"d","vendor":"v",
            "modules":[{"id":"sensor"}],"protocols":[{"id":"cap_protocol"}],"controlMode":"command" }"#;
        let m = parse(s);
        let report = resolve(&m, &StubRegistry);
        assert!(!report
            .auto_supplemented_modules
            .contains(&"ego_model".to_owned()));
    }

    #[test]
    fn stub_registry_marks_everything_stubbed() {
        let m = parse(DRONE);
        let report = resolve(&m, &StubRegistry);
        assert!(report
            .dependencies
            .iter()
            .all(|d| matches!(d.status, DependencyStatus::Stubbed)));
        assert!(!report.has_missing());
    }

    #[test]
    fn in_memory_registry_resolves_constraint_to_highest_satisfying() {
        let m = parse(DRONE);
        let reg =
            InMemoryRegistry::new().with_module("device_driver_hub", &["1.0.0", "1.4.2", "2.0.0"]);
        let report = resolve(&m, &reg);
        let dep = report
            .dependencies
            .iter()
            .find(|d| d.identifier == "device_driver_hub")
            .expect("dep present");
        // ^1.0.0 应选 <2.0.0 的最高版本 1.4.2。
        match &dep.status {
            DependencyStatus::Resolved { version: Some(v) } => assert_eq!(v.to_string(), "1.4.2"),
            other => panic!("expected resolved 1.4.2, got {other:?}"),
        }
    }

    #[test]
    fn in_memory_registry_reports_unsatisfiable_and_missing() {
        let m = parse(DRONE);
        let reg = InMemoryRegistry::new().with_module("device_driver_hub", &["2.0.0"]); // ^1.0.0 无解
        let report = resolve(&m, &reg);
        assert!(report.has_unsatisfiable());
        // sensor 未登记 -> Missing。
        let sensor = report
            .dependencies
            .iter()
            .find(|d| d.identifier == "sensor")
            .expect("sensor dep");
        assert_eq!(sensor.status, DependencyStatus::Missing);
    }

    #[test]
    fn drone_phase_gaps_match_blueprint_minimums() {
        let m = parse(DRONE);
        let report = resolve(&m, &StubRegistry);
        // Phase 1 缺：credential_management / first_person_tracer / simulated_operation 模块，
        // interactive_dialogue_protocol 协议；fayid + ego_model + faying_protocol 已自动补充。
        let p1 = report
            .phase_gaps
            .iter()
            .find(|g| g.phase == 1)
            .expect("phase 1 gap");
        assert!(p1
            .missing_modules
            .contains(&"simulated_operation".to_owned()));
        assert!(!p1.missing_modules.contains(&"fayid".to_owned()));
        assert!(p1
            .missing_protocols
            .contains(&"interactive_dialogue_protocol".to_owned()));
        // Phase 2 缺：personal_data_heap。
        let p2 = report
            .phase_gaps
            .iter()
            .find(|g| g.phase == 2)
            .expect("phase 2 gap");
        assert!(p2
            .missing_modules
            .contains(&"personal_data_heap".to_owned()));
    }
}
