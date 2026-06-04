# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M8 里程碑**需求文档。M8 落实蓝图第 6 章的两条横切关注点（cross-cutting wires）：**可观测性事件总线**（§6.5–§6.8）与**跨层错误因果链**（§6.2–§6.3）。

- **可观测性事件总线（EventBus）**：提供 `publish` / `subscribe` / `set_debug_enabled` 的最小事件总线，承载五类事件（Lifecycle / Quota / Loader / Adapter / Debug），订阅者按过滤器接收事件。Debug 级事件默认关闭，显式开启后才投递。
- **生命周期事件序列一致性**：对任意合法迁移序列，发出的事件序列与之等长、逐项 `from`/`to` 对齐、时间戳单调非减（蓝图 §6.5）。
- **跨层错误因果链**：在统一 `FaygerError` 之上提供链遍历能力（`chain` / `layer_chain` / `root_cause` / `depth`）与逐层包装构造器（`wrap`）；上层包装下层错误时把下层错误置入 `cause` 且不重写其 `source_layer`（蓝图 §6.2）。沿 `cause` 遍历得到的 `source_layer` 序列即"由外到内"的实际产生顺序（蓝图 §6.3）。
- **运行层事件接入**：`Runtime` 在生命周期迁移、注入故障、注入资源用量/超限暂停时向其内置 EventBus 发出对应事件（加性集成，默认无订阅者时不改变既有可观测状态）。

M8 **不**交付：真实时钟绑定（事件时间戳为确定性逻辑时钟，便于可测；真实 wall-clock / 外部 trace 对齐留待后续）；宿主日志/trace 通道绑定（§6.6，仅保留映射等级的语义占位，不绑定具体宿主通道）；性能事件（`ExecutionMetric` / `TierTransition` 属第二阶段）；异步/多线程投递（M8 为同步、单线程、确定性投递）。

完成 M8 后，Fayger 三层的失败可经统一错误链由外到内定位，运行过程可经事件总线被订阅观测，且这两条横切语义都以可被 PBT 验证的形式表达。

## Glossary

- **EventBus**：可观测性事件总线。维护逻辑时钟、调试开关与订阅表，提供发布/订阅/调试开关。
- **FaygerEvent**：一条已成形的事件，含 `category`、`level`、`timestamp`（逻辑时钟）、`instance_id`（可选）与结构化 `payload`。
- **EventDraft**：发布前的事件草稿（不含 timestamp）；EventBus 在 `publish` 时为其分配逻辑时钟时间戳形成 `FaygerEvent`。
- **EventCategory**：事件类别枚举：`Lifecycle` / `Quota` / `Loader` / `Adapter` / `Debug`。
- **EventLevel**：事件等级：`Debug` < `Info` < `Warn` < `Error`（全序）。
- **EventPayload**：类型化事件负载（Lifecycle 含 `from`/`to`；Quota 含资源维度与是否暂停；Loader 含 `phase`/`status`；Adapter / Debug 含 `detail`）。
- **PhaseStatus**：Loader 阶段状态：`Enter` / `Complete` / `Fail`。
- **EventFilter**：订阅过滤器，可按类别集合、最低等级、实例 ID 过滤。
- **SubscriptionId**：订阅句柄，用于 `unsubscribe`。
- **逻辑时钟（Logical Clock）**：EventBus 内部单调递增计数器。每次成功投递的发布令其递增，作为事件 `timestamp`，保证单调非减（确定性、可测）。
- **错误因果链（Cause Chain）**：`FaygerError.cause` 串起的链。`chain()` 自外向内遍历；`layer_chain()` 给出对应的 `source_layer` 序列。

## Requirements

### Requirement 1: 事件总线发布与时间戳

**User Story:** 作为运维人员，我希望运行过程产生的事件能被统一发布并带有单调递增的时间戳，以便我按时间顺序观测系统行为。

#### Acceptance Criteria

