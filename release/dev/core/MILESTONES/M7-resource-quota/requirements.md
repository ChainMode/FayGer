# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M7 里程碑**需求文档。M7 在 M2（运行层骨架）之上落实蓝图运行层的**资源隔离、配额监测与故障隔离**（Req 7.x）：

- **运行时数据区隔离**：每个 BuF_Instance 维护独立的运行时数据区，不同实例之间默认不共享内存（不共享可变状态）。
- **配额声明与监测**：当 BuF_Manifest 声明 CPU / 内存 / I/O 配额时，运行层在执行期间对该实例的资源使用量进行监测。
- **超限暂停**：当某实例的资源使用量超出声明配额，运行层暂停该实例并报告超限的资源类型与当前用量。
- **故障隔离**：当某个实例进入 `Failed`，运行层保证其他处于 `Running` 的实例不受影响。

M7 **不**交付：真实 OS 级 cgroups / 线程级 CPU 计量（M7 的"资源用量"是由调用方注入的确定性采样模型，便于可测；真实计量绑定留待后续）；可观测性事件总线 / 跨层错误链（M8）；运行层与适配层的统一桥接。M7 在 M2 的生命周期状态机之上叠加"配额超限→暂停"这一新的自动迁移触发源。

完成 M7 后，运行层能在多实例并存时保证彼此隔离、按声明配额监测并自动暂停越界实例、并把单实例故障限制在自身——即"宿主稳定性可控"的最小可证形态。

## Glossary

- **RuntimeDataArea**：单个 BuF_Instance 的运行时数据区。M7 中至少包含该实例独立持有的可变运行状态（程序计数器、最后指令、失败信息、资源用量记录），不同实例之间不共享其可变引用。
- **ResourceQuota**：BuF_Manifest 中声明的资源配额（M0 已定义），含 `cpu_ms_per_sec`、`memory_bytes`、`io_bytes_per_sec`，均为可选。
- **ResourceKind**：资源类型枚举：`Cpu`（对应 cpu_ms_per_sec）、`Memory`（对应 memory_bytes）、`Io`（对应 io_bytes_per_sec）。
- **ResourceUsage**：某次采样的资源用量快照，含 `cpu_ms_per_sec`、`memory_bytes`、`io_bytes_per_sec` 三个 u64 维度（实际用量）。
- **ResourceMonitor**：运行层的配额监测组件。为实例附加配额，对注入的用量采样判定是否超限。
- **QuotaExceeded**：超限判定结果，含首次超限的 `ResourceKind` 与触发超限的 `ResourceUsage`。
- **采样注入（Usage Injection）**：M7 的资源用量由调用方通过 `observe_usage(id, usage)` 注入（确定性、可测），运行层据此判定超限。真实 OS 计量是后续工作。

## Requirements

### Requirement 1: 运行时数据区隔离

**User Story:** 作为宿主管理员，我希望每个 BuF_Instance 的运行时数据彼此独立，以便一个实例不会篡改另一个实例的状态。

#### Acceptance Criteria

1. THE Runtime_Layer SHALL 为每一个 BuF_Instance 维护独立的运行时数据区（独立的程序计数器、最后指令、失败信息与资源用量记录）。
2. WHEN 对某个 BuF_Instance 的运行时数据区执行任意写操作（推进执行、注入故障、注入用量、生命周期迁移），THE Runtime_Layer SHALL NOT 改变任何其它 BuF_Instance 的运行时数据区。
3. WHEN 同一 Runtime 加载多个 BuFObject，THE Runtime_Layer SHALL 为每个实例分配互不相同的 `InstanceId`，且默认不在实例之间共享可变运行状态。

### Requirement 2: 配额附加与查询

**User Story:** 作为 Fay 应用的发布者，我希望在 BuF_Manifest 中声明的配额被运行层识别并用于监测，以便我能约束实例的资源占用。

#### Acceptance Criteria

1. WHEN Runtime_Layer 加载一个 BuFObject，THE Runtime_Layer SHALL 从 `manifest.quotas` 读取该实例的 ResourceQuota（可能为 None，表示无配额）。
2. THE Runtime_Layer SHALL 暴露查询接口，返回某实例当前生效的 ResourceQuota（或 None）。
3. WHERE BuF_Manifest 未声明 quotas，THE Runtime_Layer SHALL 把该实例视为无配额约束（任何用量都不触发超限）。
4. WHERE BuF_Manifest 声明了 quotas 中的部分维度（如仅 `memory_bytes`），THE Runtime_Layer SHALL 仅对已声明的维度进行超限判定，未声明维度不触发超限。

### Requirement 3: 资源用量监测

**User Story:** 作为运维人员，我希望运行层在执行期间监测实例的资源用量，以便发现越界。

#### Acceptance Criteria

1. THE Runtime_Layer SHALL 暴露注入资源用量采样的接口 `observe_usage(id, usage)`，记录该实例的最近用量。
2. WHEN 对一个存在的实例注入用量采样，THE Runtime_Layer SHALL 基于该实例当前生效的配额判定本次采样是否超限。
3. THE 超限判定 SHALL 定义为：对配额声明的每个维度 `d`，若该维度配额为 `Some(limit)` 且本次采样 `usage[d] > limit`，则维度 `d` 超限；任一维度超限即本次采样超限。
4. WHEN 对一个不存在的实例注入用量，THE Runtime_Layer SHALL 返回错误码 `RT_INSTANCE_NOT_FOUND`。

