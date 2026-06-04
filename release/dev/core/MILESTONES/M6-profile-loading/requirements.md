# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M6 里程碑**需求文档。M6 在 M5（BuF_Source + Eager/Lazy）之上引入**按运行环境的部分加载（Profile-Driven Partial Loading）**，对应蓝图加载链中的 `Select(Sections by LoadProfile)` 阶段：

- **LoadProfile**：加载方提供的运行环境画像，至少包含目标终端类别、可用能力集、单段体积上限与运行时特征位。
- **Section 选择决策**：基于 LoadProfile 与每个 SectionDescriptor 的 `profile_constraints`，确定性地决定哪些 Section 被选中、哪些被跳过。`Required` Section 不满足约束时整体加载失败；`Optional` Section 不满足时跳过并记录原因。
- **部分加载结果**：被跳过的 Optional Section 不被读取至运行时数据区；加载结果记录被跳过的 Section_Id 列表与每个 Section 的跳过原因。
- **部分加载下的签名/摘要一致性**：部分加载完成后，BuF 的整体 Manifest 签名仍可校验通过（签名作用域不含 Section 段体）；每个被实际加载的 Section 满足自身 digest 校验。

M6 **不**交付：资源监测 / 配额（M7）；可观测性事件总线 / 跨层错误链（M8）；运行层 / 适配层改动。M6 在 M5 的 Eager / Lazy 两种策略之上叠加 profile 选择——被选中 Section 集合从"全部"收窄为"profile 命中的子集"。

完成 M6 后，同一份 BuF 在加载时可仅读取与当前运行环境相关的 Section 子集——即"BuF 单一来源、按终端资源约束差异化加载"的最小可证形态（蓝图针对无人机 / 摄像头等性能受限终端的核心诉求）。

## Glossary

- **LoadProfile**：运行环境画像。至少包含 `target_class`（目标终端类别）、`capabilities`（当前终端可用能力集）、`max_section_bytes`（单段允许读入的体积上限，可选）、`features`（运行时特征位集合）。
- **TargetClass**：目标终端类别（如 `desktop` / `server` / `browser` / `inapp` / `embedded` / `drone` / `camera`）。M6 以字符串建模，保持开放。
- **ProfileConstraints**：SectionDescriptor 上的 profile 约束（M0 已定义），含 `required_capabilities`、`max_size`、`required_features`。
- **SectionVisibility**：Section 可见性（M0 已定义）：`Required`（缺失即加载失败）/ `Optional`（不满足时跳过降级）。
- **fits(section, profile)**：选择谓词。当且仅当 `profile` 满足该 Section 的全部 `profile_constraints` 时为真（见 Requirement 2）。
- **Selected Section（被选中 Section）**：`fits` 为真的 Section 集合。
- **SkipRecord**：被跳过的 Optional Section 的记录，含 `section_id` 与非空的跳过原因集合 `reasons`。
- **SkipReason**：跳过原因枚举，至少包含 `CapabilityNotGranted`、`OverMaxSize`、`FeatureMissing`。
- **SelectionResult**：选择结果，含被选中 Section 的有序 id 列表 `selected` 与被跳过记录列表 `skipped`。
- **PartialArtifact**：M6 部分加载的产出物（在 M5 的 LoadedArtifact 基础上携带 `selected` / `skipped` 信息）。

## Requirements

### Requirement 1: LoadProfile 模型

**User Story:** 作为性能受限终端的集成方，我希望以一个运行环境画像驱动加载，以便 Fayger 据此裁剪 Section。

#### Acceptance Criteria

1. THE M6 实现 SHALL 定义 LoadProfile，至少包含 `target_class`（字符串）、`capabilities`（字符串集合）、`max_section_bytes`（可选 u64）、`features`（字符串集合）。
2. THE LoadProfile SHALL 可由调用方在加载时提供，与 LoadStrategy（Eager/Lazy）正交组合。
3. WHEN 调用方未提供任何约束相关字段（capabilities/features 为空、max_section_bytes 为 None），THE LoadProfile SHALL 表示"无能力/特征/体积约束"的最宽松画像。

### Requirement 2: Section 选择谓词

**User Story:** 作为 BuF 的发布方，我希望每个 Section 的 profile 约束以明确、确定的规则与 LoadProfile 匹配，以便选择结果可预期。

#### Acceptance Criteria

