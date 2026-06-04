//! BuF_Instance 生命周期状态机。
//!
//! 状态：`Loaded → Initialized → Running → Suspended → Terminated/Failed`。
//! `Terminated` 与 `Failed` 是终态，拒绝一切命令。`Failed` 不由命令触发，而是执行期
//! 未处理错误（见 [`crate::runtime::Runtime::inject_fault`]）。
//!
//! 合法迁移表（命令驱动部分）：
//!
//! | 当前状态 | Init | Start | Suspend | Resume | Terminate |
//! | --- | --- | --- | --- | --- | --- |
//! | Loaded | →Initialized | — | — | — | →Terminated |
//! | Initialized | — | →Running | — | — | →Terminated |
//! | Running | — | — | →Suspended | — | →Terminated |
//! | Suspended | — | →Running | — | →Running | →Terminated |
//! | Terminated | — | — | — | — | — |
//! | Failed | — | — | — | — | — |

/// BuF_Instance 的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    /// 加载完成，尚未初始化。
    Loaded,
    /// 已初始化，尚未开始执行。
    Initialized,
    /// 正在执行。
    Running,
    /// 已暂停（不派发新指令）。
    Suspended,
    /// 已终止（终态）。
    Terminated,
    /// 执行中发生未处理错误（终态）。
    Failed,
}

/// 驱动生命周期迁移的命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleCommand {
    /// `Loaded → Initialized`。
    Init,
    /// `Initialized → Running`，或 `Suspended → Running`。
    Start,
    /// `Running → Suspended`。
    Suspend,
    /// `Suspended → Running`。
    Resume,
    /// 任意非终态 → `Terminated`。
    Terminate,
}

impl LifecycleState {
    /// 是否为终态（`Terminated` 或 `Failed`）。
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Terminated | Self::Failed)
    }

    /// 当前状态下被允许的命令集合（确定性、稳定排序）。
    ///
    /// 顺序固定为 `[Init, Start, Suspend, Resume, Terminate]` 的子序列，便于错误诊断与测试。
    #[must_use]
    pub const fn allowed_commands(self) -> &'static [LifecycleCommand] {
        use LifecycleCommand::{Init, Resume, Start, Suspend, Terminate};
        match self {
            Self::Loaded => &[Init, Terminate],
            Self::Initialized => &[Start, Terminate],
            Self::Running => &[Suspend, Terminate],
            Self::Suspended => &[Start, Resume, Terminate],
            Self::Terminated | Self::Failed => &[],
        }
    }

    /// 应用一条命令。
    ///
    /// 返回 `Some(next)` 表示合法迁移到 `next`；返回 `None` 表示当前状态不接受该命令
    /// （调用方据此构造 `RT_ILLEGAL_TRANSITION`）。
    #[must_use]
    #[allow(clippy::match_same_arms)] // 各迁移语义不同，合并相同 body 会损害可读性
    pub const fn apply(self, cmd: LifecycleCommand) -> Option<Self> {
        use LifecycleCommand as C;
        let next = match (self, cmd) {
            (Self::Loaded, C::Init) => Self::Initialized,
            (Self::Initialized, C::Start) => Self::Running,
            (Self::Running, C::Suspend) => Self::Suspended,
            (Self::Suspended, C::Start | C::Resume) => Self::Running,
            (Self::Loaded | Self::Initialized | Self::Running | Self::Suspended, C::Terminate) => {
                Self::Terminated
            }
            // 其它组合（含终态收任意命令）均非法。
            _ => return None,
        };
        Some(next)
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{LifecycleCommand as C, LifecycleState as S};

    const ALL_STATES: [S; 6] = [
        S::Loaded,
        S::Initialized,
        S::Running,
        S::Suspended,
        S::Terminated,
        S::Failed,
    ];
    const ALL_COMMANDS: [C; 5] = [C::Init, C::Start, C::Suspend, C::Resume, C::Terminate];

    /// 期望迁移表：返回 None 表示非法。
    #[allow(clippy::match_same_arms)]
    fn expected(state: S, cmd: C) -> Option<S> {
        match (state, cmd) {
            (S::Loaded, C::Init) => Some(S::Initialized),
            (S::Initialized, C::Start) => Some(S::Running),
            (S::Running, C::Suspend) => Some(S::Suspended),
            (S::Suspended, C::Start | C::Resume) => Some(S::Running),
            (S::Loaded | S::Initialized | S::Running | S::Suspended, C::Terminate) => {
                Some(S::Terminated)
            }
            _ => None,
        }
    }

    #[test]
    fn apply_matches_full_transition_table() {
        for &state in &ALL_STATES {
            for &cmd in &ALL_COMMANDS {
                let got = state.apply(cmd);
                assert_eq!(
                    got,
                    expected(state, cmd),
                    "transition mismatch for ({state:?}, {cmd:?})"
                );
            }
        }
    }

    #[test]
    fn terminal_states_reject_every_command() {
        for &state in &[S::Terminated, S::Failed] {
            assert!(state.is_terminal());
            assert!(state.allowed_commands().is_empty());
            for &cmd in &ALL_COMMANDS {
                assert!(
                    state.apply(cmd).is_none(),
                    "{state:?} should reject {cmd:?}"
                );
            }
        }
    }

    #[test]
    fn allowed_commands_are_exactly_the_legal_ones() {
        for &state in &ALL_STATES {
            let allowed: std::collections::BTreeSet<_> = state
                .allowed_commands()
                .iter()
                .map(|c| format!("{c:?}"))
                .collect();
            let legal: std::collections::BTreeSet<_> = ALL_COMMANDS
                .iter()
                .filter(|&&cmd| state.apply(cmd).is_some())
                .map(|c| format!("{c:?}"))
                .collect();
            assert_eq!(allowed, legal, "allowed_commands mismatch for {state:?}");
        }
    }

    #[test]
    fn suspended_accepts_both_start_and_resume() {
        assert_eq!(S::Suspended.apply(C::Start), Some(S::Running));
        assert_eq!(S::Suspended.apply(C::Resume), Some(S::Running));
    }

    #[test]
    fn happy_path_sequence() {
        let mut s = S::Loaded;
        for (cmd, expect) in [
            (C::Init, S::Initialized),
            (C::Start, S::Running),
            (C::Suspend, S::Suspended),
            (C::Resume, S::Running),
            (C::Terminate, S::Terminated),
        ] {
            s = s.apply(cmd).expect("legal");
            assert_eq!(s, expect);
        }
        assert!(s.is_terminal());
    }
}
