# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M5 里程碑**需求文档。M5 在 M0（BuF 格式）、M1（加载链）、M4（签名）之上引入**多源存储抽象与延迟加载**（蓝图加载链中 `Read` 与 `Read(Selected Section Bodies)?` 阶段的可替换前端）：

- **BuF_Source 抽象**：把"从哪里读字节"从加载链中解耦。任意存储后端（本地文件、内存、网络流、对象存储、用户自定义）通过实现 BuF_Source 接入，最低要求暴露随机访问读。
- **Eager / Lazy 两种 LoadStrategy**：`Eager` 在 load 调用返回前完成所有被选中 Section 的读取与 digest 校验；`Lazy` 仅在返回前读取并校验 Header / Manifest / Section_Index，Section 段体在首次访问时才从 BuF_Source 取回并校验。
- **SectionLoader（按需读取）**：Lazy 模式下封装"首次访问触发读取 + digest 校验 + 缓存"。
- **阶段错误标注**：加载层错误区分 Eager 阶段失败与 Lazy 后续访问阶段失败（`context.phase ∈ {eager, lazy}`），并对源读取失败、源不可用、Lazy 摘要不匹配使用独立错误码。

M5 **不**交付：LoadProfile / 部分加载（profile 驱动 Section 选择是 M6——M5 中"被选中 Section"即全部 Section）；部分加载下的签名一致性（M6 Property）；资源监测 / 配额（M7）；可观测性事件总线 / 跨层错误链（M8）；运行层 / 适配层改动。

完成 M5 后，Fayger 能从任意随机访问存储后端加载 BuF，并在"启动时延"与"按需取数"之间用 Eager / Lazy 策略做权衡——即"同一份 BuF 既可一次性加载、也可按段延迟加载，且两种模式语义等价"的最小可证形态。

## Glossary

- **BuF_Source**：BuF 的存储后端抽象。最低要求暴露随机访问读 `read_at(offset, length)` 与制品总长度 `length()`。
- **MemorySource**：M5 内置的 BuF_Source 实现，把一段内存字节作为后端（测试与常见内存加载场景）。
- **LoadStrategy**：加载策略枚举：`Eager`、`Lazy`。
- **Selected Section（被选中 Section）**：在当前加载下需要纳入的 Section 集合。M5 无 LoadProfile，被选中 Section 即 Manifest 声明的全部 Section。
- **SectionLoader**：Lazy 模式下的按需读取器，持有 BuF_Source 引用与 Section_Index 快照，负责"首次访问读取 + digest 校验 + 缓存"。
- **LoadedArtifact**：M5 source 加载链的产出物。Eager 模式下全部 Section 已在内存；Lazy 模式下持有 SectionLoader，Section 缓存按需填充。它携带 Manifest、原始 Header / Trailer 与加载策略。
- **可信骨架（Trusted Skeleton）**：Header / Manifest / Section_Index 三段。无论 Eager 还是 Lazy，都必须在 load 调用返回前完整读取并校验（manifest_digest），是后续按需读取的可信索引来源。
- **Phase（阶段标注）**：错误上下文中的 `phase` 字段，取值 `eager` 或 `lazy`，标注错误产生于 load 调用栈内（eager）还是 SectionLoader.fetch 调用栈内（lazy）。

## Requirements

### Requirement 1: BuF_Source 抽象

**User Story:** 作为 BuF 的发布方，我希望把 BuF 托管在任意存储后端，并以统一接口被 Fayger 读取，以便不绑定具体存储。

#### Acceptance Criteria

1. THE M5 实现 SHALL 定义 BuF_Source 抽象，至少暴露随机访问读 `read_at(offset, length) -> Result<Bytes, _>` 与总长度查询 `length() -> Result<u64, _>`。
2. WHEN 调用方以 `(offset, length)` 请求读取且该区间落在 `[0, length())` 内，THE BuF_Source SHALL 返回恰好 `length` 字节。
3. IF 请求区间越过 `[0, length())` 边界，THEN THE BuF_Source 的实现 SHALL 返回错误（而非截断或 panic），由加载层映射为结构化错误。
4. THE M5 实现 SHALL 提供至少一个内置 BuF_Source 实现 MemorySource，把一段内存字节作为后端。
5. THE BuF_Source 抽象 SHALL 不要求实现方支持写、追加或顺序流语义；随机访问读与总长度是唯一必需能力。

### Requirement 2: Eager 加载策略

