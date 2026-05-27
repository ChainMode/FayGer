# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M0 里程碑**需求文档。M0 只交付一件事：让 BuF 制品成为可执行的参照物——可以构造、解析、再序列化、再解析的字节流，并以属性测试（PBT）保证往返一致性。

M0 不交付加载链、运行层、适配层、签名、版本协商、Lazy 加载或部分加载等任何后续能力。这些都明确推迟到 M1 及以后的里程碑。

完成 M0 后，整个项目拥有一份"BuF 在内存中长什么样"的可执行规范，所有后续里程碑都基于它扩展。

## Glossary

- **BuF（Built-up Fay）**：Fay 的可分发、可加载实体。M0 仅关心其字节布局。
- **BuFObject**：内存中的 BuF 对象，包含 manifest、sections、原始 header / trailer 字节。
- **BuF_Parser**：把字节流解析为 BuFObject 的组件。
- **BuF_Serializer**：把 BuFObject 序列化回字节流的组件。
- **Manifest**：BuF 内部用于描述自身元数据的清单。
- **SectionDescriptor**：Manifest 中 sections 列表的条目，描述单个 Section 的 id、kind、digest、length 等。
- **Section_Index**：BuF 中位于 Manifest 之后、Sections 之前的索引段，描述每个 Section 的位置、长度、digest 等。
- **Trailer**：BuF 末尾的尾段，含整体长度与 Manifest 摘要。
- **确定性 CBOR（Deterministic CBOR）**：RFC 8949 §4.2 描述的 CBOR 编码模式，保证同一逻辑值序列化为唯一字节序。
- **往返一致性（Round-trip Equivalence）**：对任意合法 BuFObject `o`，有 `parse(serialize(o)) ≡ o`。在确定性 CBOR 下进一步要求 `serialize(parse(bytes)) == bytes`（字节级强往返）。

## Requirements

### Requirement 1: BuF 字节布局

**User Story:** 作为 Fayger 工具链的开发者，我希望 BuF 的字节布局有清晰的、可验证的规范，以便不同实现解析出的对象具有一致语义。

#### Acceptance Criteria

1. THE BuF 字节流 SHALL 由五个段构成且按以下顺序排列：Header、Manifest、Section_Index、Sections、Trailer。
2. THE Header SHALL 以固定 8 字节起始，包含 magic `"BUF\0"`（4 字节）与 format_version（4 字节小端无符号整数）。
3. THE Header SHALL 包含 manifest_offset、manifest_length、section_index_offset、section_index_length 共 4 个 8 字节小端无符号整数字段。
4. THE Manifest 段 SHALL 是一份确定性 CBOR 编码的 BuFManifest。
5. THE Section_Index 段 SHALL 是一份确定性 CBOR 编码的列表，每个元素包含 section_id、kind、offset、length、digest（M0 暂作占位）、visibility（M0 暂固定为 Required）、profile_constraints（M0 暂为空对象）。
6. THE Sections 段 SHALL 是按 Section_Index 中 offset 顺序排列的二进制内容序列。
7. THE Trailer SHALL 包含 total_length（8 字节小端无符号整数）、manifest_digest（占位字段，M0 不验证内容）、crc32（4 字节小端无符号整数）。

### Requirement 2: BuFManifest 字段

**User Story:** 作为 BuF 制品的生产者，我希望 Manifest 中的字段定义清晰，以便正确填充元数据。

#### Acceptance Criteria

1. THE BuFManifest SHALL 至少包含以下必填字段：schema_version（字符串，SemVer 格式）、runtime_interface_min（字符串，SemVer 格式）、entry（字符串）、sections（列表）。
2. THE BuFManifest SHALL 至少包含以下可选字段：deprecation_notice（字符串）、capabilities（字符串集合）、quotas（结构体）、signature（M0 总为 null）、extensions（任意 CBOR 值的映射）。
3. THE BuFManifest 必填字段中的 sections 列表 SHALL 包含 0 或更多 SectionDescriptor，每个 SectionDescriptor 至少包含 id（字符串）与 kind（枚举：Code | Data | Asset | Signature | Custom）。
4. WHEN 用户构造 BuFManifest 但未提供任一必填字段，THE BuF_Serializer SHALL 拒绝序列化并返回缺失字段名列表。

