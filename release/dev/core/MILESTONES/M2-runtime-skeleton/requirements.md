# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M2 里程碑**需求文档。M2 在 M0（BuF 格式 + Parser + Serializer）与 M1（加载链：Verify(Digest) + Negotiate(Version)）之上，落实蓝图三层结构中的**运行层（Runtime Layer）骨架**：

- **Runtime_Interface 统一契约**：定义一份语言中立的运行层接口，覆盖 BuF_Instance 的加载、初始化、执行、暂停、恢复、终止与状态查询；通过 Universal_Instruction 与适配层通信，不引用任何 Platform_Adapter 细节。
- **Runtime_Implementation 注册与路由**：校验一个候选实现是否声明了 Runtime_Interface 的全部必需操作；当存在多个已注册实现时，按 BuF_Manifest 声明的选择策略路由。
- **BuF_Instance 生命周期状态机**：`Loaded → Initialized → Running → Suspended → Terminated/Failed` 的合法迁移、非法迁移拒绝、失败状态保留错误上下文。
- **Mock Adapter 与最小 Universal_Instruction**：M2 提供一个最小的 `UniversalInstruction` 占位类型与一个 Mock Adapter，作为运行层"派发指令"的接收端，使生命周期执行路径可被观察与测试。

M2 **不**交付：完整的 8 类 Universal_Instruction 指令集、真实 Platform_Adapter、能力裁剪（M3）；资源监测与配额（M7）；可观测性事件总线与跨层错误链（M8）；签名（M4）；BuF_Source / 部分加载（M5/M6）。这些明确推迟到后续里程碑。

完成 M2 后，项目能够：把一个经 M1 加载的 BuFObject 交给运行层，创建一个处于 `Loaded` 状态的 BuF_Instance，驱动它走完合法生命周期，并在执行过程中把 Universal_Instruction 派发给（Mock）适配层——即"BuF 成为一个可被启动/暂停/停止的运行实体"的最小可证形态。

## Glossary

- **Runtime_Layer**：运行层，负责执行 BuF 语义的子系统。
- **Runtime_Interface**：运行层对外暴露的稳定接口契约。M2 在 Rust 中以 trait 表达，方法语义与蓝图 §运行层一致。
- **Runtime_Implementation**：Runtime_Interface 的一种具体实现。M2 提供一个参考实现（解释执行骨架）。
- **RuntimeDescriptor**：对一个 Runtime_Implementation 的数据化描述，至少包含实现 id、`rt_version`、声明支持的操作集合（`supported_operations`）。注册完备性检查针对该描述符进行。
- **RuntimeOperation**：Runtime_Interface 的必需操作枚举，至少包含 `Load`、`Init`、`Start`、`Suspend`、`Resume`、`Terminate`、`State`、`FailureInfo`、`Emit`、`OnEvent`。
- **RuntimeRegistry**：运行层的实现注册表，负责注册完备性校验、列举与路由。
- **BuF_Instance**：被运行层加载、处于某个生命周期状态的运行时实体。M2 中由 `InstanceId` 唯一标识。
- **InstanceId**：BuF_Instance 在单个 Runtime_Implementation 内的唯一标识。
- **LifecycleState**：BuF_Instance 的生命周期状态枚举：`Loaded`、`Initialized`、`Running`、`Suspended`、`Terminated`、`Failed`。其中 `Terminated` 与 `Failed` 为终态。
- **LifecycleCommand**：驱动状态迁移的指令枚举：`Init`、`Start`、`Suspend`、`Resume`、`Terminate`。
- **Universal_Instruction**：运行层与适配层之间约定的、与平台无关的指令。M2 仅提供最小占位类型（category / opcode / operands），完整 8 类指令集留待 M3。
- **Adapter（Mock）**：M2 的适配层占位接收端，记录运行层派发的 Universal_Instruction，不接触真实宿主。真实 Platform_Adapter 留待 M3。
- **FailureInfo**：BuF_Instance 进入 `Failed` 时保留的失败上下文，至少包含触发失败的最末一条 Universal_Instruction 与对应错误对象。
- **SelectionStrategy**：BuF_Manifest 中声明的运行层选择策略：`Strict`、`PreferThenAny`、`Any`（M0 已在 fayger-buf 定义）。
- **compatible(impl, manifest)**：兼容判定。M2 定义为 `impl.rt_version >= manifest.runtime_interface_min`（复用 M1 的 SemVer 全序）。