**User Story:** 作为低时延优先的集成方，我希望一次性把整个 BuF 加载并校验完毕，以便运行期无需再访问存储。

#### Acceptance Criteria

1. WHEN 加载策略为 `Eager`，THE 加载链 SHALL 在 load 调用返回前完成 Header / Manifest / Section_Index 与所有被选中 Section 段体的读取。
2. WHEN 加载策略为 `Eager`，THE 加载链 SHALL 在 load 调用返回前对所有被选中 Section 段体执行各自在 Manifest 中声明的 digest 校验。
3. WHEN Eager 加载成功，THE LoadedArtifact SHALL 持有全部被选中 Section 的内容，且后续访问任一 Section 不再触发 BuF_Source 读取。
4. IF Eager 加载阶段任一 Section digest 校验失败，THEN THE 加载链 SHALL 返回错误码 `LDR_DIGEST_MISMATCH`，错误上下文 `phase` 为 `eager`，并包含 `section_id`。
5. THE Eager 加载结果 SHALL 与 M1 在等价完整字节流上的 `parse + verify_digest` 在 Manifest、Section 内容与原始 Header/Trailer 上一致。

### Requirement 3: Lazy 加载策略

**User Story:** 作为带宽或启动时延受限终端的集成方，我希望加载时只读取可信骨架，Section 体按需取回，以便缩短启动时延。

#### Acceptance Criteria

1. WHEN 加载策略为 `Lazy`，THE 加载链 SHALL 在 load 调用返回前仅完成 Header、Manifest 与 Section_Index 的读取与校验（manifest_digest 校验），并保留 BuF_Source 引用以供后续按需读取。
2. WHEN 加载策略为 `Lazy`，THE 加载链 SHALL NOT 在 load 调用返回前读取任何 Section 段体。
3. WHEN Lazy 模式下首次访问某个被选中但尚未读取的 Section，THE SectionLoader SHALL 通过 BuF_Source 读取该 Section 段体，并对其执行 Manifest 中声明的 digest 校验。
4. WHEN Lazy 模式下某 Section 的 digest 校验通过，THE SectionLoader SHALL 缓存该 Section；再次访问 SHALL 命中缓存且不再触发 BuF_Source 读取。
5. IF Lazy 模式下某 Section 首次访问时 digest 校验不通过，THEN THE SectionLoader SHALL 返回错误码 `LDR_LAZY_DIGEST_MISMATCH`，错误上下文 `phase` 为 `lazy`，并包含 `section_id`，且该 Section 不被加入缓存。

### Requirement 4: 源读取失败处理

**User Story:** 作为运维人员，我希望存储后端不可用或读取失败时得到结构化、可定位的错误，以便区分基础设施问题与制品篡改。

#### Acceptance Criteria

1. IF Eager 模式下任一 BuF_Source 读取失败（read_at / length 返回错误或返回字节数不符），THEN THE 加载链 SHALL 返回错误码 `LDR_SOURCE_READ_FAILED`，错误上下文 `phase` 为 `eager`，并包含失败区间信息与底层错误描述。
2. IF Lazy 模式下后续访问某 Section 时 BuF_Source 不可用（read_at 返回错误），THEN THE SectionLoader SHALL 返回错误码 `LDR_LAZY_SOURCE_UNAVAILABLE`，错误上下文 `phase` 为 `lazy`，并包含 `section_id` 与底层错误描述。
3. WHEN BuF_Source 的 `read_at` 返回的字节数与请求的 `length` 不一致，THE 加载链 SHALL 视为读取失败并返回 `LDR_SOURCE_READ_FAILED`（携带期望与实际字节数）。
4. THE 源读取失败的错误描述 SHALL 不泄漏指针、内存地址等不可序列化或敏感信息；仅包含区间、长度、section_id 与可读的底层错误文本。

### Requirement 5: 阶段错误标注

**User Story:** 作为 Fay 开发者，我希望加载层错误能区分发生在一次性加载阶段还是后续按需访问阶段，以便分别处理。

#### Acceptance Criteria

1. THE 加载层在 source 加载链中产生的每条错误 SHALL 在 `context.phase` 中标注 `eager` 或 `lazy` 之一。
2. WHEN 错误产生于 load 调用栈内（含 Eager 模式的 Section 读取，以及任何模式下 Header / Manifest / Section_Index 的读取与校验），THE `context.phase` SHALL 为 `eager`。
3. WHEN 错误产生于 Lazy 模式 SectionLoader.fetch 调用栈内，THE `context.phase` SHALL 为 `lazy`。
4. THE 加载层 SHALL 使两类失败可被调用方按错误码或 `context.phase` 字段独立处理。