1. THE EventBus SHALL 暴露 `publish(draft)` 接口，把一个不含时间戳的 EventDraft 成形为带时间戳的 FaygerEvent。
2. WHEN 一次发布被实际投递，THE EventBus SHALL 为该事件分配当前逻辑时钟值作为 `timestamp`，随后令逻辑时钟递增。
3. THE 任意两条被投递事件 `eᵢ`、`eⱼ`（`i` 先于 `j` 发布）SHALL 满足 `eᵢ.timestamp <= eⱼ.timestamp`（单调非减）。
4. THE EventBus 的发布与投递 SHALL 为同步、确定性的（同一发布序列与订阅集合产生同一投递序列），不依赖真实时钟、线程或外部 I/O。

### Requirement 2: 订阅与过滤

**User Story:** 作为诊断工具的开发者，我希望只接收我关心的事件，以便聚焦特定实例或类别。

#### Acceptance Criteria

1. THE EventBus SHALL 暴露 `subscribe(filter, handler)` 接口并返回唯一的 SubscriptionId。
2. WHEN 一条事件被发布且通过某订阅的 EventFilter，THE EventBus SHALL 把该事件投递给该订阅的 handler。
3. WHEN 一条事件被发布但未通过某订阅的 EventFilter，THE EventBus SHALL NOT 把该事件投递给该订阅的 handler。
4. THE EventFilter SHALL 至少支持按类别集合（None 表示全部）、最低等级（低于该等级不投递）与实例 ID（None 表示任意）过滤。
5. THE EventBus SHALL 暴露 `unsubscribe(id)` 接口；移除后该订阅 SHALL NOT 再接收任何事件，且其余订阅不受影响。
6. WHEN 多个订阅匹配同一事件，THE EventBus SHALL 按订阅注册顺序确定性地投递。

### Requirement 3: 调试事件开关

**User Story:** 作为开发者，我希望默认不被海量调试事件淹没，但需要深度诊断时能显式开启，以便控制可观测性开销。

#### Acceptance Criteria

1. THE EventBus 的调试事件投递 SHALL 默认关闭。
2. WHILE 调试开关为关闭，WHEN 发布一条 `Debug` 类别事件，THE EventBus SHALL NOT 投递它，且 SHALL NOT 递增逻辑时钟。
3. WHEN 调用 `set_debug_enabled(true)` 之后发布 `Debug` 类别事件，THE EventBus SHALL 按过滤规则正常投递它。
4. THE 调试开关 SHALL NOT 影响非 `Debug` 类别事件的投递。

### Requirement 4: 生命周期事件序列一致性

**User Story:** 作为可观测性使用者，我希望发出的生命周期事件忠实反映状态迁移序列，以便据此重建实例的状态历史。

#### Acceptance Criteria

1. WHEN 一个实例发生一次生命周期迁移 `(from, to)`，THE 运行层 SHALL 发布一条 `Lifecycle` 事件，其 `from`、`to` 分别等于迁移的源与目标状态。
2. THE 对任意合法迁移序列 `[(s₀,s₁), (s₁,s₂), …]` 发出的 `Lifecycle` 事件序列 SHALL 与之等长。
3. THE 第 `i` 条 `Lifecycle` 事件 SHALL 满足 `eᵢ.from == sᵢ₋₁` 且 `eᵢ.to == sᵢ`。
4. THE 发出的 `Lifecycle` 事件序列的 `timestamp` SHALL 单调非减。

### Requirement 5: 跨层错误因果链

**User Story:** 作为 Fayger 调用方，我希望沿错误的 `cause` 链由外到内定位失败的产生层序，以便快速判断根因在哪一层。

#### Acceptance Criteria

1. THE FaygerError SHALL 暴露 `chain()`，自身在前、随后逐层 `cause`，遍历整条因果链。
2. THE FaygerError SHALL 暴露 `layer_chain()`，返回沿 `chain()` 的 `source_layer` 序列（由外到内）。
3. THE FaygerError SHALL 暴露 `root_cause()`（链尾元素）与 `depth()`（链长，至少为 1）。
4. WHEN 上层用 `wrap(code, layer, message, cause)` 包装一个下层错误，THE 结果 SHALL 把下层错误置于 `cause`，且 SHALL NOT 改写下层错误的 `source_layer`、`code` 或 `context`。
5. THE 对任意由 `n` 个错误逐层包装而成的链，`depth()` SHALL 等于 `n`，且 `layer_chain()` SHALL 等于由外到内每层 `source_layer` 的序列。

