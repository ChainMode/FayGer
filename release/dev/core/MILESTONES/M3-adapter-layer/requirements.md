# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M3 里程碑**需求文档。M3 在 M0（BuF 格式）、M1（加载链）、M2（运行层骨架）之上落实蓝图三层结构中的**适配层（Adapter Layer）**：

- **Universal_Instruction 指令集**：定义平台无关的指令模型，固定 8 个类别（`io`、`net`、`ui`、`time`、`random`、`crypto`、`proc`、`host`），每条指令声明其所需能力（capabilities）；并定义系统事件归一化为 Universal_Event 的反向通道占位。
- **Platform_Adapter 契约与四类内置适配器**：定义 Platform_Adapter 接口（描述符、宿主匹配、指令翻译、事件归一化），并提供四类内置适配器——原生桌面（NativeDesktop）、服务器（Server）、浏览器（Browser）、宿主 In-App（InApp）。
- **Host_Environment 检测与适配器选择**：从宿主环境画像中确定性地选取匹配的 Platform_Adapter；无匹配时阻止进入可服务状态。
- **能力裁剪（Capability Negotiation）**：以集合代数 `granted = requested ∩ available ∩ policy` 计算实际授予能力；声明的必需能力未被满足时拒绝进入可服务/运行状态。
- **指令派发与不支持指令处理**：把 Universal_Instruction 经能力检查后交由当前适配器翻译；适配器不支持的指令返回 `ADP_UNSUPPORTED_INSTRUCTION` 且不触达宿主。

M3 **不**交付：真实操作系统系统调用 / 真实浏览器 Web API 绑定（内置适配器以确定性 mock 翻译为基线，真实 FFI/WASM 绑定在 `release/dev/adapters/` 的后续工作中完成）；资源监测与配额（M7）；可观测性事件总线与跨层错误链（M8）；签名（M4）；BuF_Source / 部分加载（M5/M6）。

完成 M3 后，Fayger 拥有完整的三层契约骨架：加载层（M0/M1）产出可信 BuF，运行层（M2）驱动生命周期并发出 Universal_Instruction，适配层（M3）在能力裁剪后把指令翻译为目标宿主可执行的形态——即"同一份 BuF 在桌面/服务器/浏览器/In-App 四类宿主上可被适配执行"的最小可证形态。

## Glossary

- **Adapter_Layer**：适配层，负责把 Universal_Instruction 翻译为目标宿主系统调用、并把宿主事件归一化回 Universal_Event 的子系统。
- **Universal_Instruction**：运行层与适配层之间约定的、与平台无关的指令。M3 的形态为 `{ category, opcode, operands, capabilities_required }`。
- **Universal_Event**：宿主侧事件归一化后的平台无关表示 `{ category, opcode, payload }`。
- **InstructionCategory**：指令类别枚举，固定 8 个：`Io`、`Net`、`Ui`、`Time`、`Random`、`Crypto`、`Proc`、`Host`。
- **InstructionKey**：一条指令的"种类键" `(category, opcode)`，用于适配器声明它支持哪些指令、以及未支持判定。
- **Capability**：能力标识（字符串，如 `fs.read`、`net.http`、`ui.dom`、`proc.spawn`）。指令通过 `capabilities_required` 声明所需能力。
- **HostSyscall**：适配器翻译 Universal_Instruction 得到的、面向目标宿主的调用描述。M3 中是一个确定性的数据结构（mock），不触达真实系统调用。
- **HostEvent**：宿主侧原始事件。M3 中是一个数据结构占位。
- **HostEnvironment**：宿主环境画像，至少包含 `platform_id`、`os_kind`、`arch`、`sandbox_kind`、`available_capabilities`。
- **HostEnvironmentDetector**：从运行环境产出 HostEnvironment 的组件。M3 提供一个可注入画像的检测器（测试可控）。
- **Platform_Adapter**：针对某一类宿主的具体适配器。M3 内置四类：NativeDesktop、Server、Browser、InApp。
- **AdapterDescriptor**：对一个 Platform_Adapter 的描述，至少包含 `platform_id`、`supported_capabilities`、`supported_instructions`（InstructionKey 集合）。
- **AdapterRegistry**：适配器注册表，负责注册、依宿主环境选择匹配适配器、并暴露派发入口。
- **HostPolicy**：宿主策略，声明在该宿主上**允许**授予的能力集合（与适配器自身的 `available_capabilities` 取交）。
- **CapabilityNegotiator**：能力裁剪器，计算 `granted = requested ∩ available ∩ policy`、`denied = requested \ granted`。
- **NegotiationResult**：裁剪结果 `{ granted, denied }`。
- **Serviceable（可服务状态）**：AdapterRegistry 成功选中一个匹配适配器后进入的状态；无匹配适配器时不进入。