### Requirement 6: Eager / Lazy 等价性

**User Story:** 作为 Fayger 实现的维护者，我希望同一 BuF 在 Eager 与 Lazy 两种策略下加载得到语义等价的结果，以便策略选择不改变行为。

#### Acceptance Criteria

1. WHEN 同一 BuF（同一 BuF_Source 内容）分别以 `Eager` 与 `Lazy` 策略加载，THE 两次加载产出的 Manifest、原始 Header 与原始 Trailer SHALL 相等。
2. WHEN 对 Lazy 加载结果触发对所有被选中 Section 的访问（force-all），THE 最终缓存的 Section 内容集合 SHALL 与 Eager 加载结果的 Section 内容集合按段逐字节相等。
3. WHEN 同一 BuF 分别以两种策略加载且 BuF_Source 始终可用，THE 两次加载 SHALL 同样成功，或在制品本身损坏时同样失败（失败的具体阶段可不同，但成败结论一致）。

### Requirement 7: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M5 新增的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M5 错误码集合 SHALL 在既有码之外至少新增 `LDR_SOURCE_READ_FAILED`、`LDR_LAZY_SOURCE_UNAVAILABLE`、`LDR_LAZY_DIGEST_MISMATCH` 三个稳定值。
2. THE M5 新增错误 SHALL 复用统一的 `FaygerError` 结构，`source_layer` 为 `Loader`。
3. WHEN M5 各组件产生错误，THE 错误对象 SHALL 在 `context` 中提供足以定位失败的结构化字段（如 `phase`、`section_id`、`offset`、`length`、`detail`）。
4. THE M0/M1/M2/M3/M4 既有错误码 SHALL 保持语义不变，M5 不复用或重定义它们。

### Requirement 8: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M5 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M5 实现 SHALL 至少包含以下属性测试：Eager / Lazy 等价性、Lazy 首次访问校验（fetch 触发读取并校验、缓存命中不再读取）、源读取失败与不可用的阶段标注、Lazy 摘要篡改被检测。
2. THE M5 实现 SHALL 提供至少一个端到端用例：用 MemorySource 承载一个封装好的 BuF，分别以 Eager 与 Lazy 加载，断言等价；并构造一个会失败的 Source 验证 Lazy 不可用错误。
3. THE M5 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M5 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0–M4 既有测试在 M5 引入后仍全部通过。

### Requirement 9: M5 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M5 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M5 实现 SHALL NOT 引入 LoadProfile 或 profile 驱动的 Section 选择（那是 M6）；M5 中被选中 Section 即 Manifest 声明的全部 Section。
2. THE M5 实现 SHALL NOT 引入部分加载下的签名一致性 Property（那是 M6）。
3. THE M5 实现 SHALL NOT 引入资源监测、配额、可观测性事件总线或跨层 `cause` 错误链（M7/M8）；M5 错误 `cause` 可为 None。
4. THE M5 实现 SHALL NOT 修改 M0–M4 已固化的 BuF 字节布局、加载链既有入口（`load` / `load_signed`）行为、运行层/适配层契约或既有错误码语义；M5 的 source 加载通过新入口提供。
5. THE M5 实现 SHALL NOT 要求 BuF_Source 支持写或顺序流；仅随机访问读与总长度。
6. THE Lazy 模式 SHALL NOT 在 load 返回前校验 crc32（crc32 覆盖 Section 段体，在 Lazy 模式下不在可信骨架范围内）；crc32 校验是 Eager 模式（完整字节可用）的职责。

## 约束与假设

- **目标语言**：M5 仅交付 Rust 实现。
- **crate 归属**：M5 的 BuF_Source、LoadStrategy、SectionLoader 与 source 加载链落在 `fayger-loader` crate。Lazy 模式下每个 Section 的偏移由 Manifest 中（受 manifest_digest 保护的）各 Section 声明长度按 Manifest 顺序累加得到——这与 M0 序列化器布局 Section 的方式一致，因此无需暴露 `fayger-buf` 内部的 Section_Index 条目类型（不改 `fayger-buf` 可见性、字节布局或行为）。
- **新增依赖**：M5 不引入新的第三方运行时依赖；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M5 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
- **Eager 复用 M1**：Eager 模式读取完整字节后复用 M1 的 `parse + verify_digest`，以保证与既有加载链的一致性（Requirement 2.5）。
