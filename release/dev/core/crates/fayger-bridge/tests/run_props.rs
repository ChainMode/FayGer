//! Feature: fayger-m10, Property 2: 桥接运行在能力充分宿主上 syscall 数等于 section 数。
//!
//! 对随机 section 列表构造的 BuF：native-desktop 画像 + 全能力 + 规范映射下 run_bridged，
//! 最终 Terminated，dispatched == section 数，syscalls.len() == section 数，failures 0，
//! 每个 syscall 的 platform_id == "native-desktop"、args 等于段长小端。
//!
//! Validates: Requirements 2.2, 2.3, 2.4, 2.6

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
use fayger_adapter::{Capability, HostEnvironment, HostPolicy};
use fayger_bridge::{run_bridged, InstructionMapping};
use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_runtime::LifecycleState;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
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

fn object_with(payloads: &[Vec<u8>]) -> BuFObject {
    let descriptors = payloads
        .iter()
        .enumerate()
        .map(|(i, p)| SectionDescriptor {
            id: format!("s{i}"),
            kind: SectionKind::Code,
            digest: Digest::placeholder(),
            length: p.len() as u64,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let sections = payloads
        .iter()
        .enumerate()
        .map(|(i, p)| Section::new(format!("s{i}"), p.clone()))
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

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 能力充分宿主 + 规范映射：syscall 数等于 section 数，args 等于段长小端。
    #[test]
    fn bridged_run_syscalls_match_sections(
        payloads in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..=8),
            0..=6,
        ),
    ) {
        let obj = object_with(&payloads);
        let report = run_bridged(
            obj,
            &desktop_host(),
            &HostPolicy::allow_all(DESKTOP_CAPS),
            InstructionMapping::canonical_io(),
        )
        .expect("run_bridged");

        prop_assert_eq!(report.final_state, LifecycleState::Terminated);
        prop_assert_eq!(report.dispatched, payloads.len());
        prop_assert_eq!(report.syscalls.len(), payloads.len());
        prop_assert_eq!(report.failures, 0);
        if !payloads.is_empty() {
            prop_assert_eq!(&report.platform_id, "native-desktop");
        }
        // 每个 syscall 的 args 等于对应段长的小端 u32（运行层派生程序 operands = payload 长度）。
        for (sc, payload) in report.syscalls.iter().zip(payloads.iter()) {
            prop_assert_eq!(&sc.platform_id, "native-desktop");
            let expected = (payload.len() as u32).to_le_bytes().to_vec();
            prop_assert_eq!(&sc.args, &expected);
            prop_assert_eq!(&sc.name, "io.op0");
        }
    }
}
