# Design Document

## Overview

本文是 M6 里程碑（LoadProfile + profile 驱动的部分加载）的技术设计。M6 在 `fayger-loader` 中引入 [`LoadProfile`] 与确定性的 Section 选择谓词，并在 M5（BuF_Source + Eager/Lazy）之上把"被选中 Section"从"全部"收窄为"profile 命中的子集"。

设计目标按优先级：

1. **正确性优先**：Section 选择决策性、Required 缺失拒绝、Optional 跳过记录、部分加载下签名一致性，都以可被 PBT 验证的形式表达。
2. **不破坏既有契约**：M6 不改 M0–M5 的对外行为。M5 的 `load_from_source(source, strategy)` 保持"全选"语义不变；profile 加载通过**新入口** `load_with_profile(...)` 提供。只新增一个 Loader 错误码 `LDR_PROFILE_REQUIRED_SECTION_MISSING`。
3. **复用 M4/M5**：被选中 Section 的偏移与读取/校验复用 M5 的"骨架 + 按段 plan"机制；签名一致性是 M4 `signed_scope`（作用域不含 Section 段体）的直接推论。
4. **签名作用域不变**：M6 不改签名作用域定义；部分加载下"不同 profile 子集签名结果相同"正是该定义的推论（Req 9.4）。

## Architecture

### M6 在蓝图加载链里的位置

蓝图加载链：`... → Verify(Digest) → Verify(Signature) → NegotiateVersion → Select(Sections by LoadProfile) → Resolve → Read(Selected Section Bodies)? → HandOff`

- M6 落实 `Select(Sections by LoadProfile)`，并把后续 `Read(Selected Section Bodies)?` 收窄到被选中子集。
- 选择发生在 **读取被选中 Section 段体之前**（Req 6.3）：Required 缺失即失败，不读任何段体。

### 部分加载数据流

```
load_with_profile(source, strategy, profile):
    # 1. 可信骨架（同 M5 Lazy）：读 Header → sections_region_offset → 骨架 → Trailer
    obj = parse_skeleton(skeleton, trailer, total)
    verify_manifest_digest(skeleton, obj)          # phase=eager
    # 2. 选择（M6 核心）
    sel = select_sections(&obj.manifest, &profile)?    # Required 缺失 → LDR_PROFILE_REQUIRED_SECTION_MISSING
    # 3. 按策略加载被选中 Section
    plan = build_plan(&obj.manifest)               # 偏移按"全部"section 累加（文件布局含被跳过段）
    match strategy:
        Eager: 逐个 selected section read+verify，存入 map；skipped 不读
        Lazy:  保留 source + plan + selected 集合，按需 fetch
    return PartialArtifact { manifest, header, trailer, strategy, selected, skipped, bodies/loader }
```

> 关键：**plan 偏移按 manifest 全部 section 顺序累加**，因为被跳过的 Section 仍占据文件字节，被选中 Section 的偏移依赖其前所有段（含被跳过段）的长度。"选不选"只影响是否读取/可访问，不影响偏移计算。

### crc32 在部分加载下不校验

与 M5 Lazy 一致：crc32 覆盖全部 Section 段体，部分加载按定义不读全，故不校验 crc32。被选中 Section 的完整性由 `manifest_digest`（骨架，覆盖 Section_Index 含各段 digest）+ 每段 `digest`（读取时）保证。

### Crate 拓扑（M6 扩展 fayger-loader）

```
release/dev/core/crates/fayger-loader/src/
├── select.rs       (M6：LoadProfile / SkipReason / SkipRecord / SelectionResult / select_sections)
├── profile_load.rs (M6：load_with_profile / load_with_profile_signed / PartialArtifact)
└── lib.rs          (pub use)
```

依赖不变（M6 不引入第三方依赖）。

## Components and Interfaces

### `fayger-error` crate（M6 增量）

```rust
#[non_exhaustive]
pub enum ErrorCode {
    // M0–M5 ...
    /// `LDR_PROFILE_REQUIRED_SECTION_MISSING`：某 Required Section 在当前 LoadProfile 下
    /// 不满足约束。context 含 section_id 与未满足的约束项列表、phase=eager。
    LdrProfileRequiredSectionMissing,
}
```

### 模块 `select`：LoadProfile 与选择谓词

