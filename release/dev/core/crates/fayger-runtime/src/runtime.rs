//! 参考 Runtime_Implementation：组合生命周期、Mock 执行与指令派发。
//!
//! [`Runtime`] 持有一个实例表与一个 [`Adapter`]，实现 [`RuntimeInterface`]，并额外暴露
//! 驱动执行的 [`Runtime::step`] 与注入失败的 [`Runtime::inject_fault`]。
//!
//! M2 的"执行"是从 BuFObject 派生的**确定性 mock 程序**：每个 Section 生成一条
//! Universal_Instruction，operands 取该 Section payload 的长度（小端 u32），**不读取**
//! payload 内容字节——保证"不执行 BuF 内容"且执行序列仅依赖 BuFObject（见需求 §10.6 / §6.4）。

use crate::adapter::Adapter;
use crate::instance::{FailureInfo, InstanceData, InstanceId};
use crate::instruction::UniversalInstruction;
use crate::interface::{RuntimeDescriptor, RuntimeInterface, RuntimeOperation};
use crate::lifecycle::{LifecycleCommand, LifecycleState};
use crate::resource::{first_exceeded, QuotaExceeded, ResourceUsage};
use fayger_buf::manifest::types::ResourceQuota;
use fayger_buf::manifest::SemVer;
use fayger_buf::object::BuFObject;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use fayger_observability::{EventBus, EventDraft};
use std::collections::BTreeMap;

/// 参考实现的固定标识。
const REF_IMPL_ID: &str = "fayger-runtime-ref";

/// 参考运行层实现。泛型于 [`Adapter`]，便于注入 [`crate::adapter::MockAdapter`] 做测试。
#[derive(Debug)]
pub struct Runtime<A: Adapter> {
    rt_version: SemVer,
    adapter: A,
    instances: BTreeMap<u64, InstanceData>,
    next_id: u64,
    events: EventBus,
}

impl<A: Adapter> Runtime<A> {
    /// 用给定 Adapter 构造一个运行层实例。`rt_version` 固定为 `1.0.0`。
    #[must_use]
    pub const fn new(adapter: A) -> Self {
        Self {
            // 参考实现提供 Runtime_Interface 1.0.0；解析常量字面量不会失败。
            rt_version: SemVer {
                major: 1,
                minor: 0,
                patch: 0,
                pre: None,
            },
            adapter,
            instances: BTreeMap::new(),
            next_id: 0,
            events: EventBus::new(),
        }
    }

    /// 不可变借用底层 Adapter（测试用于断言收到的指令）。
    pub const fn adapter(&self) -> &A {
        &self.adapter
    }

    /// 不可变借用内置事件总线（M8）。
    pub const fn events(&self) -> &EventBus {
        &self.events
    }

    /// 可变借用内置事件总线（M8），用于订阅 / 取消订阅 / 调试开关。
    pub const fn events_mut(&mut self) -> &mut EventBus {
        &mut self.events
    }

    /// 推进一步执行。
    ///
    /// - 实例不存在 → `RT_INSTANCE_NOT_FOUND`。
    /// - 实例非 `Running` → 不派发，返回 `Ok(false)`（`Suspended` 期间不派发，见需求 §6.2）。
    /// - `Running` 且仍有未派发指令 → 派发下一条、记录 `last_instruction`、`pc += 1`、`Ok(true)`。
    /// - `Running` 但程序耗尽 → `Ok(false)`。
    ///
    /// # Errors
    /// 实例不存在时返回 `RT_INSTANCE_NOT_FOUND`。
    pub fn step(&mut self, id: InstanceId) -> Result<bool, FaygerError> {
        let data = self
            .instances
            .get_mut(&id.0)
            .ok_or_else(|| instance_not_found(id))?;
        if data.state != LifecycleState::Running {
            return Ok(false);
        }
        if data.pc >= data.program.len() {
            return Ok(false);
        }
        let instr = data.program[data.pc].clone();
        self.adapter.dispatch(&instr);
        data.last_instruction = Some(instr);
        data.pc += 1;
        Ok(true)
    }

