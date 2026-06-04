# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M10 里程碑**需求文档。M10 落实蓝图反复提到的「运行层与适配层的统一桥接集成」：把运行层（M2/M7/M8）产生的执行流，真正经由适配层（M3）的**适配器选择 → 能力裁剪 → 指令翻译 → HostSyscall**派发出去，并在失败时构造蓝图 §6.3 的**跨层错误因果链**（`RT_INSTANCE_FAILED` → cause `ADP_*`）。

在 M10 之前：运行层只把指令派发给它自己的最小 `Adapter` trait（M2 的 `MockAdapter` 仅做记录），从不接触真实适配层；适配层（M3）能选择适配器、裁剪能力、把指令翻译成 `HostSyscall`，但没有被运行层驱动过。`fayger run` 因此只走 `MockAdapter`。M10 把二者接起来。

关键架构约束（蓝图 §8.8 工具建议第 1 条）：**运行层不得 import 任何 Platform_Adapter 类型**。因此桥接不能让 `fayger-runtime` 依赖 `fayger-adapter`；M10 新增一个**集成层 crate `fayger-bridge`**，它同时依赖运行层与适配层，通过运行层暴露的 `Adapter` trait 这个接缝把两层粘合——依赖方向仍是单向（bridge → runtime / adapter），不在两个库 crate 间引入环。

M10 交付：

- **指令翻译**：把运行层最小 `UniversalInstruction`（M2：`category: u8` / `opcode` / `operands`）确定性翻译为适配层 `UniversalInstruction`（M3：`InstructionCategory` 枚举 / `opcode` / `operands` / `capabilities_required`）。
- **桥接适配器（BridgeAdapter）**：实现运行层 `Adapter` trait；每收到一条运行层指令就翻译并经 `AdapterRegistry` 派发，记录每条派发的结果（`HostSyscall` 或 `ADP_*` 错误）。
- **端到端桥接运行（run_bridged）**：在四类内置适配器上按宿主画像选择适配器，把一个已加载 BuF 跑完生命周期，其指令经真实适配层派发为 `HostSyscall`，产出桥接运行报告。
- **跨层错误链**：当一条指令在适配层被拒（不支持 / 能力不足），桥接把该 `ADP_*` 错误包装为 `RT_INSTANCE_FAILED`（`source_layer = Runtime`）的 cause，`layer_chain()` 为 `[Runtime, Adapter]`。
- **CLI 接入**：`fayger bridge <file>` 子命令，把一个 BuF 经真实适配层在当前宿主上运行并打印产生的 `HostSyscall` 与桥接报告。

M10 **不**交付：真实宿主系统调用（适配层的 `translate` 仍是 M3 的确定性 mock，产出 `HostSyscall` 数据结构而非真正执行）；运行层指令模型升级为完整 8 类指令集（M2 的最小程序仍是「每段一条、operands 为段长」的确定性 mock，桥接把它规范映射为一条 `Io.op0` 宿主写调用——丰富指令语义留待运行层采用完整指令集）；适配层回送事件（`UniversalEvent`）经桥接回流到运行层（单向派发即可满足本里程碑）；性能/JIT（第二阶段）。

完成 M10 后，「一个 BuF 被加载 → 在所选宿主适配器上执行 → 指令经能力裁剪后翻译为宿主调用」这条贯穿三层的主路径首次被打通并可被 PBT 验证；跨层错误也首次以真实的 `[Runtime, Adapter]` 因果链呈现。

## Glossary

- **Bridge（桥接层）**：M10 的集成层 crate `fayger-bridge`，同时依赖运行层与适配层，通过运行层 `Adapter` trait 把两层粘合。
- **InstructionMapping**：运行层指令 → 适配层指令的翻译策略：可选地覆盖 `category` / `opcode`，并声明该指令所需能力集合（`capabilities_required`）。
- **to_adapter_instruction**：纯翻译函数：`(运行层指令, InstructionMapping) -> 适配层指令`。
- **BridgeAdapter**：实现运行层 `Adapter` trait 的桥接适配器；持有一个已选中的 `AdapterRegistry` 与一个 `InstructionMapping`，每次派发翻译后调用 `registry.dispatch`，记录结果（`DispatchOutcome`）。
- **DispatchOutcome**：一次桥接派发的结果：`Ok(HostSyscall)` 或 `Err(FaygerError)`（`ADP_*`）。
- **run_bridged**：端到端桥接运行入口：建注册表 → 选适配器 → 在 `Runtime<BridgeAdapter>` 上跑完生命周期 → 收集报告。
- **BridgeReport**：桥接运行报告：最终生命周期状态、派发指令数、产生的 `HostSyscall` 列表、失败数。
- **dispatch_one**：单条桥接派发（纯，给定已选中注册表）：翻译并派发一条运行层指令，返回 `DispatchOutcome`。
- **wrap_dispatch_failure**：把一个适配层 `ADP_*` 错误包装为 `RT_INSTANCE_FAILED`（`source_layer = Runtime`）的 cause，构造跨层错误链。
- **规范映射（canonical_io mapping）**：`run_bridged` 默认使用的翻译：把每条运行层指令映射为适配层 `Io.op0`（无需能力），代表「一次宿主 I/O 写」，因 M2 程序是最小 mock。

