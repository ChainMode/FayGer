//! Fayger 可观测性事件总线与跨层错误链支撑（蓝图 §6.5–§6.8，M8 里程碑）。
//!
//! 这个 crate 提供 Fayger 第一阶段的**可观测性事件总线**：一个同步、确定性、单线程的
//! [`EventBus`]，承载五类事件（Lifecycle / Quota / Loader / Adapter / Debug），支持按
//! [`EventFilter`] 订阅、取消订阅与调试开关。事件时间戳为内部逻辑时钟（单调非减）。
//!
//! 跨层错误**因果链**的能力（`wrap` / `chain` / `layer_chain` / `root_cause` / `depth`）作为
//! 加性扩展落在 [`fayger_error::FaygerError`] 上（见该 crate），本 crate 不重复实现。
//!
//! 模块：
//!
//! - [`mod@event`]：[`EventCategory`] / [`EventLevel`] / [`EventPayload`] / [`EventDraft`] / [`FaygerEvent`]。
//! - [`mod@filter`]：[`EventFilter`] 与三维 `matches`。
//! - [`mod@bus`]：[`EventBus`] 与 [`SubscriptionId`]。
//!
//! 详见 `release/dev/core/MILESTONES/M8-observability/design.md`。
//!
//! # Example
//!
//! ```
//! use fayger_observability::{EventBus, EventDraft, EventFilter};
//! use std::cell::RefCell;
//! use std::rc::Rc;
//!
//! let mut bus = EventBus::new();
//! let seen = Rc::new(RefCell::new(0usize));
//! let s = Rc::clone(&seen);
//! bus.subscribe(EventFilter::any(), Box::new(move |_e| *s.borrow_mut() += 1));
//!
//! bus.publish(EventDraft::lifecycle("Loaded", "Initialized", 0));
//! assert_eq!(*seen.borrow(), 1);
//! assert_eq!(bus.clock(), 1);
//! ```

#![doc(html_root_url = "https://docs.rs/fayger-observability/0.0.0")]

pub mod bus;
pub mod event;
pub mod filter;

pub use bus::{EventBus, SubscriptionId};
pub use event::{EventCategory, EventDraft, EventLevel, EventPayload, FaygerEvent, PhaseStatus};
pub use filter::EventFilter;