### Requirement 3: BuF_Parser

**User Story:** 作为 Fayger 实现，我希望 BuF_Parser 能把字节流解析为内存对象，以便后续阶段消费。

#### Acceptance Criteria

1. THE BuF_Parser SHALL 暴露 `parse(bytes: &[u8]) -> Result<BuFObject, ParseError>` 接口。
2. WHEN 输入字节流符合 Requirement 1 与 Requirement 2 的格式规范，THE BuF_Parser SHALL 在不执行任何 Section 内容的前提下产出 BuFObject。
3. WHEN 输入字节流前 4 字节不等于 `"BUF\0"`，THE BuF_Parser SHALL 返回错误码 `LDR_PARSE_FAIL`，错误上下文 reason 为 `MagicMismatch`，并包含实际读到的前 4 字节十六进制表示。
4. WHEN Header 中的 manifest_offset、manifest_length、section_index_offset、section_index_length 任一字段越过字节流边界，THE BuF_Parser SHALL 返回错误码 `LDR_PARSE_FAIL`，错误上下文 reason 为 `OffsetOutOfBounds`，并包含越界的字段名与实际值。
5. WHEN Manifest 段不是合法的确定性 CBOR 或缺少 Requirement 2 中规定的必填字段，THE BuF_Parser SHALL 返回错误码 `LDR_PARSE_FAIL`，错误上下文 reason 为 `ManifestInvalid`，并包含具体原因（CBOR 解码错误信息或缺失字段名）。
6. WHEN Section_Index 中的某个条目的 offset 加 length 超过 Sections 段的实际长度，THE BuF_Parser SHALL 返回错误码 `LDR_PARSE_FAIL`，错误上下文 reason 为 `SectionOutOfBounds`，并包含 section_id。
7. WHEN Trailer 段的 total_length 与字节流总长度不一致，THE BuF_Parser SHALL 返回错误码 `LDR_PARSE_FAIL`，错误上下文 reason 为 `TrailerLengthMismatch`。
8. WHEN 字节流被截断（长度不足以容纳 Header + Trailer），THE BuF_Parser SHALL 返回错误码 `LDR_PARSE_FAIL`，错误上下文 reason 为 `Truncated`，并包含期望的最小长度与实际长度。

### Requirement 4: BuF_Serializer

**User Story:** 作为 Fayger 工具链开发者，我希望可以把内存中的 BuFObject 写回为字节流，以便支持工具链构造、缓存、重打包。

#### Acceptance Criteria

1. THE BuF_Serializer SHALL 暴露 `serialize(obj: &BuFObject) -> Result<Vec<u8>, SerializeError>` 接口。
2. WHEN 输入 BuFObject 的 manifest 满足 Requirement 2 中所有必填字段，THE BuF_Serializer SHALL 输出一份符合 Requirement 1 规范的字节流。
3. THE BuF_Serializer 写出的 Manifest 与 Section_Index SHALL 使用确定性 CBOR 编码（RFC 8949 §4.2）。
4. WHEN 输入 BuFObject 的 manifest 缺少 Requirement 2 中规定的任一必填字段，THE BuF_Serializer SHALL 返回错误码 `LDR_MISSING_REQUIRED_FIELD`，错误上下文 missing_fields 包含所有缺失字段名。
5. WHEN 输入 BuFObject 的 sections 与 manifest.sections 描述不一致（数量、id 集合或 length 不匹配），THE BuF_Serializer SHALL 返回错误码 `LDR_MISSING_REQUIRED_FIELD`，错误上下文 reason 为 `SectionInconsistency`，并包含具体不一致项。

### Requirement 5: 往返一致性

**User Story:** 作为 Fayger 实现的维护者，我希望 Parser 与 Serializer 的组合满足往返一致性，以便保证序列化层的语义稳定。

#### Acceptance Criteria

