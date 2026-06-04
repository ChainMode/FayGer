//! Feature: fayger-m5, Property 1: Eager / Lazy equivalence.
//!
//! 同一 BuF 分别以 Eager 与 Lazy 加载，对 Lazy force-all 后：manifest / raw_header /
//! raw_trailer 相等，且每个 Section 内容逐字节相等。
//!
//! Validates: Requirements 6.1, 6.2, 6.3, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::redundant_clone
)]

use fayger_buf::serialize;
use fayger_buf::testing::arb_buf_object;
use fayger_loader::{load_from_source, seal, LoadStrategy, MemorySource};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn sealed_bytes_strategy() -> impl Strategy<Value = Vec<u8>> {
    arb_buf_object().prop_map(|mut obj| {
        seal(&mut obj).expect("seal");
        serialize(&obj).expect("serialize")
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn eager_and_lazy_are_equivalent(bytes in sealed_bytes_strategy()) {
        let mut eager =
            load_from_source(Box::new(MemorySource::new(bytes.clone())), LoadStrategy::Eager)
                .expect("eager load");
        let mut lazy =
            load_from_source(Box::new(MemorySource::new(bytes.clone())), LoadStrategy::Lazy)
                .expect("lazy load");

        // 骨架等价
        prop_assert_eq!(&eager.manifest, &lazy.manifest);
        prop_assert_eq!(&eager.raw_header, &lazy.raw_header);
        prop_assert_eq!(&eager.raw_trailer, &lazy.raw_trailer);

        // 每段内容逐字节相等（force-all lazy）
        let ids = eager.section_ids();
        for id in ids {
            let e = eager.section(&id).expect("eager section").to_vec();
            let l = lazy.section(&id).expect("lazy section").to_vec();
            prop_assert_eq!(e, l, "section {} differs", id);
        }
    }
}
