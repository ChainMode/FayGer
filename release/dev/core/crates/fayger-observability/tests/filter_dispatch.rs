//! Feature: fayger-m8, Property 2 + 3 + 5: 订阅过滤 / 调试门控 / 取消订阅与多订阅隔离。
//!
//! Property 2：某订阅收到的事件恰为"被投递且通过该 filter"的事件子序列。
//! Property 3：调试关闭时无 Debug 被投递且逻辑时钟仅在被投递事件上前进；开启后 Debug 按 filter
//! 正常投递；非 Debug 不受开关影响。
//! Property 5：unsubscribe 后该订阅不再接收，其余订阅与"从未存在被取消订阅"一致；未知 id 返回 false。
//!
//! Validates: Requirements 1.2, 2.2, 2.3, 2.4, 2.5, 2.6, 3.1, 3.2, 3.3, 3.4, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_observability::{
    EventBus, EventCategory, EventDraft, EventFilter, EventLevel, FaygerEvent, PhaseStatus,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

type Sink = Rc<RefCell<Vec<FaygerEvent>>>;

fn sink() -> Sink {
    Rc::new(RefCell::new(Vec::new()))
}

fn collector(s: &Sink) -> Box<dyn FnMut(&FaygerEvent)> {
    let s = Rc::clone(s);
    Box::new(move |e| s.borrow_mut().push(e.clone()))
}

fn arb_category() -> impl Strategy<Value = EventCategory> {
    prop_oneof![
        Just(EventCategory::Lifecycle),
        Just(EventCategory::Quota),
        Just(EventCategory::Loader),
        Just(EventCategory::Adapter),
        Just(EventCategory::Debug),
    ]
}

fn arb_level() -> impl Strategy<Value = EventLevel> {
    prop_oneof![
        Just(EventLevel::Debug),
        Just(EventLevel::Info),
        Just(EventLevel::Warn),
        Just(EventLevel::Error),
    ]
}

/// 生成任意一条草稿。Debug 类别强制 Debug 级（与运行层语义一致），其余类别用随机等级。
fn arb_draft() -> impl Strategy<Value = EventDraft> {
    (arb_category(), arb_level(), proptest::option::of(0u64..4)).prop_map(
        |(category, level, instance_id)| match category {
            EventCategory::Debug => EventDraft::debug("d", instance_id),
            EventCategory::Lifecycle => {
                let mut d = EventDraft::lifecycle("A", "B", instance_id.unwrap_or(0));
                d.level = level;
                d.instance_id = instance_id;
                d
            }
            EventCategory::Quota => {
                let mut d = EventDraft::quota(instance_id.unwrap_or(0), 1, 2, 3, false, None);
                d.level = level;
                d.instance_id = instance_id;
                d
            }
            EventCategory::Loader => {
                let mut d = EventDraft::loader("p", PhaseStatus::Enter, instance_id);
                d.level = level;
                d
            }
            EventCategory::Adapter => EventDraft::adapter("a", level, instance_id),
        },
    )
}

