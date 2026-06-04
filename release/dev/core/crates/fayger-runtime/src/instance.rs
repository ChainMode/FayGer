//! BuF_Instance 标识与运行时数据。
//!
//! [`InstanceId`] 在单个 [`crate::runtime::Runtime`] 内唯一标识一个 BuF_Instance。
//! [`FailureInfo`] 保存实例进入 `Failed` 时的诊断上下文。

use crate::instruction::UniversalInstruction;
use crate::lifecycle::LifecycleState;
use crate::resource::{QuotaExceeded, ResourceUsage};
use fayger_buf::manifest::types::ResourceQuota;
use fayger_error::FaygerError;

/// BuF_Instance 在单个 Runtime 内的唯一标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(pub u64);

/// 实例进入 `Failed` 时保留的失败上下文。
///
/// 包含触发失败的最末一条 Universal_Instruction（若失败前曾派发过指令）与对应的
/// `FaygerError` 错误对象。
#[derive(Debug)]
pub struct FailureInfo {
    /// 失败前最后一次派发的指令；若实例尚未派发任何指令则为 `None`。
    pub last_instruction: Option<UniversalInstruction>,
    /// 触发失败的错误对象（`code == RT_INSTANCE_FAILED`）。
    pub error: FaygerError,
}

/// 单个实例的运行时数据（运行层内部）。
#[derive(Debug)]
pub(crate) struct InstanceData {
    /// 当前生命周期状态。
    pub(crate) state: LifecycleState,
    /// 从 `BuFObject` 派生的确定性 mock 程序。
    pub(crate) program: Vec<UniversalInstruction>,
    /// 程序计数器：已派发到第几条（下一条待派发的索引）。
    pub(crate) pc: usize,
    /// 失败上下文（仅 `Failed` 时为 `Some`）。
    pub(crate) failure: Option<FailureInfo>,
    /// 最后一次派发的指令（用于失败时填入 `FailureInfo`）。
    pub(crate) last_instruction: Option<UniversalInstruction>,
    /// 该实例生效的资源配额（来自 `manifest.quotas`，M7）。
    pub(crate) quota: Option<ResourceQuota>,
    /// 最近一次注入的资源用量采样（M7）。
    pub(crate) last_usage: Option<ResourceUsage>,
    /// 因超限暂停时记录的超限信息（M7）；未超限时为 `None`。
    pub(crate) quota_exceeded: Option<QuotaExceeded>,
}

impl InstanceData {
    /// 用初始 `Loaded` 状态、给定程序与配额构造。
    pub(crate) const fn new(
        program: Vec<UniversalInstruction>,
        quota: Option<ResourceQuota>,
    ) -> Self {
        Self {
            state: LifecycleState::Loaded,
            program,
            pc: 0,
            failure: None,
            last_instruction: None,
            quota,
            last_usage: None,
            quota_exceeded: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FailureInfo, InstanceData, InstanceId};
    use crate::instruction::UniversalInstruction;
    use crate::lifecycle::LifecycleState;
    use fayger_error::{ErrorCode, FaygerError, Layer};

    #[test]
    fn instance_id_equality_and_ordering() {
        assert_eq!(InstanceId(1), InstanceId(1));
        assert_ne!(InstanceId(1), InstanceId(2));
        assert!(InstanceId(1) < InstanceId(2));
    }

    #[test]
    fn failure_info_holds_instruction_and_error() {
        let instr = UniversalInstruction::new(0, 3, vec![1]);
        let err = FaygerError::new(ErrorCode::RtInstanceFailed, Layer::Runtime, "boom");
        let fi = FailureInfo {
            last_instruction: Some(instr.clone()),
            error: err,
        };
        assert_eq!(fi.last_instruction, Some(instr));
        assert_eq!(fi.error.code, ErrorCode::RtInstanceFailed);
    }

    #[test]
    fn instance_data_starts_loaded() {
        let d = InstanceData::new(vec![], None);
        assert_eq!(d.state, LifecycleState::Loaded);
        assert_eq!(d.pc, 0);
        assert!(d.failure.is_none());
        assert!(d.last_instruction.is_none());
        assert!(d.quota.is_none());
        assert!(d.last_usage.is_none());
        assert!(d.quota_exceeded.is_none());
    }

    #[test]
    fn instance_data_carries_quota() {
        use fayger_buf::manifest::types::ResourceQuota;
        let q = ResourceQuota {
            cpu_ms_per_sec: None,
            memory_bytes: Some(100),
            io_bytes_per_sec: None,
        };
        let d = InstanceData::new(vec![], Some(q.clone()));
        assert_eq!(d.quota, Some(q));
    }
}
