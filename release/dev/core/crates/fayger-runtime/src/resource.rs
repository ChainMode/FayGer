//! 资源类型、用量与配额超限判定（M7）。
//!
//! [`first_exceeded`] 按固定顺序 Cpu → Memory → Io 判定首个超限维度（确定性）。判定用严格
//! 大于（达到上限不算越界），且只对配额声明（`Some`）的维度判定。

use fayger_buf::manifest::types::ResourceQuota;

/// 资源类型。顺序固定 `Cpu` → `Memory` → `Io`，用于多维超限时的确定性首选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceKind {
    /// 对应 `ResourceQuota::cpu_ms_per_sec`。
    Cpu,
    /// 对应 `ResourceQuota::memory_bytes`。
    Memory,
    /// 对应 `ResourceQuota::io_bytes_per_sec`。
    Io,
}

impl ResourceKind {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "Cpu",
            Self::Memory => "Memory",
            Self::Io => "Io",
        }
    }
}

/// 一次采样的资源用量（实际用量，三个维度均为 `u64`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResourceUsage {
    /// 每秒 CPU 毫秒数。
    pub cpu_ms_per_sec: u64,
    /// 内存字节数。
    pub memory_bytes: u64,
    /// 每秒 I/O 字节数。
    pub io_bytes_per_sec: u64,
}

impl ResourceUsage {
    /// 便利构造。
    #[must_use]
    pub const fn new(cpu_ms_per_sec: u64, memory_bytes: u64, io_bytes_per_sec: u64) -> Self {
        Self {
            cpu_ms_per_sec,
            memory_bytes,
            io_bytes_per_sec,
        }
    }
}

/// 超限判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaExceeded {
    /// 首个超限的资源类型。
    pub resource: ResourceKind,
    /// 触发超限的用量采样。
    pub usage: ResourceUsage,
}

/// 返回首个超限维度（顺序 Cpu → Memory → Io）；无超限返回 `None`。
///
/// 仅对配额声明（`Some`）的维度判定；判定用严格大于（`usage > limit`）。
#[must_use]
pub fn first_exceeded(quota: &ResourceQuota, usage: &ResourceUsage) -> Option<ResourceKind> {
    if quota
        .cpu_ms_per_sec
        .is_some_and(|limit| usage.cpu_ms_per_sec > u64::from(limit))
    {
        return Some(ResourceKind::Cpu);
    }
    if quota
        .memory_bytes
        .is_some_and(|limit| usage.memory_bytes > limit)
    {
        return Some(ResourceKind::Memory);
    }
    if quota
        .io_bytes_per_sec
        .is_some_and(|limit| usage.io_bytes_per_sec > limit)
    {
        return Some(ResourceKind::Io);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{first_exceeded, ResourceKind, ResourceUsage};
    use fayger_buf::manifest::types::ResourceQuota;

    fn quota(cpu: Option<u32>, mem: Option<u64>, io: Option<u64>) -> ResourceQuota {
        ResourceQuota {
            cpu_ms_per_sec: cpu,
            memory_bytes: mem,
            io_bytes_per_sec: io,
        }
    }

    #[test]
    fn no_quota_dimensions_never_exceeds() {
        let q = quota(None, None, None);
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(u64::MAX, u64::MAX, u64::MAX)),
            None
        );
    }

    #[test]
    fn cpu_exceeds() {
        let q = quota(Some(10), None, None);
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(11, 0, 0)),
            Some(ResourceKind::Cpu)
        );
    }

    #[test]
    fn memory_exceeds() {
        let q = quota(None, Some(100), None);
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(0, 101, 0)),
            Some(ResourceKind::Memory)
        );
    }

    #[test]
    fn io_exceeds() {
        let q = quota(None, None, Some(50));
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(0, 0, 51)),
            Some(ResourceKind::Io)
        );
    }

    #[test]
    fn boundary_equal_does_not_exceed() {
        let q = quota(Some(10), Some(100), Some(50));
        assert_eq!(first_exceeded(&q, &ResourceUsage::new(10, 100, 50)), None);
    }

    #[test]
    fn multi_dimension_returns_first_in_order() {
        let q = quota(Some(10), Some(100), Some(50));
        // cpu + memory 同时超限 → Cpu（顺序首选）
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(11, 101, 0)),
            Some(ResourceKind::Cpu)
        );
        // memory + io 超限（cpu 不超）→ Memory
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(0, 101, 51)),
            Some(ResourceKind::Memory)
        );
    }

    #[test]
    fn undeclared_dimension_not_checked() {
        // 只声明 memory；cpu 用量极大但 cpu 配额为 None → 不超限。
        let q = quota(None, Some(100), None);
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(u64::MAX, 50, 0)),
            None
        );
    }

    #[test]
    fn cpu_quota_u32_promoted_to_u64() {
        // cpu 配额 u32::MAX；用量 u32::MAX as u64 + 1 → 超限。
        let q = quota(Some(u32::MAX), None, None);
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(u64::from(u32::MAX) + 1, 0, 0)),
            Some(ResourceKind::Cpu)
        );
        assert_eq!(
            first_exceeded(&q, &ResourceUsage::new(u64::from(u32::MAX), 0, 0)),
            None
        );
    }
}