```rust
/// 运行环境画像。
#[derive(Debug, Clone, Default)]
pub struct LoadProfile {
    pub target_class: String,              // 描述性，M6 不据它选择（Req 9.6）
    pub capabilities: BTreeSet<String>,
    pub max_section_bytes: Option<u64>,
    pub features: BTreeSet<String>,
}

/// 跳过原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason { CapabilityNotGranted, FeatureMissing, OverMaxSize }

/// 被跳过的 Optional Section 记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipRecord { pub section_id: String, pub reasons: BTreeSet<SkipReason> }

/// 选择结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionResult {
    pub selected: Vec<String>,        // 被选中 id（按 manifest 顺序）
    pub skipped: Vec<SkipRecord>,     // 被跳过的 Optional（按 manifest 顺序）
}

/// 选择谓词：profile 是否满足该 section 的全部约束。返回未满足原因集合（空 = fits）。
fn unmet_reasons(section: &SectionDescriptor, profile: &LoadProfile) -> BTreeSet<SkipReason>;

/// 选择：fits 的进 selected；不 fits 的 Required → Err，Optional → skipped。
pub fn select_sections(manifest: &BuFManifest, profile: &LoadProfile)
    -> Result<SelectionResult, FaygerError>;
```

`unmet_reasons` 实现（对应 Req 2.1）：

```
reasons = {}
c = section.profile_constraints
if !(c.required_capabilities ⊆ profile.capabilities): reasons += CapabilityNotGranted
if !(c.required_features ⊆ profile.features):          reasons += FeatureMissing
if (c.max_size = Some(m) && section.length > m) ||
   (profile.max_section_bytes = Some(p) && section.length > p): reasons += OverMaxSize
return reasons   // 空 = fits
```

`select_sections` 实现（对应 Req 3，确定性按 manifest 顺序）：

```
selected = []; skipped = []
for s in manifest.sections:       # manifest 顺序
    r = unmet_reasons(s, profile)
    if r.is_empty(): selected.push(s.id)
    else if s.visibility == Required:
        return Err(LDR_PROFILE_REQUIRED_SECTION_MISSING {
            section_id: s.id, unmet: r (有序列表), phase: eager })   # 第一个 Required 缺失即失败
    else: # Optional
        skipped.push(SkipRecord { s.id, r })
return Ok(SelectionResult { selected, skipped })
```

> Required 缺失立即返回第一个（manifest 顺序），满足 Req 3.5 的确定性。

### 模块 `profile_load`：部分加载入口与 PartialArtifact

```rust
/// profile 驱动的部分加载产出物。
pub struct PartialArtifact {
    pub manifest: BuFManifest,
    pub raw_header: Header,
    pub raw_trailer: Trailer,
    pub strategy: LoadStrategy,
    pub selection: SelectionResult,   // selected + skipped
    // Eager：已读取并校验的被选中段体；Lazy：source + plan + selected 集合
    eager_bodies: Option<BTreeMap<String, Vec<u8>>>,
    lazy: Option<LazyState>,
}
struct LazyState {
    source: Box<dyn BuFSource>,
    sections_region_offset: u64,
    plan: Vec<SectionPlan>,           // 全部 section 的偏移/长度/digest
    selected: BTreeSet<String>,
    cache: BTreeMap<String, Vec<u8>>,
}

impl PartialArtifact {
    pub fn section(&mut self, id: &str) -> Result<&[u8], FaygerError>;
    pub fn is_selected(&self, id: &str) -> bool;
    pub fn selected_ids(&self) -> &[String];
    pub fn skipped(&self) -> &[SkipRecord];
}

/// 部分加载入口（不含签名）。
pub fn load_with_profile(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
    profile: &LoadProfile,
) -> Result<PartialArtifact, FaygerError>;

/// 部分加载入口（含签名校验，复用 M4 verify_signature）。
pub fn load_with_profile_signed(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
    profile: &LoadProfile,
    truststore: &TrustStore,
) -> Result<PartialArtifact, FaygerError>;
```

`section(id)` 行为（Req 4.4）：

```
if id ∉ selection.selected:
    return Err(LDR_PARSE_FAIL { reason: SectionSkipped, section_id })
match (eager_bodies, lazy):
    (Some(map), _): Ok(map[id])
    (_, Some(state)): state.fetch(id)   # 首访读取+校验+缓存（同 M5 Lazy 语义）
```

`load_with_profile` Eager 分支：

```
读骨架 → parse_skeleton → verify_manifest_digest（phase=eager）
sel = select_sections(manifest, profile)?
plan = build_plan(manifest)   # 全 section
eager_bodies = {}
for id in sel.selected:
    p = plan.find(id)
    bytes = read_exact(source, sections_region_offset + p.offset, p.length, eager)
    verify sha256(bytes) == p.digest else LDR_DIGEST_MISMATCH(phase=eager, section_id)
    eager_bodies[id] = bytes
PartialArtifact::eager(...)
```

