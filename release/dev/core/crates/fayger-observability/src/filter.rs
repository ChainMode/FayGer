//! 订阅过滤器（M8）。
//!
//! [`EventFilter`] 按三个维度过滤：类别集合、最低等级、实例 ID。三条件按 **AND** 组合；
//! 任一维度为 `None` 表示该维度不限制。

use crate::event::{EventCategory, EventLevel, FaygerEvent};
use std::collections::BTreeSet;

/// 事件订阅过滤器。
///
/// 各维度语义：
/// - `categories`：`None` 通过任意类别；`Some(set)` 要求 `set.contains(event.category)`。
/// - `min_level`：`None` 不限等级；`Some(l)` 要求 `event.level >= l`。
/// - `instance_id`：`None` 匹配任意实例（含无实例事件）；`Some(id)` 要求 `event.instance_id == Some(id)`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventFilter {
    /// 允许的类别集合；`None` 表示全部。
    pub categories: Option<BTreeSet<EventCategory>>,
    /// 最低等级（含）；`None` 表示不限。
    pub min_level: Option<EventLevel>,
    /// 关注的实例 ID；`None` 表示任意。
    pub instance_id: Option<u64>,
}

impl EventFilter {
    /// 不设任何限制的过滤器（匹配一切被投递事件）。
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// 限定类别集合。
    #[must_use]
    pub fn with_categories(mut self, categories: impl IntoIterator<Item = EventCategory>) -> Self {
        self.categories = Some(categories.into_iter().collect());
        self
    }

    /// 限定最低等级。
    #[must_use]
    pub const fn with_min_level(mut self, level: EventLevel) -> Self {
        self.min_level = Some(level);
        self
    }

    /// 限定实例 ID。
    #[must_use]
    pub const fn for_instance(mut self, id: u64) -> Self {
        self.instance_id = Some(id);
        self
    }

    /// 该事件是否通过本过滤器（三条件 AND）。
    #[must_use]
    pub fn matches(&self, event: &FaygerEvent) -> bool {
        if let Some(cats) = self.categories.as_ref() {
            if !cats.contains(&event.category) {
                return false;
            }
        }
        if let Some(min) = self.min_level {
            if event.level < min {
                return false;
            }
        }
        if let Some(id) = self.instance_id {
            if event.instance_id != Some(id) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::EventFilter;
    use crate::event::{EventCategory, EventDraft, EventLevel, FaygerEvent};

    fn evt() -> FaygerEvent {
        FaygerEvent::from_draft(EventDraft::lifecycle("Running", "Suspended", 5), 0)
    }

    #[test]
    fn any_matches_everything() {
        assert!(EventFilter::any().matches(&evt()));
    }

    #[test]
    fn category_filter() {
        let only_quota = EventFilter::any().with_categories([EventCategory::Quota]);
        assert!(!only_quota.matches(&evt())); // 事件是 Lifecycle
        let only_lifecycle = EventFilter::any().with_categories([EventCategory::Lifecycle]);
        assert!(only_lifecycle.matches(&evt()));
    }

    #[test]
    fn min_level_boundary_is_inclusive() {
        // 事件等级 Info
        let at_info = EventFilter::any().with_min_level(EventLevel::Info);
        assert!(at_info.matches(&evt()));
        let at_warn = EventFilter::any().with_min_level(EventLevel::Warn);
        assert!(!at_warn.matches(&evt()));
        let at_debug = EventFilter::any().with_min_level(EventLevel::Debug);
        assert!(at_debug.matches(&evt()));
    }

    #[test]
    fn instance_filter() {
        assert!(EventFilter::any().for_instance(5).matches(&evt()));
        assert!(!EventFilter::any().for_instance(6).matches(&evt()));
    }

    #[test]
    fn instance_filter_rejects_eventless_when_required() {
        let mut e = evt();
        e.instance_id = None;
        assert!(!EventFilter::any().for_instance(5).matches(&e));
        // 无 instance 限制时通过
        assert!(EventFilter::any().matches(&e));
    }

    #[test]
    fn combined_filters_are_anded() {
        let f = EventFilter::any()
            .with_categories([EventCategory::Lifecycle])
            .with_min_level(EventLevel::Info)
            .for_instance(5);
        assert!(f.matches(&evt()));
        // 改一个维度即不通过
        assert!(!f.for_instance(99).matches(&evt()));
    }
}