## Requirements

### Requirement 1: Runtime_Interface 契约

**User Story:** 作为运行层实现者，我希望存在一份与语言无关、覆盖完整生命周期的运行层接口契约，以便任意实现都遵循同一语义。

#### Acceptance Criteria

1. THE Runtime_Layer SHALL 定义一份 Runtime_Interface，覆盖 BuF_Instance 的加载（load）、初始化（init）、执行（start）、暂停（suspend）、恢复（resume）、终止（terminate）、状态查询（state）与失败查询（failure_info）操作。
2. THE Runtime_Interface SHALL 通过 Universal_Instruction 与适配层通信，且其定义不引用任何 Platform_Adapter 具体类型。
3. THE Runtime_Interface 的每个操作 SHALL 在失败时返回统一错误模型 `FaygerError`，`source_layer` 为 `Runtime`。
4. WHEN Runtime_Interface 的某个操作以 `InstanceId` 为参数且该实例不存在，THE Runtime_Layer SHALL 返回错误码 `RT_INSTANCE_NOT_FOUND`，错误上下文包含该 `InstanceId`。

### Requirement 2: Runtime_Implementation 注册完备性

**User Story:** 作为 Fayger 的集成方，我希望在注册一个运行层实现时校验它是否覆盖全部必需操作，以便不完整的实现被尽早拒绝。

#### Acceptance Criteria

1. THE RuntimeRegistry SHALL 暴露注册操作，输入一个 RuntimeDescriptor（含实现 id、`rt_version`、`supported_operations`）。
2. THE 必需操作集合（REQUIRED_OPERATIONS）SHALL 至少包含 `Load`、`Init`、`Start`、`Suspend`、`Resume`、`Terminate`、`State`、`FailureInfo`、`Emit`、`OnEvent`。
3. WHEN 候选 RuntimeDescriptor 的 `supported_operations` 是 REQUIRED_OPERATIONS 的超集（即包含全部必需操作），THE RuntimeRegistry SHALL 接受注册并返回一个注册标识（RegistrationId）。
4. IF 候选 RuntimeDescriptor 缺少 REQUIRED_OPERATIONS 中的任一操作，THEN THE RuntimeRegistry SHALL 拒绝注册并返回错误码 `RT_IMPL_MISSING_METHODS`，错误上下文 `missing_methods` 恰为 `REQUIRED_OPERATIONS \ supported_operations`。
5. THE RuntimeRegistry SHALL 暴露列举操作，返回所有已注册实现的 RuntimeDescriptor。

### Requirement 3: 运行层选择与路由

**User Story:** 作为多实现场景的维护者，我希望按 BuF_Manifest 声明的策略把 BuF 路由到合适的运行层实现，以便同一 Fayger 内多实现共存。

#### Acceptance Criteria

1. THE RuntimeRegistry SHALL 暴露路由操作，输入一个 BuFManifest，输出被选中实现的 RegistrationId。
2. THE 兼容判定 compatible(impl, manifest) SHALL 定义为 `impl.rt_version >= manifest.runtime_interface_min`。
3. WHERE BuFManifest 声明的选择策略为 `Strict`，THE RuntimeRegistry SHALL 仅在 `preferred_impl` 已注册且兼容时返回它，否则返回错误码 `RT_IMPL_NOT_FOUND`。
4. WHERE 选择策略为 `PreferThenAny`，THE RuntimeRegistry SHALL 在 `preferred_impl` 已注册且兼容时返回它，否则在所有兼容实现中按确定性顺序返回一个；若无兼容实现则返回 `RT_IMPL_NOT_FOUND`。
5. WHERE 选择策略为 `Any`（或 BuFManifest 未声明 runtime 选择需求），THE RuntimeRegistry SHALL 在所有兼容实现中按确定性顺序返回一个；若无兼容实现则返回 `RT_IMPL_NOT_FOUND`。
6. THE 路由结果 SHALL 是确定性的：对相同的已注册实现集合与相同 BuFManifest，多次路由返回相同 RegistrationId。

### Requirement 4: BuF_Instance 创建与状态查询