`load_with_profile` Lazy 分支：

```
读骨架 → parse_skeleton → verify_manifest_digest（phase=eager）
sel = select_sections(manifest, profile)?
PartialArtifact::lazy(source, sections_region_offset, build_plan(manifest), sel.selected_set, ...)
# fetch 时：id 必须 ∈ selected；否则 SectionSkipped；否则 read+verify（LDR_LAZY_DIGEST_MISMATCH / LDR_LAZY_SOURCE_UNAVAILABLE，phase=lazy）
```

`load_with_profile_signed` 在 `verify_manifest_digest` 之后、`select_sections` 之前插入：

```
verify_signature(skeleton, &obj_skeleton, truststore)?   # 复用 M4；signed_scope 只用 header+manifest+section_index，全在骨架内
```

> 签名校验只需骨架字节：`signed_scope` 读 Header（归一化）+ Manifest_without_sig（重编码）+ Section_Index（从 bytes 切，位于 `[0, sections_region_offset)` 即骨架内）。因此把骨架字节传给 `verify_signature` 即可，无需读 Section 段体——这正是"部分加载下签名一致"的实现基础。

## Data Models

### 选择谓词与跳过原因

```
fits(s, p) ⟺ unmet_reasons(s, p) == ∅

unmet_reasons 的三类：
  CapabilityNotGranted ⟸ s.required_capabilities ⊄ p.capabilities
  FeatureMissing       ⟸ s.required_features ⊄ p.features
  OverMaxSize          ⟸ (s.max_size=Some(m) ∧ s.length>m) ∨ (p.max_section_bytes=Some(q) ∧ s.length>q)
```

### 选择决策表

| fits(s) | s.visibility | 结果 |
| --- | --- | --- |
| 真 | * | s ∈ selected |
| 假 | Required | Err(LDR_PROFILE_REQUIRED_SECTION_MISSING)（第一个，manifest 顺序）|
| 假 | Optional | s ∈ skipped（reasons 非空）|

### 部分加载下签名一致性（图）

```
signed_scope = Header_normalized || canonical(Manifest_without_sig) || Section_Index
                                                                        └─ 全在骨架内
被跳过/未读取的 Section 段体 —— 不在 signed_scope 内
⟹ 任意 profile 选中的不同 Section 子集，signed_scope 相同 ⟹ 签名校验结果相同
```

## Error Handling

### 选择/部分加载错误映射

| 触发条件 | ErrorCode | phase | 关键 context |
| --- | --- | --- | --- |
| Required Section fits 为假 | LdrProfileRequiredSectionMissing | eager | section_id, unmet（约束项列表）|
| 访问被跳过/未选中 Section | LdrParseFail | — | reason=SectionSkipped, section_id |
| 被选中 Section（Eager）digest 失配 | LdrDigestMismatch | eager | section_id |
| 被选中 Section（Lazy）digest 失配 | LdrLazyDigestMismatch | lazy | section_id |
| 源读取失败 | LdrSourceReadFailed / LdrLazySourceUnavailable | eager/lazy | offset/section_id |
| 签名失败（signed 入口）| LdrSignatureFail | — | reason |

所有错误 `source_layer = Loader`，`cause = None`。

### 默认安全姿态

- **Required 缺失即拒**：在读取任何被选中 Section 段体之前失败（Req 6.3）。
- **被跳过段不可访问**：访问被跳过 id 返回 `SectionSkipped`，绝不读取它。
- **偏移按全段累加**：被跳过段仍占文件字节，偏移计算不能跳过它们，否则被选中段读错位置。
- **不 panic**：选择与读取走 Result。

## Correctness Properties

> 从需求 Requirement 8 提取，对应 PBT。沿用前序里程碑的 `.regressions` 归档与配置。

### Property 1: Section 选择决策性

*For any* `(manifest, profile)`：`select_sections` 的 `selected` 恰为 `{ s : unmet_reasons(s,profile)=∅ }` 的 id（manifest 顺序）；每个被跳过 Optional 的 `reasons` 非空且等于其 unmet_reasons；对相同输入多次调用结果完全相同。若存在 fits 为假的 Required，则返回 `LDR_PROFILE_REQUIRED_SECTION_MISSING`。

**Validates: Requirements 2.1–2.4, 3.1–3.5, 8.1**

### Property 2: 被选中 Section 内容与全选一致

*For any* 封装好的 BuF 与任意 profile（其下加载成功）：每个被选中 Section 的内容与不带 profile（M5 全选）加载在相同 Section 上逐字节相等。