## Requirements

### Requirement 1: Universal_Instruction 指令模型

**User Story:** 作为运行层与适配层之间契约的维护者，我希望有一个平台无关、可声明能力需求的指令模型，以便适配层据此翻译与裁剪。

#### Acceptance Criteria

1. THE Universal_Instruction SHALL 至少包含 `category`（InstructionCategory）、`opcode`（u16）、`operands`（字节序列）、`capabilities_required`（Capability 集合）四个字段。
2. THE InstructionCategory SHALL 恰好包含 8 个类别：`Io`、`Net`、`Ui`、`Time`、`Random`、`Crypto`、`Proc`、`Host`。
3. THE Adapter_Layer SHALL 定义 InstructionKey 为 `(category, opcode)`，作为指令的种类标识。
4. THE Universal_Event SHALL 至少包含 `category`、`opcode`、`payload` 三个字段，作为宿主事件归一化后的表示。

### Requirement 2: Platform_Adapter 契约

**User Story:** 作为适配层实现者，我希望存在一份统一的适配器契约，以便每类宿主以一致的方式接入。

#### Acceptance Criteria

1. THE Platform_Adapter SHALL 暴露描述符查询，返回 AdapterDescriptor（含 `platform_id`、`supported_capabilities`、`supported_instructions`）。
2. THE Platform_Adapter SHALL 暴露宿主匹配判定 `matches(host)`，对一个 HostEnvironment 返回是否由本适配器负责。
3. THE Platform_Adapter SHALL 暴露翻译操作 `translate(instr)`，对一条受支持的 Universal_Instruction 返回 HostSyscall。
4. THE Platform_Adapter SHALL 暴露事件归一化操作 `normalize(event)`，对一个 HostEvent 返回 Universal_Event。
5. IF 一条 Universal_Instruction 的 InstructionKey 不在 `supported_instructions` 中，THEN THE Platform_Adapter 的翻译操作 SHALL 返回错误码 `ADP_UNSUPPORTED_INSTRUCTION`，错误上下文包含该指令的 category 与 opcode。

### Requirement 3: 四类内置适配器

**User Story:** 作为跨平台用户，我希望同一份 BuF 在桌面、服务器、浏览器与宿主 In-App 四类宿主中都有对应适配器，以便只打包一次。

#### Acceptance Criteria

1. THE Adapter_Layer SHALL 至少提供四类内置 Platform_Adapter：NativeDesktop、Server、Browser、InApp。
2. THE NativeDesktop 适配器 SHALL 声明支持全部 8 个 InstructionCategory 的能力全集。
3. THE Server 适配器 SHALL NOT 声明 `Ui` 类别相关能力（无 GUI 场景）。
4. THE Browser 适配器 SHALL NOT 声明 `Proc` 类别相关能力，且其 `Io` 与 `Net` 能力受限于浏览器沙箱可用集。
5. THE InApp 适配器 SHALL 仅声明由宿主显式注入的能力集合（最严格，默认空注入时仅 `Host` 类别基础能力）。
6. WHEN 任一内置适配器被查询描述符，THE 适配器 SHALL 返回稳定的 `platform_id`（`native-desktop` / `server` / `browser` / `inapp`）。

### Requirement 4: Host_Environment 检测与适配器选择

