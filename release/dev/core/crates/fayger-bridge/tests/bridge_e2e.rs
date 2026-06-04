//! Feature: fayger-m10, 端到端：加载链产物经 run_bridged 在桌面画像上运行 + 跨层错误链。
//!
//! 用 fayger-loader 的 seal + serialize + load 产出一个可信 BuF，在 native-desktop 画像上
//! run_bridged，断言 syscall 序列与 section 一致；再构造一次受限派发并 wrap_dispatch_failure，
//! 断言 [Runtime, Adapter] 跨层错误链。
//!
//! Validates: Requirements 7.2

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation
)]

use fayger_adapter::builtin::{
    CAP_CRYPTO_HASH, CAP_FS_READ, CAP_FS_WRITE, CAP_HOST_CALL, CAP_NET_HTTP, CAP_PROC_SPAWN,
    CAP_RANDOM_BYTES, CAP_TIME_NOW, CAP_UI_DOM,
};
use fayger_adapter::{AdapterRegistry, BrowserAdapter, Capability, HostEnvironment, HostPolicy};
use fayger_bridge::{dispatch_one, run_bridged, wrap_dispatch_failure, InstructionMapping};
use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_error::{ErrorCode, Layer};
use fayger_loader::{load, seal, LoadPolicy};
use fayger_runtime::{LifecycleState, UniversalInstruction as RtInstruction};
use std::collections::{BTreeMap, BTreeSet};

const DESKTOP_CAPS: [&str; 9] = [
    CAP_FS_READ,
    CAP_FS_WRITE,
    CAP_NET_HTTP,
    CAP_UI_DOM,
    CAP_TIME_NOW,
    CAP_RANDOM_BYTES,
    CAP_CRYPTO_HASH,
    CAP_PROC_SPAWN,
    CAP_HOST_CALL,
];

fn desktop_host() -> HostEnvironment {
    let caps: BTreeSet<Capability> = DESKTOP_CAPS.into_iter().map(Capability::new).collect();
    HostEnvironment::new("native-desktop", "linux", "x86_64", "none", caps)
}

fn sealed_object(payloads: &[(&str, Vec<u8>)]) -> BuFObject {
    let descriptors = payloads
        .iter()
        .map(|(id, p)| SectionDescriptor {
            id: (*id).to_owned(),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: p.len() as u64,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let sections = payloads
        .iter()
        .map(|(id, p)| Section::new(*id, p.clone()))
        .collect();
    let mut obj = BuFObject {
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
    };
    seal(&mut obj).expect("seal");
    obj
}

fn policy() -> LoadPolicy {
    LoadPolicy::new(
        (
            SemVer::parse("1.0.0").expect("ok"),
            SemVer::parse("1.9.9").expect("ok"),
        ),
        BTreeSet::new(),
        SemVer::parse("1.0.0").expect("ok"),
    )
}

#[test]
fn load_chain_to_bridged_run_on_desktop() {
    // 经完整加载链产出可信 BuF。
    let obj = sealed_object(&[("code", vec![1, 2, 3]), ("data", vec![9, 9])]);
    let bytes = fayger_buf::serialize(&obj).expect("serialize");
    let loaded = load(&bytes, &policy()).expect("load");

    // 桥接运行。
    let report = run_bridged(
        loaded,
        &desktop_host(),
        &HostPolicy::allow_all(DESKTOP_CAPS),
        InstructionMapping::canonical_io(),
    )
    .expect("run_bridged");

    assert_eq!(report.final_state, LifecycleState::Terminated);
    assert_eq!(report.dispatched, 2);
    assert_eq!(report.syscalls.len(), 2);
    assert_eq!(report.failures, 0);
    assert_eq!(report.platform_id, "native-desktop");
    // 段长小端：code 长 3、data 长 2。
    assert_eq!(report.syscalls[0].args, 3u32.to_le_bytes().to_vec());
    assert_eq!(report.syscalls[1].args, 2u32.to_le_bytes().to_vec());
}

#[test]
fn restricted_dispatch_yields_cross_layer_chain() {
    // Browser 画像（无 proc.spawn）；映射要求 proc.spawn → 派发被能力拒。
    let mut registry = AdapterRegistry::new();
    registry.register(Box::new(BrowserAdapter::new()));
    let host = HostEnvironment::new("browser", "browser", "wasm32", "browser", BTreeSet::new());
    registry
        .select(&host, &HostPolicy::default())
        .expect("select");

    let mapping = InstructionMapping::canonical_io().requiring(["proc.spawn"]);
    let adp_err = dispatch_one(&registry, &mapping, &RtInstruction::new(0, 0, vec![1]))
        .expect_err("should be denied");
    assert_eq!(adp_err.code, ErrorCode::AdpCapabilityDenied);

    // 桥接为跨层错误链。
    let wrapped = wrap_dispatch_failure(adp_err);
    assert_eq!(wrapped.code, ErrorCode::RtInstanceFailed);
    assert_eq!(wrapped.layer_chain(), [Layer::Runtime, Layer::Adapter]);
    assert_eq!(wrapped.root_cause().code, ErrorCode::AdpCapabilityDenied);
}
