//! 事件类型：类别、等级、负载、草稿与成形事件（M8）。
//!
//! [`EventDraft`] 是发布前的草稿（不含时间戳）；[`crate::bus::EventBus`] 在 `publish` 时为其
//! 分配逻辑时钟时间戳形成 [`FaygerEvent`]。生命周期状态以 `String` 承载（`format!("{state:?}")`），
//! 避免本 crate 反向依赖运行层的 `LifecycleState`（防止依赖环）。

/// 事件类别（蓝图 §6.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventCategory {
    /// BuF_Instance 生命周期状态迁移。
    Lifecycle,
    /// 资源采样 / 超限 / 暂停。
    Quota,
    /// 加载链阶段进入 / 完成 / 失败。
    Loader,
    /// 指令派发、能力裁剪结果、不支持指令记录。
    Adapter,
    /// 仅在调试开关开启时投递的深度诊断事件。
    Debug,
}

/// 事件等级（全序：`Debug` < `Info` < `Warn` < `Error`，蓝图 §6.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventLevel {
    /// 调试级。
    Debug,
    /// 信息级（Lifecycle / Loader 默认）。
    Info,
    /// 警告级。
    Warn,
    /// 错误级（Quota / Adapter 错误默认）。
    Error,
}

/// 加载链阶段状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhaseStatus {
    /// 进入某阶段。
    Enter,
    /// 完成某阶段。
    Complete,
    /// 某阶段失败。
    Fail,
}

/// 类型化事件负载。各类别携带其特有字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    /// 生命周期迁移：源状态名 → 目标状态名。
    Lifecycle {
        /// 迁移前状态名。
        from: String,
        /// 迁移后状态名。
        to: String,
    },
    /// 资源采样：三维用量 + 是否因本次采样暂停 + 超限维度名（若有）。
    Quota {
        /// 每秒 CPU 毫秒。
        cpu_ms_per_sec: u64,
        /// 内存字节。
        memory_bytes: u64,
        /// 每秒 I/O 字节。
        io_bytes_per_sec: u64,
        /// 是否因本次采样触发暂停。
        suspended: bool,
        /// 首个超限的资源维度名（如 `"Memory"`）；未超限为 `None`。
        exceeded: Option<String>,
    },
    /// 加载阶段：阶段名 + 状态。
    Loader {
        /// 阶段名（如 `"verify_digest"`）。
        phase: String,
        /// 阶段状态。
        status: PhaseStatus,
    },
    /// 适配层：自由文本细节。
    Adapter {
        /// 细节描述。
        detail: String,
    },
    /// 调试：自由文本细节。
    Debug {
        /// 细节描述。
        detail: String,
    },
}

/// 发布前的事件草稿（不含时间戳）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDraft {
    /// 事件类别。
    pub category: EventCategory,
    /// 事件等级。
    pub level: EventLevel,
    /// 关联实例（可选）。
    pub instance_id: Option<u64>,
    /// 类型化负载。
    pub payload: EventPayload,
}

impl EventDraft {
    /// 构造一条生命周期事件草稿（`Lifecycle` 类别 / `Info` 级）。
    #[must_use]
    pub fn lifecycle(from: impl Into<String>, to: impl Into<String>, instance_id: u64) -> Self {
        Self {
            category: EventCategory::Lifecycle,
            level: EventLevel::Info,
            instance_id: Some(instance_id),
            payload: EventPayload::Lifecycle {
                from: from.into(),
                to: to.into(),
            },
        }
    }

    /// 构造一条资源采样事件草稿（`Quota` 类别）。
    ///
    /// 暂停时（`suspended == true`）等级为 `Warn`，否则 `Info`。
    #[must_use]
    pub const fn quota(
        instance_id: u64,
        cpu_ms_per_sec: u64,
        memory_bytes: u64,
        io_bytes_per_sec: u64,
        suspended: bool,
        exceeded: Option<String>,
    ) -> Self {
        Self {
            category: EventCategory::Quota,
            level: if suspended {
                EventLevel::Warn
            } else {
                EventLevel::Info
            },
            instance_id: Some(instance_id),
            payload: EventPayload::Quota {
                cpu_ms_per_sec,
                memory_bytes,
                io_bytes_per_sec,
                suspended,
                exceeded,
            },
        }
    }

