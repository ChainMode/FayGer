//! Universal_Instruction 指令模型（平台无关）。
//!
//! 固定 8 个指令类别（[`InstructionCategory`]）。每条指令通过 `capabilities_required`
//! 声明所需能力，供适配层做能力裁剪。[`InstructionKey`] = `(category, opcode)` 是指令的
//! 种类标识，适配器据此声明它支持哪些指令。

use crate::capability::Capability;
use std::collections::BTreeSet;

/// 指令类别。第一阶段固定 8 类（蓝图 §适配层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InstructionCategory {
    /// 文件 / 标准流。
    Io,
    /// 网络 I/O。
    Net,
    /// UI 渲染与输入。
    Ui,
    /// 时钟与定时器。
    Time,
    /// 随机数。
    Random,
    /// 加密原语。
    Crypto,
    /// 进程 / 线程 / 信号。
    Proc,
    /// 宿主特定调用。
    Host,
}

impl InstructionCategory {
    /// 全部 8 个类别（稳定顺序）。
    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::Io,
            Self::Net,
            Self::Ui,
            Self::Time,
            Self::Random,
            Self::Crypto,
            Self::Proc,
            Self::Host,
        ]
    }

    /// 类别的稳定短名（用于 HostSyscall 命名与诊断）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Net => "net",
            Self::Ui => "ui",
            Self::Time => "time",
            Self::Random => "random",
            Self::Crypto => "crypto",
            Self::Proc => "proc",
            Self::Host => "host",
        }
    }

    /// 从 0..=7 的原始编号映射到类别；越界归 [`InstructionCategory::Host`]。
    #[must_use]
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Io,
            1 => Self::Net,
            2 => Self::Ui,
            3 => Self::Time,
            4 => Self::Random,
            5 => Self::Crypto,
            6 => Self::Proc,
            _ => Self::Host,
        }
    }
}

/// 指令种类键：`(category, opcode)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstructionKey {
    /// 指令类别。
    pub category: InstructionCategory,
    /// 类内操作码。
    pub opcode: u16,
}

impl InstructionKey {
    /// 便利构造。
    #[must_use]
    pub const fn new(category: InstructionCategory, opcode: u16) -> Self {
        Self { category, opcode }
    }
}

/// 平台无关指令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniversalInstruction {
    /// 指令类别。
    pub category: InstructionCategory,
    /// 类内操作码。
    pub opcode: u16,
    /// 操作数字节。
    pub operands: Vec<u8>,
    /// 执行该指令所需的能力集合。
    pub capabilities_required: BTreeSet<Capability>,
}

impl UniversalInstruction {
    /// 便利构造。
    #[must_use]
    pub fn new(
        category: InstructionCategory,
        opcode: u16,
        operands: impl Into<Vec<u8>>,
        capabilities_required: BTreeSet<Capability>,
    ) -> Self {
        Self {
            category,
            opcode,
            operands: operands.into(),
            capabilities_required,
        }
    }

    /// 该指令的种类键。
    #[must_use]
    pub const fn key(&self) -> InstructionKey {
        InstructionKey::new(self.category, self.opcode)
    }
}

/// 宿主事件归一化后的平台无关表示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniversalEvent {
    /// 事件类别。
    pub category: InstructionCategory,
    /// 类内事件码。
    pub opcode: u16,
    /// 事件负载。
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::{InstructionCategory, InstructionKey, UniversalInstruction};
    use crate::capability::Capability;
    use std::collections::BTreeSet;

    #[test]
    fn all_categories_has_eight() {
        assert_eq!(InstructionCategory::all().len(), 8);
    }

    #[test]
    fn category_str_is_stable() {
        assert_eq!(InstructionCategory::Io.as_str(), "io");
        assert_eq!(InstructionCategory::Host.as_str(), "host");
    }

    #[test]
    fn from_raw_maps_and_clamps() {
        assert_eq!(InstructionCategory::from_raw(0), InstructionCategory::Io);
        assert_eq!(InstructionCategory::from_raw(7), InstructionCategory::Host);
        assert_eq!(
            InstructionCategory::from_raw(200),
            InstructionCategory::Host
        );
    }

    #[test]
    fn instruction_key_extraction() {
        let mut caps = BTreeSet::new();
        caps.insert(Capability::new("net.http"));
        let instr = UniversalInstruction::new(InstructionCategory::Net, 7, vec![1, 2], caps);
        assert_eq!(
            instr.key(),
            InstructionKey::new(InstructionCategory::Net, 7)
        );
    }

    #[test]
    fn equality_and_clone() {
        let a = UniversalInstruction::new(InstructionCategory::Io, 1, vec![], BTreeSet::new());
        let b = a.clone();
        assert_eq!(a, b);
    }
}
