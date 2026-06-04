//! Feature: fayger-m1, Property 5: SemVer total order.
//!
//! `SemVer::cmp` 必须构成全序：自反、反对称、传递；与 `==` 一致；且预发布版本
//! 严格低于同 `major.minor.patch` 的正式版本。
//!
//! Validates: Requirements 6.1, 6.2, 6.3, 6.4, 11.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use core::cmp::Ordering;
use fayger_buf::manifest::SemVer;
use fayger_buf::testing::arb_semver;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

/// 把一个 SemVer 强制设为不带 pre 的正式版本。
fn release_of(v: &SemVer) -> SemVer {
    SemVer {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        pre: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// 自反性：a.cmp(a) == Equal。
    #[test]
    fn reflexive(a in arb_semver()) {
        prop_assert_eq!(a.cmp(&a), Ordering::Equal);
    }

    /// 反对称性：cmp(a,b) 与 cmp(b,a) 互为相反；相等当且仅当 a == b。
    #[test]
    fn antisymmetric(a in arb_semver(), b in arb_semver()) {
        let ab = a.cmp(&b);
        let ba = b.cmp(&a);
        prop_assert_eq!(ab, ba.reverse());
        prop_assert_eq!(ab == Ordering::Equal, a == b);
    }

    /// 传递性：a<=b 且 b<=c ⟹ a<=c。
    #[test]
    fn transitive(a in arb_semver(), b in arb_semver(), c in arb_semver()) {
        if a <= b && b <= c {
            prop_assert!(a <= c);
        }
        if a >= b && b >= c {
            prop_assert!(a >= c);
        }
    }

    /// 与相等关系一致：cmp == Equal ⟺ ==。
    #[test]
    fn consistent_with_eq(a in arb_semver(), b in arb_semver()) {
        prop_assert_eq!(a.cmp(&b) == Ordering::Equal, a == b);
    }

    /// 预发布版本严格低于同 major.minor.patch 的正式版本（§11.3）。
    #[test]
    fn pre_release_lower_than_release(a in arb_semver()) {
        let release = release_of(&a);
        if a.pre.is_some() {
            prop_assert!(a < release, "{} should be < {}", a, release);
        } else {
            prop_assert_eq!(a, release);
        }
    }
}
