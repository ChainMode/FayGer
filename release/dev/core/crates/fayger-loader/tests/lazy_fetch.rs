//! Feature: fayger-m5, Property 2: lazy first-access verification & caching.
//!
//! Lazy 模式首次 section() 触发对 source 恰好一次 read_at（含 digest 校验）；再次 section()
//! 命中缓存不再 read_at；内容与 Eager 一致。
//!
//! Validates: Requirements 3.3, 3.4, 8.1

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
use fayger_loader::{load_from_source, seal, BuFSource, LoadStrategy, MemorySource, SourceError};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::cell::Cell;
use std::rc::Rc;

/// 计数型 source：包裹 MemorySource，通过共享 Rc<Cell> 记录 read_at 调用次数，
/// 使测试在 source 移入 artifact 后仍能读取计数。
struct CountingSource {
    inner: MemorySource,
    reads: Rc<Cell<usize>>,
}

impl CountingSource {
    fn new(bytes: Vec<u8>, reads: Rc<Cell<usize>>) -> Self {
        Self {
            inner: MemorySource::new(bytes),
            reads,
        }
    }
}

impl BuFSource for CountingSource {
    fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read_at(offset, length)
    }
    fn length(&self) -> Result<u64, SourceError> {
        self.inner.length()
    }
}

fn sealed_with_sections() -> impl Strategy<Value = Vec<u8>> {
    // 至少 1 段，便于验证 fetch；用 prop_filter 保证非空。
    arb_buf_object()
        .prop_filter("non-empty sections", |o| !o.sections.is_empty())
        .prop_map(|mut obj| {
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

    /// 首访触发读取并缓存；再访命中缓存（不增加 read_at 次数）。
    #[test]
    fn first_access_reads_then_caches(bytes in sealed_with_sections()) {
        // Eager 基线内容
        let mut eager =
            load_from_source(Box::new(MemorySource::new(bytes.clone())), LoadStrategy::Eager)
                .expect("eager");

        // 由于 CountingSource 持有 Cell，我们用裸指针读取计数前先把 source 移交 artifact。
        // 这里改用一个外部计数：把 CountingSource 的 reads 暴露为 Rc 不便；
        // 改为：构造时记录骨架读取次数基线，再逐段 fetch 验证增量。
        // 简化：用 MemorySource 跑 Lazy，断言内容等价（计数语义由单元测试覆盖）。
        let mut lazy =
            load_from_source(Box::new(MemorySource::new(bytes.clone())), LoadStrategy::Lazy)
                .expect("lazy");

        let ids = lazy.section_ids();
        for id in &ids {
            // 首次访问前未加载
            prop_assert!(!lazy.loader().expect("loader").is_loaded(id));
            let first = lazy.section(id).expect("first").to_vec();
            // 访问后已加载
            prop_assert!(lazy.loader().expect("loader").is_loaded(id));
            // 再次访问内容一致
            let second = lazy.section(id).expect("second").to_vec();
            prop_assert_eq!(&first, &second);
            // 与 eager 一致
            let e = eager.section(id).expect("eager section").to_vec();
            prop_assert_eq!(first, e);
        }
    }

    /// 用 CountingSource 验证：首访增加一次 read_at，再访命中缓存不增加。
    #[test]
    fn second_fetch_hits_cache_no_read(bytes in sealed_with_sections()) {
        let reads = Rc::new(Cell::new(0usize));
        let mut lazy = load_from_source(
            Box::new(CountingSource::new(bytes.clone(), Rc::clone(&reads))),
            LoadStrategy::Lazy,
        )
        .expect("lazy");
        let ids = lazy.section_ids();
        if let Some(id) = ids.first() {
            let baseline = reads.get(); // 骨架读取（Header+骨架+Trailer）已发生
            let a = lazy.section(id).expect("a").to_vec();
            let after_first = reads.get();
            prop_assert_eq!(after_first, baseline + 1, "first fetch should read exactly once");
            let b = lazy.section(id).expect("b").to_vec();
            let after_second = reads.get();
            prop_assert_eq!(after_second, after_first, "second fetch must hit cache (no read)");
            prop_assert_eq!(a, b);
        }
    }
}
