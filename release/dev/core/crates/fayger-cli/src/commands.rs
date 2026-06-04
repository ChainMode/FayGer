//! 命令核心逻辑（纯，不触达文件系统）（M9）。
//!
//! 这里集中 inspect / run / pack / demo 的纯函数实现，便于单元测试与 PBT 直接覆盖。
//! 文件读写由 [`crate::fs_ops`] 薄封装，命令编排在 [`crate::run_cli`] 中拼接。

use crate::error::CliError;
use fayger_adapter::HostPolicy;
use fayger_bridge::{run_bridged, BridgeReport, InstructionMapping};
use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_loader::{load, seal, LoadPolicy};
use fayger_observability::{EventCategory, EventFilter, FaygerEvent};
use fayger_runtime::{LifecycleState, MockAdapter, Runtime, RuntimeInterface};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::rc::Rc;

/// CLI 默认加载策略：schema 支持区间 `[1.0.0, 1.9.9]`，运行层提供 `1.0.0`，无弃用集。
#[must_use]
pub fn default_policy() -> LoadPolicy {
    LoadPolicy::new(
        (semver("1.0.0"), semver("1.9.9")),
        BTreeSet::new(),
        semver("1.0.0"),
    )
}

/// 解析一个保证合法的 SemVer 字面量（仅用于内部常量）。
fn semver(s: &str) -> SemVer {
    // 内部仅以编译期已知的合法字面量调用；解析失败属于编程错误。
    SemVer::parse(s).unwrap_or(SemVer {
        major: 1,
        minor: 0,
        patch: 0,
        pre: None,
    })
}

/// 把 kind 字符串映射为 [`SectionKind`]。未知 kind 归 [`SectionKind::Data`]
/// （解析阶段已校验 kind ∈ {code,data,asset}，此处只在已校验输入上调用）。
fn section_kind(kind: &str) -> SectionKind {
    match kind {
        "code" => SectionKind::Code,
        "asset" => SectionKind::Asset,
        _ => SectionKind::Data,
    }
}

/// 对一个已加载的 BuFObject 产出可读 Manifest 摘要（纯）。
#[must_use]
pub fn inspect_object(obj: &BuFObject) -> String {
    let m = &obj.manifest;
    let mut s = String::new();
    let _ = writeln!(s, "BuF Manifest");
    let _ = writeln!(s, "  schema_version:        {}", m.schema_version);
    let _ = writeln!(s, "  runtime_interface_min: {}", m.runtime_interface_min);
    let _ = writeln!(s, "  entry:                 {}", m.entry.as_str());
    let _ = writeln!(s, "  sections:              {}", m.sections.len());
    for d in &m.sections {
        let _ = writeln!(
            s,
            "    - {} [{}] len={} {}",
            d.id,
            kind_label(&d.kind),
            d.length,
            visibility_label(d.visibility),
        );
    }
    if m.capabilities.is_empty() {
        let _ = writeln!(s, "  capabilities:          (none)");
    } else {
        let caps: Vec<&str> = m.capabilities.iter().map(String::as_str).collect();
        let _ = writeln!(s, "  capabilities:          {}", caps.join(", "));
    }
    match &m.quotas {
        Some(q) => {
            let _ = writeln!(
                s,
                "  quotas:                cpu_ms_per_sec={:?} memory_bytes={:?} io_bytes_per_sec={:?}",
                q.cpu_ms_per_sec, q.memory_bytes, q.io_bytes_per_sec
            );
        }
        None => {
            let _ = writeln!(s, "  quotas:                (none)");
        }
    }
    let _ = writeln!(
        s,
        "  signature:             {}",
        if m.signature.is_some() {
            "present"
        } else {
            "absent"
        }
    );
    if let Some(notice) = &m.deprecation_notice {
        let _ = writeln!(s, "  deprecation:           {notice}");
    }
    s
}

fn kind_label(kind: &SectionKind) -> String {
    match kind {
        SectionKind::Code => "code".to_owned(),
        SectionKind::Data => "data".to_owned(),
        SectionKind::Asset => "asset".to_owned(),
        SectionKind::Signature => "signature".to_owned(),
        SectionKind::Custom(name) => format!("custom:{name}"),
    }
}

const fn visibility_label(v: SectionVisibility) -> &'static str {
    match v {
        SectionVisibility::Required => "required",
        SectionVisibility::Optional => "optional",
    }
}

/// 运行一个 BuF 实例后的结构化报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// 运行结束时的生命周期状态（正常为 `Terminated`）。
    pub final_state: LifecycleState,
    /// 派发的指令数（等于 section 数）。
    pub dispatched: usize,
    /// 发出的生命周期事件数。
    pub lifecycle_events: usize,
    /// 发出的配额事件数。
    pub quota_events: usize,
}

