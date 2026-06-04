# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M1 里程碑**需求文档。M1 在 M0（BuF 字节布局 + Parser + Serializer + 往返一致性）之上，补齐蓝图加载链中 M0 之后的两个阶段：

- **Verify(Digest)**：对 Header / Manifest / Section_Index 的整体摘要（Trailer `manifest_digest`）、每个 Section 段体的摘要（`SectionDescriptor.digest`）、以及整文件的 `crc32` 做实际计算与校验。M0 阶段这些字段都只是占位（写零、不校验），M1 让它们成为真正的完整性护栏。
- **Negotiate(Version)**：对 `schema_version` 做支持范围检查与弃用提示，对 `runtime_interface_min` 做最低版本检查。M0 只校验 SemVer 语法合法性，M1 引入 SemVer 比较与版本协商决策。

M1 还把上述两阶段编排进一条 **加载链入口** `load(bytes, policy)`：内部依次执行 `parse`（M0 已完成的结构解析）→ Verify(Digest) → Negotiate(Version)，产出一个版本兼容、完整性已校验的 BuF 内存对象。

M1 **不**交付签名校验（M4）、BuF_Source 抽象 / Eager-Lazy / 部分加载（M5/M6）、运行层 / 生命周期（M2）、适配层（M3）。这些明确推迟到后续里程碑。

完成 M1 后，项目拥有"把一段 BuF 字节加载成可信、版本兼容的内存对象"的能力，为 M2 的运行层接收方提供合格输入。

## Glossary

- **加载链（Load Chain）**：把 BuF 字节加载为可交付运行层的内存对象的阶段序列。M0 落实 Parse / Verify(Structural)；M1 追加 Verify(Digest) 与 Negotiate(Version)。
- **Verify(Digest)**：完整性校验阶段。包含 Manifest 摘要、Section 段体摘要、整文件 crc32 三类校验。
- **manifest_digest**：Trailer 中的 32 字节字段，是 Header || Manifest || Section_Index 三段连续字节的 SHA-256 摘要。
- **section digest**：`SectionDescriptor.digest`，是单个 Section payload 字节的摘要（M1 支持 `sha256`）。
- **crc32**：Trailer 中的 4 字节字段，是整文件除末尾 crc32 自身 4 字节外全部字节的 CRC-32（IEEE 多项式）。
- **Sealing（封装）**：对一个 BuFObject 计算并填入全部 section digest、manifest_digest、crc32，使其再序列化后可通过 Verify(Digest)。BuF 生产者用它产出合法制品。
- **Negotiate(Version)**：版本协商阶段。基于 `LoadPolicy` 中的支持范围、弃用集合与提供版本，决定加载是否通过、是否附带弃用提示。
- **LoadPolicy**：加载方提供的版本策略，至少包含 `supported_schema_range`（schema 版本闭区间）、`deprecated_schemas`（弃用 schema 版本集合）、`runtime_interface_provided`（当前 Fayger 提供的 Runtime_Interface 版本）。
- **SemVer 优先级（Precedence）**：SemVer 2.0.0 §11 定义的版本比较规则。M1 在 M0 仅做语法解析的基础上引入此比较关系。
- **LoadedBuF**：加载链产出物。M1 中以 `BuFObject`（其 `deprecation_notice` 字段在弃用场景被加载链填入）表示。

## Requirements

### Requirement 1: 摘要基元

**User Story:** 作为加载链实现者，我希望有确定性的摘要与校验和基元，以便在完整性校验阶段复用。

#### Acceptance Criteria

1. THE M1 实现 SHALL 提供对任意字节切片计算 SHA-256（32 字节输出）的函数。
2. THE M1 实现 SHALL 提供对任意字节切片计算 CRC-32（IEEE 多项式，4 字节输出）的函数。
3. WHEN 对同一字节切片重复计算 SHA-256 或 CRC-32，THE 摘要基元 SHALL 产出完全相同的输出（确定性）。
4. THE 摘要基元 SHALL 不依赖网络、文件系统或全局可变状态。

### Requirement 2: Manifest 摘要校验