### Requirement 4: 超限暂停

**User Story:** 作为宿主管理员，我希望越界实例被自动暂停并报告超限信息，以便保护宿主稳定。

#### Acceptance Criteria

1. WHEN 注入的用量采样使一个处于 `Running` 状态的实例超限，THE Runtime_Layer SHALL 将该实例状态迁移至 `Suspended`。
2. WHEN 因超限触发暂停，THE Runtime_Layer SHALL 提供查询接口，返回该次超限的 QuotaExceeded（含首次超限的 ResourceKind 与触发的 ResourceUsage）。
3. WHEN 一次采样在多个维度同时超限，THE QuotaExceeded 报告的 `ResourceKind` SHALL 为按固定顺序（Cpu、Memory、Io）的第一个超限维度（确定性）。
4. WHILE 实例处于 `Running` 之外的状态（如 `Suspended` / `Initialized` / 终态），WHEN 注入超限用量，THE Runtime_Layer SHALL NOT 触发新的超限暂停迁移（不在非 Running 状态下自动迁移），但 SHALL 仍记录最近用量。
5. WHEN 注入的用量采样未超限，THE Runtime_Layer SHALL 保持实例状态不变并清空（或不设置）该实例的 QuotaExceeded 记录。

### Requirement 5: 故障隔离

**User Story:** 作为宿主管理员，我希望一个实例失败不影响其它正在运行的实例，以便单点故障不扩散。

#### Acceptance Criteria

1. WHEN 某个 BuF_Instance 进入 `Failed`（注入未处理错误），THE Runtime_Layer SHALL 保证其它处于 `Running` 的实例状态不变。
2. WHEN 某个实例因超限被暂停，THE Runtime_Layer SHALL 保证其它实例的状态与资源记录不变。
3. WHEN 多个实例并存，对任一实例的失败 / 暂停 / 用量注入操作 SHALL NOT 改变其它实例的程序计数器、最后指令、失败信息或资源用量记录。
4. WHEN 某实例进入 `Failed` 后继续对其它 `Running` 实例 `step`，THE 其它实例 SHALL 正常推进执行，其派发的指令序列与无故障基线一致。

### Requirement 6: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M7 新增的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M7 错误码集合 SHALL 在既有码之外至少新增 `RT_QUOTA_EXCEEDED` 一个稳定值。
2. THE M7 新增错误 SHALL 复用统一的 `FaygerError` 结构，`source_layer` 为 `Runtime`。
3. WHEN M7 各组件产生与超限相关的可查询信息，THE 该信息 SHALL 在结构化字段中提供足以定位的内容（如 `resource`、`usage` 维度值、`instance_id`）。
4. THE M0–M6 既有错误码 SHALL 保持语义不变，M7 不复用或重定义它们。

### Requirement 7: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M7 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M7 实现 SHALL 至少包含以下属性测试：运行时数据区隔离（对一个实例的写不改变另一个）、配额超限触发暂停（首次超限维度确定性）、未超限不暂停、无配额不触发、故障隔离（一个 Failed 不影响其它 Running 的状态与指令序列）。
2. THE M7 实现 SHALL 提供至少一个端到端用例：加载两个带不同配额的实例，对其一注入超限用量使其暂停、对其一注入正常用量并继续执行，断言两者状态与指令序列互不影响。
3. THE M7 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M7 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0–M6 既有测试在 M7 引入后仍全部通过。

### Requirement 8: M7 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M7 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M7 实现 SHALL NOT 进行真实 OS 级资源计量（cgroups / 线程 CPU 时间 / RSS 采样）；用量由调用方通过 `observe_usage` 注入（确定性、可测）。
2. THE M7 实现 SHALL NOT 引入可观测性事件总线或跨层 `cause` 错误链（M8）；M7 的超限信息通过实例查询接口暴露，错误 `cause` 可为 None。
3. THE M7 实现 SHALL NOT 引入跨实例共享内存或 IPC（蓝图明确为第一阶段非目标）；隔离即"默认不共享可变状态"。
4. THE M7 实现 SHALL NOT 修改 M0–M6 已固化的 BuF 字节布局、加载链、适配层契约或既有错误码语义；M7 仅在运行层新增资源监测组件与运行层错误码。
5. THE M7 的超限暂停 SHALL 复用 M2 的生命周期状态机（`Running → Suspended` 是 M2 已有的合法迁移），不引入新的生命周期状态。
6. THE M7 实现 SHALL NOT 在非 `Running` 状态下因用量超限而自动迁移状态。

## 约束与假设

- **目标语言**：M7 仅交付 Rust 实现。
- **crate 归属**：M7 的资源监测落在 `fayger-runtime` crate（扩展 `Runtime` 与 `InstanceData`，新增 `resource` 模块）。
- **新增依赖**：M7 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M7 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
- **隔离的可测建模**：运行时数据区隔离以"每实例独立的 InstanceData + 不共享可变引用"实现；属性测试通过"对 A 的操作不改变 B 的可观测状态"验证，而非依赖 OS 内存隔离。
