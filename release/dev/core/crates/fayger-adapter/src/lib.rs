//! Fayger 适配层：Universal_Instruction 指令集、Platform_Adapter 与能力裁剪。
//!
//! 这个 crate 落实蓝图三层结构中的**适配层**：把运行层产生的平台无关
//! [`UniversalInstruction`] 在能力裁剪后翻译为目标宿主可执行的 [`HostSyscall`]，
//! 并把宿主事件归一化回 [`UniversalEvent`]。
//!
//! 模块：
//!
//! - [`mod@instruction`]：[`InstructionCategory`]（8 类）、[`InstructionKey`]、
//!   [`UniversalInstruction`]、[`UniversalEvent`]（M3 任务 3）。
//! - [`mod@capability`]：[`Capability`]、[`HostPolicy`]、[`negotiate`]、[`NegotiationResult`]（M3 任务 4）。
//! - [`mod@host`]：[`HostEnvironment`]、[`HostEnvironmentDetector`]、[`HostSyscall`]、[`HostEvent`]（M3 任务 5）。
//! - [`mod@adapter`]：[`PlatformAdapter`] 契约与 [`AdapterDescriptor`]（M3 任务 6）。
//! - [`mod@builtin`]：四类内置适配器 NativeDesktop / Server / Browser / InApp（M3 任务 7）。
//! - [`mod@registry`]：[`AdapterRegistry`] 选择、能力门槛与派发（M3 任务 8-9）。
//!
//! 详见 `release/dev/core/MILESTONES/M3-adapter-layer/design.md`。
//!
//! # 与运行层的关系
//!
//! M3 的 [`UniversalInstruction`] 是适配层视角的完整指令模型（含 8 类 `category` 与
//! `capabilities_required`）。它与 `fayger-runtime`（M2）里那个最小占位指令是**不同 crate
//! 的不同类型**；把二者统一（运行层改用适配层指令类型）是后续集成里程碑的工作。

#![doc(html_root_url = "https://docs.rs/fayger-adapter/0.0.0")]

pub mod adapter;
pub mod builtin;
pub mod capability;
pub mod host;
pub mod instruction;
pub mod registry;

pub use adapter::{AdapterDescriptor, PlatformAdapter};
pub use builtin::{BrowserAdapter, InAppAdapter, NativeDesktopAdapter, ServerAdapter};
pub use capability::{negotiate, Capability, HostPolicy, NegotiationResult};
pub use host::{FixedDetector, HostEnvironment, HostEnvironmentDetector, HostEvent, HostSyscall};
pub use instruction::{InstructionCategory, InstructionKey, UniversalEvent, UniversalInstruction};
pub use registry::AdapterRegistry;