**User Story:** 作为 Fay 应用的使用者，我希望加载成功后得到一个可查询状态的运行实例，以便我能观察它的生命周期。

#### Acceptance Criteria

1. WHEN Runtime_Layer 成功加载一个 BuFObject，THE Runtime_Layer SHALL 创建一个 BuF_Instance 并使其初始状态为 `Loaded`，同时返回唯一的 `InstanceId`。
2. THE Runtime_Layer SHALL 暴露状态查询操作，对一个存在的 `InstanceId` 返回其当前 `LifecycleState`。
3. WHEN 同一 Runtime_Implementation 加载多个 BuFObject，THE Runtime_Layer SHALL 为每个实例分配互不相同的 `InstanceId`。

### Requirement 5: 生命周期合法迁移

**User Story:** 作为 Fay 应用的使用者，我希望实例的状态迁移遵循明确的合法规则，以便启动、暂停、恢复与停止行为可预期。

#### Acceptance Criteria

1. WHEN BuF_Instance 处于 `Loaded` 且收到 `Init` 指令，THE Runtime_Layer SHALL 将其状态迁移至 `Initialized`。
2. WHEN BuF_Instance 处于 `Initialized` 或 `Suspended` 且收到 `Start` 指令，THE Runtime_Layer SHALL 将其状态迁移至 `Running`。
3. WHEN BuF_Instance 处于 `Suspended` 且收到 `Resume` 指令，THE Runtime_Layer SHALL 将其状态迁移至 `Running`。
4. WHEN BuF_Instance 处于 `Running` 且收到 `Suspend` 指令，THE Runtime_Layer SHALL 将其状态迁移至 `Suspended`。
5. WHEN BuF_Instance 处于 `Loaded`、`Initialized`、`Running` 或 `Suspended` 且收到 `Terminate` 指令，THE Runtime_Layer SHALL 将其状态迁移至 `Terminated` 并释放其运行层资源。
6. THE 合法迁移集合 SHALL 恰好为：`Loaded→{Initialized,Terminated}`、`Initialized→{Running,Terminated}`、`Running→{Suspended,Terminated}`、`Suspended→{Running,Terminated}`（外加任意非终态可因未处理错误迁移至 `Failed`，见 Requirement 7）。
7. IF BuF_Instance 收到与当前 `LifecycleState` 不兼容的 LifecycleCommand，THEN THE Runtime_Layer SHALL 拒绝该指令、保持状态不变，并返回错误码 `RT_ILLEGAL_TRANSITION`，错误上下文包含 `current_state` 与 `allowed_commands`（当前状态下被允许的指令集合）。
8. IF BuF_Instance 处于终态（`Terminated` 或 `Failed`）且收到任意 LifecycleCommand，THEN THE Runtime_Layer SHALL 拒绝该指令并返回 `RT_ILLEGAL_TRANSITION`。

### Requirement 6: 执行与指令派发

**User Story:** 作为 Fay 应用的使用者，我希望实例在 `Running` 状态下把指令派发到适配层，并在暂停后停止派发，以便执行行为可控。

#### Acceptance Criteria

1. WHEN BuF_Instance 处于 `Running` 且运行层推进一步执行，THE Runtime_Layer SHALL 把一条 Universal_Instruction 派发给已配置的 Adapter。
2. WHILE BuF_Instance 处于 `Suspended`，THE Runtime_Layer SHALL NOT 向 Adapter 派发任何新的 Universal_Instruction。
3. WHEN 处于 `Suspended` 的 BuF_Instance 被 `Resume` 回到 `Running` 后再次推进执行，THE Runtime_Layer SHALL 恢复向 Adapter 派发 Universal_Instruction。
4. THE Runtime_Layer 派发给 Adapter 的 Universal_Instruction 序列 SHALL 仅依赖 BuFObject 的内容与执行步数，不依赖 Adapter 的具体类型。

### Requirement 7: 失败状态与上下文保留

**User Story:** 作为 Fay 开发者，我希望实例执行中发生未处理错误时进入失败态并保留诊断信息，以便事后排查。

#### Acceptance Criteria

