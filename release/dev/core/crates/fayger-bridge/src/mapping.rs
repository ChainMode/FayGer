//! 运行层指令 → 适配层指令的翻译（M10）。
//!
//! 运行层的 [`fayger_runtime::UniversalInstruction`]（最小占位：`category: u8` / `opcode` /
//! `operands`）与适配层的 [`fayger_adapter::UniversalInstruction`]（完整模型：
//! [`InstructionCategory`] 枚举 + `capabilities_required`）是**不同 crate 的不同类型**。
//! [`to_adapter_instruction`] 是把前者确定性翻译为后者的纯函数。

use fayger_adapter::{Capability, InstructionCategory, UniversalInstruction as AdpInstruction};
use fayger_runtime::UniversalInstruction as RtInstruction;
use std::collections::BTreeSet;

/// 运行层指令 → 适配层指令的翻译策略。
///
/// `category` / `opcode` 为 `None` 时分别用 `InstructionCategory::from_raw(rt.category)` /
/// 保留 `rt.opcode`；`capabilities_required` 是翻译产物声明的所需能力（供适配层裁剪）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstructionMapping {
    /// 覆盖目标类别；`None` 时用 `from_raw(rt.category)`。
    pub category: Option<InstructionCategory>,
    /// 覆盖目标 opcode；`None` 时保留 `rt.opcode`。
    pub opcode: Option<u16>,
    /// 翻译产物声明的所需能力集合。
    pub capabilities_required: BTreeSet<Capability>,
}

impl InstructionMapping {
    /// 规范映射：把每条运行层指令视作一次 `Io.op0` 宿主写（无需能力）。
    ///
    /// 因 M2 运行层程序是「每段一条、operands 为段长」的最小 mock，规范映射把它当作一次
    /// 「宿主 I/O 写」。`Io.op0` 是四类内置适配器都支持的指令，能力充分宿主上恒可派发成功。
    #[must_use]
    pub const fn canonical_io() -> Self {
        Self {
            category: Some(InstructionCategory::Io),
            opcode: Some(0),
            capabilities_required: BTreeSet::new(),
        }
    }

    /// 声明该映射所需能力（builder）。
    #[must_use]
    pub fn requiring<I, S>(mut self, caps: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.capabilities_required = caps.into_iter().map(Capability::new).collect();
        self
    }
}

/// 把一条运行层指令翻译为适配层指令（纯）。
///
/// 字段规则见 [`InstructionMapping`]。`operands` 原样保留。
#[must_use]
pub fn to_adapter_instruction(
    instr: &RtInstruction,
    mapping: &InstructionMapping,
) -> AdpInstruction {
    let category = mapping
        .category
        .unwrap_or_else(|| InstructionCategory::from_raw(instr.category));
    let opcode = mapping.opcode.unwrap_or(instr.opcode);
    AdpInstruction::new(
        category,
        opcode,
        instr.operands.clone(),
        mapping.capabilities_required.clone(),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{to_adapter_instruction, InstructionMapping};
    use fayger_adapter::{Capability, InstructionCategory};
    use fayger_runtime::UniversalInstruction as RtInstruction;
    use std::collections::BTreeSet;

    fn rt(category: u8, opcode: u16, operands: Vec<u8>) -> RtInstruction {
        RtInstruction::new(category, opcode, operands)
    }

    #[test]
    fn default_mapping_uses_from_raw_and_keeps_opcode() {
        let m = InstructionMapping::default();
        // category 1 → Net；opcode 保留 5；operands 原样。
        let out = to_adapter_instruction(&rt(1, 5, vec![9, 9]), &m);
        assert_eq!(out.category, InstructionCategory::Net);
        assert_eq!(out.opcode, 5);
        assert_eq!(out.operands, vec![9, 9]);
        assert!(out.capabilities_required.is_empty());
    }

    #[test]
    fn category_out_of_range_clamps_to_host() {
        let out = to_adapter_instruction(&rt(200, 0, vec![]), &InstructionMapping::default());
        assert_eq!(out.category, InstructionCategory::Host);
    }

    #[test]
    fn override_category_and_opcode() {
        let m = InstructionMapping {
            category: Some(InstructionCategory::Crypto),
            opcode: Some(42),
            capabilities_required: BTreeSet::new(),
        };
        let out = to_adapter_instruction(&rt(0, 7, vec![1]), &m);
        assert_eq!(out.category, InstructionCategory::Crypto);
        assert_eq!(out.opcode, 42);
    }

    #[test]
    fn canonical_io_is_io_op0_no_caps() {
        let out = to_adapter_instruction(
            &rt(3, 9, vec![4, 5, 6]),
            &InstructionMapping::canonical_io(),
        );
        assert_eq!(out.category, InstructionCategory::Io);
        assert_eq!(out.opcode, 0);
        assert_eq!(out.operands, vec![4, 5, 6]);
        assert!(out.capabilities_required.is_empty());
    }

    #[test]
    fn requiring_declares_capabilities() {
        let m = InstructionMapping::canonical_io().requiring(["proc.spawn", "net.http"]);
        let out = to_adapter_instruction(&rt(0, 0, vec![]), &m);
        assert!(out
            .capabilities_required
            .contains(&Capability::new("proc.spawn")));
        assert!(out
            .capabilities_required
            .contains(&Capability::new("net.http")));
    }

    #[test]
    fn translation_is_deterministic() {
        let m = InstructionMapping::canonical_io();
        let i = rt(2, 3, vec![7, 8]);
        assert_eq!(
            to_adapter_instruction(&i, &m),
            to_adapter_instruction(&i, &m)
        );
    }
}
