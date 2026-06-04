# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M4 里程碑**需求文档。M4 在 M0（BuF 格式）、M1（加载链：Verify(Digest) + Negotiate(Version)）之上，补齐蓝图加载链中 **Verify(Signature)** 阶段与安全横切组件：

- **签名校验**：当 BuF_Manifest 含签名信息时，用其声明的算法对"签名作用域"验证签名。M4 支持 `ed25519`。
- **可信根（TrustStore）**：维护一组可信公钥（trusted roots）；签名验证以该集合为信任锚。可信根可更新，更新在下一次加载生效。
- **强制签名模式**：当 Fayger 配置为强制签名时，拒绝加载任何未携带签名的 BuF。
- **加载链集成**：把 Verify(Signature) 插入加载链 `Parse → Verify(Digest) → Verify(Signature) → Negotiate(Version)` 的正确位置，并保持阶段错误优先级。

M4 **不**交付：证书链 / X.509 / 撤销列表；多算法（仅 `ed25519`，`ecdsa-p256` / `rsa-pss` 留待后续）；密钥管理 / 轮换策略的持久化（TrustStore 是内存集合）；BuF_Source / 部分加载下的签名（部分加载的签名一致性是 M6 的 Property）；运行层 / 适配层改动。

完成 M4 后，加载链具备来源与完整性的双重护栏：Verify(Digest) 防篡改，Verify(Signature) 防伪造来源，强制签名模式拒绝无签名制品——即"只有被可信方签名且未被篡改的 BuF 才能进入运行层"的最小可证形态。

## Glossary

- **签名作用域（Signed Scope）**：签名实际覆盖的字节区间。M4 定义为 `Header || Manifest_without_signature || Section_Index`，与设计文档（顶层 spec design.md §签名作用范围）一致——不依赖任何 Section 段体是否被读取。
- **SignatureBlock**：BuF_Manifest 中的签名信息，至少包含 `algorithm`（签名算法）、`public_key_ref`（公钥引用/标识）、`signed_digest`（签名字节）。M0 已定义该结构（M0 中总为 None）。
- **SignatureAlgorithm**：签名算法标识。M4 支持 `ed25519`。
- **TrustedRoot**：一条可信根，至少包含 `key_ref`（与 SignatureBlock.public_key_ref 对应的标识）与 `public_key`（公钥字节）。
- **TrustedRootSet**：一组 TrustedRoot 的集合。
- **TrustStore**：可信根集合的持有者，暴露当前可信根查询、更新（下次加载生效）与强制签名开关查询。
- **EnforceMode（强制签名模式）**：布尔开关。开启时拒绝加载未携带签名的 BuF。
- **Manifest_without_signature**：把 BuF_Manifest 的 `signature` 字段置空后，按 canonical CBOR 编码得到的字节。这是签名作用域中的 Manifest 部分（避免"对含签名的自身做签名"的循环）。
- **VerifyOutcome**：签名验证结果，区分 `Valid`（有签名且验证通过）、`Invalid`（有签名但验证失败）、`Absent`（无签名）。

## Requirements

### Requirement 1: 签名算法基元

**User Story:** 作为签名校验实现者，我希望有确定性的签名验证基元，以便在加载链中复用。

#### Acceptance Criteria

1. THE M4 实现 SHALL 提供对 `ed25519` 算法的签名验证函数：输入公钥字节、消息字节、签名字节，输出验证是否通过。
2. WHEN 公钥、消息、签名三者匹配（由对应私钥对该消息签名产生），THE 验证函数 SHALL 返回通过。
3. WHEN 三者中任一不匹配（错误公钥、被篡改消息、错误签名），THE 验证函数 SHALL 返回不通过，而非 panic 或错误中止进程。
4. IF 公钥字节或签名字节长度不符合 `ed25519` 规范（公钥 32 字节、签名 64 字节），THEN THE 验证函数 SHALL 返回结构化错误，而非 panic。
5. THE 验证函数 SHALL 不依赖网络、文件系统或全局可变状态（确定性、纯函数式）。

### Requirement 2: 签名作用域计算

**User Story:** 作为加载链实现者，我希望签名作用域有明确、可重现的定义，以便签名与验证两侧一致。

#### Acceptance Criteria

