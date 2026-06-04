//! 能力标识与能力裁剪（Capability Negotiation）。
//!
//! 借鉴 WASI 的 capability-based security 与 OCI 的 capabilities 裁剪：BuF 与指令声明
//! 请求的能力，宿主与策略声明可用/允许的能力，实际授予是三者的交集。
//!
//! [`negotiate`] 是纯函数：`granted = requested ∩ available ∩ policy.allowed`，
//! `denied = requested \ granted`。结果只依赖三个输入集合，确定性可测。

use std::collections::BTreeSet;

/// 能力标识。
///
/// M3 用 newtype 包裹 [`String`]，便于集合运算且支持宿主注入自定义能力（开放字典）。
/// 约定用点分命名，如 `fs.read`、`net.http`、`ui.dom`、`proc.spawn`、`host.call`。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Capability(pub String);

impl Capability {
    /// 从任意可转字符串构造能力标识。
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// 取底层字符串。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 宿主策略：在该宿主上**允许**授予的能力集合。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostPolicy {
    /// 允许集合。能力裁剪时与请求集、适配器可用集取交。
    pub allowed: BTreeSet<Capability>,
}

impl HostPolicy {
    /// 用一组能力构造策略。
    #[must_use]
    pub const fn new(allowed: BTreeSet<Capability>) -> Self {
        Self { allowed }
    }

    /// 允许全部所列能力的便捷构造。
    pub fn allow_all<I, S>(caps: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed: caps.into_iter().map(Capability::new).collect(),
        }
    }
}

/// 能力裁剪结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiationResult {
    /// 实际授予的能力 = `requested ∩ available ∩ policy.allowed`。
    pub granted: BTreeSet<Capability>,
    /// 被拒绝的能力 = `requested \ granted`。
    pub denied: BTreeSet<Capability>,
}

/// 纯函数能力裁剪。
///
/// 不变式：`granted ⊆ requested`、`granted ⊆ available`、`granted ⊆ policy.allowed`、
/// `granted ∩ denied == ∅`、`granted ∪ denied == requested`。
#[must_use]
pub fn negotiate(
    requested: &BTreeSet<Capability>,
    available: &BTreeSet<Capability>,
    policy: &HostPolicy,
) -> NegotiationResult {
    let granted: BTreeSet<Capability> = requested
        .iter()
        .filter(|c| available.contains(*c) && policy.allowed.contains(*c))
        .cloned()
        .collect();
    let denied: BTreeSet<Capability> = requested.difference(&granted).cloned().collect();
    NegotiationResult { granted, denied }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{negotiate, Capability, HostPolicy};
    use std::collections::BTreeSet;

    fn caps(items: &[&str]) -> BTreeSet<Capability> {
        items.iter().map(|s| Capability::new(*s)).collect()
    }

    #[test]
    fn granted_is_three_way_intersection() {
        let requested = caps(&["fs.read", "net.http", "proc.spawn"]);
        let available = caps(&["fs.read", "net.http", "ui.dom"]);
        let policy = HostPolicy::new(caps(&["fs.read", "proc.spawn"]));
        let r = negotiate(&requested, &available, &policy);
        // fs.read 在三者中都有；net.http 不在 policy；proc.spawn 不在 available。
        assert_eq!(r.granted, caps(&["fs.read"]));
        assert_eq!(r.denied, caps(&["net.http", "proc.spawn"]));
    }

    #[test]
    fn empty_requested_yields_empty() {
        let r = negotiate(
            &BTreeSet::new(),
            &caps(&["a"]),
            &HostPolicy::allow_all(["a"]),
        );
        assert!(r.granted.is_empty());
        assert!(r.denied.is_empty());
    }

    #[test]
    fn full_grant_when_all_present() {
        let requested = caps(&["a", "b"]);
        let r = negotiate(
            &requested,
            &caps(&["a", "b", "c"]),
            &HostPolicy::allow_all(["a", "b"]),
        );
        assert_eq!(r.granted, requested);
        assert!(r.denied.is_empty());
    }

    #[test]
    fn disjoint_denies_everything() {
        let requested = caps(&["a", "b"]);
        let r = negotiate(
            &requested,
            &caps(&["x"]),
            &HostPolicy::allow_all(["a", "b"]),
        );
        assert!(r.granted.is_empty());
        assert_eq!(r.denied, requested);
    }

    #[test]
    fn invariants_hold() {
        let requested = caps(&["a", "b", "c"]);
        let available = caps(&["a", "b"]);
        let policy = HostPolicy::allow_all(["b", "c"]);
        let r = negotiate(&requested, &available, &policy);
        // granted ⊆ requested / available / policy
        assert!(r.granted.is_subset(&requested));
        assert!(r.granted.is_subset(&available));
        assert!(r.granted.is_subset(&policy.allowed));
        // granted ∩ denied == ∅ ; granted ∪ denied == requested
        assert!(r.granted.is_disjoint(&r.denied));
        let union: BTreeSet<_> = r.granted.union(&r.denied).cloned().collect();
        assert_eq!(union, requested);
    }
}
