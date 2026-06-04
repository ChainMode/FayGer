//! Feature: fayger-m7, Property 1 + 5: 运行时数据区隔离 / 故障隔离。
//!
//! Property 1：对实例 A 的任意操作（step / inject_fault / observe_usage / 生命周期迁移）
//! 不改变并存实例 B 的可观测状态（state / last_instruction 存在性 / failure 存在性 /
//! last_usage / quota_exceeded）。
//!
//! Property 5：N 个 Running 实例中令某一个进入 Failed（或因超限 Suspended），在不再对其它
//! 实例注入额外故障/超限的前提下，其它实例仍保持 Running，且继续 step 派发的指令序列与无故障
//! 基线一致。
//!
//! Validates: Requirements 1.1, 1.2, 1.3, 5.1, 5.2, 5.3, 5.4, 7.1, 7.4

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
    InstanceId, LifecycleState, MockAdapter, Runtime, RuntimeInterface, UniversalInstruction,
};
use fayger_runtime::{QuotaExceeded, ResourceUsage};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
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

/// 实例 B 的可观测快照（用于隔离断言）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    state: LifecycleState,
    has_failure: bool,
    last_usage: Option<ResourceUsage>,
    quota_exceeded: Option<QuotaExceeded>,
}

fn snapshot<A: fayger_runtime::Adapter>(r: &Runtime<A>, id: InstanceId) -> Snapshot {
    Snapshot {
        state: r.state(id).expect("state"),
        has_failure: r.failure_info(id).expect("fi").is_some(),
        last_usage: r.usage(id).expect("usage"),
        quota_exceeded: r.quota_exceeded(id).expect("qe"),
    }
}

/// 对实例 A 施加的一种操作。
#[derive(Debug, Clone)]
enum Op {
    Init,
    Start,
    Suspend,
    Resume,
    Terminate,
    Step,
    Fault,
    Observe(ResourceUsage),
}

fn arb_usage() -> impl Strategy<Value = ResourceUsage> {
    (0u64..=300, 0u64..=300, 0u64..=300).prop_map(|(c, m, i)| ResourceUsage::new(c, m, i))
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        Just(Op::Init),
        Just(Op::Start),
        Just(Op::Suspend),
        Just(Op::Resume),
        Just(Op::Terminate),
        Just(Op::Step),
        Just(Op::Fault),
        arb_usage().prop_map(Op::Observe),
    ]
}

fn apply<A: fayger_runtime::Adapter>(r: &mut Runtime<A>, id: InstanceId, op: &Op) {
    // 所有操作均可能因状态非法而返回 Err；隔离性不关心 A 自己的结果，只关心不触及 B。
    match op {
        Op::Init => {
            let _ = r.init(id);
        }
        Op::Start => {
            let _ = r.start(id);
        }
        Op::Suspend => {
            let _ = r.suspend(id);
        }
        Op::Resume => {
            let _ = r.resume(id);
        }
        Op::Terminate => {
            let _ = r.terminate(id);
        }
        Op::Step => {
            let _ = r.step(id);
        }
        Op::Fault => {
            let _ = r.inject_fault(id, "boom");
        }
        Op::Observe(u) => {
            let _ = r.observe_usage(id, *u);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 1：对 A 的任意操作序列不改变 B 的可观测状态。
    #[test]
    fn ops_on_a_do_not_affect_b(
        a_sections in 0usize..=5,
        b_sections in 0usize..=5,
        b_quota_mem in proptest::option::of(1u64..=100),
        ops in proptest::collection::vec(arb_op(), 0..=12),
        b_advance in 0usize..=3,
    ) {
        let mut r = Runtime::new(MockAdapter::new());
        let b_quota = b_quota_mem.map(|m| ResourceQuota {
            cpu_ms_per_sec: None,
            memory_bytes: Some(m),
            io_bytes_per_sec: None,
        });
        let a = r.load(object_with(a_sections, None)).expect("load a");
        let b = r.load(object_with(b_sections, b_quota)).expect("load b");

        // 把 B 推进到一个稳定的中间状态（可选地 init/start 若干步）。
        if b_advance >= 1 {
            r.init(b).expect("b init");
        }
        if b_advance >= 2 {
            r.start(b).expect("b start");
        }
        if b_advance >= 3 {
            let _ = r.step(b);
        }

        let before = snapshot(&r, b);

        for op in &ops {
            apply(&mut r, a, op);
        }

        let after = snapshot(&r, b);
        prop_assert_eq!(before, after);
    }

    /// Property 5：一个实例 Failed/Suspended 不影响其它 Running 实例的状态与 step 指令序列。
    #[test]
    fn one_failing_instance_does_not_disturb_others(
        n in 2usize..=5,
        victim in 0usize..5,
        sections in 1usize..=4,
        suspend_instead in any::<bool>(),
    ) {
        let n = n.max(2);
        let victim = victim % n;

        let mut r = Runtime::new(MockAdapter::new());
        let mut ids = Vec::new();
        for _ in 0..n {
            // 给每个实例一个会被超限的小配额（用于 suspend_instead 分支）。
            let q = ResourceQuota {
                cpu_ms_per_sec: None,
                memory_bytes: Some(10),
                io_bytes_per_sec: None,
            };
            let id = r.load(object_with(sections, Some(q))).expect("load");
            r.init(id).expect("init");
            r.start(id).expect("start");
            ids.push(id);
        }

        // 基线：在无故障情况下，每个非 victim 实例 step 一次派发的指令。
        // 由于隔离 + 确定性 derive_program，每个实例的 program[0] 都是 opcode 0、operands = 1u32。
        let baseline = UniversalInstruction {
            category: 0,
            opcode: 0,
            operands: 1u32.to_le_bytes().to_vec(),
        };

        // 让 victim 出事。
        let v = ids[victim];
        if suspend_instead {
            r.observe_usage(v, ResourceUsage::new(0, 999, 0)).expect("observe");
            prop_assert_eq!(r.state(v).expect("state"), LifecycleState::Suspended);
        } else {
            r.inject_fault(v, "boom").expect("fault");
            prop_assert_eq!(r.state(v).expect("state"), LifecycleState::Failed);
        }

        // 其它实例仍 Running，且 step 派发与基线一致。
        for (i, &id) in ids.iter().enumerate() {
            if i == victim {
                continue;
            }
            prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Running);
            let before_len = r.adapter().received.len();
            let dispatched = r.step(id).expect("step");
            prop_assert!(dispatched);
            let received = &r.adapter().received;
            prop_assert_eq!(received.len(), before_len + 1);
            prop_assert_eq!(received.last().expect("last"), &baseline);
        }
    }
}