1. THE 签名作用域 SHALL 定义为 `Header || Manifest_without_signature || Section_Index` 三段连续字节。
2. THE Manifest_without_signature SHALL 是把 BuF_Manifest 的 `signature` 字段置为 None 后、按 canonical CBOR 编码得到的字节。
3. THE 签名作用域 SHALL NOT 包含任何 Section 段体；被跳过或未读取的 Section 不影响签名作用域。
4. WHEN 对同一 BuF 对象重复计算签名作用域，THE 结果 SHALL 字节级一致（确定性）。

### Requirement 3: 签名验证决策

**User Story:** 作为分发方，我希望 Fayger 在加载时按明确的决策表处理签名，以便伪造或缺失签名被正确拒绝。

#### Acceptance Criteria

1. WHEN BuF_Manifest 不含签名（`signature == None`）且强制签名模式关闭，THE Verify(Signature) 阶段 SHALL 通过（VerifyOutcome 为 `Absent`）。
2. IF BuF_Manifest 不含签名且强制签名模式开启，THEN THE Verify(Signature) 阶段 SHALL 返回错误码 `LDR_SIGNATURE_FAIL`，错误上下文 reason 为 `MissingSignature`。
3. WHEN BuF_Manifest 含签名且其 `public_key_ref` 在 TrustedRootSet 中存在对应可信根、且签名对签名作用域用该公钥验证通过，THE Verify(Signature) 阶段 SHALL 通过（VerifyOutcome 为 `Valid`）。
4. IF BuF_Manifest 含签名但 `public_key_ref` 在 TrustedRootSet 中不存在对应可信根，THEN THE Verify(Signature) 阶段 SHALL 返回 `LDR_SIGNATURE_FAIL`，错误上下文 reason 为 `UnknownKey`，并包含该 `public_key_ref`。
5. IF BuF_Manifest 含签名且找到可信根但签名验证不通过，THEN THE Verify(Signature) 阶段 SHALL 返回 `LDR_SIGNATURE_FAIL`，错误上下文 reason 为 `InvalidSignature`。
6. IF BuF_Manifest 含签名但其 `algorithm` 不是 M4 支持的算法（M4 支持 `ed25519`），THEN THE Verify(Signature) 阶段 SHALL 返回 `LDR_SIGNATURE_FAIL`，错误上下文 reason 为 `UnsupportedAlgorithm`，并包含声明的算法标识。

### Requirement 4: TrustStore 与可信根更新

**User Story:** 作为安全运维人员，我希望可以更新可信根集合，并明确更新何时生效，以便密钥轮换可控。

#### Acceptance Criteria

1. THE TrustStore SHALL 暴露当前可信根集合的查询。
2. THE TrustStore SHALL 暴露强制签名开关的查询。
3. WHEN 调用方更新 TrustStore 的可信根集合，THE 更新 SHALL 在该更新之后发起的加载中生效。
4. WHEN 在两次加载之间发生可信根更新，THE 后一次加载的签名验证 SHALL 使用更新后的可信根集合，前一次加载不受影响。
5. THE TrustStore SHALL 允许在构造时设置初始可信根集合与初始强制签名开关。

### Requirement 5: 加载链集成

**User Story:** 作为 Fayger 调用方，我希望签名校验自动成为加载链的一环，以便无需自行串联。

#### Acceptance Criteria

1. THE M4 实现 SHALL 提供一个携带 TrustStore 的加载入口，按顺序执行 Parse → Verify(Digest) → Verify(Signature) → Negotiate(Version)。
2. WHERE 多个阶段都会失败，THE 加载链 SHALL 按阶段顺序返回最先失败阶段的错误：结构（`LDR_PARSE_FAIL`）→ 摘要（`LDR_DIGEST_MISMATCH`）→ 签名（`LDR_SIGNATURE_FAIL`）→ 版本（`LDR_SCHEMA_UNSUPPORTED` / `LDR_RUNTIME_VERSION_TOO_HIGH`）。
3. WHEN 全部阶段通过，THE 加载链 SHALL 返回一个 BuFObject，其 `deprecation_notice` 反映版本协商结果。
4. THE Verify(Signature) 阶段 SHALL NOT 改变 M1 既有 `load(bytes, policy)` 入口的行为；M4 的签名集成通过新入口（携带 TrustStore）提供，M1 入口保持签名无关。
5. THE Verify(Signature) 阶段 SHALL NOT 执行任何 Section payload 内容（仅字节与签名运算）。

### Requirement 6: 封装侧签名支持

**User Story:** 作为 BuF 生产者，我希望能对一个已封装（M1 seal）的 BuF 计算并写入签名，以便产出可通过强制签名模式的制品。

