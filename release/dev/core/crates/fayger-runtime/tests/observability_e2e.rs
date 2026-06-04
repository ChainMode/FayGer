//! Feature: fayger-m8, 端到端：真实 Runtime 跑完整生命周期 + 一次超限暂停，订阅断言事件序列。
//!
//! 订阅全量事件，跑 load→init→start→observe_usage(超限)→resume→terminate；断言：发出的
//! Lifecycle 事件 from/to 序列与实际迁移一致、含一条 Running→Suspended、至少一条 Quota、
//! 时间戳单调非减。
//!
//! Validates: Requirements 8.2, 8.3, 8.5

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, ResourceQuota, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_observability::{EventCategory, EventFilter, EventPayload, FaygerEvent};
use fayger_runtime::{MockAdapter, ResourceUsage, Runtime, RuntimeInterface};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

fn object_with_quota(quota: ResourceQuota) -> BuFObject {
    let descriptors = vec![SectionDescriptor {
        id: "s0".to_owned(),
        kind: SectionKind::Code,
        digest: Digest::placeholder(),
        length: 1,
        visibility: SectionVisibility::Required,
        profile_constraints: ProfileConstraints::default(),
    }];
    BuFObject {
        manifest: BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: Some(quota),
            sections: descriptors,
            signature: None,
            extensions: BTreeMap::new(),
        },
        sections: vec![Section::new("s0", vec![0u8])],
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

#[test]
fn full_lifecycle_with_quota_suspend_emits_consistent_events() {
    let mut r = Runtime::new(MockAdapter::new());
    let id = r
        .load(object_with_quota(ResourceQuota {
            cpu_ms_per_sec: None,
            memory_bytes: Some(100),
            io_bytes_per_sec: None,
        }))
        .expect("load");

    // 订阅全量事件。
    let sink: Rc<RefCell<Vec<FaygerEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::clone(&sink);
    r.events_mut().subscribe(
        EventFilter::any(),
        Box::new(move |e| s.borrow_mut().push(e.clone())),
    );

    // 完整路径：init → start → 超限(暂停) → resume → terminate。
    r.init(id).expect("init");
    r.start(id).expect("start");
    r.observe_usage(id, ResourceUsage::new(0, 200, 0))
        .expect("observe"); // 超限 → Suspended
    r.resume(id).expect("resume");
    r.terminate(id).expect("terminate");

    let events = sink.borrow();

    // Lifecycle from/to 序列与实际迁移一致。
    let lifecycle: Vec<(String, String)> = events
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::Lifecycle { from, to } => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        lifecycle,
        vec![
            ("Loaded".to_owned(), "Initialized".to_owned()),
            ("Initialized".to_owned(), "Running".to_owned()),
            ("Running".to_owned(), "Suspended".to_owned()), // 超限暂停
            ("Suspended".to_owned(), "Running".to_owned()),
            ("Running".to_owned(), "Terminated".to_owned()),
        ]
    );

    // 至少一条 Quota 事件，且存在 suspended=true 的那条。
    let quota_events: Vec<&FaygerEvent> = events
        .iter()
        .filter(|e| e.category == EventCategory::Quota)
        .collect();
    assert!(
        !quota_events.is_empty(),
        "expected at least one Quota event"
    );
    assert!(quota_events.iter().any(|e| matches!(
        &e.payload,
        EventPayload::Quota {
            suspended: true,
            exceeded: Some(k),
            ..
        } if k == "Memory"
    )));

    // 时间戳单调非减。
    for w in events.windows(2) {
        assert!(w[0].timestamp <= w[1].timestamp);
    }

    // 在超限暂停处，Lifecycle(Running→Suspended) 紧邻其后的 Quota(suspended=true) 之前。
    let susp_pos = events
        .iter()
        .position(|e| matches!(&e.payload, EventPayload::Lifecycle { to, .. } if to == "Suspended"))
        .expect("has suspend lifecycle");
    let quota_susp_pos = events
        .iter()
        .position(|e| {
            matches!(
                &e.payload,
                EventPayload::Quota {
                    suspended: true,
                    ..
                }
            )
        })
        .expect("has suspended quota");
    assert!(
        susp_pos < quota_susp_pos,
        "Lifecycle should precede its Quota"
    );
}