impl RunReport {
    /// 渲染为可读文本。
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "Run report\n  final_state:      {:?}\n  dispatched:       {}\n  lifecycle_events: {}\n  quota_events:     {}",
            self.final_state, self.dispatched, self.lifecycle_events, self.quota_events
        )
    }
}

/// 在运行层参考实现上跑完整生命周期，返回 [`RunReport`]（纯，不触达 FS）。
///
/// 流程：建 `Runtime` + 订阅事件总线 → `load` → `init` → `start` → `step` 直至程序耗尽
/// → `terminate`。
///
/// # Errors
/// 运行层任一步骤返回 `FaygerError` 时透出（参考实现稳定，正常输入不会触发）。
pub fn run_object(obj: BuFObject) -> Result<RunReport, CliError> {
    let mut rt = Runtime::new(MockAdapter::new());

    let sink: Rc<RefCell<Vec<FaygerEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::clone(&sink);
    rt.events_mut().subscribe(
        EventFilter::any(),
        Box::new(move |e| s.borrow_mut().push(e.clone())),
    );

    let id = rt.load(obj)?;
    rt.init(id)?;
    rt.start(id)?;
    let mut dispatched = 0usize;
    while rt.step(id)? {
        dispatched += 1;
    }
    rt.terminate(id)?;

    let final_state = rt.state(id)?;
    let events = sink.borrow();
    let lifecycle_events = events
        .iter()
        .filter(|e| e.category == EventCategory::Lifecycle)
        .count();
    let quota_events = events
        .iter()
        .filter(|e| e.category == EventCategory::Quota)
        .count();

    Ok(RunReport {
        final_state,
        dispatched,
        lifecycle_events,
        quota_events,
    })
}