**User Story:** 作为 BuF 的消费者，我希望加载时校验 Header / Manifest / Section_Index 未被篡改，以便信任后续阶段读取的元数据。

#### Acceptance Criteria

1. THE Verify(Digest) 阶段 SHALL 把 `manifest_digest` 定义为对字节区间 `[0, sections_offset)` 的 SHA-256，其中 `sections_offset = section_index_offset + section_index_length`（即 Header || Manifest || Section_Index 三段连续字节）。
2. WHEN 实际计算出的 Manifest 摘要与 Trailer 中的 `manifest_digest` 不一致，THE Verify(Digest) 阶段 SHALL 返回错误码 `LDR_DIGEST_MISMATCH`，错误上下文 reason 为 `ManifestDigest`，并包含期望摘要与实际摘要的十六进制表示。
3. WHEN 实际计算出的 Manifest 摘要与 Trailer 中的 `manifest_digest` 一致，THE Verify(Digest) 阶段 SHALL 视该项校验通过。

### Requirement 3: Section 段体摘要校验

**User Story:** 作为 BuF 的消费者，我希望加载时逐段校验内容摘要，以便任何被篡改的 Section 在加载阶段即被拒绝。

#### Acceptance Criteria

1. WHILE 遍历 `manifest.sections` 中每个 `SectionDescriptor`，THE Verify(Digest) 阶段 SHALL 对其对应 Section payload 计算 `SectionDescriptor.digest.algorithm` 所声明算法的摘要并与 `SectionDescriptor.digest.value` 比较。
2. IF 某个 `SectionDescriptor.digest.value` 与实际计算出的段体摘要不一致，THEN THE Verify(Digest) 阶段 SHALL 返回错误码 `LDR_DIGEST_MISMATCH`，错误上下文 reason 为 `SectionDigest`，并包含 `section_id`、期望摘要与实际摘要。
3. IF 某个 `SectionDescriptor.digest.algorithm` 不是 M1 支持的算法（M1 支持 `sha256`），THEN THE Verify(Digest) 阶段 SHALL 返回错误码 `LDR_DIGEST_MISMATCH`，错误上下文 reason 为 `UnsupportedDigestAlgorithm`，并包含 `section_id` 与声明的算法标识。
4. WHEN `manifest.sections` 为空列表，THE Verify(Digest) 阶段 SHALL 跳过 Section 段体摘要校验且不报错。

### Requirement 4: crc32 完整性校验

**User Story:** 作为 BuF 的消费者，我希望加载时校验整文件的 crc32，以便检测传输或存储引入的随机损坏。

#### Acceptance Criteria

1. THE Verify(Digest) 阶段 SHALL 把 `crc32` 定义为对字节区间 `[0, total_length - 4)` 的 CRC-32（即整文件除末尾 4 字节 crc32 自身外的全部字节）。
2. IF 实际计算出的 crc32 与 Trailer 中的 `crc32` 不一致，THEN THE Verify(Digest) 阶段 SHALL 返回错误码 `LDR_DIGEST_MISMATCH`，错误上下文 reason 为 `Crc32`，并包含期望值与实际值。
3. WHEN 实际计算出的 crc32 与 Trailer 中的 `crc32` 一致，THE Verify(Digest) 阶段 SHALL 视该项校验通过。

### Requirement 5: 封装（计算并填入摘要）

**User Story:** 作为 BuF 工具链的开发者，我希望能对内存中的 BuFObject 一次性计算并填入全部摘要与 crc32，以便产出可通过 Verify(Digest) 的合法制品。

#### Acceptance Criteria

1. THE M1 实现 SHALL 提供封装操作，对一个 BuFObject 计算每个 Section payload 的 SHA-256 并写入对应 `SectionDescriptor.digest`（algorithm 为 `sha256`）。
2. THE 封装操作 SHALL 在 Section 摘要写入后，计算 `manifest_digest`（区间 `[0, sections_offset)`）并写入 `raw_trailer.manifest_digest`。
3. THE 封装操作 SHALL 在 `manifest_digest` 写入后，计算 `crc32`（区间 `[0, total_length - 4)`）并写入 `raw_trailer.crc32`。
4. WHEN 对一个先经封装、再 `serialize` 的 BuFObject 调用加载链，THE Verify(Digest) 阶段 SHALL 全部校验通过。
5. IF 待封装的 BuFObject 未通过 M0 的一致性自检（sections 与 manifest.sections 不一致）或 Manifest 必填字段校验，THEN THE 封装操作 SHALL 拒绝并返回与 M0 序列化一致的错误码。