**User Story:** 作为 Fayger 的启动流程维护者，我希望系统自动选取与当前宿主匹配的适配器，并在无匹配时明确拒绝服务，以便启动行为可预期。

#### Acceptance Criteria

1. THE Adapter_Layer SHALL 暴露 HostEnvironmentDetector，产出当前 HostEnvironment 画像。
2. WHEN AdapterRegistry 依据一个 HostEnvironment 选择适配器，THE AdapterRegistry SHALL 在所有 `matches(host)` 为真的已注册适配器中按确定性顺序选取一个并进入可服务状态。
3. IF 没有任何已注册适配器的 `matches(host)` 为真，THEN THE AdapterRegistry SHALL 拒绝进入可服务状态并返回错误码 `ADP_NO_MATCHING_PLATFORM`，错误上下文包含该 HostEnvironment 的 `platform_id`。
4. THE 适配器选择结果 SHALL 是确定性的：对相同的已注册适配器集合与相同 HostEnvironment，多次选择返回相同适配器。

### Requirement 5: 能力裁剪

**User Story:** 作为宿主管理员，我希望 BuF 请求的能力在受限宿主上被裁剪为实际可用的交集，以便在沙箱等受限环境中安全运行。

#### Acceptance Criteria

1. THE CapabilityNegotiator SHALL 计算 `granted = requested ∩ available ∩ policy`，其中 `requested` 为请求能力集、`available` 为当前适配器的 `supported_capabilities`、`policy` 为 HostPolicy 允许集。
2. THE CapabilityNegotiator SHALL 计算 `denied = requested \ granted`。
3. WHEN 宿主提供受限能力集（`available` 或 `policy` 为 `requested` 的真子集），THE CapabilityNegotiator SHALL 在 NegotiationResult 中如实反映被裁剪掉的能力（出现在 `denied` 中）。
4. THE 能力裁剪结果 SHALL 仅依赖 `(requested, available, policy)` 三个集合，与适配器其它状态无关（纯函数、确定性）。

### Requirement 6: 必需能力缺失的拒绝

**User Story:** 作为 Fay 应用使用者，我希望当 BuF 声明的必需能力在当前宿主完全不可用时被明确拒绝，以便不在缺能力的环境中误启动。

#### Acceptance Criteria

1. WHEN BuF 声明一组必需能力 `required` 且 `required \ granted ≠ ∅`，THE Adapter_Layer SHALL 拒绝进入可运行状态并返回错误码 `ADP_CAPABILITY_DENIED`，错误上下文 `missing` 等于 `required \ granted`。
2. WHEN `required ⊆ granted`，THE Adapter_Layer SHALL 允许进入可运行状态。
3. THE 必需能力判定 SHALL 基于 Requirement 5 计算出的 `granted`，不引入额外能力来源。

### Requirement 7: 指令派发与翻译

**User Story:** 作为运行层，我希望把 Universal_Instruction 交给适配层，由它做能力检查后翻译为宿主调用，以便新增宿主无需改动运行层。

#### Acceptance Criteria

1. WHEN AdapterRegistry 处于可服务状态并收到一条 Universal_Instruction，THE Adapter_Layer SHALL 先校验该指令的 `capabilities_required ⊆ granted`。
2. IF 指令的 `capabilities_required \ granted ≠ ∅`，THEN THE Adapter_Layer SHALL 返回错误码 `ADP_CAPABILITY_DENIED`，错误上下文 `missing` 等于该差集，且不调用适配器翻译。
3. WHEN 指令通过能力检查且其 InstructionKey 在当前适配器 `supported_instructions` 中，THE Adapter_Layer SHALL 调用适配器翻译并返回 HostSyscall。
4. IF 指令通过能力检查但其 InstructionKey 不在当前适配器 `supported_instructions` 中，THEN THE Adapter_Layer SHALL 返回 `ADP_UNSUPPORTED_INSTRUCTION` 且不产生任何 HostSyscall。
5. THE Adapter_Layer 在 AdapterRegistry 未进入可服务状态时收到派发请求 SHALL 返回 `ADP_NO_MATCHING_PLATFORM`。

