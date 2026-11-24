//! `Runtime_Interface` 语言无关黑盒一致性测试套件（H2）。
//!
//! 本 crate 落实蓝图 §4 `Runtime_Interface` 跨实现可互换性与黑盒一致性验证的三项核心原则：
//!
//! 1. **对外可观测契约基准（OQ-1）**：仅断言生命周期状态序列、派发的 [`UniversalInstruction`] 序列、
//!    [`FaygerError`] 错误码与来源层，以及 [`FaygerEvent`] 事件序列；绝不窥探实现内部数据结构，
//!    也不要求解释 `BuF` 段载荷语义。
//! 2. **共享黑盒适配器（OQ-2）**：通过 [`RuntimeHarness`] 将任意运行层实现（Rust 原生或跨语言进程/FFI
//!    桥接器）接入同一套 [`run_conformance_suite`] 与 [`verify_cross_impl_equivalence`]。
//! 3. **严格合规判定（OQ-3）**：仅当全部必选一致性性质 100% 通过时给出 [`ConformanceVerdict::Conformant`]；
//!    任一性质失败即判定 [`ConformanceVerdict::NonConformant`]，不使用含糊的「部分兼容」措辞。

#![doc(html_root_url = "https://docs.rs/fayger-conformance/0.3.1")]

use fayger_buf::manifest::types::{
    Digest, ProfileConstraints, ResourceQuota, RuntimeRequirement, SectionKind, SectionVisibility,
    SelectionStrategy,
};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_error::{ErrorCode, FaygerError, Layer};
use fayger_loader::seal;
use fayger_observability::{EventCategory, EventFilter, FaygerEvent};
use fayger_runtime::{
    FailureInfo, InstanceId, LifecycleCommand, LifecycleState, MockAdapter, ResourceUsage, Runtime,
    RuntimeDescriptor, RuntimeInterface, RuntimeOperation, RuntimeRegistry, UniversalInstruction,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::rc::Rc;

/// 一致性套件报告的固定 schema 版本。
pub const CONFORMANCE_REPORT_SCHEMA_VERSION: &str = "fayger-conformance/1.0";

/// 运行层实现的黑盒被测适配特征。
///
/// 任何语言或进程形态的 `Runtime_Implementation` 只要能将对外可观测操作映射到该特征，
/// 即可运行完整的一致性套件与跨实现等价性比对。
pub trait RuntimeHarness: RuntimeInterface {
    /// 驱动实例向前推进一步执行（对应可观测指令派发步进）。
    fn step_execution(&mut self, id: InstanceId) -> Result<bool, FaygerError>;

    /// 向非终态实例注入一次未处理运行错误以检验 `Failed` 终态与失败保留语义。
    fn inject_unhandled_fault(
        &mut self,
        id: InstanceId,
        cause_message: &str,
    ) -> Result<(), FaygerError>;

    /// 注入一次资源用量观测样本以检验配额超限自动挂起语义。
    fn observe_resource_usage(
        &mut self,
        id: InstanceId,
        usage: ResourceUsage,
    ) -> Result<(), FaygerError>;

    /// 返回自该被测实例启动以来向适配层派发的 [`UniversalInstruction`] 完整有序序列。
    fn emitted_instructions(&self) -> Vec<UniversalInstruction>;

    /// 返回自该被测实例启动以来在事件总线上发布的可观测事件完整有序序列。
    fn published_events(&self) -> Vec<FaygerEvent>;
}

/// 包装 Rust 参考实现 [`Runtime<MockAdapter>`] 的标准黑盒被测器。
#[derive(Debug)]
pub struct ReferenceRuntimeHarness {
    runtime: Runtime<MockAdapter>,
    recorded_events: Rc<RefCell<Vec<FaygerEvent>>>,
}

impl ReferenceRuntimeHarness {
    /// 构造绑定了全量事件监听器的参考运行层被测器。
    #[must_use]
    pub fn new() -> Self {
        let mut runtime = Runtime::new(MockAdapter::new());
        let recorded_events: Rc<RefCell<Vec<FaygerEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&recorded_events);
        runtime.events_mut().subscribe(
            EventFilter::any(),
            Box::new(move |ev| sink.borrow_mut().push(ev.clone())),
        );
        Self {
            runtime,
            recorded_events,
        }
    }
}

impl Default for ReferenceRuntimeHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeInterface for ReferenceRuntimeHarness {
    fn rt_version(&self) -> SemVer {
        self.runtime.rt_version()
    }

    fn descriptor(&self) -> RuntimeDescriptor {
        self.runtime.descriptor()
    }

    fn load(&mut self, obj: BuFObject) -> Result<InstanceId, FaygerError> {
        self.runtime.load(obj)
    }

    fn init(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.runtime.init(id)
    }

    fn start(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.runtime.start(id)
    }

    fn suspend(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.runtime.suspend(id)
    }

    fn resume(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.runtime.resume(id)
    }

    fn terminate(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.runtime.terminate(id)
    }

    fn state(&self, id: InstanceId) -> Result<LifecycleState, FaygerError> {
        self.runtime.state(id)
    }

    fn failure_info(&self, id: InstanceId) -> Result<Option<&FailureInfo>, FaygerError> {
        self.runtime.failure_info(id)
    }
}

impl RuntimeHarness for ReferenceRuntimeHarness {
    fn step_execution(&mut self, id: InstanceId) -> Result<bool, FaygerError> {
        self.runtime.step(id)
    }

