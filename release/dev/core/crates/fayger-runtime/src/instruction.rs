//! 最小 Universal_Instruction 占位类型（M2）。
//!
//! 运行层与适配层之间约定的、与平台无关的指令。M2 只需要一个可被派发、可比较、
//! 可记录的最小形态，用于驱动与验证生命周期与派发路径。
//!
//! M3 会把它扩展为完整的 8 类指令集（`io` / `net` / `ui` / `time` / `random` /
//! `crypto` / `proc` / `host`）与 TLV 二进制编码；届时 `category` 升级为枚举、
//! `operands` 升级为 TLV 序列。M2 不实现这些（见需求 §10.1）。

/// 运行层派发给适配层的一条 Universal_Instruction（M2 最小占位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniversalInstruction {
    /// 指令类别。M2 用 `u8` 占位；M3 替换为 `InstructionCategory` 枚举。
    pub category: u8,
    /// 类内操作码。
    pub opcode: u16,
    /// 操作数。M2 用裸字节占位；M3 替换为 TLV 序列。
    pub operands: Vec<u8>,
}

impl UniversalInstruction {
    /// 便利构造。
    #[must_use]
    pub fn new(category: u8, opcode: u16, operands: impl Into<Vec<u8>>) -> Self {
        Self {
            category,
            opcode,
            operands: operands.into(),
        }
    }
}

/// 适配层回送给运行层的事件（M2 最小占位）。
///
/// M2 的参考实现不产生事件（单向派发），保留此类型以对齐 Runtime_Interface 的
/// `on_event` 语义并为 M3 的双向通信预留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniversalEvent {
    /// 事件类别。
    pub category: u8,
    /// 类内事件码。
    pub opcode: u16,
    /// 事件负载。
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::{UniversalEvent, UniversalInstruction};

    #[test]
    fn instruction_equality_and_clone() {
        let a = UniversalInstruction::new(0, 7, vec![1, 2, 3]);
        let b = a.clone();
        assert_eq!(a, b);
        let c = UniversalInstruction::new(0, 8, vec![1, 2, 3]);
        assert_ne!(a, c);
    }

    #[test]
    fn event_equality_and_clone() {
        let a = UniversalEvent {
            category: 1,
            opcode: 2,
            payload: vec![9],
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