    /// 注入一个未处理错误：把非终态实例迁移至 `Failed` 并保留 [`FailureInfo`]。
    ///
    /// 返回的失败错误统一使用 `RT_INSTANCE_FAILED`。
    ///
    /// # Errors
    /// 实例不存在（`RT_INSTANCE_NOT_FOUND`）或已处于终态（`RT_ILLEGAL_TRANSITION`）。
    pub fn inject_fault(&mut self, id: InstanceId, cause_message: &str) -> Result<(), FaygerError> {
        let data = self
            .instances
            .get_mut(&id.0)
            .ok_or_else(|| instance_not_found(id))?;
        if data.state.is_terminal() {
            return Err(illegal_transition(data.state, None));
        }
        let prev = data.state;
        let error = FaygerError::new(ErrorCode::RtInstanceFailed, Layer::Runtime, cause_message)
            .with_context("instance_id", ContextValue::U64(id.0));
        data.state = LifecycleState::Failed;
        data.failure = Some(FailureInfo {
            last_instruction: data.last_instruction.clone(),
            error,
        });
        // M8：发布 prev → Failed 的生命周期事件。
        self.events.publish(EventDraft::lifecycle(
            format!("{prev:?}"),
            format!("{:?}", LifecycleState::Failed),
            id.0,
        ));
        Ok(())
    }

    /// 注入资源用量采样：记录最近用量；若处于 `Running` 且超出配额则迁移到 `Suspended`
    /// 并记录 [`QuotaExceeded`]（M7）。
    ///
    /// 非 `Running` 状态下不因超限自动迁移（仅记录用量）；未超限时清空超限记录。
    ///
    /// # Errors
    /// 实例不存在时返回 `RT_INSTANCE_NOT_FOUND`。
    pub fn observe_usage(
        &mut self,
        id: InstanceId,
        usage: ResourceUsage,
    ) -> Result<(), FaygerError> {
        let data = self
            .instances
            .get_mut(&id.0)
            .ok_or_else(|| instance_not_found(id))?;
        data.last_usage = Some(usage);
        // 记录本次采样的归因，用于事件发布。
        let mut suspended_now = false;
        let mut exceeded_kind: Option<crate::resource::ResourceKind> = None;
        if let Some(quota) = data.quota.as_ref() {
            match first_exceeded(quota, &usage) {
                Some(kind) if data.state == LifecycleState::Running => {
                    data.state = LifecycleState::Suspended;
                    data.quota_exceeded = Some(QuotaExceeded {
                        resource: kind,
                        usage,
                    });
                    suspended_now = true;
                    exceeded_kind = Some(kind);
                }
                // 非 Running 超限：仅记录用量，不迁移、不设置新的超限记录。
                Some(kind) => {
                    exceeded_kind = Some(kind);
                }
                // 未超限：清空超限记录。
                None => data.quota_exceeded = None,
            }
        }
        // M8：若本次采样触发暂停，先发 Running→Suspended 的生命周期事件，再发 Quota 事件
        //（顺序固定，确定性）。
        if suspended_now {
            self.events.publish(EventDraft::lifecycle(
                format!("{:?}", LifecycleState::Running),
                format!("{:?}", LifecycleState::Suspended),
                id.0,
            ));
        }
        // exceeded 维度名仅在"因本次采样暂停"时有意义（与 quota_exceeded 记录一致）。
        let exceeded_name = if suspended_now {
            exceeded_kind.map(|k| k.as_str().to_owned())
        } else {
            None
        };
        self.events.publish(EventDraft::quota(
            id.0,
            usage.cpu_ms_per_sec,
            usage.memory_bytes,
            usage.io_bytes_per_sec,
            suspended_now,
            exceeded_name,
        ));
        Ok(())
    }

