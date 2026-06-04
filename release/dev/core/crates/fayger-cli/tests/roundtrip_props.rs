//! Feature: fayger-m9, Property 2 + 3 + 5: pack→load→run 往返 / inspect 一致 / 事件一致。
//!
//! Property 2：随机 (id,kind,payload) 列表（id 唯一）pack_bytes 成功，load 成功，run_object
//! 运行成功且 final_state == Terminated、dispatched == section 数。
//! Property 3：inspect_object 文本呈现的 section 数等于 Manifest section 数，每个 id 按序出现。
//! Property 5：run_object 的 lifecycle_events 等于实际迁移数（init/start/terminate = 3）。
//!
//! Validates: Requirements 3.5, 4.1-4.5, 5.2, 5.5, 8.1, 8.2

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_cli::{default_policy, inspect_object, pack_bytes, run_object};
use fayger_loader::load;
use fayger_runtime::LifecycleState;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn arb_kind() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("code".to_owned()),
        Just("data".to_owned()),
        Just("asset".to_owned()),
    ]
}

/// 随机 (kind, payload) 列表；id 由调用处按索引赋予以保证唯一。
fn arb_sections() -> impl Strategy<Value = Vec<(String, Vec<u8>)>> {
    proptest::collection::vec(
        (arb_kind(), proptest::collection::vec(any::<u8>(), 0..=12)),
        0..=6,
    )
}

/// 把 (kind, payload) 列表补上唯一 id，得到 (id, kind, payload)。
fn with_ids(kp: &[(String, Vec<u8>)]) -> Vec<(String, String, Vec<u8>)> {
    kp.iter()
        .enumerate()
        .map(|(i, (kind, payload))| (format!("s{i}"), kind.clone(), payload.clone()))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 2 + 5：pack → load → run 往返；Terminated；dispatched == section 数；
    /// lifecycle_events == 3（init/start/terminate）。
    #[test]
    fn pack_load_run_round_trip(kp in arb_sections(), entry in "[a-z]{1,8}") {
        let sections = with_ids(&kp);
        let bytes = pack_bytes(&entry, &sections).expect("pack");
        let obj = load(&bytes, &default_policy()).expect("load");
        prop_assert_eq!(obj.sections.len(), sections.len());

        let report = run_object(obj).expect("run");
        prop_assert_eq!(report.final_state, LifecycleState::Terminated);
        prop_assert_eq!(report.dispatched, sections.len());
        // init + start + terminate = 3 次迁移；step 不产生迁移。
        prop_assert_eq!(report.lifecycle_events, 3);
        prop_assert_eq!(report.quota_events, 0);
    }

    /// Property 3：inspect 文本与 Manifest 一致（section 数 + 每个 id 按序出现）。
    #[test]
    fn inspect_matches_manifest(kp in arb_sections(), entry in "[a-z]{1,8}") {
        let sections = with_ids(&kp);
        let bytes = pack_bytes(&entry, &sections).expect("pack");
        let obj = load(&bytes, &default_policy()).expect("load");

        let text = inspect_object(&obj);
        // section 数一致
        let count_line = format!("sections:              {}", sections.len());
        prop_assert!(text.contains(&count_line));
        // 每个 id 出现，且按序（用 find 的递增位置验证顺序）
        let mut last_pos = 0usize;
        for (id, _, _) in &sections {
            let pos = text.find(id).expect("id present in inspect text");
            prop_assert!(pos >= last_pos, "ids should appear in order");
            last_pos = pos;
        }
        // entry 也应出现
        prop_assert!(text.contains(&entry));
    }
}