#### Acceptance Criteria

1. THE M4 实现 SHALL 提供签名操作：输入一个已计算 digest 的 BuF 对象与一个私钥（及其 key_ref），对签名作用域用 `ed25519` 签名并写入 BuF_Manifest 的 `signature` 字段。
2. WHEN 对一个先签名、再序列化的 BuF 用包含对应公钥的 TrustStore 加载，THE Verify(Signature) 阶段 SHALL 通过。
3. THE 签名操作 SHALL 在写入签名前，先按 M1 seal 流程保证 digest 与 crc32 已就绪（或要求调用方先 seal），使最终制品同时通过 Verify(Digest) 与 Verify(Signature)。
4. WHEN 签名写入后重新计算签名作用域，THE 作用域 SHALL 与签名时使用的作用域一致（即写入 signature 字段不改变 Manifest_without_signature 的编码）。

### Requirement 7: 错误模型扩展

**User Story:** 作为 Fayger 调用方，我希望 M4 的失败沿用统一错误模型，以便按稳定错误码分支处理。

#### Acceptance Criteria

1. THE M4 错误码集合 SHALL 在既有码之外至少新增 `LDR_SIGNATURE_FAIL` 一个稳定值。
2. THE M4 新增错误 SHALL 复用统一的 `FaygerError` 结构，`source_layer` 为 `Loader`。
3. WHEN M4 各组件产生错误，THE 错误对象 SHALL 在 `context` 中提供足以定位失败的结构化字段（如 `reason`、`public_key_ref`、`algorithm`）。
4. THE M0/M1/M2/M3 既有错误码 SHALL 保持语义不变，M4 不复用或重定义它们。

### Requirement 8: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M4 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M4 实现 SHALL 至少包含以下属性测试：签名验证决策表（Absent/Valid/Invalid/UnknownKey/UnsupportedAlgorithm × EnforceMode）、签名后篡改被检测（对签名作用域内任一字节修改后验证失败）、可信根更新可见性（更新后加载用新根集合）、签名-加载往返（sign→serialize→load with matching TrustStore 通过）。
2. THE M4 实现 SHALL 提供至少一个端到端用例：生成密钥对 → seal + sign 一个 BuF → 用含对应公钥的 TrustStore 加载成功 → 用不含该公钥的 TrustStore 加载失败（UnknownKey）。
3. THE M4 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M4 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0/M1/M2/M3 既有测试在 M4 引入后仍全部通过。

### Requirement 9: M4 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M4 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M4 实现 SHALL NOT 引入 X.509 / 证书链 / 撤销列表；TrustStore 是一组裸公钥（key_ref → public_key）。
2. THE M4 实现 SHALL NOT 支持 `ed25519` 以外的签名算法；其它算法声明一律 `UnsupportedAlgorithm`。
3. THE M4 实现 SHALL NOT 持久化 TrustStore 或密钥；它们是内存对象。
4. THE M4 实现 SHALL NOT 修改 M0/M1/M2/M3 已固化的 BuF 字节布局、运行层契约、适配层契约或既有错误码语义；M4 仅在加载链中新增 Verify(Signature) 阶段与签名横切组件。
5. THE M4 实现 SHALL NOT 引入部分加载下的签名一致性 Property（那是 M6）；M4 在完整 `&[u8]` 上验证签名。
6. THE M4 实现 SHALL NOT 引入运行层 / 适配层改动。

## 约束与假设

- **目标语言**：M4 仅交付 Rust 实现。
- **crate 归属**：M4 的签名与 TrustStore 逻辑落在 `fayger-loader` crate（签名是加载链的一环）；签名基元可置于 `fayger-loader` 的子模块。
- **新增依赖**：引入一个成熟、纯 Rust、活跃维护的 ed25519 实现（候选 `ed25519-dalek`），版本在技术设计阶段固化到 `Cargo.toml`。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **错误码稳定性**：M4 新增错误码遵循蓝图 §8.7 第 3 条——只新增、不复用、不改语义。
- **签名作用域与 M1 digest 区间的关系**：M1 的 `manifest_digest` 覆盖 `Header || Manifest || Section_Index`（含 signature 字段）；M4 的签名作用域覆盖 `Header || Manifest_without_signature || Section_Index`（不含 signature 字段）。两者区间相关但 Manifest 部分不同——这是有意的：digest 防整体篡改，签名防来源伪造且避免自引用循环。