**Validates: Requirements 4.5, 8.1**

### Property 3: 被跳过 Section 不可访问且不被读取

*For any* 部分加载产出 与任一被跳过 id：`section(id)` 返回 `SectionSkipped` 错误；且 Eager 模式下 `eager_bodies` 不含该 id、Lazy 模式下对该 id 的访问不触发 source 读取。

**Validates: Requirements 4.2, 4.3, 4.4, 8.1**

### Property 4: 部分加载下签名一致性

*For any* 已签名的 BuF 与任意两个 profile `p1`、`p2`（均加载成功，可能选中不同子集）：用同一 TrustStore 的签名校验结果相同（均通过）。篡改被跳过 Section 段体不改变签名结果；篡改 Section_Index 改变签名结果。

**Validates: Requirements 5.2, 5.3, 5.4, 5.5, 8.1**

### Property 5: Required 缺失拒绝 / Optional 跳过降级

*For any* 含一个带约束的 Section：当该 Section 为 Required 且 profile 不满足约束 → 加载失败 `LDR_PROFILE_REQUIRED_SECTION_MISSING`；当为 Optional 且不满足 → 加载成功且该 id 在 skipped 中、reasons 非空。

**Validates: Requirements 3.2, 3.3, 8.1**

## Testing Strategy

### 测试金字塔

```
        ┌──────────────────────────────────────────┐
        │ 端到端：Required+Optional 带约束 BuF →     │
        │ 满足/不满足 profile 加载，断言选中/跳过/失败 │
        └──────────────────────────────────────────┘
      ┌────────────────────────────────────────────────┐
      │  PBT (5 properties × ≥500x)                     │
      └────────────────────────────────────────────────┘
   ┌─────────────────────────────────────────────────────┐
   │ 单元：unmet_reasons 各约束、select_sections 决策表、   │
   │ PartialArtifact section 访问、Eager/Lazy 部分加载      │
   └─────────────────────────────────────────────────────┘
```

### 单元测试

- `select`：unmet_reasons 对 capability/feature/max_size 各缺一类；select_sections 全选/部分跳过/Required 失败；空 manifest。
- `profile_load`：Eager 部分加载只存被选中段；Lazy 访问被跳过 id 报 SectionSkipped；被选中段内容正确；signed 入口签名校验。

### PBT 生成器（fayger-loader 测试内）

```rust
// 复用 fayger_buf::testing::arb_buf_object（其 sections 默认无约束、Required）
// 新增：给若干 section 注入 profile_constraints 与 Optional 可见性的生成器
fn arb_constrained_manifest_bytes() -> impl Strategy<Value = (Vec<u8>, /*约束信息*/)>;
fn arb_load_profile() -> impl Strategy<Value = LoadProfile>;
```

> 注：`arb_buf_object` 生成的 section 默认 `visibility=Required`、`profile_constraints` 默认空（恒 fits）。M6 测试在其上注入约束与 Optional，再 seal/序列化。

### 端到端测试

`tests/profile_e2e.rs`：构造一个含 `code`(Required,无约束) + `gpu`(Optional, required_features={"gpu"}) + `huge`(Optional, max_size=10) 的 BuF。
- profile A（features={"gpu"}，无 max）：选中 code+gpu，跳过 huge（OverMaxSize，若 huge.length>10）。
- profile B（features={}）：选中 code，跳过 gpu（FeatureMissing）+ huge。
- 构造一个 Required 带 features 约束、profile 不满足 → 加载失败。
- 签名：对带约束 BuF 签名后，两个 profile 加载的签名校验都通过。

### 反例归档 / CI

沿用前序里程碑：每个 PBT 文件 `failure_persistence = WithSource("regressions")`，`.regressions` 进 git。

## Dependency Versions

M6 不引入新的第三方运行时依赖。

## Open Questions

- **Q1**：被选中 Section 的偏移是否要按"被选中子集"还是"全部 section"累加？
  - 决定：按**全部** manifest section 累加。文件布局含被跳过段，被选中段在文件中的物理偏移依赖其前所有段（含被跳过）。按子集累加会读错位置。Property 2（内容与全选一致）会立刻暴露此类错误。
- **Q2**：`target_class` 是否参与选择？
  - 决定：不参与（Req 9.6）。M6 约束仅来自 capabilities / features / max_size。target_class 是描述性字段，为后续"按类别预置约束集"预留。
- **Q3**：部分加载 Eager 是否校验 crc32？
  - 决定：不校验（同 M5 Lazy）。crc32 覆盖全部段体，部分加载不读全。被选中段完整性由 manifest_digest + 每段 digest 保证。