    /// 查询该实例当前生效的资源配额（M7）。
    ///
    /// # Errors
    /// 实例不存在时返回 `RT_INSTANCE_NOT_FOUND`。
    pub fn quota(&self, id: InstanceId) -> Result<Option<&ResourceQuota>, FaygerError> {
        self.instances
            .get(&id.0)
            .map(|d| d.quota.as_ref())
            .ok_or_else(|| instance_not_found(id))
    }

    /// 查询该实例最近一次注入的资源用量（M7）。
    ///
    /// # Errors
    /// 实例不存在时返回 `RT_INSTANCE_NOT_FOUND`。
    pub fn usage(&self, id: InstanceId) -> Result<Option<ResourceUsage>, FaygerError> {
        self.instances
            .get(&id.0)
            .map(|d| d.last_usage)
            .ok_or_else(|| instance_not_found(id))
    }

    /// 查询该实例的超限记录（M7）；因超限暂停时设置，未超限时为 `None`。
    ///
    /// # Errors
    /// 实例不存在时返回 `RT_INSTANCE_NOT_FOUND`。
    pub fn quota_exceeded(&self, id: InstanceId) -> Result<Option<QuotaExceeded>, FaygerError> {
        self.instances
            .get(&id.0)
            .map(|d| d.quota_exceeded)
            .ok_or_else(|| instance_not_found(id))
    }

    /// 统一的生命周期迁移：查实例 → 应用命令 → 更新状态。
    fn transition(&mut self, id: InstanceId, cmd: LifecycleCommand) -> Result<(), FaygerError> {
        let data = self
            .instances
            .get_mut(&id.0)
            .ok_or_else(|| instance_not_found(id))?;
        match data.state.apply(cmd) {
            Some(next) => {
                let prev = data.state;
                data.state = next;
                // M8：发布生命周期迁移事件（加性副作用，不改变返回语义）。
                self.events.publish(EventDraft::lifecycle(
                    format!("{prev:?}"),
                    format!("{next:?}"),
                    id.0,
                ));
                Ok(())
            }
            None => Err(illegal_transition(data.state, Some(cmd))),
        }
    }
}

/// 从 BuFObject 派生确定性 mock 程序：每段一条指令，operands = payload 长度（小端 u32）。
fn derive_program(obj: &BuFObject) -> Vec<UniversalInstruction> {
    obj.sections
        .iter()
        .enumerate()
        .map(|(i, section)| {
            let len = u32::try_from(section.payload.len()).unwrap_or(u32::MAX);
            UniversalInstruction {
                category: 0,
                opcode: u16::try_from(i).unwrap_or(u16::MAX),
                operands: len.to_le_bytes().to_vec(),
            }
        })
        .collect()
}

fn instance_not_found(id: InstanceId) -> FaygerError {
    FaygerError::new(
        ErrorCode::RtInstanceNotFound,
        Layer::Runtime,
        "no instance with the given id",
    )
    .with_context("instance_id", ContextValue::U64(id.0))
}

/// 构造 `RT_ILLEGAL_TRANSITION`。`cmd` 为 `None` 表示对终态发出任意命令的拒绝。
fn illegal_transition(current: LifecycleState, cmd: Option<LifecycleCommand>) -> FaygerError {
    let allowed: Vec<ContextValue> = current
        .allowed_commands()
        .iter()
        .map(|c| ContextValue::string(format!("{c:?}")))
        .collect();
    let mut err = FaygerError::new(
        ErrorCode::RtIllegalTransition,
        Layer::Runtime,
        "lifecycle command is not allowed in the current state",
    )
    .with_context(
        "current_state",
        ContextValue::string(format!("{current:?}")),
    )
    .with_context("allowed_commands", ContextValue::List(allowed));
    if let Some(c) = cmd {
        err = err.with_context("command", ContextValue::string(format!("{c:?}")));
    }
    err
}