### Requirement 8: 跨适配器行为一致性

**User Story:** 作为可移植性维护者，我希望同一指令序列在两个能力相同的适配器上得到一致的处理结果，以便 BuF 行为可移植。

#### Acceptance Criteria

1. WHERE 两个 Platform_Adapter 声明相同的 `supported_capabilities` 与相同的 `supported_instructions`，WHEN 同一条 Universal_Instruction 分别经两者派发，THE Adapter_Layer SHALL 对两者产生一致的接受/拒绝判定（同样通过、或同样返回相同错误码）。
2. WHERE 上述两适配器对受支持指令的翻译均为确定性，WHEN 同一指令序列分别经两者翻译，THE 产生的 HostSyscall 序列 SHALL 在结构上可比较且其接受/拒绝序列一致。

### Requirement 9: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M3 新增的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M3 错误码集合 SHALL 在既有码之外至少新增 `ADP_NO_MATCHING_PLATFORM`、`ADP_UNSUPPORTED_INSTRUCTION`、`ADP_CAPABILITY_DENIED`、`ADP_HOST_CALL_FAILED` 四个稳定值。
2. THE M3 新增错误 SHALL 复用统一的 `FaygerError` 结构，`source_layer` 为 `Adapter`。
3. WHEN M3 各组件产生错误，THE 错误对象 SHALL 在 `context` 中提供足以定位失败的结构化字段（如 `category`、`opcode`、`missing`、`platform_id`）。
4. THE M0/M1/M2 既有错误码 SHALL 保持语义不变，M3 不复用或重定义它们。

### Requirement 10: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M3 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M3 实现 SHALL 至少包含以下属性测试：适配器选择决策性（无匹配→错误、有匹配→确定性选中）、不支持指令处理（未声明指令→`ADP_UNSUPPORTED_INSTRUCTION` 且不翻译）、能力裁剪集合代数（granted/denied 满足集合等式）、必需能力缺失拒绝、跨适配器行为一致性。
2. THE M3 实现 SHALL 提供至少一个端到端用例：构造一个 HostEnvironment → 选中内置适配器 → 对一组指令做能力裁剪与派发 → 断言通过/拒绝/翻译结果符合预期。
3. THE M3 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M3 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0/M1/M2 既有测试在 M3 引入后仍全部通过。

### Requirement 11: M3 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M3 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M3 内置适配器的 `translate` SHALL NOT 调用真实操作系统系统调用、真实浏览器 Web API 或真实宿主 FFI；其翻译产出是确定性的 HostSyscall 数据结构（mock），真实绑定在 `release/dev/adapters/` 后续工作中完成。
2. THE M3 实现 SHALL NOT 引入资源监测、配额或 `RT_QUOTA_EXCEEDED`（这是 M7）。
3. THE M3 实现 SHALL NOT 引入可观测性事件总线或跨层 `cause` 错误链（这是 M8）；M3 的错误 `cause` 可为 None。
4. THE M3 实现 SHALL NOT 修改 M0/M1/M2 已固化的 BuF 字节布局、加载链、运行层契约或既有错误码语义；M3 仅新增适配层组件与适配层错误码。
5. THE M3 实现 SHALL NOT 强制要求 fayger-runtime 依赖 fayger-adapter；运行层与适配层通过各自的 Universal_Instruction 数据形态解耦，二者的统一桥接留待后续集成里程碑。

## 约束与假设

- **目标语言**：M3 仅交付 Rust 实现。
- **crate 归属**：M3 的适配层逻辑落在新的 `fayger-adapter` crate（依赖 `fayger-error`；可选依赖 `fayger-buf` 以复用 Capability 字符串约定，但不强制）。
- **新增依赖**：M3 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M3 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
- **能力与指令的数据化建模**：适配器以数据描述（`supported_capabilities` / `supported_instructions`）声明自身能力，使适配器选择、不支持指令判定与能力裁剪都成为可被属性测试验证的集合运算。