fn arb_filter() -> impl Strategy<Value = EventFilter> {
    let cats = proptest::option::of(
        proptest::collection::btree_set(arb_category(), 0..=5)
            .prop_map(|s| s.into_iter().collect::<BTreeSet<_>>()),
    );
    let lvl = proptest::option::of(arb_level());
    let inst = proptest::option::of(0u64..4);
    (cats, lvl, inst).prop_map(|(categories, min_level, instance_id)| EventFilter {
        categories,
        min_level,
        instance_id,
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 2 + 3：单订阅收到的恰为"被投递且通过 filter"的子序列；调试门控正确；时钟只在被投递事件上前进。
    #[test]
    fn subscriber_receives_exactly_delivered_and_matching(
        drafts in proptest::collection::vec(arb_draft(), 0..=20),
        filter in arb_filter(),
        debug_enabled in any::<bool>(),
    ) {
        let mut bus = EventBus::new();
        bus.set_debug_enabled(debug_enabled);
        let s = sink();
        bus.subscribe(filter.clone(), collector(&s));

        // 期望模型：逐条计算被投递事件（Debug 在关闭时丢弃且不递增时钟）。
        let mut expected_delivered: Vec<FaygerEvent> = Vec::new();
        let mut clock = 0u64;
        for d in &drafts {
            let gated = d.category == EventCategory::Debug && !debug_enabled;
            if gated {
                continue;
            }
            let event = FaygerEvent::from_draft(d.clone(), clock);
            clock += 1;
            if filter.matches(&event) {
                expected_delivered.push(event);
            }
        }

        // 实际发布。
        for d in drafts {
            bus.publish(d);
        }

        prop_assert_eq!(bus.clock(), clock);
        prop_assert_eq!(&*s.borrow(), &expected_delivered);
    }

    /// Property 3 补充：Debug 关闭时无 Debug 投递；非 Debug 不受开关影响（同一草稿集合在两种开关下，非 Debug 投递一致）。
    #[test]
    fn debug_toggle_only_affects_debug_events(
        drafts in proptest::collection::vec(arb_draft(), 0..=20),
    ) {
        // 收全部。
        let run = |debug_enabled: bool| -> Vec<FaygerEvent> {
            let mut bus = EventBus::new();
            bus.set_debug_enabled(debug_enabled);
            let s = sink();
            bus.subscribe(EventFilter::any(), collector(&s));
            for d in &drafts {
                bus.publish(d.clone());
            }
            let out = s.borrow().clone();
            out
        };
        let off = run(false);
        let on = run(true);

        // 关闭时不含任何 Debug 事件。
        prop_assert!(off.iter().all(|e| e.category != EventCategory::Debug));
        // 两种开关下，非 Debug 事件（按 payload 序列）一致。
        let off_non_debug: Vec<_> = off
            .iter()
            .filter(|e| e.category != EventCategory::Debug)
            .map(|e| e.payload.clone())
            .collect();
        let on_non_debug: Vec<_> = on
            .iter()
            .filter(|e| e.category != EventCategory::Debug)
            .map(|e| e.payload.clone())
            .collect();
        prop_assert_eq!(off_non_debug, on_non_debug);
    }

    /// Property 5：取消订阅后该订阅不再接收，其余订阅与基线一致；未知 id 返回 false。
    #[test]
    fn unsubscribe_isolates_without_disturbing_others(
        drafts in proptest::collection::vec(arb_draft(), 1..=20),
        cut in 0usize..20,
    ) {
        // 三个订阅，全用 any() 收全；在第 cut 条发布前取消 sub_b。
        let mut bus = EventBus::new();
        bus.set_debug_enabled(true); // 避免 Debug 门控干扰，聚焦取消订阅语义
        let sa = sink();
        let sb = sink();
        let sc = sink();
        bus.subscribe(EventFilter::any(), collector(&sa));
        let id_b = bus.subscribe(EventFilter::any(), collector(&sb));
        bus.subscribe(EventFilter::any(), collector(&sc));

        let cut = cut.min(drafts.len());
        // 基线：sb 应只收到前 cut 条；sa/sc 收全部。
        for (i, d) in drafts.iter().enumerate() {
            if i == cut {
                prop_assert!(bus.unsubscribe(id_b));
                // 再次取消应为 false，且不影响其余。
                prop_assert!(!bus.unsubscribe(id_b));
            }
            bus.publish(d.clone());
        }

        prop_assert_eq!(sa.borrow().len(), drafts.len());
        prop_assert_eq!(sc.borrow().len(), drafts.len());
        prop_assert_eq!(sb.borrow().len(), cut);
        // sa 与 sc 收到的事件序列一致（多订阅隔离 + 确定性顺序）。
        prop_assert_eq!(&*sa.borrow(), &*sc.borrow());
    }
}
