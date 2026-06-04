//! Feature: fayger-m7, 端到端：两实例不同配额，一超限暂停、一正常继续，互不影响。
//!
//! 加载实例 A（memory_bytes=100）与 B（无配额），均 init+start。对 A 注入 usage{memory=200}
//! → A 暂停且 quota_exceeded.resource=Memory；对 B 注入 usage{memory=999} → B 仍 Running。
//! 继续对 B step 派发指令，断言 A 状态不受 B 影响、B 指令序列与基线一致。
//!
//! Validates: Requirements 7.2, 7.3, 7.5

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
use fayger_runtime::{
    LifecycleState, MockAdapter, ResourceKind, ResourceUsage, Runtime, RuntimeInterface,
    UniversalInstruction,
};
use std::collections::{BTreeMap, BTreeSet};

fn object_with(n_sections: usize, quotas: Option<ResourceQuota>) -> BuFObject {
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
            quotas,
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

#[test]
fn two_instances_quota_isolation_e2e() {
    let mut r = Runtime::new(MockAdapter::new());

    let quota_a = ResourceQuota {
        cpu_ms_per_sec: None,
        memory_bytes: Some(100),
        io_bytes_per_sec: None,
    };
    let a = r.load(object_with(2, Some(quota_a))).expect("load a");
    let b = r.load(object_with(2, None)).expect("load b");

    for id in [a, b] {
        r.init(id).expect("init");
        r.start(id).expect("start");
    }

    // A 超限 → 暂停（Memory 维度）。
    r.observe_usage(a, ResourceUsage::new(0, 200, 0))
        .expect("observe a");
    assert_eq!(r.state(a).expect("state a"), LifecycleState::Suspended);
    let qe = r.quota_exceeded(a).expect("qe a").expect("some");
    assert_eq!(qe.resource, ResourceKind::Memory);
    assert_eq!(qe.usage, ResourceUsage::new(0, 200, 0));

    // B 无配额 → 即使用量更大也保持 Running。
    r.observe_usage(b, ResourceUsage::new(0, 999, 0))
        .expect("observe b");
    assert_eq!(r.state(b).expect("state b"), LifecycleState::Running);
    assert_eq!(r.quota_exceeded(b).expect("qe b"), None);

    // 继续对 B step，指令序列与基线一致；A 的暂停状态不受影响。
    let baseline = [
        UniversalInstruction {
            category: 0,
            opcode: 0,
            operands: 1u32.to_le_bytes().to_vec(),
        },
        UniversalInstruction {
            category: 0,
            opcode: 1,
            operands: 1u32.to_le_bytes().to_vec(),
        },
    ];
    assert!(r.step(b).expect("b step 1"));
    assert!(r.step(b).expect("b step 2"));
    assert!(!r.step(b).expect("b step 3")); // 程序耗尽

    let received = &r.adapter().received;
    assert_eq!(received.len(), 2);
    assert_eq!(received[0], baseline[0]);
    assert_eq!(received[1], baseline[1]);

    // A 始终保持 Suspended，未被 B 的活动影响。
    assert_eq!(r.state(a).expect("state a"), LifecycleState::Suspended);
    assert_eq!(
        r.quota_exceeded(a).expect("qe a").expect("some").resource,
        ResourceKind::Memory
    );
}