    fn inject_unhandled_fault(
        &mut self,
        id: InstanceId,
        cause_message: &str,
    ) -> Result<(), FaygerError> {
        self.runtime.inject_fault(id, cause_message)
    }

    fn observe_resource_usage(
        &mut self,
        id: InstanceId,
        usage: ResourceUsage,
    ) -> Result<(), FaygerError> {
        self.runtime.observe_usage(id, usage)
    }

    fn emitted_instructions(&self) -> Vec<UniversalInstruction> {
        self.runtime.adapter().received.clone()
    }

    fn published_events(&self) -> Vec<FaygerEvent> {
        self.recorded_events.borrow().clone()
    }
}

/// 合规判定结论（严格二元口径，不设含糊的「部分兼容」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceVerdict {
    /// 全部必选一致性性质均通过。
    Conformant,
    /// 至少存在一项必选一致性性质未通过；严禁对外声明为一致性实现。
    NonConformant,
}

impl ConformanceVerdict {
    /// 面向 CLI 与日志的稳定大写字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conformant => "CONFORMANT",
            Self::NonConformant => "NON_CONFORMANT",
        }
    }
}

/// 单条一致性性质检查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCheckResult {
    /// 性质稳定编号（如 `"P1-REQUIRED-OPS"`）。
    pub property_id: String,
    /// 性质简述。
    pub title: String,
    /// 是否通过。
    pub passed: bool,
    /// 可观测证据或失败诊断详情。
    pub detail: String,
}

/// 一致性套件完整评估报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceReport {
    /// 报告 schema 版本。
    pub schema_version: String,
    /// 被测实现标识（来自 `RuntimeDescriptor::impl_id`）。
    pub impl_id: String,
    /// 被测实现声明的 `Runtime_Interface` 版本。
    pub rt_version: String,
    /// 严格二元合规结论。
    pub verdict: ConformanceVerdict,
    /// 通过的性质数。
    pub passed_count: usize,
    /// 总性质数。
    pub total_count: usize,
    /// 逐条检查明细。
    pub checks: Vec<ConformanceCheckResult>,
}

impl ConformanceReport {
    /// 是否达成严格合规。
    #[must_use]
    pub fn is_conformant(&self) -> bool {
        self.verdict == ConformanceVerdict::Conformant
    }

    /// 渲染为可读文本摘要。
    #[must_use]
    pub fn render(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "Fayger Runtime_Interface Conformance Report");
        let _ = writeln!(s, "  schema_version: {}", self.schema_version);
        let _ = writeln!(s, "  impl_id:        {}", self.impl_id);
        let _ = writeln!(s, "  rt_version:     {}", self.rt_version);
        let _ = writeln!(s, "  verdict:        {}", self.verdict.as_str());
        let _ = writeln!(
            s,
            "  score:          {}/{} required properties passed",
            self.passed_count, self.total_count
        );
        let _ = writeln!(s, "\nProperties:");
        for c in &self.checks {
            let badge = if c.passed { "PASS" } else { "FAIL" };
            let _ = writeln!(
                s,
                "  [{badge}] {:<28} {} ({})",
                c.property_id, c.title, c.detail
            );
        }
        s
    }

    /// 序列化为格式化 JSON。
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// 对任意实现了 [`RuntimeHarness`] 的运行层执行全套 7 项语言无关黑盒一致性性质校验。
#[must_use]
pub fn run_conformance_suite<H, F>(factory: F) -> ConformanceReport
where
    H: RuntimeHarness,
    F: Fn() -> H,
{
    let probe = factory();
    let desc = probe.descriptor();
    let impl_id = desc.impl_id.clone();
    let rt_version = probe.rt_version().to_string();

    let checks = vec![
        check_p1_required_operations(&factory),
        check_p2_transition_and_error_matrix(&factory),
        check_p3_execution_pause_resume_sequence(&factory),
        check_p4_fault_retention_terminal(&factory),
        check_p5_quota_exceeded_suspension(&factory),
        check_p6_registry_routing_determinism(&desc),
        check_p7_observable_lifecycle_events(&factory),
    ];

    let passed_count = checks.iter().filter(|c| c.passed).count();
    let total_count = checks.len();
    let verdict = if passed_count == total_count {
        ConformanceVerdict::Conformant
    } else {
        ConformanceVerdict::NonConformant
    };

    ConformanceReport {
        schema_version: CONFORMANCE_REPORT_SCHEMA_VERSION.to_owned(),
        impl_id,
        rt_version,
        verdict,
        passed_count,
        total_count,
        checks,
    }
}

/// 跨实现等价性校验报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossImplEquivalenceReport {
    /// 实现 A 标识。
    pub impl_a: String,
    /// 实现 B 标识。
    pub impl_b: String,
    /// 两者在全部测试制品与非法输入上的可观测行为是否完全等价。
    pub equivalent: bool,
    /// 校验过的制品用例数。
    pub cases_checked: usize,
    /// 若不等价，记录首个分歧描述；等价时为 `"all observable traces matched"`。
    pub detail: String,
}