### Requirement 6: 运行层事件接入

**User Story:** 作为运维人员，我希望运行层在关键时刻自动发出事件，以便无需侵入业务即可观测。

#### Acceptance Criteria

1. THE Runtime SHALL 内置一个 EventBus，并暴露对其的只读与可变访问接口。
2. WHEN Runtime 成功执行一次生命周期迁移（init/start/suspend/resume/terminate），THE Runtime SHALL 发布对应的 `Lifecycle` 事件。
3. WHEN Runtime 因注入故障使实例进入 `Failed`，THE Runtime SHALL 发布一条由原状态到 `Failed` 的 `Lifecycle` 事件。
4. WHEN Runtime 注入资源用量采样，THE Runtime SHALL 发布一条 `Quota` 事件；WHEN 该采样使 `Running` 实例超限并暂停，THE Runtime SHALL 额外发布对应的 `Lifecycle`（`Running`→`Suspended`）事件。
5. WHERE 无订阅者，THE Runtime 的事件发布 SHALL NOT 改变 M2–M7 任何既有可观测行为（状态、指令派发序列、查询结果均不变）。

### Requirement 7: 错误模型与契约稳定性

**User Story:** 作为 Fayger 项目的维护者，我希望 M8 不破坏既有契约。

#### Acceptance Criteria

1. THE M8 SHALL 复用统一的 `FaygerError` 结构；M8 的错误链能力为既有结构的加性扩展（新增方法），不改变既有字段语义。
2. THE M0–M7 既有错误码 SHALL 保持语义不变，M8 不复用或重定义它们；M8 不要求新增错误码。
3. THE M8 SHALL NOT 修改 M0–M7 已固化的 BuF 字节布局、加载链、适配层契约或运行层既有方法的返回语义。
4. THE 运行层对 EventBus 的集成 SHALL 为加性（新增字段与访问接口、在既有方法中追加事件发布副作用），且默认不改变既有方法的返回值与状态迁移结果。

### Requirement 8: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M8 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M8 实现 SHALL 至少包含以下属性测试：生命周期事件序列一致性、订阅过滤正确性、调试开关门控、错误因果链顺序保持、取消订阅后不再接收且其余订阅不受影响。
2. THE M8 实现 SHALL 提供至少一个端到端用例：用真实 `Runtime` 驱动一个完整生命周期并注入一次超限暂停，订阅事件总线，断言发出的生命周期事件序列与迁移序列一致、且发出了 `Quota` 事件。
3. THE M8 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M8 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0–M7 既有测试在 M8 引入后仍全部通过。

### Requirement 9: M8 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M8 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M8 实现 SHALL NOT 绑定真实时钟；事件 `timestamp` 为确定性逻辑时钟（单调非减），真实 wall-clock / 外部 trace 对齐留待后续。
2. THE M8 实现 SHALL NOT 绑定具体宿主日志/trace 通道（§6.6）；仅保留事件等级（debug/info/warn/error）的语义，通道映射留待后续。
3. THE M8 实现 SHALL NOT 引入异步或多线程事件投递；投递为同步、单线程、确定性。
4. THE M8 实现 SHALL NOT 引入性能事件类别（`ExecutionMetric` / `TierTransition` 属第二阶段）。
5. THE M8 的可观测性总线 SHALL NOT 持久化历史事件；事件在发布时投递给当时已注册的订阅者，未投递即不保留。

## 约束与假设

- **目标语言**：M8 仅交付 Rust 实现。
- **crate 归属**：M8 新增 `fayger-observability` crate（EventBus 与事件类型）；错误链能力扩展 `fayger-error`；运行层事件接入扩展 `fayger-runtime`。
- **依赖方向**：`fayger-observability` 不依赖 `fayger-runtime` / `fayger-adapter` / `fayger-loader`（避免环）；`fayger-runtime` 加性依赖 `fayger-observability`。
- **新增依赖**：M8 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency（`fayger-error` 新增 `proptest` 为 dev-dependency 以承载错误链 PBT）。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义；M8 不要求新增错误码。
- **可测建模**：事件时间戳以逻辑时钟建模（确定性）；宿主通道与真实时钟为后续工作。