impl<A: Adapter> RuntimeInterface for Runtime<A> {
    fn rt_version(&self) -> SemVer {
        self.rt_version.clone()
    }

    fn descriptor(&self) -> RuntimeDescriptor {
        RuntimeDescriptor {
            impl_id: REF_IMPL_ID.to_owned(),
            rt_version: self.rt_version.clone(),
            supported_operations: RuntimeOperation::required(),
        }
    }

    fn load(&mut self, obj: BuFObject) -> Result<InstanceId, FaygerError> {
        let program = derive_program(&obj);
        let quota = obj.manifest.quotas;
        let id = self.next_id;
        self.next_id += 1;
        self.instances.insert(id, InstanceData::new(program, quota));
        Ok(InstanceId(id))
    }

    fn init(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.transition(id, LifecycleCommand::Init)
    }

    fn start(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.transition(id, LifecycleCommand::Start)
    }

    fn suspend(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.transition(id, LifecycleCommand::Suspend)
    }

    fn resume(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.transition(id, LifecycleCommand::Resume)
    }

    fn terminate(&mut self, id: InstanceId) -> Result<(), FaygerError> {
        self.transition(id, LifecycleCommand::Terminate)
    }

    fn state(&self, id: InstanceId) -> Result<LifecycleState, FaygerError> {
        self.instances
            .get(&id.0)
            .map(|d| d.state)
            .ok_or_else(|| instance_not_found(id))
    }

