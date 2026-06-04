//! M2 端到端：从（经 M1 封装并加载的）BuF 走完完整生命周期，断言指令派发序列。
//!
//! 路径：seal + serialize（M1）→ load（M1 加载链）→ Runtime::load（M2）→
//! Init → Start → step×N → Suspend → step（不派发）→ Resume → step → Terminate。
//!
//! Validates: Requirements 9.2, 9.5

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_buf::manifest::{
    types::{Digest, ProfileConstraints, SectionKind, SectionVisibility},
    BuFManifest, EntryPoint, SectionDescriptor, SemVer,
};
use fayger_buf::object::{BuFObject, Section};
use fayger_buf::serialize;
use fayger_loader::{load, seal, LoadPolicy};
use fayger_runtime::{LifecycleState, MockAdapter, Runtime, RuntimeInterface};
use std::collections::{BTreeMap, BTreeSet};

fn v(s: &str) -> SemVer {
    SemVer::parse(s).expect("parse")
}

fn descriptor(id: &str, kind: SectionKind, length: u64) -> SectionDescriptor {
    SectionDescriptor {
        id: id.into(),
        kind,
        digest: Digest::placeholder(),
        length,
        visibility: SectionVisibility::Required,
        profile_constraints: ProfileConstraints::default(),
    }
}

fn source_object() -> BuFObject {
    let manifest = BuFManifest {
        schema_version: v("1.0.0"),
        runtime_interface_min: v("1.0.0"),
        deprecation_notice: None,
        entry: EntryPoint::new("main"),
        runtime: None,
        capabilities: BTreeSet::new(),
        quotas: None,
        sections: vec![
            descriptor("code", SectionKind::Code, 4),
            descriptor("asset", SectionKind::Asset, 3),
        ],
        signature: None,
        extensions: BTreeMap::new(),
    };
    BuFObject {
        manifest,
        sections: vec![
            Section::new("code", vec![0xde, 0xad, 0xbe, 0xef]),
            Section::new("asset", vec![0x01, 0x02, 0x03]),
        ],
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

fn policy() -> LoadPolicy {
    LoadPolicy::new((v("1.0.0"), v("1.9.9")), BTreeSet::new(), v("1.0.0"))
}

#[test]
fn full_lifecycle_with_loader_and_runtime() {
    // M1：封装 → 序列化 → 加载链。
    let mut src = source_object();
    seal(&mut src).expect("seal");
    let bytes = serialize(&src).expect("serialize");
    let loaded = load(&bytes, &policy()).expect("M1 load chain");
    assert_eq!(loaded.sections.len(), 2);

    // M2：交给运行层。
    let mut rt = Runtime::new(MockAdapter::new());
    let id = rt.load(loaded).expect("runtime load");
    assert_eq!(rt.state(id).unwrap(), LifecycleState::Loaded);

    // Init → Initialized
    rt.init(id).expect("init");
    assert_eq!(rt.state(id).unwrap(), LifecycleState::Initialized);

    // Start → Running
    rt.start(id).expect("start");
    assert_eq!(rt.state(id).unwrap(), LifecycleState::Running);

    // step：派发第一条（code 段，payload 长度 4）
    assert!(rt.step(id).expect("step"));
    assert_eq!(rt.adapter().received.len(), 1);

    // Suspend → Suspended；此时 step 不派发
    rt.suspend(id).expect("suspend");
    assert_eq!(rt.state(id).unwrap(), LifecycleState::Suspended);
    assert!(!rt.step(id).expect("step while suspended"));
    assert_eq!(rt.adapter().received.len(), 1);

    // Resume → Running；step 派发第二条（asset 段，payload 长度 3）
    rt.resume(id).expect("resume");
    assert!(rt.step(id).expect("step"));
    assert_eq!(rt.adapter().received.len(), 2);

    // 程序耗尽
    assert!(!rt.step(id).expect("step"));

    // Terminate → Terminated
    rt.terminate(id).expect("terminate");
    assert_eq!(rt.state(id).unwrap(), LifecycleState::Terminated);

    // 断言派发序列：两条指令 opcode 0/1，operands 为各段 payload 长度（小端 u32）
    let received = &rt.adapter().received;
    assert_eq!(received[0].opcode, 0);
    assert_eq!(received[0].operands, 4u32.to_le_bytes().to_vec());
    assert_eq!(received[1].opcode, 1);
    assert_eq!(received[1].operands, 3u32.to_le_bytes().to_vec());
}
