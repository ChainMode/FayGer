// Feature: fayger-m0
// Property 4: missing required Manifest fields are rejected by serialize
//
// 蓝图 Properties §"必填字段缺失被拒" 与 M0 需求文档 Requirement 4.4 / 7.1。

#![allow(missing_docs, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_buf::manifest::EntryPoint;
use fayger_buf::serialize;
use fayger_buf::testing::arb_buf_object;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("regressions"),
        )),
        ..ProptestConfig::default()
    })]

    /// 把 entry 改为空字符串后 serialize 必须失败，error.context.missing_fields
    /// 必须包含 "entry"。
    ///
    /// 注：M0 的「必填」语义是字段非默认/非空，由 `BuFManifest::validate` 检查。
    /// 我们没办法用 prop_map 把字段「从结构上删掉」，所以选择把字段值置为空。
    #[test]
    fn empty_entry_is_rejected(obj in arb_buf_object()) {
        let mut o = obj;
        o.manifest.entry = EntryPoint::new("");
        let err = serialize(&o).expect_err("missing entry must fail");
        prop_assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        let mf = err.context.get("missing_fields");
        prop_assert!(matches!(
            mf,
            Some(fayger_error::ContextValue::List(items)) if items.iter().any(
                |v| matches!(v, fayger_error::ContextValue::String(s) if s == "entry")
            )
        ));
    }

    /// 将其中一个 section descriptor 的 id 置空 -> SectionInconsistency
    #[test]
    fn empty_section_id_is_rejected(obj in arb_buf_object()) {
        let mut o = obj;
        if o.manifest.sections.is_empty() {
            return Ok(()); // 无 section 时跳过
        }
        o.manifest.sections[0].id = String::new();
        let err = serialize(&o).expect_err("empty section id must fail");
        prop_assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        let reason = err.context.get("reason");
        prop_assert!(matches!(
            reason,
            Some(fayger_error::ContextValue::String(s)) if s == "SectionInconsistency"
        ));
    }

    /// sections 数量与 manifest.sections 不一致 -> SectionInconsistency
    #[test]
    fn payload_count_mismatch_is_rejected(obj in arb_buf_object()) {
        let mut o = obj;
        if o.sections.is_empty() {
            return Ok(());
        }
        o.sections.pop();
        let err = serialize(&o).expect_err("payload count mismatch must fail");
        prop_assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
        let reason = err.context.get("reason");
        prop_assert!(matches!(
            reason,
            Some(fayger_error::ContextValue::String(s)) if s == "SectionInconsistency"
        ));
    }

    /// payload 与 descriptor.length 不一致 -> SectionInconsistency
    #[test]
    fn payload_length_mismatch_is_rejected(obj in arb_buf_object()) {
        let mut o = obj;
        if o.sections.is_empty() {
            return Ok(());
        }
        o.sections[0].payload.push(42); // 长度多 1 字节
        let err = serialize(&o).expect_err("length mismatch must fail");
        prop_assert_eq!(err.code, fayger_error::ErrorCode::LdrMissingRequiredField);
    }

    /// 兜底类型断言：合法 BuFObject 始终能 serialize（serialize 不会
    /// 在「合法输入」上误报）。
    #[test]
    fn legitimate_object_always_serializes(obj in arb_buf_object()) {
        let _ = serialize(&obj).expect("legitimate object should serialize");
    }
}
