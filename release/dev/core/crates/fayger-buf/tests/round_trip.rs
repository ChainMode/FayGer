// Feature: fayger-m0
// Property 1: round-trip equivalence (structural)
// Property 2: round-trip equivalence (byte-level under canonical CBOR)
//
// 蓝图 Properties §"Correctness Properties" 与 M0 需求文档 Requirement 5。

#![allow(missing_docs, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fayger_buf::{parse, serialize, testing::arb_buf_object};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        // 反例归档落到 crate 根的 proptest-regressions/round_trip.txt
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("regressions"),
        )),
        ..ProptestConfig::default()
    })]

    /// Property 1: 对任意合法 BuFObject `o`，serialize(o) 之后再 parse 得到 `o'`，
    /// 应有 `o ≡ o'`（结构等价：manifest 字段集合相等且 sections 内容逐字节相等）。
    ///
    /// raw_header / raw_trailer 中的 offset / length 不参与等价比较（serialize
    /// 会按实际写入字节重算这些字段）。
    #[test]
    fn round_trip_structural(obj in arb_buf_object()) {
        let bytes = serialize(&obj).expect("serialize ok");
        let parsed = parse(&bytes).expect("parse ok");
        prop_assert_eq!(&parsed.manifest, &obj.manifest);
        prop_assert_eq!(&parsed.sections, &obj.sections);
    }

    /// Property 2: 对 parse(bytes) 的结果再次 serialize，输出的字节流必须与
    /// 输入完全相等（依赖于 canonical CBOR 的确定性编码）。
    #[test]
    fn round_trip_byte_level(obj in arb_buf_object()) {
        let bytes_1 = serialize(&obj).expect("serialize 1");
        let parsed = parse(&bytes_1).expect("parse ok");
        let bytes_2 = serialize(&parsed).expect("serialize 2");
        prop_assert_eq!(bytes_1, bytes_2);
    }

    /// 编码确定性：同一对象多次 serialize 字节相等。
    #[test]
    fn serialize_is_deterministic(obj in arb_buf_object()) {
        let a = serialize(&obj).expect("serialize 1");
        let b = serialize(&obj).expect("serialize 2");
        prop_assert_eq!(a, b);
    }
}
