//! 适配层接收端契约（M2 最小形态）与记录型 Mock Adapter。
//!
//! M2 的运行层只依赖 [`Adapter`] trait 与 [`UniversalInstruction`]，不依赖任何具体
//! Platform_Adapter。真实适配器、双向翻译、`UnsupportedInstruction` 与能力裁剪是 M3。

use crate::instruction::UniversalInstruction;

/// 适配层接收端契约。
///
/// 运行层在推进执行时把 [`UniversalInstruction`] 派发给一个 `Adapter`。M2 只需要单向
/// 派发；M3 会扩展为带返回值与事件回送的双向接口。
pub trait Adapter {
    /// 接收运行层派发的一条 Universal_Instruction。
    fn dispatch(&mut self, instr: &UniversalInstruction);
}

/// 记录型 Mock Adapter：把收到的指令按序存入 [`Vec`]，供测试断言。
///
/// 不接触任何真实宿主资源。
#[derive(Debug, Default, Clone)]
pub struct MockAdapter {
    /// 按派发顺序记录的全部指令。
    pub received: Vec<UniversalInstruction>,
}

impl MockAdapter {
    /// 构造一个空的 Mock Adapter。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            received: Vec::new(),
        }
    }
}

impl Adapter for MockAdapter {
    fn dispatch(&mut self, instr: &UniversalInstruction) {
        self.received.push(instr.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::{Adapter, MockAdapter};
    use crate::instruction::UniversalInstruction;

    #[test]
    fn mock_adapter_records_in_order() {
        let mut a = MockAdapter::new();
        let i0 = UniversalInstruction::new(0, 0, vec![]);
        let i1 = UniversalInstruction::new(0, 1, vec![1]);
        a.dispatch(&i0);
        a.dispatch(&i1);
        assert_eq!(a.received, vec![i0, i1]);
    }

    #[test]
    fn default_is_empty() {
        let a = MockAdapter::default();
        assert!(a.received.is_empty());
    }
}
