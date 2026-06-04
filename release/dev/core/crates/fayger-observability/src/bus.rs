//! 事件总线（M8）。
//!
//! [`EventBus`] 是同步、确定性、单线程的事件总线：维护逻辑时钟、调试开关与按注册顺序排列的
//! 订阅表。`publish` 为草稿盖上当前逻辑时钟、按注册顺序投递给匹配的订阅、随后递增逻辑时钟。
//!
//! 关键语义：
//! - 逻辑时钟仅在**被投递**的发布上前进；被调试开关丢弃的 `Debug` 事件既不投递也不递增（蓝图 §6.7）。
//! - 未匹配任何订阅的发布仍是一次"被投递"的发布（占用一个时间戳），保证时间戳与发布因果一致。
//! - 投递顺序按订阅注册顺序（确定性）。

use crate::event::{EventCategory, EventDraft, FaygerEvent};
use crate::filter::EventFilter;

/// 订阅句柄，用于 [`EventBus::unsubscribe`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubscriptionId(pub u64);

/// 事件处理回调。`FnMut` 以便订阅者把事件累积进自身缓冲。
type Handler = Box<dyn FnMut(&FaygerEvent)>;

/// 一个订阅条目。
struct Subscription {
    id: SubscriptionId,
    filter: EventFilter,
    handler: Handler,
}

/// 可观测性事件总线。
///
/// 默认构造时无订阅者、调试关闭、逻辑时钟为 0。事件不持久化：发布即投递给当时已注册的订阅者，
/// 未投递不保留。
pub struct EventBus {
    clock: u64,
    debug_enabled: bool,
    next_sub_id: u64,
    subscriptions: Vec<Subscription>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// 新建一个空的事件总线（调试关闭，逻辑时钟从 0 开始）。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            clock: 0,
            debug_enabled: false,
            next_sub_id: 0,
            subscriptions: Vec::new(),
        }
    }

    /// 当前逻辑时钟值（即下一条被投递事件将获得的时间戳）。
    #[must_use]
    pub const fn clock(&self) -> u64 {
        self.clock
    }

    /// 调试事件是否启用。
    #[must_use]
    pub const fn debug_enabled(&self) -> bool {
        self.debug_enabled
    }

    /// 当前订阅数。
    #[must_use]
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// 开启 / 关闭调试事件投递（蓝图 §6.7）。默认关闭。
    pub const fn set_debug_enabled(&mut self, enabled: bool) {
        self.debug_enabled = enabled;
    }

    /// 注册一个订阅，返回唯一的 [`SubscriptionId`]。投递顺序按注册顺序。
    pub fn subscribe(&mut self, filter: EventFilter, handler: Handler) -> SubscriptionId {
        let id = SubscriptionId(self.next_sub_id);
        self.next_sub_id += 1;
        self.subscriptions.push(Subscription {
            id,
            filter,
            handler,
        });
        id
    }

    /// 移除一个订阅。返回是否确有该订阅被移除（未知 id 返回 `false`）。
    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        let before = self.subscriptions.len();
        self.subscriptions.retain(|s| s.id != id);
        self.subscriptions.len() != before
    }

    /// 发布一条事件草稿。
    ///
    /// 返回 `Some(event)` 表示该草稿被成形并投递（即使无订阅匹配也算被投递）；返回 `None`
    /// 表示草稿因调试开关关闭而被丢弃（既不投递也不递增逻辑时钟）。
    pub fn publish(&mut self, draft: EventDraft) -> Option<FaygerEvent> {
        if draft.category == EventCategory::Debug && !self.debug_enabled {
            return None;
        }
        let event = FaygerEvent::from_draft(draft, self.clock);
        for sub in &mut self.subscriptions {
            if sub.filter.matches(&event) {
                (sub.handler)(&event);
            }
        }
        self.clock += 1;
        Some(event)
    }
}