### Requirement 6: SemVer 比较

**User Story:** 作为版本协商实现者，我希望 SemVer 支持比较，以便判断版本是否落在支持范围内、是否高于提供版本。

#### Acceptance Criteria

1. THE M1 实现 SHALL 为 SemVer 提供全序比较，先按 major、再 minor、再 patch 的数值升序比较。
2. WHERE 两个 SemVer 的 major / minor / patch 相等，THE 比较 SHALL 遵循 SemVer 2.0.0 优先级：带预发布段的版本低于不带预发布段的版本。
3. WHERE 两个 SemVer 的 major / minor / patch 相等且均带预发布段，THE 比较 SHALL 按点分标识符逐段比较：纯数字标识符按数值比较、含非数字标识符按 ASCII 字典序比较、数字标识符低于非数字标识符；当一方是另一方的真前缀时，标识符更多的一方更高。
4. THE SemVer 比较关系 SHALL 满足自反性、反对称性与传递性（构成全序）。

### Requirement 7: Schema 版本协商

**User Story:** 作为长期维护者，我希望 Fayger 在加载时检查 BuF 的 schema 版本是否受支持，以便拒绝不兼容制品并对弃用版本给出提示。

#### Acceptance Criteria

1. WHEN `manifest.schema_version` 不落在 `LoadPolicy.supported_schema_range` 闭区间内，THE Negotiate(Version) 阶段 SHALL 返回错误码 `LDR_SCHEMA_UNSUPPORTED`，错误上下文包含 `expected_range`（区间下界与上界）与实际 `schema_version`。
2. WHEN `manifest.schema_version` 落在支持区间内且不属于 `LoadPolicy.deprecated_schemas`，THE Negotiate(Version) 阶段 SHALL 通过且不产生弃用提示。
3. WHEN `manifest.schema_version` 落在支持区间内且属于 `LoadPolicy.deprecated_schemas`，THE Negotiate(Version) 阶段 SHALL 通过并产出一条非空弃用提示，由加载链写入加载结果的 `deprecation_notice` 字段。
4. WHERE 加载未触发弃用，THE 加载链 SHALL 保持加载结果的 `deprecation_notice` 字段为 `None`。

### Requirement 8: Runtime_Interface 版本协商

**User Story:** 作为长期维护者，我希望老版本 BuF 在新版本 Fayger 上仍可加载，而要求过高的 BuF 被明确拒绝。

#### Acceptance Criteria

1. WHEN `manifest.runtime_interface_min` 不高于 `LoadPolicy.runtime_interface_provided`，THE Negotiate(Version) 阶段 SHALL 允许加载继续。
2. IF `manifest.runtime_interface_min` 高于 `LoadPolicy.runtime_interface_provided`，THEN THE Negotiate(Version) 阶段 SHALL 返回错误码 `LDR_RUNTIME_VERSION_TOO_HIGH`，错误上下文包含 `required`（manifest 要求版本）与 `provided`（当前提供版本）。
3. WHERE schema 版本检查与 runtime 版本检查同时不通过，THE Negotiate(Version) 阶段 SHALL 优先返回 schema 版本错误（`LDR_SCHEMA_UNSUPPORTED`）。

### Requirement 9: 加载链编排

**User Story:** 作为 Fayger 调用方，我希望有单一入口把字节加载为可信且版本兼容的 BuF 对象，以便无需自行串联各阶段。

#### Acceptance Criteria

