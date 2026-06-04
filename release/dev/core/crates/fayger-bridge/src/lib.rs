//! Fayger 运行层↔适配层统一桥接（M10 里程碑）。
//!
//! 这个集成层 crate 把运行层（[`fayger_runtime`]）的执行流真正经由适配层
//! （[`fayger_adapter`]）的「选择 → 能力裁剪 → 翻译」派发为 [`fayger_adapter::HostSyscall`]，
//! 并在失败时构造蓝图 §6.3 的 `[Runtime, Adapter]` 跨层错误链。
//!
//! 关键分层约束（蓝图 §8.8）：**运行层不得 import 任何 Platform_Adapter 类型**。因此桥接经
//! 运行层暴露的 [`fayger_runtime::Adapter`] trait 这一接缝完成——`fayger-runtime` 不依赖
//! `fayger-adapter`，只有本 crate 同时依赖二者，依赖方向单向无环。
//!
//! 模块：
//!
//! - [`mod@mapping`]：[`InstructionMapping`] 与 [`to_adapter_instruction`]（运行层指令 → 适配层指令）。
//! - [`mod@adapter`]：[`BridgeAdapter`]（实现 [`fayger_runtime::Adapter`]）与 [`dispatch_one`]。
//! - [`mod@run`]：[`run_bridged`] / [`BridgeReport`] / [`register_builtin_adapters`]。
//! - [`mod@error`]：[`wrap_dispatch_failure`]（`ADP_*` → `RT_INSTANCE_FAILED` cause）。
//!
//! 详见 `release/dev/core/MILESTONES/M10-runtime-adapter-bridge/design.md`。
//!
//! # Example
//!
//! ```
//! use fayger_bridge::{run_bridged, InstructionMapping};
//! use fayger_adapter::{HostEnvironment, HostPolicy, builtin::CAP_HOST_CALL};
//! use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
//! use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
//! use fayger_buf::object::{BuFObject, Section};
//! use std::collections::{BTreeMap, BTreeSet};
//!
//! // 构造一个最小单段 BuF 对象。
//! let manifest = BuFManifest {
//!     schema_version: SemVer::parse("1.0.0").unwrap(),
//!     runtime_interface_min: SemVer::parse("1.0.0").unwrap(),
//!     deprecation_notice: None,
//!     entry: EntryPoint::new("main"),
//!     runtime: None,
//!     capabilities: BTreeSet::new(),
//!     quotas: None,
//!     sections: vec![SectionDescriptor {
//!         id: "code".into(), kind: SectionKind::Code, digest: Digest::placeholder(),
//!         length: 2, visibility: SectionVisibility::Required,
//!         profile_constraints: ProfileConstraints::default(),
//!     }],
//!     signature: None,
//!     extensions: BTreeMap::new(),
//! };
//! let obj = BuFObject {
//!     manifest,
//!     sections: vec![Section::new("code", vec![1, 2])],
//!     raw_header: fayger_buf::format::Header {
//!         format_version: fayger_buf::format::FORMAT_VERSION_M0,
//!         manifest_offset: 0, manifest_length: 0,
//!         section_index_offset: 0, section_index_length: 0,
//!     },
//!     raw_trailer: fayger_buf::format::Trailer::placeholder(0),
//! };
//!
//! let host = HostEnvironment::new(
//!     "native-desktop", "linux", "x86_64", "none",
//!     std::iter::once(fayger_adapter::Capability::new(CAP_HOST_CALL)).collect(),
//! );
//! let report = run_bridged(obj, &host, &HostPolicy::allow_all([CAP_HOST_CALL]),
//!                          InstructionMapping::canonical_io()).unwrap();
//! assert_eq!(report.dispatched, 1);
//! assert_eq!(report.platform_id, "native-desktop");
//! ```

#![doc(html_root_url = "https://docs.rs/fayger-bridge/0.0.0")]

pub mod adapter;
pub mod error;
pub mod mapping;
pub mod run;

pub use adapter::{dispatch_one, BridgeAdapter, DispatchOutcome};
pub use error::wrap_dispatch_failure;
pub use mapping::{to_adapter_instruction, InstructionMapping};
pub use run::{register_builtin_adapters, run_bridged, BridgeReport};
