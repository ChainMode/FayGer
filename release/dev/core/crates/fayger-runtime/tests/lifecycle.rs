//! Feature: fayger-m2, Property 1: lifecycle legal transitions & illegal rejection.
//!
//! 对初始 `Loaded` 实例与任意命令序列：合法命令使状态按迁移表前进；非法命令返回
//! `RT_ILLEGAL_TRANSITION`、状态不变、context 含 current_state/allowed_commands；
//! 状态始终落在 6 个合法状态内；终态后任何命令都被拒绝。
//!
//! Validates: Requirements 5.1-5.8, 9.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_buf::manifest::{
    types::{ProfileConstraints, SectionVisibility},
    BuFManifest, EntryPoint, SemVer,
};
use fayger_buf::object::BuFObject;
use fayger_error::ErrorCode;
use fayger_runtime::{
    Adapter, InstanceId, LifecycleCommand, LifecycleState, MockAdapter, Runtime, RuntimeInterface,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::{BTreeMap, BTreeSet};

fn empty_object() -> BuFObject {
    let _ = (ProfileConstraints::default(), SectionVisibility::Required);
    BuFObject {
        manifest: BuFManifest {
            schema_version: SemVer::parse("1.0.0").expect("ok"),
            runtime_interface_min: SemVer::parse("1.0.0").expect("ok"),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime: None,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![],
            signature: None,
            extensions: BTreeMap::new(),
        },
        sections: vec![],
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

fn arb_command() -> impl Strategy<Value = LifecycleCommand> {
    prop_oneof![
        Just(LifecycleCommand::Init),
        Just(LifecycleCommand::Start),
        Just(LifecycleCommand::Suspend),
        Just(LifecycleCommand::Resume),
        Just(LifecycleCommand::Terminate),
    ]
}

fn apply_cmd<A: Adapter>(
    r: &mut Runtime<A>,
    id: InstanceId,
    cmd: LifecycleCommand,
) -> Result<(), fayger_error::FaygerError> {
    match cmd {
        LifecycleCommand::Init => r.init(id),
        LifecycleCommand::Start => r.start(id),
        LifecycleCommand::Suspend => r.suspend(id),
        LifecycleCommand::Resume => r.resume(id),
        LifecycleCommand::Terminate => r.terminate(id),
    }
}

const ALL_STATES: [LifecycleState; 6] = [
    LifecycleState::Loaded,
    LifecycleState::Initialized,
    LifecycleState::Running,
    LifecycleState::Suspended,
    LifecycleState::Terminated,
    LifecycleState::Failed,
];

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 对任意命令序列，运行层的状态演化与纯状态机一致；非法命令保持状态不变。
    #[test]
    fn runtime_matches_pure_state_machine(cmds in prop::collection::vec(arb_command(), 0..=16)) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(empty_object()).expect("load");

        // 纯状态机的并行模型（不含 Failed，因为命令序列不触发 fault）。
        let mut model = LifecycleState::Loaded;

        for cmd in cmds {
            let before = r.state(id).expect("state");
            prop_assert_eq!(before, model);

            let result = apply_cmd(&mut r, id, cmd);
            let after = r.state(id).expect("state");

            if let Some(next) = model.apply(cmd) {
                // 合法：运行层应成功且状态前进。
                prop_assert!(result.is_ok(), "expected legal {:?} from {:?}", cmd, before);
                prop_assert_eq!(after, next);
                model = next;
            } else {
                // 非法：运行层应返回 RT_ILLEGAL_TRANSITION 且状态不变。
                let err = result.expect_err("expected illegal transition error");
                prop_assert_eq!(err.code, ErrorCode::RtIllegalTransition);
                prop_assert!(err.context.contains_key("current_state"));
                prop_assert!(err.context.contains_key("allowed_commands"));
                prop_assert_eq!(after, before);
            }
            // 状态恒落在合法集合内。
            prop_assert!(ALL_STATES.contains(&after));
        }
    }

    /// 一旦到达终态（Terminated），后续任何命令都被拒绝且状态不变。
    #[test]
    fn terminal_state_absorbs_all_commands(tail in prop::collection::vec(arb_command(), 0..=8)) {
        let mut r = Runtime::new(MockAdapter::new());
        let id = r.load(empty_object()).expect("load");
        r.terminate(id).expect("terminate from Loaded is legal");
        prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Terminated);

        for cmd in tail {
            let err = apply_cmd(&mut r, id, cmd).expect_err("terminal must reject");
            prop_assert_eq!(err.code, ErrorCode::RtIllegalTransition);
            prop_assert_eq!(r.state(id).expect("state"), LifecycleState::Terminated);
        }
    }
}