1. THE M1 实现 SHALL 暴露 `load(bytes: &[u8], policy: &LoadPolicy) -> Result<BuFObject, FaygerError>` 接口。
2. THE 加载链 SHALL 按以下固定顺序执行：Parse 与 Verify(Structural)（复用 M0 `parse`）→ Verify(Digest) → Negotiate(Version)。
3. WHERE 多个阶段都会失败，THE 加载链 SHALL 按阶段顺序返回最先失败阶段的错误：结构错误（`LDR_PARSE_FAIL`）优先于摘要错误（`LDR_DIGEST_MISMATCH`），摘要错误优先于版本错误（`LDR_SCHEMA_UNSUPPORTED` / `LDR_RUNTIME_VERSION_TOO_HIGH`）。
4. WHEN 全部阶段通过，THE 加载链 SHALL 返回一个 `BuFObject`，其 `deprecation_notice` 反映 Negotiate(Version) 的结果。
5. THE 加载链 SHALL NOT 执行任何 Section payload 的内容（保持纯字节与摘要操作）。

### Requirement 10: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M1 新增的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M1 错误码集合 SHALL 在 M0 既有码之外至少新增 `LDR_DIGEST_MISMATCH`、`LDR_SCHEMA_UNSUPPORTED`、`LDR_RUNTIME_VERSION_TOO_HIGH` 三个稳定值。
2. THE M1 新增错误码 SHALL 复用 M0 的 `FaygerError` 结构，`source_layer` 固定为 `Loader`。
3. WHEN M1 各阶段产生错误，THE 错误对象 SHALL 在 `context` 中提供足以定位失败的结构化字段（如 `reason`、`section_id`、`expected`、`actual`、`expected_range`、`required`、`provided`）。
4. THE M1 既有错误码（`LDR_PARSE_FAIL`、`LDR_MISSING_REQUIRED_FIELD`）SHALL 保持语义不变，M1 不复用或重定义它们。

### Requirement 11: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M1 与 M0 保持同一质量基线。

#### Acceptance Criteria

1. THE M1 实现 SHALL 至少包含以下属性测试：封装-加载往返（封装后序列化再加载，Verify(Digest) 必通过）、摘要篡改被检测（对 Section payload 或 Header/Manifest/Index 区间的任一单点修改在不更新对应摘要时，加载返回 `LDR_DIGEST_MISMATCH`）、crc32 篡改被检测、版本协商决策表、SemVer 全序性质。
2. THE M1 实现 SHALL 提供至少一个经封装的合法 BuF 样例字节流文件作为快照，并在测试中验证其加载结果稳定。
3. THE M1 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用 M0 的 `.regressions` 归档机制）。
5. THE M1 实现 SHALL 与 M0 共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0 既有测试在 M1 引入后仍全部通过。

### Requirement 12: M1 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M1 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M1 实现 SHALL NOT 引入签名校验、强制签名模式或可信根（这些是 M4）；`BuFManifest.signature` 在 M1 仍不参与校验。
2. THE M1 实现 SHALL NOT 引入 BuF_Source 抽象、Eager / Lazy 加载策略、LoadProfile 或部分加载（这些是 M5/M6）；加载链在 M1 仍接收完整的 `&[u8]`。
3. THE M1 实现 SHALL NOT 引入运行层、Runtime_Interface、生命周期状态机或适配层（这些是 M2/M3）。
4. THE M1 实现 SHALL NOT 修改 M0 已固化的 BuF 字节布局；M1 仅赋予既有占位字段（`manifest_digest`、`crc32`、`SectionDescriptor.digest`）实际语义。
5. THE M1 实现 SHALL NOT 改变 M0 `parse` / `serialize` 的对外行为；二者在 M1 仍保持 digest 无关（原样读写占位字段），M1 的摘要逻辑位于独立的加载链组件中。

## 约束与假设

- **目标语言**：M1 仅交付 Rust 实现。
- **新增依赖**：SHA-256 与 CRC-32 各引入一个成熟、纯 Rust、活跃维护的 crate；版本在技术设计阶段固化到 `Cargo.toml`。
- **crate 归属**：M1 的加载链逻辑落在新的 `fayger-loader` crate（依赖 `fayger-buf` 与 `fayger-error`）；SemVer 比较关系作为类型固有能力补充进 `fayger-buf`。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M1 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