/// 由 `entry` 与 `(id, kind, payload)` 列表产出可加载的 BuF 字节流（纯，不触达 FS）。
///
/// schema / runtime_min 取 `1.0.0`；每段构造 `SectionDescriptor`（可见性 Required，约束默认，
/// digest 占位）与 `Section`；随后 `seal` + `serialize`。产出经 [`load()`] 必然成功。
///
/// # Errors
/// 若构造出的对象未通过 `seal`（如 section id 重复 / entry 为空），透出 `FaygerError`。
pub fn pack_bytes(
    entry: &str,
    sections: &[(String, String, Vec<u8>)],
) -> Result<Vec<u8>, CliError> {
    let descriptors: Vec<SectionDescriptor> = sections
        .iter()
        .map(|(id, kind, payload)| SectionDescriptor {
            id: id.clone(),
            kind: section_kind(kind),
            digest: Digest::placeholder(),
            length: payload.len() as u64,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let secs: Vec<Section> = sections
        .iter()
        .map(|(id, _, payload)| Section::new(id.clone(), payload.clone()))
        .collect();

    let manifest = BuFManifest {
        schema_version: semver("1.0.0"),
        runtime_interface_min: semver("1.0.0"),
        deprecation_notice: None,
        entry: EntryPoint::new(entry),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: descriptors,
        signature: None,
        extensions: BTreeMap::new(),
    };
    let mut obj = BuFObject {
        manifest,
        sections: secs,
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    };
    seal(&mut obj)?;
    let bytes = fayger_buf::serialize(&obj)?;
    Ok(bytes)
}

/// 构造一个内存示例 BuF（demo 用，不触达 FS）。
///
/// 含两个 section（一个 code、一个 data），entry 为 `main`，已封装为可加载形态。
#[must_use]
pub fn demo_object() -> BuFObject {
    // demo 输入固定合法：pack_bytes 不会失败。
    let bytes = pack_bytes(
        "main",
        &[
            (
                "code".to_owned(),
                "code".to_owned(),
                b"fay-demo-code".to_vec(),
            ),
            ("data".to_owned(), "data".to_owned(), b"hello".to_vec()),
        ],
    )
    .unwrap_or_default();
    load(&bytes, &default_policy()).unwrap_or_else(|_| empty_object())
}

/// 经真实适配层在当前宿主上运行一个已加载 BuF，返回桥接报告（M10）。
///
/// 用 [`crate::host_profile::detect_host_environment`] 的画像 + 全画像能力策略 + 规范映射
/// （`Io.op0`）调用 [`fayger_bridge::run_bridged`]。
///
/// # Errors
/// 无匹配适配器（`ADP_NO_MATCHING_PLATFORM`）或运行层步骤失败时透出 `FaygerError`。
pub fn bridge_object(obj: BuFObject) -> Result<BridgeReport, CliError> {
    let host = crate::host_profile::detect_host_environment();
    let policy = HostPolicy::allow_all(
        host.available_capabilities
            .iter()
            .map(|c| c.as_str().to_owned()),
    );
    let report = run_bridged(obj, &host, &policy, InstructionMapping::canonical_io())?;
    Ok(report)
}

/// 渲染桥接报告为可读文本。
#[must_use]
pub fn render_bridge_report(report: &BridgeReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "Bridge report");
    let _ = writeln!(s, "  platform_id:  {}", report.platform_id);
    let _ = writeln!(s, "  final_state:  {:?}", report.final_state);
    let _ = writeln!(s, "  dispatched:   {}", report.dispatched);
    let _ = writeln!(s, "  failures:     {}", report.failures);
    let _ = writeln!(s, "  host syscalls:");
    if report.syscalls.is_empty() {
        let _ = writeln!(s, "    (none)");
    } else {
        for sc in &report.syscalls {
            let _ = writeln!(s, "    - {} args={} bytes", sc.name, sc.args.len());
        }
    }
    s
}

/// 兜底空对象（仅当 demo 构造异常时使用，正常路径不会触发）。
fn empty_object() -> BuFObject {
    let manifest = BuFManifest {
        schema_version: semver("1.0.0"),
        runtime_interface_min: semver("1.0.0"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![],
        signature: None,
        extensions: BTreeMap::new(),
    };
    BuFObject {
        manifest,
        sections: vec![],
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{default_policy, demo_object, inspect_object, pack_bytes, run_object};
    use fayger_loader::load;
    use fayger_runtime::LifecycleState;

    #[test]
    fn default_policy_accepts_1_0_0() {
        let p = default_policy();
        assert_eq!(p.runtime_interface_provided.to_string(), "1.0.0");
    }

    #[test]
    fn pack_bytes_round_trips_through_load() {
        let bytes = pack_bytes(
            "main",
            &[
                ("code".to_owned(), "code".to_owned(), vec![1, 2, 3]),
                ("res".to_owned(), "asset".to_owned(), vec![9; 10]),
            ],
        )
        .expect("pack");
        let obj = load(&bytes, &default_policy()).expect("load");
        assert_eq!(obj.sections.len(), 2);
        assert_eq!(obj.manifest.entry.as_str(), "main");
    }

    #[test]
    fn inspect_object_shows_schema_entry_and_sections() {
        let bytes =
            pack_bytes("go", &[("code".to_owned(), "code".to_owned(), vec![1, 2])]).expect("pack");
        let obj = load(&bytes, &default_policy()).expect("load");
        let text = inspect_object(&obj);
        assert!(text.contains("schema_version"));
        assert!(text.contains("go"));
        assert!(text.contains("code"));
        assert!(text.contains("sections:              1"));
    }

    #[test]
    fn run_object_on_demo_terminates() {
        let report = run_object(demo_object()).expect("run");
        assert_eq!(report.final_state, LifecycleState::Terminated);
        // demo 有 2 段 → 派发 2 条。
        assert_eq!(report.dispatched, 2);
        // init/start/terminate = 3 次生命周期迁移。
        assert_eq!(report.lifecycle_events, 3);
    }

    #[test]
    fn run_object_dispatch_equals_sections() {
        let bytes = pack_bytes(
            "main",
            &[
                ("a".to_owned(), "code".to_owned(), vec![1]),
                ("b".to_owned(), "data".to_owned(), vec![2, 3]),
                ("c".to_owned(), "asset".to_owned(), vec![4, 5, 6]),
            ],
        )
        .expect("pack");
        let obj = load(&bytes, &default_policy()).expect("load");
        let report = run_object(obj).expect("run");
        assert_eq!(report.dispatched, 3);
        assert_eq!(report.final_state, LifecycleState::Terminated);
    }

    #[test]
    fn render_report_is_readable() {
        let report = run_object(demo_object()).expect("run");
        let text = report.render();
        assert!(text.contains("final_state"));
        assert!(text.contains("Terminated"));
    }

    #[test]
    fn bridge_object_on_demo_produces_report() {
        // 当前测试平台为 native-desktop；demo 有 2 段。
        use super::{bridge_object, render_bridge_report};
        let report = bridge_object(demo_object()).expect("bridge");
        assert_eq!(report.final_state, LifecycleState::Terminated);
        assert_eq!(report.dispatched, 2);
        assert_eq!(report.failures, 0);
        let text = render_bridge_report(&report);
        assert!(text.contains("Bridge report"));
        assert!(text.contains("host syscalls"));
    }
}
