//! Feature: fayger-m8, Property 1: 生命周期事件序列一致性。
//!
//! 对随机命令序列驱动的 Runtime：订阅 Lifecycle 事件后收集到的事件序列与实际发生的成功迁移
//! 序列等长，逐项 from/to 对齐，且 timestamp 单调非减。
//!
//! Validates: Requirements 4.1, 4.2, 4.3, 4.4, 6.2, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_observability::{EventCategory, EventFilter, EventPayload, FaygerEvent};
use fayger_runtime::{InstanceId, MockAdapter, Runtime, RuntimeInterface};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// 被测命令（不含 load）。
#[derive(Debug, Clone, Copy)]
enum Cmd {
    Init,
    Start,
    Suspend,
    Resume,
    Terminate,
}

fn arb_cmd() -> impl Strategy<Value = Cmd> {
    prop_oneof![
        Just(Cmd::Init),
        Just(Cmd::Start),
        Just(Cmd::Suspend),
        Just(Cmd::Resume),
        Just(Cmd::Terminate),
    ]
}

fn object_with(n_sections: usize) -> BuFObject {
    let descriptors: Vec<SectionDescriptor> = (0..n_sections)
        .map(|i| SectionDescriptor {
            id: format!("s{i}"),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: 1,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let sections: Vec<Section> = (0..n_sections)
        .map(|i| Section::new(format!("s{i}"), vec![i as u8]))
        .collect();
    BuFObject {
        manifest: BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: descriptors,
            signature: None,
            extensions: BTreeMap::new(),
        },
        sections,
        raw_header: fayger_buf::format::Header {
            format_version: fayger_buf::format::FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: fayger_buf::format::Trailer::placeholder(0),
    }
}

fn apply(r: &mut Runtime<MockAdapter>, id: InstanceId, cmd: Cmd) -> Result<(), ()> {
    let res = match cmd {
        Cmd::Init => r.init(id),
        Cmd::Start => r.start(id),
        Cmd::Suspend => r.suspend(id),
        Cmd::Resume => r.resume(id),
        Cmd::Terminate => r.terminate(id),
    };
    res.map_err(|_| ())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 发出的 Lifecycle 事件序列与实际成功迁移序列等长、逐项 from/to 对齐、时间戳单调非减。
    #[test]
    fn lifecycle_events_track_successful_transitions(
        n_sections in 0usize..=4,
        cmds in prop::collection::vec(arb_cmd(), 0..=24),
    ) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(object_with(n_sections)).expect("load");

        // 订阅 Lifecycle 事件。
        let sink: Rc<RefCell<Vec<FaygerEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let s = Rc::clone(&sink);
        r.events_mut().subscribe(
            EventFilter::any().with_categories([EventCategory::Lifecycle]),
            Box::new(move |e| s.borrow_mut().push(e.clone())),
        );

        // 期望迁移序列：仅在命令成功时记录 (from, to)。
        let mut expected: Vec<(String, String)> = Vec::new();
        for cmd in cmds {
            let before = r.state(id).expect("state");
            if apply(&mut r, id, cmd).is_ok() {
                let after = r.state(id).expect("state");
                expected.push((format!("{before:?}"), format!("{after:?}")));
            }
        }

        let events = sink.borrow();
        // 等长
        prop_assert_eq!(events.len(), expected.len());
        // 逐项 from/to 对齐
        for (e, (from, to)) in events.iter().zip(expected.iter()) {
            match &e.payload {
                EventPayload::Lifecycle { from: ef, to: et } => {
                    prop_assert_eq!(ef, from);
                    prop_assert_eq!(et, to);
                }
                other => prop_assert!(false, "expected Lifecycle payload, got {:?}", other),
            }
        }
        // 时间戳单调非减
        for w in events.windows(2) {
            prop_assert!(w[0].timestamp <= w[1].timestamp);
        }
    }
}