## Requirements

### Requirement 1: 指令翻译

**User Story:** 作为集成开发者，我希望运行层的最小指令能被确定性地翻译为适配层指令，以便经适配层派发。

#### Acceptance Criteria

1. THE Bridge SHALL 提供纯函数 `to_adapter_instruction(运行层指令, mapping)`，产出一条适配层 `UniversalInstruction`。
2. WHERE mapping 未覆盖 category，THE 翻译 SHALL 用 `InstructionCategory::from_raw(运行层指令.category)` 作为类别。
3. WHERE mapping 未覆盖 opcode，THE 翻译 SHALL 保留运行层指令的 `opcode`。
4. THE 翻译 SHALL 原样保留运行层指令的 `operands` 字节序列。
5. THE 翻译产出的 `capabilities_required` SHALL 等于 mapping 声明的能力集合。
6. THE 翻译 SHALL 为确定性的：同一（指令, mapping）输入恒产出相等的适配层指令。

### Requirement 2: 桥接适配器与端到端运行

**User Story:** 作为运维人员，我希望一个 BuF 的执行真正经过适配层派发到宿主调用，以便验证三层主路径打通。

#### Acceptance Criteria

1. THE Bridge SHALL 提供实现运行层 `Adapter` trait 的 `BridgeAdapter`，每收到一条运行层指令即翻译并经其 `AdapterRegistry` 派发，记录该次 `DispatchOutcome`。
2. THE Bridge SHALL 提供 `run_bridged(obj, host, policy, mapping)`：注册四类内置适配器、按 `host` 选择适配器、在 `Runtime<BridgeAdapter>` 上执行 `load → init → start → step* → terminate`，返回 `BridgeReport`。
3. WHEN 在一个能力充分的宿主（如 native-desktop）上以规范映射运行，THE `run_bridged` SHALL 使最终生命周期状态为 `Terminated`，派发指令数等于该 BuF 的 section 数。
4. WHEN 在能力充分的宿主上以规范映射运行，THE 产生的 `HostSyscall` 数 SHALL 等于派发指令数（每条派发都成功翻译），且失败数为 0。
5. WHEN 没有任何内置适配器匹配 `host`，THE `run_bridged` SHALL 返回 `ADP_NO_MATCHING_PLATFORM`（`source_layer = Adapter`），不 panic。
6. THE 每个 `HostSyscall` 的 `platform_id` SHALL 等于被选中适配器的 `platform_id`，`args` SHALL 等于被翻译指令的 `operands`。

### Requirement 3: 能力裁剪在桥接路径生效

**User Story:** 作为宿主管理员，我希望经桥接派发的指令仍受能力裁剪约束，以便受限宿主上越权指令被拒。

#### Acceptance Criteria

1. WHEN 一条被翻译指令声明的 `capabilities_required` 不是当前宿主授予能力（granted）的子集，THE 桥接派发 SHALL 返回 `ADP_CAPABILITY_DENIED`，其 `context.missing` 为未授予的能力集合。
2. THE 桥接路径的授予能力 SHALL 与适配层一致：`granted = adapter.supported ∩ host.available ∩ policy.allowed`。
3. WHEN 一条被翻译指令的种类不在所选适配器的 `supported_instructions` 中，THE 桥接派发 SHALL 返回 `ADP_UNSUPPORTED_INSTRUCTION`。
4. THE 能力检查 SHALL 先于支持判定：当一条指令既能力不足又不被支持时，SHALL 返回 `ADP_CAPABILITY_DENIED`。

### Requirement 4: 跨层错误因果链

**User Story:** 作为 Fayger 调用方，我希望适配层派发失败时能看到由运行层到适配层的因果链，以便定位失败发生在哪一层。

#### Acceptance Criteria