1. THE 选择谓词 fits(section, profile) SHALL 当且仅当下列全部条件成立时为真：
   - `section.profile_constraints.required_capabilities ⊆ profile.capabilities`；
   - `section.profile_constraints.required_features ⊆ profile.features`；
   - 若 `section.profile_constraints.max_size` 为 `Some(m)` 则 `section.length ≤ m`；且若 `profile.max_section_bytes` 为 `Some(p)` 则 `section.length ≤ p`。
2. WHEN section 的 `profile_constraints` 为默认空（无 required_capabilities / required_features / max_size），且 profile 的 max_section_bytes 为 None，THE fits SHALL 为真（无约束的 Section 总是被选中）。
3. THE fits 谓词 SHALL 是确定性的纯函数：对相同 `(section, profile)` 输入恒返回相同结果。
4. WHEN fits 为假，THE 选择逻辑 SHALL 能产出非空的跳过原因集合，反映哪一类约束未满足（`CapabilityNotGranted` / `FeatureMissing` / `OverMaxSize`）。

### Requirement 3: Required / Optional 选择决策

**User Story:** 作为 Fay 应用使用者，我希望必需 Section 缺失时整体加载失败、可选 Section 缺失时优雅降级，以便加载行为安全且可用。

#### Acceptance Criteria

1. THE 选择逻辑 SHALL 把被选中集合定义为 `selected = { s ∈ manifest.sections : fits(s, profile) }`。
2. IF 存在某 Section `s` 使 `fits(s, profile)` 为假且 `s.visibility == Required`，THEN THE 加载 SHALL 失败并返回错误码 `LDR_PROFILE_REQUIRED_SECTION_MISSING`，错误上下文至少包含 `section_id` 与未满足的约束项列表。
3. WHEN 某 Section `s` 使 `fits(s, profile)` 为假且 `s.visibility == Optional`，THE 选择逻辑 SHALL 把 `s.id` 加入 `skipped`，且其 `reasons` 非空。
4. THE 选择结果 SHALL 是确定性的：对相同 `(manifest, profile)`，`selected` 与 `skipped` 完全相同（含顺序）。
5. WHEN 多个 Required Section 都不满足约束，THE 加载 SHALL 失败并报告其中按 manifest 顺序的第一个（确定性）。

### Requirement 4: 部分加载与被跳过 Section

**User Story:** 作为带宽/存储受限终端的集成方，我希望被跳过的 Section 完全不被读取，以便节省资源。

#### Acceptance Criteria

1. WHEN profile 驱动的部分加载完成，THE 被选中 Section SHALL 按 M5 的 Eager / Lazy 策略加载（Eager 立即读取并校验、Lazy 首访读取并校验）。
2. THE 被跳过的 Optional Section SHALL NOT 被读取至运行时数据区（Eager 模式下不读取其段体，Lazy 模式下其不进入可访问 Section 集合）。
3. THE PartialArtifact SHALL 暴露被选中 Section 的 id 列表与被跳过记录（`skipped`：每条含 `section_id` 与非空 `reasons`）。
4. WHEN 调用方访问一个被跳过的 Section id，THE PartialArtifact SHALL 返回错误码 `LDR_PARSE_FAIL`（reason 为 `SectionSkipped`）或等价的"该 Section 未被选中"错误，而非读取它。
5. THE 被选中 Section 的内容 SHALL 与不带 profile（全选）加载在相同 Section 上逐字节一致（profile 只影响"选不选"，不影响"选中后内容"）。

### Requirement 5: 部分加载下的摘要与签名一致性

**User Story:** 作为分发方，我希望部分加载不破坏 BuF 的整体签名与各段摘要校验，以便部分加载的制品仍可信。

#### Acceptance Criteria

1. THE 部分加载 SHALL 对每个被实际加载（被选中且已读取）的 Section 执行其在 Manifest 中声明的 digest 校验（Eager 立即、Lazy 首访）。
2. WHERE BuF 携带有效的整体 Manifest 签名，WHEN 以任意两个不同的 LoadProfile（可能选中不同 Section 子集）部分加载，THE 签名校验结果 SHALL 相同（均通过）——因为签名作用域为 `Header || Manifest_without_signature || Section_Index`，与被选中 Section 子集无关。
3. WHERE BuF 的整体 Manifest 签名无效，WHEN 以任意 LoadProfile 部分加载，THE 签名校验 SHALL 失败，与被选中的 Section 子集无关。
4. WHEN 对被跳过 Section 的段体做任意修改（不更新 Section_Index 与签名），THE 整体 Manifest 签名校验结果 SHALL 不变（被跳过段不在签名作用域内）。
5. WHEN 对 Section_Index 做任意修改，THE 整体 Manifest 签名校验结果 SHALL 改变（Section_Index 在签名作用域内）。