    fn failure_info(&self, id: InstanceId) -> Result<Option<&FailureInfo>, FaygerError> {
        self.instances
            .get(&id.0)
            .map(|d| d.failure.as_ref())
            .ok_or_else(|| instance_not_found(id))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::Runtime;
    use crate::adapter::MockAdapter;
    use crate::instance::InstanceId;
    use crate::interface::RuntimeInterface;
    use crate::lifecycle::LifecycleState;
    use fayger_buf::manifest::{
        types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
        BuFManifest, EntryPoint, SectionDescriptor, SemVer,
    };
    use fayger_buf::object::{BuFObject, Section};
    use fayger_error::ErrorCode;
    use std::collections::{BTreeMap, BTreeSet};

    fn descriptor(id: &str, length: u64) -> SectionDescriptor {
        SectionDescriptor {
            id: id.into(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        }
    }

    fn object_with_sections(sections: Vec<(&str, Vec<u8>)>) -> BuFObject {
        object_with_quota(sections, None)
    }

    fn object_with_quota(
        sections: Vec<(&str, Vec<u8>)>,
        quotas: Option<fayger_buf::manifest::types::ResourceQuota>,
    ) -> BuFObject {
        let descriptors = sections
            .iter()
            .map(|(id, p)| descriptor(id, p.len() as u64))
            .collect();
        let manifest = BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas,
            sections: descriptors,
            signature: None,
            extensions: BTreeMap::new(),
        };
        let secs = sections
            .into_iter()
            .map(|(id, p)| Section::new(id, p))
            .collect();
        BuFObject {
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
        }
    }

    fn rt() -> Runtime<MockAdapter> {
        Runtime::new(MockAdapter::new())
    }

    #[test]
    fn load_returns_loaded_state_and_increasing_ids() {
        let mut r = rt();
        let a = r
            .load(object_with_sections(vec![("code", vec![1, 2])]))
            .unwrap();
        let b = r.load(object_with_sections(vec![])).unwrap();
        assert_eq!(a, InstanceId(0));
        assert_eq!(b, InstanceId(1));
        assert_eq!(r.state(a).unwrap(), LifecycleState::Loaded);
        assert_eq!(r.state(b).unwrap(), LifecycleState::Loaded);
    }

    #[test]
    fn happy_lifecycle_path() {
        let mut r = rt();
        let id = r
            .load(object_with_sections(vec![("code", vec![1])]))
            .unwrap();
        r.init(id).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Initialized);
        r.start(id).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Running);
        r.suspend(id).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Suspended);
        r.resume(id).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Running);
        r.terminate(id).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Terminated);
    }

    #[test]
    fn illegal_transition_keeps_state_and_reports_context() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![])).unwrap();
        // Loaded 收到 Start 非法
        let err = r.start(id).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::RtIllegalTransition);
        assert_eq!(r.state(id).unwrap(), LifecycleState::Loaded); // 状态不变
        assert!(err.context.contains_key("current_state"));
        assert!(err.context.contains_key("allowed_commands"));
        assert!(err.context.contains_key("command"));
    }

    #[test]
    fn terminal_state_rejects_commands() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![])).unwrap();
        r.terminate(id).unwrap();
        assert!(r.init(id).is_err());
        assert!(r.start(id).is_err());
        assert!(r.terminate(id).is_err());
    }

    #[test]
    fn unknown_instance_id_errors() {
        let r = rt();
        let err = r.state(InstanceId(999)).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::RtInstanceNotFound);
        assert!(err.context.contains_key("instance_id"));
    }

    #[test]
    fn step_dispatches_only_when_running() {
        let mut r = rt();
        let id = r
            .load(object_with_sections(vec![
                ("a", vec![1, 2]),
                ("b", vec![3]),
            ]))
            .unwrap();
        // Loaded：不派发
        assert!(!r.step(id).unwrap());
        assert_eq!(r.adapter().received.len(), 0);
        r.init(id).unwrap();
        r.start(id).unwrap();
        // Running：派发第一条
        assert!(r.step(id).unwrap());
        assert_eq!(r.adapter().received.len(), 1);
        // 派发第二条
        assert!(r.step(id).unwrap());
        assert_eq!(r.adapter().received.len(), 2);
        // 程序耗尽
        assert!(!r.step(id).unwrap());
        assert_eq!(r.adapter().received.len(), 2);
    }

    #[test]
    fn suspend_stops_dispatch_resume_continues() {
        let mut r = rt();
        let id = r
            .load(object_with_sections(vec![
                ("a", vec![1]),
                ("b", vec![2]),
                ("c", vec![3]),
            ]))
            .unwrap();
        r.init(id).unwrap();
        r.start(id).unwrap();
        assert!(r.step(id).unwrap()); // 派发 a
        r.suspend(id).unwrap();
        // Suspended：step 不派发
        assert!(!r.step(id).unwrap());
        assert_eq!(r.adapter().received.len(), 1);
        r.resume(id).unwrap();
        assert!(r.step(id).unwrap()); // 派发 b
        assert_eq!(r.adapter().received.len(), 2);
    }

    #[test]
    fn derived_program_uses_payload_length_as_operands() {
        let mut r = rt();
        let id = r
            .load(object_with_sections(vec![("a", vec![9; 5])]))
            .unwrap();
        r.init(id).unwrap();
        r.start(id).unwrap();
        r.step(id).unwrap();
        let instr = &r.adapter().received[0];
        assert_eq!(instr.category, 0);
        assert_eq!(instr.opcode, 0);
        // operands = 5u32 小端
        assert_eq!(instr.operands, 5u32.to_le_bytes().to_vec());
    }

    #[test]
    fn inject_fault_sets_failed_and_failure_info() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![("a", vec![1])])).unwrap();
        r.init(id).unwrap();
        r.start(id).unwrap();
        r.step(id).unwrap(); // 派发 a，记 last_instruction
        r.inject_fault(id, "boom").unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Failed);
        let fi = r.failure_info(id).unwrap().expect("has failure info");
        assert_eq!(fi.error.code, ErrorCode::RtInstanceFailed);
        assert!(fi.last_instruction.is_some());
    }

    #[test]
    fn failure_info_none_for_non_failed() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![])).unwrap();
        assert!(r.failure_info(id).unwrap().is_none());
    }

    #[test]
    fn inject_fault_rejects_terminal() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![])).unwrap();
        r.terminate(id).unwrap();
        assert!(r.inject_fault(id, "boom").is_err());
    }

    // ---- M7：资源监测 / 配额 / 隔离 ----

    use crate::resource::{QuotaExceeded, ResourceKind, ResourceUsage};
    use fayger_buf::manifest::types::ResourceQuota;

    fn quota(cpu: Option<u32>, mem: Option<u64>, io: Option<u64>) -> ResourceQuota {
        ResourceQuota {
            cpu_ms_per_sec: cpu,
            memory_bytes: mem,
            io_bytes_per_sec: io,
        }
    }

    fn running_with_quota(q: Option<ResourceQuota>) -> (Runtime<MockAdapter>, InstanceId) {
        let mut r = rt();
        let id = r.load(object_with_quota(vec![("a", vec![1])], q)).unwrap();
        r.init(id).unwrap();
        r.start(id).unwrap();
        (r, id)
    }

    #[test]
    fn load_reads_quota_from_manifest() {
        let mut r = rt();
        let q = quota(None, Some(100), None);
        let with = r
            .load(object_with_quota(vec![("a", vec![1])], Some(q.clone())))
            .unwrap();
        let without = r.load(object_with_sections(vec![("b", vec![2])])).unwrap();
        assert_eq!(r.quota(with).unwrap(), Some(&q));
        assert_eq!(r.quota(without).unwrap(), None);
    }

    #[test]
    fn queries_reject_unknown_id() {
        let r = rt();
        assert_eq!(
            r.quota(InstanceId(42)).expect_err("err").code,
            ErrorCode::RtInstanceNotFound
        );
        assert_eq!(
            r.usage(InstanceId(42)).expect_err("err").code,
            ErrorCode::RtInstanceNotFound
        );
        assert_eq!(
            r.quota_exceeded(InstanceId(42)).expect_err("err").code,
            ErrorCode::RtInstanceNotFound
        );
    }

    #[test]
    fn observe_usage_unknown_id_errors() {
        let mut r = rt();
        let err = r
            .observe_usage(InstanceId(7), ResourceUsage::new(1, 1, 1))
            .expect_err("err");
        assert_eq!(err.code, ErrorCode::RtInstanceNotFound);
    }

    #[test]
    fn observe_usage_running_exceed_suspends() {
        let (mut r, id) = running_with_quota(Some(quota(None, Some(100), None)));
        r.observe_usage(id, ResourceUsage::new(0, 200, 0)).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Suspended);
        let qe = r.quota_exceeded(id).unwrap().expect("exceeded");
        assert_eq!(
            qe,
            QuotaExceeded {
                resource: ResourceKind::Memory,
                usage: ResourceUsage::new(0, 200, 0),
            }
        );
        assert_eq!(r.usage(id).unwrap(), Some(ResourceUsage::new(0, 200, 0)));
    }

    #[test]
    fn observe_usage_within_quota_keeps_running_and_clears() {
        let (mut r, id) = running_with_quota(Some(quota(None, Some(100), None)));
        r.observe_usage(id, ResourceUsage::new(0, 100, 0)).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Running);
        assert_eq!(r.quota_exceeded(id).unwrap(), None);
        assert_eq!(r.usage(id).unwrap(), Some(ResourceUsage::new(0, 100, 0)));
    }

    #[test]
    fn observe_usage_no_quota_never_suspends() {
        let (mut r, id) = running_with_quota(None);
        r.observe_usage(id, ResourceUsage::new(u64::MAX, u64::MAX, u64::MAX))
            .unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Running);
        assert_eq!(r.quota_exceeded(id).unwrap(), None);
        assert_eq!(
            r.usage(id).unwrap(),
            Some(ResourceUsage::new(u64::MAX, u64::MAX, u64::MAX))
        );
    }

    #[test]
    fn observe_usage_non_running_does_not_migrate_but_records_usage() {
        let mut r = rt();
        // 停在 Loaded（非 Running）。
        let id = r
            .load(object_with_quota(
                vec![("a", vec![1])],
                Some(quota(None, Some(100), None)),
            ))
            .unwrap();
        r.observe_usage(id, ResourceUsage::new(0, 999, 0)).unwrap();
        assert_eq!(r.state(id).unwrap(), LifecycleState::Loaded);
        assert_eq!(r.quota_exceeded(id).unwrap(), None);
        assert_eq!(r.usage(id).unwrap(), Some(ResourceUsage::new(0, 999, 0)));
    }

    #[test]
    fn observe_usage_multi_dimension_picks_first_in_order() {
        let (mut r, id) = running_with_quota(Some(quota(Some(10), Some(100), Some(50))));
        // cpu + memory 同时超限 → Cpu（顺序首选）。
        r.observe_usage(id, ResourceUsage::new(11, 101, 0)).unwrap();
        let qe = r.quota_exceeded(id).unwrap().expect("exceeded");
        assert_eq!(qe.resource, ResourceKind::Cpu);
    }

    #[test]
    fn observe_usage_clears_stale_exceeded_when_back_within() {
        let (mut r, id) = running_with_quota(Some(quota(None, Some(100), None)));
        r.observe_usage(id, ResourceUsage::new(0, 200, 0)).unwrap();
        assert!(r.quota_exceeded(id).unwrap().is_some());
        // 回到 Running 后再注入未超限用量，超限记录应被清空。
        r.resume(id).unwrap();
        r.observe_usage(id, ResourceUsage::new(0, 50, 0)).unwrap();
        assert_eq!(r.quota_exceeded(id).unwrap(), None);
        assert_eq!(r.state(id).unwrap(), LifecycleState::Running);
    }

    // ---- M8：事件接入 ----

    use fayger_observability::{EventCategory, EventDraft, EventFilter, EventPayload, FaygerEvent};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Sink = Rc<RefCell<Vec<FaygerEvent>>>;

    fn subscribe_all(r: &mut Runtime<MockAdapter>) -> Sink {
        let sink: Sink = Rc::new(RefCell::new(Vec::new()));
        let s = Rc::clone(&sink);
        r.events_mut().subscribe(
            EventFilter::any(),
            Box::new(move |e| s.borrow_mut().push(e.clone())),
        );
        sink
    }

    fn lifecycle_pairs(sink: &Sink) -> Vec<(String, String)> {
        sink.borrow()
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::Lifecycle { from, to } => Some((from.clone(), to.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn transitions_publish_lifecycle_events() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![("a", vec![1])])).unwrap();
        let sink = subscribe_all(&mut r);
        r.init(id).unwrap();
        r.start(id).unwrap();
        r.suspend(id).unwrap();
        r.resume(id).unwrap();
        r.terminate(id).unwrap();
        assert_eq!(
            lifecycle_pairs(&sink),
            vec![
                ("Loaded".to_owned(), "Initialized".to_owned()),
                ("Initialized".to_owned(), "Running".to_owned()),
                ("Running".to_owned(), "Suspended".to_owned()),
                ("Suspended".to_owned(), "Running".to_owned()),
                ("Running".to_owned(), "Terminated".to_owned()),
            ]
        );
    }

    #[test]
    fn illegal_transition_publishes_no_event() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![])).unwrap();
        let sink = subscribe_all(&mut r);
        assert!(r.start(id).is_err()); // Loaded 收 Start 非法
        assert!(sink.borrow().is_empty());
    }

    #[test]
    fn inject_fault_publishes_failed_lifecycle_event() {
        let mut r = rt();
        let id = r.load(object_with_sections(vec![("a", vec![1])])).unwrap();
        r.init(id).unwrap();
        r.start(id).unwrap();
        let sink = subscribe_all(&mut r);
        r.inject_fault(id, "boom").unwrap();
        assert_eq!(
            lifecycle_pairs(&sink),
            vec![("Running".to_owned(), "Failed".to_owned())]
        );
    }

    #[test]
    fn observe_usage_within_publishes_quota_event_not_suspended() {
        let (mut r, id) = running_with_quota(Some(quota(None, Some(100), None)));
        let sink = subscribe_all(&mut r);
        r.observe_usage(id, ResourceUsage::new(0, 50, 0)).unwrap();
        let events = sink.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, EventCategory::Quota);
        match &events[0].payload {
            EventPayload::Quota {
                memory_bytes,
                suspended,
                exceeded,
                ..
            } => {
                assert_eq!(*memory_bytes, 50);
                assert!(!suspended);
                assert_eq!(*exceeded, None);
            }
            other => panic!("expected Quota, got {other:?}"),
        }
    }

    #[test]
    fn observe_usage_exceed_publishes_lifecycle_then_quota() {
        let (mut r, id) = running_with_quota(Some(quota(None, Some(100), None)));
        let sink = subscribe_all(&mut r);
        r.observe_usage(id, ResourceUsage::new(0, 200, 0)).unwrap();
        let events = sink.borrow();
        assert_eq!(events.len(), 2);
        // 先 Lifecycle(Running→Suspended)
        assert_eq!(events[0].category, EventCategory::Lifecycle);
        assert_eq!(
            events[0].payload,
            EventPayload::Lifecycle {
                from: "Running".to_owned(),
                to: "Suspended".to_owned(),
            }
        );
        // 后 Quota(suspended=true, exceeded=Memory)
        assert_eq!(events[1].category, EventCategory::Quota);
        match &events[1].payload {
            EventPayload::Quota {
                suspended,
                exceeded,
                ..
            } => {
                assert!(suspended);
                assert_eq!(exceeded.as_deref(), Some("Memory"));
            }
            other => panic!("expected Quota, got {other:?}"),
        }
        // 时间戳单调非减
        assert!(events[0].timestamp <= events[1].timestamp);
    }

    #[test]
    fn observe_usage_non_running_publishes_quota_only() {
        let mut r = rt();
        let id = r
            .load(object_with_quota(
                vec![("a", vec![1])],
                Some(quota(None, Some(100), None)),
            ))
            .unwrap();
        // 停在 Loaded（非 Running）。
        let sink = subscribe_all(&mut r);
        r.observe_usage(id, ResourceUsage::new(0, 999, 0)).unwrap();
        let events = sink.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, EventCategory::Quota);
        // 非 Running 超限：不发 Lifecycle，Quota.suspended=false、exceeded=None。
        match &events[0].payload {
            EventPayload::Quota {
                suspended,
                exceeded,
                ..
            } => {
                assert!(!suspended);
                assert_eq!(*exceeded, None);
            }
            other => panic!("expected Quota, got {other:?}"),
        }
    }

    #[test]
    fn no_subscriber_does_not_change_observable_behavior() {
        // 无订阅者时，事件发布只推进 EventBus 逻辑时钟，不改变状态/派发/查询。
        let mut r = rt();
        let id = r.load(object_with_sections(vec![("a", vec![2])])).unwrap();
        r.init(id).unwrap();
        r.start(id).unwrap();
        assert!(r.step(id).unwrap());
        assert_eq!(r.adapter().received.len(), 1);
        assert_eq!(r.state(id).unwrap(), LifecycleState::Running);
        // 逻辑时钟前进（init/start 两次迁移事件）。
        assert!(r.events().clock() >= 2);
    }

    #[test]
    fn unused_draft_constructors_are_exercised() {
        // 触达 loader/adapter/debug 便利构造器，避免未使用告警并锁定语义。
        let _ = EventDraft::loader("verify", fayger_observability::PhaseStatus::Enter, None);
        let _ = EventDraft::adapter("dispatch", fayger_observability::EventLevel::Info, Some(1));
        let _ = EventDraft::debug("trace", None);
    }
}