impl std::fmt::Debug for EventBus {
    /// 不打印 handler（trait object 不可 `Debug`）；只打印计量字段。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("clock", &self.clock)
            .field("debug_enabled", &self.debug_enabled)
            .field("subscriptions", &self.subscriptions.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{EventBus, EventFilter};
    use crate::event::{EventCategory, EventDraft};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Sink = Rc<RefCell<Vec<crate::event::FaygerEvent>>>;

    fn sink() -> Sink {
        Rc::new(RefCell::new(Vec::new()))
    }

    fn collector(s: &Sink) -> Box<dyn FnMut(&crate::event::FaygerEvent)> {
        let s = Rc::clone(s);
        Box::new(move |e| s.borrow_mut().push(e.clone()))
    }

    #[test]
    fn publish_stamps_clock_and_increments() {
        let mut bus = EventBus::new();
        let s = sink();
        bus.subscribe(EventFilter::any(), collector(&s));
        let e0 = bus
            .publish(EventDraft::lifecycle("a", "b", 1))
            .expect("delivered");
        let e1 = bus
            .publish(EventDraft::lifecycle("b", "c", 1))
            .expect("delivered");
        assert_eq!(e0.timestamp, 0);
        assert_eq!(e1.timestamp, 1);
        assert_eq!(bus.clock(), 2);
        assert_eq!(s.borrow().len(), 2);
    }

    #[test]
    fn no_subscriber_still_advances_clock() {
        let mut bus = EventBus::new();
        bus.publish(EventDraft::lifecycle("a", "b", 1))
            .expect("delivered");
        assert_eq!(bus.clock(), 1);
    }

    #[test]
    fn debug_event_dropped_when_disabled_and_clock_not_advanced() {
        let mut bus = EventBus::new();
        let s = sink();
        bus.subscribe(EventFilter::any(), collector(&s));
        assert!(bus.publish(EventDraft::debug("x", None)).is_none());
        assert_eq!(bus.clock(), 0);
        assert!(s.borrow().is_empty());
    }

    #[test]
    fn debug_event_delivered_when_enabled() {
        let mut bus = EventBus::new();
        let s = sink();
        bus.subscribe(EventFilter::any(), collector(&s));
        bus.set_debug_enabled(true);
        assert!(bus.publish(EventDraft::debug("x", None)).is_some());
        assert_eq!(bus.clock(), 1);
        assert_eq!(s.borrow().len(), 1);
    }

    #[test]
    fn filter_gates_delivery_but_not_clock() {
        let mut bus = EventBus::new();
        let s = sink();
        // 只收 Quota
        bus.subscribe(
            EventFilter::any().with_categories([EventCategory::Quota]),
            collector(&s),
        );
        // 发一条 Lifecycle：不投递给该订阅，但时钟仍前进
        bus.publish(EventDraft::lifecycle("a", "b", 1))
            .expect("delivered");
        assert_eq!(bus.clock(), 1);
        assert!(s.borrow().is_empty());
    }

    #[test]
    fn subscriptions_receive_in_registration_order() {
        let mut bus = EventBus::new();
        let order = Rc::new(RefCell::new(Vec::<u8>::new()));
        let o1 = Rc::clone(&order);
        let o2 = Rc::clone(&order);
        bus.subscribe(
            EventFilter::any(),
            Box::new(move |_| o1.borrow_mut().push(1)),
        );
        bus.subscribe(
            EventFilter::any(),
            Box::new(move |_| o2.borrow_mut().push(2)),
        );
        bus.publish(EventDraft::lifecycle("a", "b", 1))
            .expect("delivered");
        assert_eq!(*order.borrow(), vec![1, 2]);
    }

    #[test]
    fn unsubscribe_removes_only_that_subscription() {
        let mut bus = EventBus::new();
        let s1 = sink();
        let s2 = sink();
        let id1 = bus.subscribe(EventFilter::any(), collector(&s1));
        bus.subscribe(EventFilter::any(), collector(&s2));
        assert!(bus.unsubscribe(id1));
        // 未知 id
        assert!(!bus.unsubscribe(id1));
        bus.publish(EventDraft::lifecycle("a", "b", 1))
            .expect("delivered");
        assert!(s1.borrow().is_empty());
        assert_eq!(s2.borrow().len(), 1);
    }
}