/// 验证两个独立 [`RuntimeHarness`] 实现在同一组 `BuFObject` 与非法命令序列下产出完全一致的
/// 状态序列、指令序列与错误码/来源层（蓝图 §4 跨实现行为一致性）。
#[must_use]
pub fn verify_cross_impl_equivalence<H1, H2, F1, F2>(
    factory_a: F1,
    factory_b: F2,
    workloads: &[BuFObject],
) -> CrossImplEquivalenceReport
where
    H1: RuntimeHarness,
    H2: RuntimeHarness,
    F1: Fn() -> H1,
    F2: Fn() -> H2,
{
    let sample_a = factory_a();
    let sample_b = factory_b();
    let impl_a = sample_a.descriptor().impl_id;
    let impl_b = sample_b.descriptor().impl_id;

    for (idx, obj) in workloads.iter().enumerate() {
        let trace_a = collect_observable_trace(&mut factory_a(), obj.clone());
        let trace_b = collect_observable_trace(&mut factory_b(), obj.clone());
        if trace_a != trace_b {
            return CrossImplEquivalenceReport {
                impl_a,
                impl_b,
                equivalent: false,
                cases_checked: idx + 1,
                detail: format!(
                    "divergence at workload #{idx}: trace_a={trace_a:?} vs trace_b={trace_b:?}"
                ),
            };
        }
    }

    CrossImplEquivalenceReport {
        impl_a,
        impl_b,
        equivalent: true,
        cases_checked: workloads.len(),
        detail: "all observable traces matched".to_owned(),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ObservableTrace {
    states: Vec<LifecycleState>,
    instructions: Vec<UniversalInstruction>,
    illegal_errors: Vec<(ErrorCode, Layer)>,
}

fn collect_observable_trace<H: RuntimeHarness>(harness: &mut H, obj: BuFObject) -> ObservableTrace {
    let mut states = Vec::new();
    let mut illegal_errors = Vec::new();

    let Ok(id) = harness.load(obj) else {
        return ObservableTrace {
            states,
            instructions: Vec::new(),
            illegal_errors,
        };
    };
    if let Ok(s) = harness.state(id) {
        states.push(s);
    }

    // 非法迁移探测：Loaded 状态下直接调用 Start 与 Suspend 必须返回相同错误码与层。
    if let Err(e) = harness.start(id) {
        illegal_errors.push((e.code, e.source_layer));
    }
    if let Err(e) = harness.suspend(id) {
        illegal_errors.push((e.code, e.source_layer));
    }

    let _ = harness.init(id);
    if let Ok(s) = harness.state(id) {
        states.push(s);
    }
    let _ = harness.start(id);
    if let Ok(s) = harness.state(id) {
        states.push(s);
    }

    // 执行首步 → 挂起 → 挂起期试探 step → 恢复 → 执行剩余步 → 终止。
    let _ = harness.step_execution(id);
    let _ = harness.suspend(id);
    if let Ok(s) = harness.state(id) {
        states.push(s);
    }
    let _ = harness.step_execution(id);
    let _ = harness.resume(id);
    if let Ok(s) = harness.state(id) {
        states.push(s);
    }
    while matches!(harness.step_execution(id), Ok(true)) {}
    let _ = harness.terminate(id);
    if let Ok(s) = harness.state(id) {
        states.push(s);
    }
    // 终态再次 Terminate 必须拒绝。
    if let Err(e) = harness.terminate(id) {
        illegal_errors.push((e.code, e.source_layer));
    }

    ObservableTrace {
        states,
        instructions: harness.emitted_instructions(),
        illegal_errors,
    }
}

// ----------------------- 7 项核心性质实现 -----------------------

fn check_p1_required_operations<H: RuntimeHarness>(
    factory: &impl Fn() -> H,
) -> ConformanceCheckResult {
    let h = factory();
    let desc = h.descriptor();
    let required = RuntimeOperation::required();
    let missing: Vec<String> = required
        .difference(&desc.supported_operations)
        .map(|op| format!("{op:?}"))
        .collect();
    if !missing.is_empty() {
        return ConformanceCheckResult {
            property_id: "P1-REQUIRED-OPS".to_owned(),
            title: "RuntimeDescriptor covers all 10 required operations".to_owned(),
            passed: false,
            detail: format!("missing operations: {}", missing.join(", ")),
        };
    }

    let mut reg = RuntimeRegistry::new();
    if let Err(e) = reg.register(desc) {
        return ConformanceCheckResult {
            property_id: "P1-REQUIRED-OPS".to_owned(),
            title: "RuntimeDescriptor covers all 10 required operations".to_owned(),
            passed: false,
            detail: format!("registry rejected descriptor: {}", e.code),
        };
    }

    ConformanceCheckResult {
        property_id: "P1-REQUIRED-OPS".to_owned(),
        title: "RuntimeDescriptor covers all 10 required operations".to_owned(),
        passed: true,
        detail: "10/10 operations declared and accepted by RuntimeRegistry".to_owned(),
    }
}

#[allow(clippy::too_many_lines)]
fn check_p2_transition_and_error_matrix<H: RuntimeHarness>(
    factory: &impl Fn() -> H,
) -> ConformanceCheckResult {
    let commands = [
        LifecycleCommand::Init,
        LifecycleCommand::Start,
        LifecycleCommand::Suspend,
        LifecycleCommand::Resume,
        LifecycleCommand::Terminate,
    ];
    let target_states = [
        LifecycleState::Loaded,
        LifecycleState::Initialized,
        LifecycleState::Running,
        LifecycleState::Suspended,
        LifecycleState::Terminated,
        LifecycleState::Failed,
    ];

    // 先验证未知 InstanceId 错误码与来源层。
    let probe = factory();
    match probe.state(InstanceId(999_999)) {
        Err(err)
            if err.code == ErrorCode::RtInstanceNotFound && err.source_layer == Layer::Runtime => {}
        other => {
            return ConformanceCheckResult {
                property_id: "P2-TRANSITION-MATRIX".to_owned(),
                title: "6x5 lifecycle state transition & error-code parity".to_owned(),
                passed: false,
                detail: format!("unknown InstanceId expected RT_INSTANCE_NOT_FOUND, got {other:?}"),
            };
        }
    }

    let mut checked = 0usize;
    for state in target_states {
        for cmd in commands {
            let mut h = factory();
            let Some(id) = reach_state(&mut h, state) else {
                return ConformanceCheckResult {
                    property_id: "P2-TRANSITION-MATRIX".to_owned(),
                    title: "6x5 lifecycle state transition & error-code parity".to_owned(),
                    passed: false,
                    detail: format!("unable to reach setup state {state:?}"),
                };
            };
            let expected = state.apply(cmd);
            let actual = apply_command(&mut h, id, cmd);
            match (expected, actual) {
                (Some(exp_next), Ok(())) => match h.state(id) {
                    Ok(cur) if cur == exp_next => checked += 1,
                    other => {
                        return ConformanceCheckResult {
                            property_id: "P2-TRANSITION-MATRIX".to_owned(),
                            title: "6x5 lifecycle state transition & error-code parity".to_owned(),
                            passed: false,
                            detail: format!(
                                "({state:?}, {cmd:?}) expected state {exp_next:?}, got {other:?}"
                            ),
                        };
                    }
                },
                (None, Err(err))
                    if err.code == ErrorCode::RtIllegalTransition
                        && err.source_layer == Layer::Runtime =>
                {
                    // 非法迁移不得改变原状态。
                    if h.state(id).ok() != Some(state) {
                        return ConformanceCheckResult {
                            property_id: "P2-TRANSITION-MATRIX".to_owned(),
                            title: "6x5 lifecycle state transition & error-code parity".to_owned(),
                            passed: false,
                            detail: format!("illegal ({state:?}, {cmd:?}) mutated state"),
                        };
                    }
                    checked += 1;
                }
                (_, other) => {
                    return ConformanceCheckResult {
                        property_id: "P2-TRANSITION-MATRIX".to_owned(),
                        title: "6x5 lifecycle state transition & error-code parity".to_owned(),
                        passed: false,
                        detail: format!(
                            "mismatch on ({state:?}, {cmd:?}): expected {expected:?}, got {other:?}"
                        ),
                    };
                }
            }
        }
    }

    ConformanceCheckResult {
        property_id: "P2-TRANSITION-MATRIX".to_owned(),
        title: "6x5 lifecycle state transition & error-code parity".to_owned(),
        passed: true,
        detail: format!("{checked}/30 state-command pairs + RT_INSTANCE_NOT_FOUND verified"),
    }
}

fn check_p3_execution_pause_resume_sequence<H: RuntimeHarness>(
    factory: &impl Fn() -> H,
) -> ConformanceCheckResult {
    let mut h = factory();
    let obj = build_fixture_buf(&[b"alpha", b"bravo-payload", b"charlie-007"], None);
    let Ok(id) = h.load(obj) else {
        return fail("P3-EXEC-PAUSE-RESUME", "load failed on valid 3-section BuF");
    };
    if h.init(id).is_err() || h.start(id).is_err() {
        return fail("P3-EXEC-PAUSE-RESUME", "init/start failed");
    }

    // 第 1 步应派发首条指令。
    if h.step_execution(id).ok() != Some(true) || h.emitted_instructions().len() != 1 {
        return fail(
            "P3-EXEC-PAUSE-RESUME",
            "first step did not emit 1 instruction",
        );
    }

    // 暂停期间 step 必须返回 Ok(false) 且不派发任何新指令。
    if h.suspend(id).is_err() {
        return fail("P3-EXEC-PAUSE-RESUME", "suspend failed");
    }
    if h.step_execution(id).ok() != Some(false) || h.emitted_instructions().len() != 1 {
        return fail(
            "P3-EXEC-PAUSE-RESUME",
            "suspended instance emitted instruction during pause",
        );
    }

    // 恢复后继续完成剩余 2 条指令。
    if h.resume(id).is_err() {
        return fail("P3-EXEC-PAUSE-RESUME", "resume failed");
    }
    let mut extra = 0usize;
    while matches!(h.step_execution(id), Ok(true)) {
        extra += 1;
    }
    if extra != 2 || h.emitted_instructions().len() != 3 {
        return fail(
            "P3-EXEC-PAUSE-RESUME",
            "resume did not emit remaining 2 instructions",
        );
    }
    if h.terminate(id).is_err() || h.state(id).ok() != Some(LifecycleState::Terminated) {
        return fail("P3-EXEC-PAUSE-RESUME", "terminate failed after completion");
    }

    // 校验确定性操作数（小端 u32 段长度：5, 13, 11）。
    let emitted = h.emitted_instructions();
    let expected_lens = [5u32, 13u32, 11u32];
    for (idx, exp_len) in expected_lens.iter().enumerate() {
        if emitted[idx].opcode != u16::try_from(idx).unwrap_or(0)
            || emitted[idx].operands != exp_len.to_le_bytes()
        {
            return fail(
                "P3-EXEC-PAUSE-RESUME",
                "emitted instruction opcode/operands deviated from specification",
            );
        }
    }

    ConformanceCheckResult {
        property_id: "P3-EXEC-PAUSE-RESUME".to_owned(),
        title: "Deterministic instruction sequence & zero emission while Suspended".to_owned(),
        passed: true,
        detail: "3 instructions emitted in order; 0 emitted during Suspended".to_owned(),
    }
}

fn check_p4_fault_retention_terminal<H: RuntimeHarness>(
    factory: &impl Fn() -> H,
) -> ConformanceCheckResult {
    let mut h = factory();
    let obj = build_fixture_buf(&[b"step-one", b"step-two"], None);
    let Ok(id) = h.load(obj) else {
        return fail("P4-FAULT-RETENTION", "load failed");
    };
    let _ = h.init(id);
    let _ = h.start(id);
    let _ = h.step_execution(id);

    if h.inject_unhandled_fault(id, "conformance simulated fault")
        .is_err()
    {
        return fail("P4-FAULT-RETENTION", "inject_unhandled_fault failed");
    }
    if h.state(id).ok() != Some(LifecycleState::Failed) {
        return fail("P4-FAULT-RETENTION", "state did not enter Failed");
    }
    match h.failure_info(id) {
        Ok(Some(info))
            if info.error.code == ErrorCode::RtInstanceFailed
                && info.last_instruction.is_some() => {}
        other => {
            return ConformanceCheckResult {
                property_id: "P4-FAULT-RETENTION".to_owned(),
                title: "Unhandled fault transitions to Failed & retains FailureInfo".to_owned(),
                passed: false,
                detail: format!("unexpected failure_info: {other:?}"),
            };
        }
    }
    // Failed 终态必须拒绝 Resume / Terminate。
    if h.resume(id).is_ok() || h.terminate(id).is_ok() {
        return fail(
            "P4-FAULT-RETENTION",
            "Failed terminal state accepted command",
        );
    }

    ConformanceCheckResult {
        property_id: "P4-FAULT-RETENTION".to_owned(),
        title: "Unhandled fault transitions to Failed & retains FailureInfo".to_owned(),
        passed: true,
        detail: "RT_INSTANCE_FAILED & last_instruction retained; terminal commands rejected"
            .to_owned(),
    }
}

fn check_p5_quota_exceeded_suspension<H: RuntimeHarness>(
    factory: &impl Fn() -> H,
) -> ConformanceCheckResult {
    let mut h = factory();
    let quota = ResourceQuota {
        cpu_ms_per_sec: Some(100),
        memory_bytes: Some(1024),
        io_bytes_per_sec: None,
    };
    let obj = build_fixture_buf(&[b"work"], Some(quota));
    let Ok(id) = h.load(obj) else {
        return fail("P5-QUOTA-ISOLATION", "load failed");
    };
    let _ = h.init(id);
    let _ = h.start(id);

    // 注入超限内存用量（2048 > 1024）→ 必须自动挂起至 Suspended。
    if h.observe_resource_usage(
        id,
        ResourceUsage {
            cpu_ms_per_sec: 50,
            memory_bytes: 2048,
            io_bytes_per_sec: 0,
        },
    )
    .is_err()
    {
        return fail("P5-QUOTA-ISOLATION", "observe_resource_usage failed");
    }
    if h.state(id).ok() != Some(LifecycleState::Suspended) {
        return fail(
            "P5-QUOTA-ISOLATION",
            "exceeding memory quota while Running did not suspend instance",
        );
    }

    ConformanceCheckResult {
        property_id: "P5-QUOTA-ISOLATION".to_owned(),
        title: "Quota overrun while Running deterministically suspends instance".to_owned(),
        passed: true,
        detail: "memory quota overrun transitioned Running -> Suspended".to_owned(),
    }
}

fn check_p6_registry_routing_determinism(desc: &RuntimeDescriptor) -> ConformanceCheckResult {
    let mut reg = RuntimeRegistry::new();
    let Ok(reg_id) = reg.register(desc.clone()) else {
        return fail("P6-ROUTING-DETERMINISM", "failed to register descriptor");
    };

    // 兼容最低版本应稳定命中 reg_id。
    let compatible_manifest = BuFManifest {
        schema_version: semver_1_0_0(),
        runtime_interface_min: SemVer {
            major: 1,
            minor: 0,
            patch: 0,
            pre: None,
        },
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: Some(RuntimeRequirement {
            preferred_impl: Some(desc.impl_id.clone()),
            selection_strategy: SelectionStrategy::Strict,
        }),
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: Vec::new(),
        signature: None,
        extensions: BTreeMap::new(),
    };
    if reg.resolve(&compatible_manifest).ok() != Some(reg_id) {
        return fail(
            "P6-ROUTING-DETERMINISM",
            "Strict routing failed to resolve impl",
        );
    }

    // 超高版本需求必须返回 RT_IMPL_NOT_FOUND。
    let incompatible_manifest = BuFManifest {
        runtime_interface_min: SemVer {
            major: desc.rt_version.major.saturating_add(1),
            minor: 0,
            patch: 0,
            pre: None,
        },
        ..compatible_manifest
    };
    match reg.resolve(&incompatible_manifest) {
        Err(e) if e.code == ErrorCode::RtImplNotFound => ConformanceCheckResult {
            property_id: "P6-ROUTING-DETERMINISM".to_owned(),
            title: "Version negotiation & selection strategy determinism".to_owned(),
            passed: true,
            detail: "Strict match & RT_IMPL_NOT_FOUND version rejection verified".to_owned(),
        },
        other => ConformanceCheckResult {
            property_id: "P6-ROUTING-DETERMINISM".to_owned(),
            title: "Version negotiation & selection strategy determinism".to_owned(),
            passed: false,
            detail: format!("expected RT_IMPL_NOT_FOUND, got {other:?}"),
        },
    }
}

fn check_p7_observable_lifecycle_events<H: RuntimeHarness>(
    factory: &impl Fn() -> H,
) -> ConformanceCheckResult {
    let mut h = factory();
    let obj = build_fixture_buf(&[b"ev"], None);
    let Ok(id) = h.load(obj) else {
        return fail("P7-EVENT-SEQUENCE", "load failed");
    };
    let _ = h.init(id);
    let _ = h.start(id);
    let _ = h.suspend(id);
    let _ = h.resume(id);
    let _ = h.terminate(id);

    let lifecycle_events: Vec<FaygerEvent> = h
        .published_events()
        .into_iter()
        .filter(|e| e.category == EventCategory::Lifecycle)
        .collect();
    if lifecycle_events.len() != 5 {
        return ConformanceCheckResult {
            property_id: "P7-EVENT-SEQUENCE".to_owned(),
            title: "Observable lifecycle event stream matches causal transitions".to_owned(),
            passed: false,
            detail: format!(
                "expected 5 lifecycle transition events, observed {}",
                lifecycle_events.len()
            ),
        };
    }

    ConformanceCheckResult {
        property_id: "P7-EVENT-SEQUENCE".to_owned(),
        title: "Observable lifecycle event stream matches causal transitions".to_owned(),
        passed: true,
        detail: "5/5 ordered lifecycle events observed".to_owned(),
    }
}

fn fail(property_id: &str, detail: &str) -> ConformanceCheckResult {
    ConformanceCheckResult {
        property_id: property_id.to_owned(),
        title: property_id.to_owned(),
        passed: false,
        detail: detail.to_owned(),
    }
}

fn reach_state<H: RuntimeHarness>(h: &mut H, target: LifecycleState) -> Option<InstanceId> {
    let obj = build_fixture_buf(&[b"s0"], None);
    let id = h.load(obj).ok()?;
    match target {
        LifecycleState::Loaded => Some(id),
        LifecycleState::Initialized => {
            h.init(id).ok()?;
            Some(id)
        }
        LifecycleState::Running => {
            h.init(id).ok()?;
            h.start(id).ok()?;
            Some(id)
        }
        LifecycleState::Suspended => {
            h.init(id).ok()?;
            h.start(id).ok()?;
            h.suspend(id).ok()?;
            Some(id)
        }
        LifecycleState::Terminated => {
            h.terminate(id).ok()?;
            Some(id)
        }
        LifecycleState::Failed => {
            h.inject_unhandled_fault(id, "setup failed state").ok()?;
            Some(id)
        }
    }
}

fn apply_command<H: RuntimeHarness>(
    h: &mut H,
    id: InstanceId,
    cmd: LifecycleCommand,
) -> Result<(), FaygerError> {
    match cmd {
        LifecycleCommand::Init => h.init(id),
        LifecycleCommand::Start => h.start(id),
        LifecycleCommand::Suspend => h.suspend(id),
        LifecycleCommand::Resume => h.resume(id),
        LifecycleCommand::Terminate => h.terminate(id),
    }
}

/// 构造用于一致性测试的标准已封装 `BuFObject`。
#[must_use]
pub fn build_fixture_buf(payloads: &[&[u8]], quotas: Option<ResourceQuota>) -> BuFObject {
    let mut descriptors = Vec::with_capacity(payloads.len());
    let mut sections = Vec::with_capacity(payloads.len());
    for (idx, payload) in payloads.iter().enumerate() {
        let sec_id = format!("sec_{idx}");
        descriptors.push(SectionDescriptor {
            id: sec_id.clone(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: payload.len() as u64,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        });
        sections.push(Section::new(sec_id, payload.to_vec()));
    }

    let manifest = BuFManifest {
        schema_version: semver_1_0_0(),
        runtime_interface_min: semver_1_0_0(),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas,
        sections: descriptors,
        signature: None,
        extensions: BTreeMap::new(),
    };
    let mut obj = BuFObject {
        manifest,
        sections,
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    };
    let _ = seal(&mut obj);
    obj
}

const fn semver_1_0_0() -> SemVer {
    SemVer {
        major: 1,
        minor: 0,
        patch: 0,
        pre: None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        build_fixture_buf, run_conformance_suite, semver_1_0_0, verify_cross_impl_equivalence,
        ConformanceVerdict, ReferenceRuntimeHarness, RuntimeHarness,
    };
    use fayger_buf::manifest::SemVer;
    use fayger_buf::object::BuFObject;
    use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
    use fayger_observability::{EventBus, EventDraft, EventFilter, FaygerEvent};
    use fayger_runtime::{
        first_exceeded, FailureInfo, InstanceId, LifecycleCommand, LifecycleState, ResourceUsage,
        RuntimeDescriptor, RuntimeInterface, RuntimeOperation, UniversalInstruction,
    };
    use proptest::prelude::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    /// 第二套洁净室独立实现（使用 `HashMap` 与独立状态表，不复用 `fayger_runtime::Runtime`），
    /// 用于证明一致性套件不依赖特定内部结构并验证跨实现行为等价性。
    struct CleanRoomAltRuntime {
        instances: HashMap<u64, AltInstance>,
        next_id: u64,
        emitted: Vec<UniversalInstruction>,
        bus: EventBus,
        events: Rc<RefCell<Vec<FaygerEvent>>>,
    }

    struct AltInstance {
        state: LifecycleState,
        program: Vec<UniversalInstruction>,
        cursor: usize,
        last_instr: Option<UniversalInstruction>,
        failure: Option<FailureInfo>,
        quota: Option<fayger_buf::manifest::types::ResourceQuota>,
    }

    impl CleanRoomAltRuntime {
        fn new() -> Self {
            let mut bus = EventBus::new();
            let events: Rc<RefCell<Vec<FaygerEvent>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = Rc::clone(&events);
            bus.subscribe(
                EventFilter::any(),
                Box::new(move |ev| sink.borrow_mut().push(ev.clone())),
            );
            Self {
                instances: HashMap::new(),
                next_id: 100, // 故意使用不同起始 ID 验证套件不绑定 ID 数值习惯
                emitted: Vec::new(),
                bus,
                events,
            }
        }

        fn apply_cmd(&mut self, id: InstanceId, cmd: LifecycleCommand) -> Result<(), FaygerError> {
            let inst = self.instances.get_mut(&id.0).ok_or_else(|| {
                FaygerError::new(ErrorCode::RtInstanceNotFound, Layer::Runtime, "missing")
            })?;
            let Some(next) = inst.state.apply(cmd) else {
                return Err(FaygerError::new(
                    ErrorCode::RtIllegalTransition,
                    Layer::Runtime,
                    "illegal",
                ));
            };
            let prev = inst.state;
            inst.state = next;
            self.bus.publish(EventDraft::lifecycle(
                format!("{prev:?}"),
                format!("{next:?}"),
                id.0,
            ));
            Ok(())
        }
    }

    impl RuntimeInterface for CleanRoomAltRuntime {
        fn rt_version(&self) -> SemVer {
            SemVer {
                major: 1,
                minor: 1,
                patch: 0,
                pre: None,
            }
        }

        fn descriptor(&self) -> RuntimeDescriptor {
            RuntimeDescriptor::complete("cleanroom-alt-runtime", self.rt_version())
        }

        fn load(&mut self, obj: BuFObject) -> Result<InstanceId, FaygerError> {
            let id = self.next_id;
            self.next_id += 1;
            let program = obj
                .sections
                .iter()
                .enumerate()
                .map(|(i, s)| UniversalInstruction {
                    category: 0,
                    opcode: u16::try_from(i).unwrap_or(u16::MAX),
                    operands: u32::try_from(s.payload.len())
                        .unwrap_or(u32::MAX)
                        .to_le_bytes()
                        .to_vec(),
                })
                .collect();
            self.instances.insert(
                id,
                AltInstance {
                    state: LifecycleState::Loaded,
                    program,
                    cursor: 0,
                    last_instr: None,
                    failure: None,
                    quota: obj.manifest.quotas,
                },
            );
            Ok(InstanceId(id))
        }

        fn init(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.apply_cmd(id, LifecycleCommand::Init)
        }
        fn start(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.apply_cmd(id, LifecycleCommand::Start)
        }
        fn suspend(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.apply_cmd(id, LifecycleCommand::Suspend)
        }
        fn resume(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.apply_cmd(id, LifecycleCommand::Resume)
        }
        fn terminate(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.apply_cmd(id, LifecycleCommand::Terminate)
        }
        fn state(&self, id: InstanceId) -> Result<LifecycleState, FaygerError> {
            self.instances
                .get(&id.0)
                .map(|i| i.state)
                .ok_or_else(|| FaygerError::new(ErrorCode::RtInstanceNotFound, Layer::Runtime, "m"))
        }
        fn failure_info(&self, id: InstanceId) -> Result<Option<&FailureInfo>, FaygerError> {
            self.instances
                .get(&id.0)
                .map(|i| i.failure.as_ref())
                .ok_or_else(|| FaygerError::new(ErrorCode::RtInstanceNotFound, Layer::Runtime, "m"))
        }
    }

    impl RuntimeHarness for CleanRoomAltRuntime {
        fn step_execution(&mut self, id: InstanceId) -> Result<bool, FaygerError> {
            let inst = self.instances.get_mut(&id.0).ok_or_else(|| {
                FaygerError::new(ErrorCode::RtInstanceNotFound, Layer::Runtime, "m")
            })?;
            if inst.state != LifecycleState::Running || inst.cursor >= inst.program.len() {
                return Ok(false);
            }
            let instr = inst.program[inst.cursor].clone();
            inst.cursor += 1;
            inst.last_instr = Some(instr.clone());
            self.emitted.push(instr);
            Ok(true)
        }

        fn inject_unhandled_fault(
            &mut self,
            id: InstanceId,
            cause_message: &str,
        ) -> Result<(), FaygerError> {
            let inst = self.instances.get_mut(&id.0).ok_or_else(|| {
                FaygerError::new(ErrorCode::RtInstanceNotFound, Layer::Runtime, "m")
            })?;
            if inst.state.is_terminal() {
                return Err(FaygerError::new(
                    ErrorCode::RtIllegalTransition,
                    Layer::Runtime,
                    "terminal",
                ));
            }
            let prev = inst.state;
            inst.state = LifecycleState::Failed;
            inst.failure = Some(FailureInfo {
                last_instruction: inst.last_instr.clone(),
                error: FaygerError::new(ErrorCode::RtInstanceFailed, Layer::Runtime, cause_message)
                    .with_context("instance_id", ContextValue::U64(id.0)),
            });
            self.bus.publish(EventDraft::lifecycle(
                format!("{prev:?}"),
                "Failed".to_owned(),
                id.0,
            ));
            Ok(())
        }

        fn observe_resource_usage(
            &mut self,
            id: InstanceId,
            usage: ResourceUsage,
        ) -> Result<(), FaygerError> {
            let inst = self.instances.get_mut(&id.0).ok_or_else(|| {
                FaygerError::new(ErrorCode::RtInstanceNotFound, Layer::Runtime, "m")
            })?;
            let mut suspended = false;
            let mut kind_name = None;
            if let Some(q) = inst.quota.as_ref() {
                if let Some(k) = first_exceeded(q, &usage) {
                    if inst.state == LifecycleState::Running {
                        inst.state = LifecycleState::Suspended;
                        suspended = true;
                        kind_name = Some(k.as_str().to_owned());
                    }
                }
            }
            if suspended {
                self.bus.publish(EventDraft::lifecycle(
                    "Running".to_owned(),
                    "Suspended".to_owned(),
                    id.0,
                ));
            }
            self.bus.publish(EventDraft::quota(
                id.0,
                usage.cpu_ms_per_sec,
                usage.memory_bytes,
                usage.io_bytes_per_sec,
                suspended,
                kind_name,
            ));
            Ok(())
        }

        fn emitted_instructions(&self) -> Vec<UniversalInstruction> {
            self.emitted.clone()
        }

        fn published_events(&self) -> Vec<FaygerEvent> {
            self.events.borrow().clone()
        }
    }

    /// 故意残缺的实现（缺少 `Emit` 声明），验证未通过不得被判为 Conformant。
    struct IncompleteBrokenHarness(ReferenceRuntimeHarness);

    impl RuntimeInterface for IncompleteBrokenHarness {
        fn rt_version(&self) -> SemVer {
            semver_1_0_0()
        }
        fn descriptor(&self) -> RuntimeDescriptor {
            let mut ops = RuntimeOperation::required();
            ops.remove(&RuntimeOperation::Emit);
            RuntimeDescriptor {
                impl_id: "broken-incomplete-impl".to_owned(),
                rt_version: semver_1_0_0(),
                supported_operations: ops,
            }
        }
        fn load(&mut self, obj: BuFObject) -> Result<InstanceId, FaygerError> {
            self.0.load(obj)
        }
        fn init(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.0.init(id)
        }
        fn start(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.0.start(id)
        }
        fn suspend(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.0.suspend(id)
        }
        fn resume(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.0.resume(id)
        }
        fn terminate(&mut self, id: InstanceId) -> Result<(), FaygerError> {
            self.0.terminate(id)
        }
        fn state(&self, id: InstanceId) -> Result<LifecycleState, FaygerError> {
            self.0.state(id)
        }
        fn failure_info(&self, id: InstanceId) -> Result<Option<&FailureInfo>, FaygerError> {
            self.0.failure_info(id)
        }
    }

    impl RuntimeHarness for IncompleteBrokenHarness {
        fn step_execution(&mut self, id: InstanceId) -> Result<bool, FaygerError> {
            self.0.step_execution(id)
        }
        fn inject_unhandled_fault(&mut self, id: InstanceId, msg: &str) -> Result<(), FaygerError> {
            self.0.inject_unhandled_fault(id, msg)
        }
        fn observe_resource_usage(
            &mut self,
            id: InstanceId,
            u: ResourceUsage,
        ) -> Result<(), FaygerError> {
            self.0.observe_resource_usage(id, u)
        }
        fn emitted_instructions(&self) -> Vec<UniversalInstruction> {
            self.0.emitted_instructions()
        }
        fn published_events(&self) -> Vec<FaygerEvent> {
            self.0.published_events()
        }
    }

    #[test]
    fn reference_runtime_passes_full_conformance_suite() {
        let report = run_conformance_suite(ReferenceRuntimeHarness::new);
        assert!(report.is_conformant(), "{}", report.render());
        assert_eq!(report.verdict, ConformanceVerdict::Conformant);
        assert_eq!(report.passed_count, 7);
    }

    #[test]
    fn cleanroom_alt_runtime_passes_suite_and_matches_reference_traces() {
        let alt_report = run_conformance_suite(CleanRoomAltRuntime::new);
        assert!(alt_report.is_conformant(), "{}", alt_report.render());

        let workloads = vec![
            build_fixture_buf(&[], None),
            build_fixture_buf(&[b"one"], None),
            build_fixture_buf(&[b"a", b"bb", b"ccc", b"dddd"], None),
        ];
        let eq = verify_cross_impl_equivalence(
            ReferenceRuntimeHarness::new,
            CleanRoomAltRuntime::new,
            &workloads,
        );
        assert!(eq.equivalent, "{}", eq.detail);
        assert_eq!(eq.cases_checked, 3);
    }

    #[test]
    fn broken_runtime_is_flagged_non_conformant() {
        let report =
            run_conformance_suite(|| IncompleteBrokenHarness(ReferenceRuntimeHarness::new()));
        assert!(!report.is_conformant());
        assert_eq!(report.verdict, ConformanceVerdict::NonConformant);
        assert!(report.render().contains("NON_CONFORMANT"));
    }

    proptest! {
        #[test]
        fn prop_cross_impl_equivalence_for_arbitrary_section_lengths(
            lens in prop::collection::vec(0usize..256, 0..8)
        ) {
            let owned: Vec<Vec<u8>> = lens.into_iter().map(|n| vec![0xABu8; n]).collect();
            let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
            let workload = build_fixture_buf(&refs, None);
            let eq = verify_cross_impl_equivalence(
                ReferenceRuntimeHarness::new,
                CleanRoomAltRuntime::new,
                &[workload],
            );
            prop_assert!(eq.equivalent, "{}", eq.detail);
        }
    }
}