1. WHEN 对任意合法的 BuFObject `o`，令 `bytes = serialize(o)`，THE `parse(bytes)` SHALL 成功并产出 `o'`，使得 `o ≡ o'`（结构等价：manifest 字段集合相等、sections 内容逐字节相等、selected_sections 集合相等）。
2. WHEN 对任意合法的 BuF 字节流 `bytes`，令 `o = parse(bytes)`，THE `serialize(o)` SHALL 输出 `bytes'`，使得 `bytes' == bytes`（字节级强往返，依赖于确定性 CBOR）。
3. THE 往返一致性 SHALL 通过属性测试（PBT）持续验证，至少覆盖以下生成器：空 sections、单个最大允许 section（≤ 1 MiB）、多个不同 kind 的 sections、Manifest extensions 包含嵌套 CBOR 值。
4. PBT 测试每条 SHALL 至少运行 500 次迭代。

### Requirement 6: 错误模型雏形

**User Story:** 作为 Fayger 调用方，我希望 M0 阶段的错误已经按统一模型组织，以便后续里程碑无需重写错误类型。

#### Acceptance Criteria

1. THE M0 错误类型 SHALL 至少包含以下字段：code（错误码枚举）、source_layer（M0 固定为 Loader）、message（人类可读描述）、context（结构化键值对）。
2. THE M0 错误类型 SHALL 暴露 cause 字段以便后续里程碑组装错误链，M0 阶段该字段允许为 None。
3. THE M0 错误码集合 SHALL 至少包含 `LDR_PARSE_FAIL` 与 `LDR_MISSING_REQUIRED_FIELD` 两个稳定值。
4. WHEN M0 实现产生错误，THE 错误对象 SHALL 在 message 字段中给出简洁的人类可读描述，且不重复 code 字段已经表达的信息。

### Requirement 7: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M0 实现具备可重复的质量保障基线。

#### Acceptance Criteria

1. THE M0 实现 SHALL 至少包含以下属性测试：往返一致性（结构等价 + 字节级强往返）、字节流破坏被检测（位翻转 / 截断 / 单字节修改 / magic 修改至少四种破坏模式）、必填字段缺失被拒（从合法 BuFObject 中移除任一必填字段后 serialize 应失败）、Section_Index 自相矛盾被检测（篡改 offset 或 length 后 parse 应失败）。
2. THE M0 实现 SHALL 提供至少一个最小合法 BuF 的样例字节流文件作为快照，并在单元测试中验证样例的解析结果稳定。
3. THE M0 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例（counterexample），THE 测试基础设施 SHALL 自动记录最小化反例，便于后续回归。

### Requirement 8: M0 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M0 阶段不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M0 实现 SHALL NOT 包含 SignatureVerifier 或任何签名验证逻辑；BuFManifest.signature 字段在 M0 始终序列化为 null。
2. THE M0 实现 SHALL NOT 对 SectionDescriptor.digest 字段进行实际摘要计算或一致性校验；该字段在 M0 仅作为占位符存在并参与序列化。
3. THE M0 实现 SHALL NOT 对 schema_version 与 runtime_interface_min 字段执行支持范围检查；只校验它们是合法的 SemVer 字符串语法。
4. THE M0 实现 SHALL NOT 对 Trailer 中的 manifest_digest 与 crc32 字段进行实际计算或校验；写出时 manifest_digest 占位、crc32 写 0。
5. THE M0 实现 SHALL NOT 引入 BuF_Source 抽象、Eager / Lazy 加载策略、LoadProfile、部分加载、运行层、适配层、资源监测、可观测性事件总线等任何后续里程碑能力。
6. THE M0 实现 SHALL NOT 跨层组装 cause 错误链；M0 阶段所有错误来源都是 Loader 层。

## 约束与假设

- **目标语言**：M0 仅交付 Rust 实现。
- **MSRV**：使用 Rust stable 最新主版本（占位，技术设计阶段确定具体版本）。
- **CBOR 库**：技术设计阶段决定（候选：`ciborium`、`minicbor`）。
- **PBT 库**：技术设计阶段决定（候选：`proptest`、`quickcheck`）。
- **错误库**：技术设计阶段决定（候选：自定义 enum + 手写 / `thiserror`）。
- **MSRV、依赖版本、库选型** 一旦在技术设计阶段确定，必须固化到 `Cargo.toml` 与 `rust-toolchain.toml`，避免漂移。