1. THE Bridge SHALL 提供 `wrap_dispatch_failure(adp_error)`，把一个适配层错误包装为 `RT_INSTANCE_FAILED`（`source_layer = Runtime`）的 cause。
2. THE 包装结果的 `layer_chain()` SHALL 等于 `[Runtime, Adapter]`（由外到内）。
3. THE 包装 SHALL NOT 改写被包装的适配层错误的 `code`、`source_layer` 或 `context`。
4. THE 包装结果的 `root_cause()` SHALL 是那个适配层错误，其 `code` 为 `ADP_*` 之一。

### Requirement 5: 不破坏既有契约与分层

**User Story:** 作为 Fayger 项目的维护者，我希望 M10 不破坏既有契约且尊重分层方向。

#### Acceptance Criteria

1. THE `fayger-runtime` crate SHALL NOT 因 M10 而依赖 `fayger-adapter`（运行层不得 import Platform_Adapter 类型，蓝图 §8.8）。
2. THE M10 SHALL NOT 修改 M0–M9 已固化的库 crate 的对外契约（字节布局、加载链、运行层/适配层/观测接口、错误码语义）；M10 以加性方式消费既有 API，并新增 `fayger-bridge` crate。
3. THE M10 SHALL NOT 新增错误码；跨层失败复用既有 `RT_INSTANCE_FAILED` 与 `ADP_*`。
4. WHERE M10 需要运行层暴露桥接所需信息，允许对 `fayger-runtime` 做**加性**扩展（新增方法/访问器，不改既有方法签名与语义）。
5. THE 依赖方向 SHALL 为单向无环：`fayger-bridge` 依赖 `fayger-runtime` 与 `fayger-adapter`；二者互不依赖。

### Requirement 6: CLI 接入

**User Story:** 作为终端用户，我希望用 CLI 把一个 BuF 经真实适配层在当前宿主上运行，以便直观看到产生的宿主调用。

#### Acceptance Criteria

1. THE CLI SHALL 提供 `bridge <file>` 子命令：加载该 BuF、在当前宿主上经适配层运行、打印桥接报告与产生的 `HostSyscall`。
2. WHEN 当前宿主无匹配适配器，THE `bridge` 子命令 SHALL 以执行失败退出码 `1` 结束并呈现 `ADP_NO_MATCHING_PLATFORM`，不 panic。
3. WHEN 文件不可读或加载链失败，THE `bridge` 子命令 SHALL 以退出码 `1` 结束并呈现底层错误。
4. THE `bridge` 子命令的解析 SHALL 与既有 CLI 一致（缺 path → 用法错误，退出码 `2`）。

### Requirement 7: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M10 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M10 实现 SHALL 至少包含以下属性测试：指令翻译确定性与字段保持、桥接运行在能力充分宿主上 syscall 数等于 section 数且最终 Terminated、能力裁剪拒绝越权指令、不支持指令被拒、跨层错误链 `[Runtime, Adapter]` 顺序保持。
2. THE M10 实现 SHALL 提供至少一个端到端用例：用真实加载链产出的 BuF 经 `run_bridged` 在 native-desktop 画像上运行，断言 syscall 序列与 section 一致；并构造一次受限派发，断言跨层错误链。
3. THE M10 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M10 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0–M9 既有测试在 M10 引入后仍全部通过。

### Requirement 8: M10 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M10 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M10 SHALL NOT 触达真实宿主系统调用；适配层 `translate` 仍产出确定性 mock `HostSyscall`。
2. THE M10 SHALL NOT 把运行层指令模型升级为完整 8 类指令集；运行层仍用 M2 最小 mock 程序，桥接用规范映射处理。
3. THE M10 SHALL NOT 实现适配层事件（`UniversalEvent`）经桥接回流到运行层；M10 桥接为单向派发。
4. THE M10 SHALL NOT 引入新的第三方运行时依赖。
5. THE M10 SHALL NOT 让 `fayger-runtime` 依赖 `fayger-adapter`；桥接经运行层 `Adapter` trait 这一接缝完成。

## 约束与假设

- **目标语言**：M10 仅交付 Rust 实现。
- **crate 归属**：M10 新增 `fayger-bridge` crate（桥接层）；CLI 接入扩展 `fayger-cli`；如需，运行层 `fayger-runtime` 做加性扩展（访问器）。
- **依赖方向**：`fayger-bridge` 依赖 `fayger-runtime` / `fayger-adapter` / `fayger-buf` / `fayger-error`；`fayger-runtime` 与 `fayger-adapter` 互不依赖（无环）。
- **新增依赖**：M10 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M10 不新增错误码，复用 `RT_INSTANCE_FAILED` 与 `ADP_*`。
- **可测建模**：翻译、单条派发、错误链包装均为纯函数；`run_bridged` 用确定性宿主画像与内置适配器，无 FS / 网络 / 真实时钟。
