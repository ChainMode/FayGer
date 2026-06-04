//! Fayger 运行层骨架：Runtime_Interface、生命周期与 Mock Adapter。
//!
//! 这个 crate 在 M0（[`fayger_buf`] 格式）与 M1（`fayger-loader` 加载链）之上落实蓝图
//! 三层结构中的**运行层**骨架：把一个已加载的 [`fayger_buf::object::BuFObject`] 提升为一个
//! 可被启动 / 暂停 / 恢复 / 停止、并向适配层派发 Universal_Instruction 的 BuF_Instance。
//!
//! 模块：
//!
//! - [`mod@instruction`]：最小 [`UniversalInstruction`] / [`UniversalEvent`] 占位（M2 任务 3）。
//! - [`mod@adapter`]：[`Adapter`] trait 与 [`MockAdapter`]（M2 任务 4）。
//! - [`mod@lifecycle`]：[`LifecycleState`] / [`LifecycleCommand`] 与合法迁移（M2 任务 5）。
//! - [`mod@interface`]：[`RuntimeInterface`] 契约、[`RuntimeOperation`] 与 [`RuntimeDescriptor`]（M2 任务 6）。
//! - [`mod@instance`]：[`InstanceId`] 与 [`FailureInfo`]（M2 任务 7）。
//! - [`mod@registry`]：[`RuntimeRegistry`] 注册完备性与路由（M2 任务 8）。
//! - [`mod@runtime`]：[`Runtime`] 参考实现（M2 任务 9-10）。
//!
//! 详见 `release/dev/core/MILESTONES/M2-runtime-skeleton/design.md`。

#![doc(html_root_url = "https://docs.rs/fayger-runtime/0.0.0")]

pub mod adapter;
pub mod instance;
pub mod instruction;
pub mod interface;
pub mod lifecycle;
pub mod registry;
pub mod resource;
pub mod runtime;

pub use adapter::{Adapter, MockAdapter};
pub use instance::{FailureInfo, InstanceId};
pub use instruction::{UniversalEvent, UniversalInstruction};
pub use interface::{RuntimeDescriptor, RuntimeInterface, RuntimeOperation};
pub use lifecycle::{LifecycleCommand, LifecycleState};
pub use registry::{RegistrationId, RuntimeRegistry};
pub use resource::{first_exceeded, QuotaExceeded, ResourceKind, ResourceUsage};
pub use runtime::Runtime;
