//! Feature: fayger-m8, Property 4: 跨层错误因果链顺序保持。
//!
//! 对任意由 n 个 (code, layer) 逐层 wrap 而成的错误链：depth() == n；layer_chain() 等于由
//! 外到内的 source_layer 序列；root_cause() 为最内层；且每层被包装错误的 code/source_layer/
//! context 未被改写。
//!
//! Validates: Requirements 5.5, 8.1, 8.4

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn arb_layer() -> impl Strategy<Value = Layer> {
    prop_oneof![
        Just(Layer::Loader),
        Just(Layer::Runtime),
        Just(Layer::Adapter),
        Just(Layer::Host),
    ]
}

fn arb_code() -> impl Strategy<Value = ErrorCode> {
    prop_oneof![
        Just(ErrorCode::LdrParseFail),
        Just(ErrorCode::LdrDigestMismatch),
        Just(ErrorCode::RtIllegalTransition),
        Just(ErrorCode::RtInstanceFailed),
        Just(ErrorCode::AdpUnsupportedInstruction),
        Just(ErrorCode::AdpHostCallFailed),
        Just(ErrorCode::RtQuotaExceeded),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 逐层 wrap：depth/layer_chain/root_cause 正确，且被包装层字段未被改写。
    #[test]
    fn wrap_preserves_chain_order_and_inner_fields(
        // layers[0] 是最内层（root），最后一个是最外层（top）。
        layers in proptest::collection::vec((arb_code(), arb_layer()), 1..=8),
    ) {
        // 自内向外构造：先建最内层，再逐层 wrap。
        let (root_code, root_layer) = layers[0];
        let mut err = FaygerError::new(root_code, root_layer, "layer-0")
            .with_context("idx", ContextValue::U64(0));
        for (i, &(code, layer)) in layers.iter().enumerate().skip(1) {
            err = FaygerError::wrap(code, layer, format!("layer-{i}"), err)
                .with_context("idx", ContextValue::U64(i as u64));
        }

        let n = layers.len();
        // depth == n
        prop_assert_eq!(err.depth(), n);

        // layer_chain 由外到内 == layers 逆序的 layer
        let expected_layers: Vec<Layer> = layers.iter().rev().map(|&(_, l)| l).collect();
        prop_assert_eq!(err.layer_chain(), expected_layers);

        // chain 由外到内的 code 序列 == layers 逆序的 code
        let chain_codes: Vec<ErrorCode> = err.chain().map(|e| e.code).collect();
        let expected_codes: Vec<ErrorCode> = layers.iter().rev().map(|&(c, _)| c).collect();
        prop_assert_eq!(chain_codes, expected_codes);

        // root_cause 为最内层（layers[0]）
        prop_assert_eq!(err.root_cause().code, root_code);
        prop_assert_eq!(err.root_cause().source_layer, root_layer);

        // 每层 context 未被改写：第 k 个 chain 元素（由外到内）对应 layers[n-1-k]，其 idx == n-1-k。
        for (k, e) in err.chain().enumerate() {
            let original_idx = (n - 1 - k) as u64;
            match e.context.get("idx") {
                Some(ContextValue::U64(v)) => prop_assert_eq!(*v, original_idx),
                other => prop_assert!(false, "missing/altered idx context: {:?}", other),
            }
        }
    }

    /// 单元素链：depth==1，root 即自身，layer_chain 单元素。
    #[test]
    fn single_error_chain(code in arb_code(), layer in arb_layer()) {
        let err = FaygerError::new(code, layer, "solo");
        prop_assert_eq!(err.depth(), 1);
        prop_assert_eq!(err.layer_chain(), vec![layer]);
        prop_assert_eq!(err.root_cause().code, code);
    }
}