1. IF BuF_Instance 在 `Running` 状态下执行过程中抛出未被处理的错误，THEN THE Runtime_Layer SHALL 将其状态迁移至 `Failed`。
2. WHEN BuF_Instance 进入 `Failed`，THE Runtime_Layer SHALL 通过失败查询操作返回一个 FailureInfo，包含触发失败的最末一条 Universal_Instruction 与对应的 `FaygerError` 错误对象。
3. WHEN 对一个非 `Failed` 状态的实例查询失败信息，THE Runtime_Layer SHALL 返回"无失败信息"（空）。
4. THE 触发失败时返回的错误 SHALL 使用错误码 `RT_INSTANCE_FAILED`，`source_layer` 为 `Runtime`。

### Requirement 8: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M2 新增的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M2 错误码集合 SHALL 在既有码之外至少新增 `RT_ILLEGAL_TRANSITION`、`RT_IMPL_NOT_FOUND`、`RT_IMPL_MISSING_METHODS`、`RT_INSTANCE_FAILED`、`RT_INSTANCE_NOT_FOUND` 五个稳定值。
2. THE M2 新增错误 SHALL 复用统一的 `FaygerError` 结构，`source_layer` 为 `Runtime`。
3. WHEN M2 各组件产生错误，THE 错误对象 SHALL 在 `context` 中提供足以定位失败的结构化字段（如 `instance_id`、`current_state`、`allowed_commands`、`missing_methods`、`preferred_impl`、`required`、`provided`）。
4. THE M0/M1 既有错误码 SHALL 保持语义不变，M2 不复用或重定义它们。

### Requirement 9: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M2 与 M0/M1 保持同一质量基线。

#### Acceptance Criteria

1. THE M2 实现 SHALL 至少包含以下属性测试：生命周期合法迁移（合法指令迁移、非法指令拒绝并保状态、终态拒绝一切指令）、注册完备性（集合代数）、路由策略决策、失败保留（fault 后进入 Failed 且 FailureInfo 完整）、暂停停止派发（Suspended 期间不派发，Resume 后恢复）。
2. THE M2 实现 SHALL 提供至少一个端到端用例：加载一个经 M1 封装的 BuF、走完 `Loaded→Initialized→Running→Suspended→Running→Terminated`、并断言 Adapter 收到的指令序列符合预期。
3. THE M2 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用 M0/M1 的 `.regressions` 归档机制）。
5. THE M2 实现 SHALL 与 M0/M1 共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0/M1 既有测试在 M2 引入后仍全部通过。

### Requirement 10: M2 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M2 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M2 实现 SHALL NOT 定义完整的 8 类 Universal_Instruction 指令集或其 TLV 二进制编码（这是 M3）；M2 的 `UniversalInstruction` 仅为最小占位类型。
2. THE M2 实现 SHALL NOT 引入真实 Platform_Adapter、Host_Environment 检测或能力裁剪（这是 M3）；M2 仅提供 Mock Adapter。
3. THE M2 实现 SHALL NOT 引入资源监测、配额或 `RT_QUOTA_EXCEEDED`（这是 M7）。
4. THE M2 实现 SHALL NOT 引入可观测性事件总线或跨层 `cause` 错误链（这是 M8）；M2 的错误 `cause` 可为 None。
5. THE M2 实现 SHALL NOT 修改 M0/M1 已固化的 BuF 字节布局、加载链或既有错误码语义；M2 仅在其上新增运行层组件与运行层错误码。
6. THE M2 的参考 Runtime_Implementation SHALL NOT 实现真实字节码解释；其"执行"是一个确定性的 mock 程序（从 BuF 内容派生的指令派发序列），仅用于驱动与验证生命周期与派发路径。

## 约束与假设

- **目标语言**：M2 仅交付 Rust 实现。
- **crate 归属**：M2 的运行层逻辑落在新的 `fayger-runtime` crate（依赖 `fayger-buf` 与 `fayger-error`）。运行层不依赖 `fayger-loader`（它接收已加载的 `BuFObject`），但端到端测试可使用 `fayger-loader` 构造输入。
- **新增依赖**：M2 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M2 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
- **方法完备性建模**：Rust trait 在编译期即保证方法齐全，无法表达"实现缺少某方法"。M2 因此把注册完备性建模在**数据层**（RuntimeDescriptor 声明 `supported_operations`），以覆盖跨语言/插件式实现注册场景，并使该性质可被属性测试验证。