    /// 构造一条加载阶段事件草稿（`Loader` 类别）。
    ///
    /// `Fail` 状态等级为 `Error`，否则 `Info`。
    #[must_use]
    pub fn loader(phase: impl Into<String>, status: PhaseStatus, instance_id: Option<u64>) -> Self {
        Self {
            category: EventCategory::Loader,
            level: if matches!(status, PhaseStatus::Fail) {
                EventLevel::Error
            } else {
                EventLevel::Info
            },
            instance_id,
            payload: EventPayload::Loader {
                phase: phase.into(),
                status,
            },
        }
    }

    /// 构造一条适配层事件草稿（`Adapter` 类别），指定等级。
    #[must_use]
    pub fn adapter(detail: impl Into<String>, level: EventLevel, instance_id: Option<u64>) -> Self {
        Self {
            category: EventCategory::Adapter,
            level,
            instance_id,
            payload: EventPayload::Adapter {
                detail: detail.into(),
            },
        }
    }

    /// 构造一条调试事件草稿（`Debug` 类别 / `Debug` 级）。
    #[must_use]
    pub fn debug(detail: impl Into<String>, instance_id: Option<u64>) -> Self {
        Self {
            category: EventCategory::Debug,
            level: EventLevel::Debug,
            instance_id,
            payload: EventPayload::Debug {
                detail: detail.into(),
            },
        }
    }
}

/// 已成形事件：在 [`EventDraft`] 基础上由 [`crate::bus::EventBus`] 盖上逻辑时钟时间戳。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaygerEvent {
    /// 事件类别。
    pub category: EventCategory,
    /// 事件等级。
    pub level: EventLevel,
    /// 逻辑时钟时间戳（单调非减）。
    pub timestamp: u64,
    /// 关联实例（可选）。
    pub instance_id: Option<u64>,
    /// 类型化负载。
    pub payload: EventPayload,
}

impl FaygerEvent {
    /// 用给定时间戳把一个草稿成形为事件。
    #[must_use]
    pub fn from_draft(draft: EventDraft, timestamp: u64) -> Self {
        Self {
            category: draft.category,
            level: draft.level,
            timestamp,
            instance_id: draft.instance_id,
            payload: draft.payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EventCategory, EventDraft, EventLevel, EventPayload, FaygerEvent, PhaseStatus};

    #[test]
    fn level_is_totally_ordered() {
        assert!(EventLevel::Debug < EventLevel::Info);
        assert!(EventLevel::Info < EventLevel::Warn);
        assert!(EventLevel::Warn < EventLevel::Error);
    }

    #[test]
    fn lifecycle_draft_fields() {
        let d = EventDraft::lifecycle("Running", "Suspended", 7);
        assert_eq!(d.category, EventCategory::Lifecycle);
        assert_eq!(d.level, EventLevel::Info);
        assert_eq!(d.instance_id, Some(7));
        assert_eq!(
            d.payload,
            EventPayload::Lifecycle {
                from: "Running".to_owned(),
                to: "Suspended".to_owned(),
            }
        );
    }

    #[test]
    fn quota_draft_level_depends_on_suspended() {
        let normal = EventDraft::quota(1, 0, 50, 0, false, None);
        assert_eq!(normal.level, EventLevel::Info);
        let suspended = EventDraft::quota(1, 0, 200, 0, true, Some("Memory".to_owned()));
        assert_eq!(suspended.level, EventLevel::Warn);
    }

    #[test]
    fn loader_draft_fail_is_error_level() {
        let ok = EventDraft::loader("verify", PhaseStatus::Complete, None);
        assert_eq!(ok.level, EventLevel::Info);
        let fail = EventDraft::loader("verify", PhaseStatus::Fail, None);
        assert_eq!(fail.level, EventLevel::Error);
    }

    #[test]
    fn debug_draft_is_debug_category_and_level() {
        let d = EventDraft::debug("dispatch detail", Some(3));
        assert_eq!(d.category, EventCategory::Debug);
        assert_eq!(d.level, EventLevel::Debug);
    }

    #[test]
    fn from_draft_stamps_timestamp() {
        let d = EventDraft::lifecycle("Loaded", "Initialized", 0);
        let e = FaygerEvent::from_draft(d, 42);
        assert_eq!(e.timestamp, 42);
        assert_eq!(e.category, EventCategory::Lifecycle);
    }
}
