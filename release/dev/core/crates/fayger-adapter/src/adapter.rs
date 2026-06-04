//! Platform_Adapter 契约与适配器描述符。
//!
//! [`PlatformAdapter`] 是适配层对每类宿主的统一接口：描述符查询、宿主匹配、指令翻译、
//! 事件归一化。M3 内置适配器（见 [`crate::builtin`]）共用本模块的 [`translate_via_descriptor`]
//! 与 [`normalize_default`] helper，差异仅在描述符与匹配逻辑。

use crate::host::{HostEnvironment, HostEvent, HostSyscall};
use crate::instruction::{InstructionCategory, UniversalEvent, UniversalInstruction};
use crate::Capability;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use std::collections::BTreeSet;

/// 适配器描述符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDescriptor {
    /// 稳定平台标识。
    pub platform_id: String,
    /// 该适配器在宿主上可提供的能力集合。
    pub supported_capabilities: BTreeSet<Capability>,
    /// 该适配器支持翻译的指令种类集合。
    pub supported_instructions: BTreeSet<crate::instruction::InstructionKey>,
}

/// Platform_Adapter 契约。
pub trait PlatformAdapter {
    /// 本适配器的描述符。
    fn descriptor(&self) -> AdapterDescriptor;

    /// 判定本适配器是否负责给定宿主环境。
    fn matches(&self, host: &HostEnvironment) -> bool;

    /// 翻译一条受支持的 Universal_Instruction 为 HostSyscall。
    ///
    /// # Errors
    /// 指令种类不在 `supported_instructions` 中时返回 `ADP_UNSUPPORTED_INSTRUCTION`。
    fn translate(&self, instr: &UniversalInstruction) -> Result<HostSyscall, FaygerError>;

    /// 把宿主原始事件归一化为 Universal_Event。
    ///
    /// # Errors
    /// M3 默认实现不失败；保留 `Result` 以便真实适配器表达归一化错误。
    fn normalize(&self, event: &HostEvent) -> Result<UniversalEvent, FaygerError>;
}

/// 内置适配器共用的翻译逻辑：依据描述符的 `supported_instructions` 判定支持性。
///
/// # Errors
/// 指令 key 不在描述符支持集合中时返回 `ADP_UNSUPPORTED_INSTRUCTION`，
/// context 含 `category`、`opcode`、`platform_id`，且不产生任何 HostSyscall。
pub fn translate_via_descriptor(
    descriptor: &AdapterDescriptor,
    instr: &UniversalInstruction,
) -> Result<HostSyscall, FaygerError> {
    if !descriptor.supported_instructions.contains(&instr.key()) {
        return Err(unsupported_instruction(descriptor, instr));
    }
    Ok(HostSyscall {
        platform_id: descriptor.platform_id.clone(),
        name: format!("{}.op{}", instr.category.as_str(), instr.opcode),
        args: instr.operands.clone(),
    })
}

/// 内置适配器共用的事件归一化：原始类别编号 0..=7 → InstructionCategory，越界归 Host。
#[must_use]
pub fn normalize_default(event: &HostEvent) -> UniversalEvent {
    UniversalEvent {
        category: InstructionCategory::from_raw(event.raw_category),
        opcode: event.raw_opcode,
        payload: event.raw_payload.clone(),
    }
}

/// 构造 `ADP_UNSUPPORTED_INSTRUCTION` 错误。
pub(crate) fn unsupported_instruction(
    descriptor: &AdapterDescriptor,
    instr: &UniversalInstruction,
) -> FaygerError {
    FaygerError::new(
        ErrorCode::AdpUnsupportedInstruction,
        Layer::Adapter,
        "platform adapter does not support this instruction",
    )
    .with_context("category", ContextValue::string(instr.category.as_str()))
    .with_context("opcode", ContextValue::U64(u64::from(instr.opcode)))
    .with_context(
        "platform_id",
        ContextValue::string(descriptor.platform_id.clone()),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{normalize_default, translate_via_descriptor, AdapterDescriptor};
    use crate::host::HostEvent;
    use crate::instruction::{InstructionCategory, InstructionKey, UniversalInstruction};
    use fayger_error::ErrorCode;
    use std::collections::BTreeSet;

    fn descriptor_with(keys: &[InstructionKey]) -> AdapterDescriptor {
        AdapterDescriptor {
            platform_id: "test".into(),
            supported_capabilities: BTreeSet::new(),
            supported_instructions: keys.iter().copied().collect(),
        }
    }

    #[test]
    fn translate_supported_key_yields_syscall() {
        let key = InstructionKey::new(InstructionCategory::Io, 7);
        let desc = descriptor_with(&[key]);
        let instr =
            UniversalInstruction::new(InstructionCategory::Io, 7, vec![1, 2, 3], BTreeSet::new());
        let sc = translate_via_descriptor(&desc, &instr).expect("supported");
        assert_eq!(sc.platform_id, "test");
        assert_eq!(sc.name, "io.op7");
        assert_eq!(sc.args, vec![1, 2, 3]);
    }

    #[test]
    fn translate_unsupported_key_errors() {
        let desc = descriptor_with(&[InstructionKey::new(InstructionCategory::Io, 1)]);
        let instr =
            UniversalInstruction::new(InstructionCategory::Proc, 9, vec![], BTreeSet::new());
        let err = translate_via_descriptor(&desc, &instr).expect_err("unsupported");
        assert_eq!(err.code, ErrorCode::AdpUnsupportedInstruction);
        assert!(err.context.contains_key("category"));
        assert!(err.context.contains_key("opcode"));
    }

    #[test]
    fn normalize_maps_raw_category() {
        let ev = HostEvent {
            raw_category: 1,
            raw_opcode: 5,
            raw_payload: vec![9],
        };
        let ue = normalize_default(&ev);
        assert_eq!(ue.category, InstructionCategory::Net);
        assert_eq!(ue.opcode, 5);
        assert_eq!(ue.payload, vec![9]);
    }

    #[test]
    fn normalize_clamps_out_of_range_category() {
        let ev = HostEvent {
            raw_category: 200,
            raw_opcode: 0,
            raw_payload: vec![],
        };
        assert_eq!(normalize_default(&ev).category, InstructionCategory::Host);
    }
}
