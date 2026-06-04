//! Runtime_Interface 契约与注册描述符。
//!
//! [`RuntimeInterface`] 是运行层对外的稳定接口契约（Rust trait 表达），覆盖 BuF_Instance
//! 的加载、初始化、执行、暂停、恢复、终止与状态/失败查询。它通过 Universal_Instruction
//! 与适配层通信，不引用任何 Platform_Adapter 类型。
//!
//! 由于 Rust trait 在编译期即保证方法齐全，无法表达"实现缺方法"，注册完备性建模在
//! **数据层**：[`RuntimeDescriptor`] 声明 `supported_operations`，[`crate::registry`] 据此
//! 校验。这覆盖插件式 / 跨语言实现的注册场景，并使该性质可被属性测试验证。

use crate::instance::{FailureInfo, InstanceId};
use crate::lifecycle::LifecycleState;
use fayger_buf::manifest::SemVer;
use fayger_buf::object::BuFObject;
use fayger_error::FaygerError;
use std::collections::BTreeSet;

/// Runtime_Interface 的必需操作。用于注册完备性建模与属性测试。
///
/// `Emit` 与 `OnEvent` 表示运行层与适配层之间的双向通信能力。M2 的参考实现在
/// 执行路径内部调用 `Adapter::dispatch`（对应 `Emit`），不把它们暴露为 trait 公开方法；
/// 但注册校验要求一个实现"声明"覆盖它们。M3 会把双向通信提升为显式接口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuntimeOperation {
    /// 加载 BuFObject 为实例。
    Load,
    /// 初始化。
    Init,
    /// 开始执行。
    Start,
    /// 暂停。
    Suspend,
    /// 恢复。
    Resume,
    /// 终止。
    Terminate,
    /// 状态查询。
    State,
    /// 失败信息查询。
    FailureInfo,
    /// 向适配层派发 Universal_Instruction。
    Emit,
    /// 接收适配层回送的事件。
    OnEvent,
}

impl RuntimeOperation {
    /// 全部必需操作的集合（REQUIRED_OPERATIONS）。
    #[must_use]
    pub fn required() -> BTreeSet<Self> {
        [
            Self::Load,
            Self::Init,
            Self::Start,
            Self::Suspend,
            Self::Resume,
            Self::Terminate,
            Self::State,
            Self::FailureInfo,
            Self::Emit,
            Self::OnEvent,
        ]
        .into_iter()
        .collect()
    }
}

/// 对一个 Runtime_Implementation 的数据化描述。
///
/// 注册时 [`crate::registry::RuntimeRegistry`] 校验 `supported_operations` 是否覆盖
/// [`RuntimeOperation::required`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDescriptor {
    /// 实现标识（用于 `preferred_impl` 路由匹配）。
    pub impl_id: String,
    /// 该实现提供的 Runtime_Interface 版本（用于版本兼容判定）。
    pub rt_version: SemVer,
    /// 该实现声明支持的操作集合。
    pub supported_operations: BTreeSet<RuntimeOperation>,
}

impl RuntimeDescriptor {
    /// 构造一个声明支持全部必需操作的描述符。
    #[must_use]
    pub fn complete(impl_id: impl Into<String>, rt_version: SemVer) -> Self {
        Self {
            impl_id: impl_id.into(),
            rt_version,
            supported_operations: RuntimeOperation::required(),
        }
    }
}

/// Runtime_Interface：运行层对外的稳定接口契约。
///
/// 所有操作在失败时返回统一错误模型 [`FaygerError`]（`source_layer == Runtime`）。
/// 以 [`InstanceId`] 为参数的操作在实例不存在时返回 `RT_INSTANCE_NOT_FOUND`。
pub trait RuntimeInterface {
    /// 本实现提供的 Runtime_Interface 版本。
    fn rt_version(&self) -> SemVer;

    /// 本实现的描述符。
    fn descriptor(&self) -> RuntimeDescriptor;

    /// 加载一个 BuFObject，创建处于 `Loaded` 状态的实例，返回其 `InstanceId`。
    ///
    /// # Errors
    /// 实现相关的加载失败。
    fn load(&mut self, obj: BuFObject) -> Result<InstanceId, FaygerError>;

    /// `Loaded → Initialized`。
    ///
    /// # Errors
    /// 实例不存在（`RT_INSTANCE_NOT_FOUND`）或非法迁移（`RT_ILLEGAL_TRANSITION`）。
    fn init(&mut self, id: InstanceId) -> Result<(), FaygerError>;

    /// `Initialized | Suspended → Running`。
    ///
    /// # Errors
    /// 实例不存在或非法迁移。
    fn start(&mut self, id: InstanceId) -> Result<(), FaygerError>;

    /// `Running → Suspended`。
    ///
    /// # Errors
    /// 实例不存在或非法迁移。
    fn suspend(&mut self, id: InstanceId) -> Result<(), FaygerError>;

    /// `Suspended → Running`。
    ///
    /// # Errors
    /// 实例不存在或非法迁移。
    fn resume(&mut self, id: InstanceId) -> Result<(), FaygerError>;

    /// 任意非终态 → `Terminated`。
    ///
    /// # Errors
    /// 实例不存在或非法迁移（终态再终止）。
    fn terminate(&mut self, id: InstanceId) -> Result<(), FaygerError>;

    /// 查询实例当前生命周期状态。
    ///
    /// # Errors
    /// 实例不存在（`RT_INSTANCE_NOT_FOUND`）。
    fn state(&self, id: InstanceId) -> Result<LifecycleState, FaygerError>;

    /// 查询实例失败信息；非 `Failed` 状态返回 `None`。
    ///
    /// # Errors
    /// 实例不存在（`RT_INSTANCE_NOT_FOUND`）。
    fn failure_info(&self, id: InstanceId) -> Result<Option<&FailureInfo>, FaygerError>;
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{RuntimeDescriptor, RuntimeOperation};
    use fayger_buf::manifest::SemVer;

    #[test]
    fn required_contains_all_ten_operations() {
        let req = RuntimeOperation::required();
        assert_eq!(req.len(), 10);
        for op in [
            RuntimeOperation::Load,
            RuntimeOperation::Init,
            RuntimeOperation::Start,
            RuntimeOperation::Suspend,
            RuntimeOperation::Resume,
            RuntimeOperation::Terminate,
            RuntimeOperation::State,
            RuntimeOperation::FailureInfo,
            RuntimeOperation::Emit,
            RuntimeOperation::OnEvent,
        ] {
            assert!(req.contains(&op), "missing {op:?}");
        }
    }

    #[test]
    fn complete_descriptor_supports_all_required() {
        let d = RuntimeDescriptor::complete("ref", SemVer::parse("1.0.0").expect("ok"));
        assert_eq!(d.supported_operations, RuntimeOperation::required());
        assert_eq!(d.impl_id, "ref");
    }
}