### Requirement 6: 与加载链集成

**User Story:** 作为 Fayger 调用方，我希望 profile 选择自动成为加载链的一环，以便无需自行串联。

#### Acceptance Criteria

1. THE M6 实现 SHALL 提供携带 LoadProfile 与 LoadStrategy 的加载入口（可选携带 TrustStore 以同时做签名校验），按顺序执行可信骨架读取与校验 → Select(Sections by LoadProfile) → 被选中 Section 的读取与校验（按策略）。
2. THE M6 加载入口 SHALL NOT 改变 M5 既有 `load_from_source(source, strategy)` 的行为（该入口等价于"全选"profile）；M6 通过新入口提供 profile 选择。
3. WHEN profile 选择阶段失败（Required 缺失），THE 加载 SHALL 在被选中 Section 读取之前返回错误（即不读取任何被选中 Section 段体就失败）。
4. THE 选择阶段错误 SHALL 标注 `context.phase` 为 `eager`（选择发生在 load 调用栈内，无论后续策略）。

### Requirement 7: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M6 新增的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M6 错误码集合 SHALL 在既有码之外至少新增 `LDR_PROFILE_REQUIRED_SECTION_MISSING` 一个稳定值。
2. THE M6 新增错误 SHALL 复用统一的 `FaygerError` 结构，`source_layer` 为 `Loader`。
3. WHEN M6 各组件产生错误，THE 错误对象 SHALL 在 `context` 中提供足以定位失败的结构化字段（如 `section_id`、未满足的约束项列表、`reason`、`phase`）。
4. THE M0–M5 既有错误码 SHALL 保持语义不变，M6 不复用或重定义它们。

### Requirement 8: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M6 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M6 实现 SHALL 至少包含以下属性测试：Section 选择决策性（selected/skipped 满足谓词与确定性）、Required 缺失拒绝、Optional 跳过记录原因、部分加载下签名一致性（不同 profile 子集签名结果相同）、被选中 Section 内容与全选一致。
2. THE M6 实现 SHALL 提供至少一个端到端用例：构造一个含 Required + Optional Section 且带 profile 约束的 BuF，用满足/不满足约束的两个 profile 分别加载，断言选中/跳过/失败行为与签名一致。
3. THE M6 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M6 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0–M5 既有测试在 M6 引入后仍全部通过。

### Requirement 9: M6 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M6 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M6 实现 SHALL NOT 引入资源监测、配额或 `RT_QUOTA_EXCEEDED`（M7）。
2. THE M6 实现 SHALL NOT 引入可观测性事件总线或跨层 `cause` 错误链（M8）；M6 错误 `cause` 可为 None。
3. THE M6 实现 SHALL NOT 修改 M0–M5 已固化的 BuF 字节布局、加载链既有入口（`load` / `load_signed` / `load_from_source`）行为、运行层/适配层契约或既有错误码语义；M6 的 profile 加载通过新入口提供。
4. THE M6 实现 SHALL NOT 改变签名作用域定义（仍为 `Header || Manifest_without_signature || Section_Index`）；部分加载下签名一致性正是该定义的推论。
5. THE M6 实现 SHALL NOT 引入运行层 / 适配层改动。
6. THE M6 选择逻辑 SHALL NOT 把 LoadProfile 的 `target_class` 用作选择约束（M6 的约束仅来自 capabilities / features / max_size）；`target_class` 是画像的描述性字段，为后续按类别裁剪预留，M6 不据它选择。

## 约束与假设

- **目标语言**：M6 仅交付 Rust 实现。
- **crate 归属**：M6 的 LoadProfile、选择谓词、部分加载入口落在 `fayger-loader` crate（复用 M5 的 source / strategy 与 M4 的签名）。
- **新增依赖**：M6 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M6 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
- **复用 M5/M4**：被选中 Section 的读取与校验复用 M5 的 Eager/Lazy 机制；签名一致性复用 M4 的 `signed_scope`（其作用域本就不含 Section 段体）。
