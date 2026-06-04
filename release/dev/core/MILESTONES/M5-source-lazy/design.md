# Design Document

## Overview

本文是 M5 里程碑（BuF_Source 抽象 + Eager/Lazy 加载策略）的技术设计。M5 在 `fayger-loader` 中把"从哪里读字节"解耦为 [`BuFSource`] 抽象，并提供两种 [`LoadStrategy`]：`Eager`（返回前读全部）与 `Lazy`（返回前只读可信骨架，Section 段体首次访问时按需取回）。

设计目标按优先级：

1. **正确性优先**：Eager / Lazy 语义等价、Lazy 首次访问校验、阶段错误标注，都以可被 PBT 验证的形式表达。
2. **不破坏既有契约**：M5 不改 M0–M4 的对外行为。既有 `load(bytes)` / `load_signed(bytes)` 保持"全字节切片"入口不变；source 加载通过**新入口** `load_from_source(source, strategy)` 提供。只新增三个 Loader 错误码（`#[non_exhaustive]` 保证不破坏既有 match）。
3. **复用 M1**：Eager 模式读全字节后直接复用 M1 `parse + verify_digest`，保证与既有加载链一致（Req 2.5）。
4. **不暴露 fayger-buf 内部**：Lazy 模式的 Section 偏移由 Manifest 中受 manifest_digest 保护的各 Section 声明长度按顺序累加得到（与 M0 序列化器布局一致），无需暴露 `SectionIndexEntry`。

## Architecture

### M5 在蓝图加载链里的位置

蓝图加载链：`Read → Parse → Verify(Structural) → Verify(Digest) → Verify(Signature) → ... → Read(Selected Section Bodies)? → HandOff`

- M5 替换/补齐 `Read` 前端（BuF_Source）与 `Read(Selected Section Bodies)?` 的 Eager/Lazy 取舍。
- Eager：在 load 返回前完成所有 Section 段体读取（等价于把完整字节交给 M1）。
- Lazy：返回前只读可信骨架（Header / Manifest / Section_Index）并校验 manifest_digest；Section 段体推迟到首次访问。

### 两种策略的数据流

```
load_from_source(source, Eager):
    total = source.length()
    bytes = source.read_at(0, total)          # 读全
    obj = fayger_buf::parse(bytes)             # 复用 M1
    verify_digest(bytes, &obj)                 # 复用 M1（含 crc32）
    return LoadedArtifact::eager(obj)

load_from_source(source, Lazy):
    total = source.length()
    skeleton_len = sections_region_offset      # = Header+Manifest+Section_Index 终点
    skeleton = source.read_at(0, skeleton_len) # 只读可信骨架
    # 用骨架 + 一个"零 Section payload 占位"重建可被 parse 的最小字节？——见下"骨架解析"
    obj_skeleton = parse_skeleton(skeleton, total)
    verify_manifest_digest(skeleton, &obj_skeleton)   # 只校验 manifest_digest
    loader = SectionLoader::new(source, manifest 快照, sections_region_offset)
    return LoadedArtifact::lazy(obj_skeleton, loader)
```

### 骨架解析（Lazy 的关键）

Lazy 模式不能直接 `parse(skeleton)`——M1 的 `parse` 要求完整字节流（含 Sections 与 Trailer，且校验 total_length 与 Section 边界）。M5 的做法：

1. **先用一次 `read_at(0, total)` 读 Header**（前 `HEADER_SIZE` 字节）拿到 `manifest_offset/length`、`section_index_offset/length`，推导 `sections_region_offset = section_index_offset + section_index_length`。
   - 实际实现：Lazy 模式先 `read_at(0, HEADER_SIZE)` 读 Header；再 `read_at(0, sections_region_offset)` 读可信骨架（Header+Manifest+Section_Index）。
2. **校验 manifest_digest**：`sha256(skeleton[0, sections_region_offset]) == trailer.manifest_digest`？——但 Lazy 不读 Trailer 段体之外……Trailer 在文件末尾，含 manifest_digest。Lazy 必须读 Trailer 才能拿到期望的 manifest_digest。

因此 Lazy 模式读取的"可信骨架"精确为两段：

- `read_at(0, sections_region_offset)` —— Header + Manifest + Section_Index。
- `read_at(total - TRAILER_SIZE, TRAILER_SIZE)` —— Trailer（拿 manifest_digest 与 crc32 占位、total_length）。

Lazy 校验：
- 结构：Header magic、offset 边界、Manifest CBOR 合法、Manifest 必填字段、Section_Index 与 Manifest.sections 一致（这些都只依赖骨架两段，可在 Lazy 阶段完成）。
- manifest_digest：`sha256(skeleton_bytes[0, sections_region_offset]) == trailer.manifest_digest`。
- **不校验 crc32**（crc32 覆盖 Section 段体，Lazy 未读，见 Req 9.6）。
- **不校验 section digest**（推迟到首次访问）。

为复用现有解析逻辑且不暴露 fayger-buf 内部，M5 在 `fayger-buf` 暴露一个**骨架解析入口**（见下"fayger-buf 增量"）。

### Crate 拓扑（M5 扩展 fayger-loader + fayger-buf 小增量）

```
release/dev/core/crates/
├── fayger-buf/src/
│   ├── parser.rs   (新增 pub fn parse_skeleton(header_mid_bytes, trailer_bytes, total_len) -> BuFObject 骨架)
│   └── lib.rs      (pub use parse_skeleton)
└── fayger-loader/src/
    ├── source.rs   (M5：BuFSource trait + MemorySource + SourceError 映射)
    ├── strategy.rs (M5：LoadStrategy + LoadedArtifact + SectionLoader)
    ├── source_load.rs (M5：load_from_source 编排)
    └── lib.rs      (pub use)
```

依赖不变（M5 不引入第三方依赖）。

### fayger-buf 增量：骨架解析

为让 Lazy 在不读 Section 段体的情况下得到可信的 Manifest 与 Header/Trailer，`fayger-buf` 新增：

```rust
/// 仅从"骨架字节"解析 BuFObject（不含 Section 段体、不校验 crc32 与 section digest）。
///
/// skeleton: [0, sections_region_offset) 的字节（Header+Manifest+Section_Index）。
/// trailer:  末 TRAILER_SIZE 字节。
/// total_len: 制品总长度。
/// 返回的 BuFObject.sections 为空（Lazy 模式按需填充），manifest.sections 完整。
pub fn parse_skeleton(skeleton: &[u8], trailer: &[u8], total_len: u64)
    -> Result<BuFObject, FaygerError>;
```

它复用 `parse` 的 read_header / read_manifest / read_section_index / cross_check 逻辑，但跳过 Section 段体读取与 Trailer.total_length-vs-实际长度的全量校验（改为校验 total_len 参数与 Header/SectionIndex 自洽）。M5 的 manifest_digest 校验在 `fayger-loader` 侧用 `verify::verify_manifest_digest`-风格逻辑完成。

> 取舍：把骨架解析放在 `fayger-buf`（它已有全部解析私有函数），避免在 loader 里重写一份 CBOR 解析。`parse` 与 `parse_skeleton` 共享内部 helper，行为一致。

## Components and Interfaces

### `fayger-error` crate（M5 增量）

```rust
#[non_exhaustive]
pub enum ErrorCode {
    // M0–M4 ...
    /// `LDR_SOURCE_READ_FAILED`：BuF_Source 读取失败（read_at/length 出错或字节数不符）。
    LdrSourceReadFailed,
    /// `LDR_LAZY_SOURCE_UNAVAILABLE`：Lazy 后续访问时 BuF_Source 不可用。
    LdrLazySourceUnavailable,
    /// `LDR_LAZY_DIGEST_MISMATCH`：Lazy 首次访问时 Section digest 校验失败。
    LdrLazyDigestMismatch,
}
```

### 模块 `source`：BuF_Source 抽象

```rust
/// 源读取错误（实现方返回；加载层映射为 FaygerError）。
#[derive(Debug, Clone)]
pub struct SourceError { pub message: String }

/// BuF 的存储后端抽象。最低要求随机访问读 + 总长度。
pub trait BuFSource {
    fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, SourceError>;
    fn length(&self) -> Result<u64, SourceError>;
}

/// 内置内存后端。
pub struct MemorySource { bytes: Vec<u8> }
impl MemorySource { pub fn new(bytes: impl Into<Vec<u8>>) -> Self; }
impl BuFSource for MemorySource {
    fn read_at(&self, offset, length) -> Result<Vec<u8>, SourceError> {
        // 越界返回 Err；返回恰好 length 字节
    }
    fn length(&self) -> Result<u64, SourceError> { Ok(self.bytes.len() as u64) }
}
```

加载层封装一个 `read_exact(source, offset, length, phase) -> Result<Vec<u8>, FaygerError>`：
- 调 `source.read_at`；Err → `LDR_SOURCE_READ_FAILED`（eager）或 `LDR_LAZY_SOURCE_UNAVAILABLE`（lazy），按 phase。
- 返回字节数 != length → `LDR_SOURCE_READ_FAILED`（携带 expected/actual）。

### 模块 `strategy`：LoadStrategy / LoadedArtifact / SectionLoader

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStrategy { Eager, Lazy }

/// Lazy 模式的按需读取器。
pub struct SectionLoader {
    source: Box<dyn BuFSource>,
    sections_region_offset: u64,
    // 每个 section 的 (offset_in_region, length, digest)，由 manifest 顺序累加得到
    plan: Vec<SectionPlan>,
    cache: BTreeMap<String, Vec<u8>>,   // section_id -> payload
}
struct SectionPlan { id: String, offset_in_region: u64, length: u64, digest: Digest }

impl SectionLoader {
    /// 首次访问触发读取 + digest 校验 + 缓存；再次访问命中缓存。
    pub fn fetch(&mut self, section_id: &str) -> Result<&[u8], FaygerError>;
    pub fn is_loaded(&self, section_id: &str) -> bool;
    pub fn loaded_ids(&self) -> Vec<String>;
}

/// source 加载链产出物。
pub struct LoadedArtifact {
    pub manifest: BuFManifest,
    pub raw_header: Header,
    pub raw_trailer: Trailer,
    pub strategy: LoadStrategy,
    eager_sections: Option<Vec<Section>>,   // Eager 模式：全部段
    loader: Option<SectionLoader>,           // Lazy 模式：按需
}
impl LoadedArtifact {
    pub fn section(&mut self, id: &str) -> Result<&[u8], FaygerError>;  // 统一访问入口
    pub fn is_eager(&self) -> bool;
}
```

`SectionLoader::fetch` 流程（对应 Req 3 / 4 / 5）：

```
if cache contains id: return cached          # 命中缓存，不读 source
plan = plan.find(id) or error
abs_offset = sections_region_offset + plan.offset_in_region
bytes = read_exact(source, abs_offset, plan.length, phase=lazy)
        # read 失败 → LDR_LAZY_SOURCE_UNAVAILABLE (phase=lazy)
if sha256(bytes) != plan.digest.value (algorithm sha256):
        return LDR_LAZY_DIGEST_MISMATCH (phase=lazy, section_id)   # 不缓存
cache.insert(id, bytes); return cached
```

`SectionPlan` 由 manifest.sections 按顺序累加构造：`offset_in_region[i] = Σ length[0..i]`，`length[i] = manifest.sections[i].length`，`digest[i] = manifest.sections[i].digest`。这与 M0 序列化器一致，且 manifest 受 manifest_digest 保护（Lazy 阶段已校验），故 plan 可信。

### 模块 `source_load`：load_from_source

```rust
pub fn load_from_source(
    source: Box<dyn BuFSource>,
    strategy: LoadStrategy,
) -> Result<LoadedArtifact, FaygerError>;
```

Eager 分支：

```
total = length(source)                         # 失败 → LDR_SOURCE_READ_FAILED(eager)
bytes = read_exact(source, 0, total, eager)
obj = fayger_buf::parse(bytes)                 # LDR_PARSE_FAIL（结构）
verify_digest(bytes, &obj)                     # LDR_DIGEST_MISMATCH(标注 phase=eager)
LoadedArtifact::eager(obj.manifest, obj.raw_header, obj.raw_trailer, obj.sections)
```

> 注：M1 `verify_digest` 的错误未带 `phase`。M5 在 Eager 分支捕获其错误并补标 `phase=eager`（用 with_context 包裹，不改 M1）。

Lazy 分支：

```
total = length(source)                         # 失败 → LDR_SOURCE_READ_FAILED(eager)*
header_bytes = read_exact(source, 0, HEADER_SIZE, eager)
header = Header::read(header_bytes)            # 复用 fayger-buf
sections_region_offset = header.sections_offset()
skeleton = read_exact(source, 0, sections_region_offset, eager)
trailer = read_exact(source, total - TRAILER_SIZE, TRAILER_SIZE, eager)
obj = fayger_buf::parse_skeleton(skeleton, trailer, total)   # 结构 + manifest 必填
verify_manifest_digest(skeleton, &obj)         # LDR_DIGEST_MISMATCH(phase=eager)
loader = SectionLoader::from_manifest(source, &obj.manifest, sections_region_offset)
LoadedArtifact::lazy(obj.manifest, obj.raw_header, obj.raw_trailer, loader)
```

> *Lazy 模式下"读骨架/Header/Trailer"仍发生在 load 调用栈内，按 Req 5.2 标注 `phase=eager`。只有 SectionLoader.fetch 内的失败标 `phase=lazy`。

## Data Models

### Section 偏移推导（Lazy plan）

```
文件布局：Header | Manifest | Section_Index | Sections | Trailer
                                              ^sections_region_offset

sections_region_offset = header.section_index_offset + header.section_index_length

Sections 段内：
  section[0] @ [0, len0)
  section[1] @ [len0, len0+len1)
  section[i] @ [Σlen[0..i], Σlen[0..=i])

绝对偏移 abs[i] = sections_region_offset + Σlen[0..i]
```

`len[i] = manifest.sections[i].length`，`digest[i] = manifest.sections[i].digest`。

### 阶段标注

| 错误产生点 | phase |
| --- | --- |
| length/read_at（load 调用栈内：Eager 全读、Lazy 读骨架/Header/Trailer） | eager |
| parse / verify_digest / verify_manifest_digest（load 调用栈内） | eager |
| SectionLoader.fetch 内的 read_at 失败 | lazy |
| SectionLoader.fetch 内的 digest 失败 | lazy |

### Eager / Lazy 等价性

```
obj_e = load_from_source(src, Eager)
obj_l = load_from_source(src, Lazy); force_all(obj_l)

obj_e.manifest == obj_l.manifest
obj_e.raw_header == obj_l.raw_header
obj_e.raw_trailer == obj_l.raw_trailer
∀ section_id: obj_e.section(id) == obj_l.section(id)   # 逐字节
```

## Error Handling

### 源加载错误映射

| 触发条件 | ErrorCode | phase | 关键 context |
| --- | --- | --- | --- |
| length()/read_at() 出错（load 栈内） | LdrSourceReadFailed | eager | offset, length, detail |
| read_at 返回字节数 != 请求 | LdrSourceReadFailed | eager | offset, expected, actual |
| Eager Section digest 失配 | LdrDigestMismatch | eager | section_id |
| Lazy 骨架 manifest_digest 失配 | LdrDigestMismatch | eager | — |
| Lazy fetch read_at 出错 | LdrLazySourceUnavailable | lazy | section_id, detail |
| Lazy fetch digest 失配 | LdrLazyDigestMismatch | lazy | section_id |

所有错误 `source_layer = Loader`，`cause = None`。

### 默认安全姿态

- **Lazy 也校验骨架**：manifest_digest 在 load 返回前校验，保证 Section_Index（含各段 digest 与偏移）可信。
- **不缓存校验失败的段**：Lazy digest 失配的段不进缓存，避免毒化。
- **字节数严格**：read_at 返回字节数必须等于请求，否则视为读取失败（防截断）。
- **不 panic**：越界、溢出走 Result。

## Correctness Properties

> 从需求 Requirement 8 提取，对应 PBT。沿用前序里程碑的 `.regressions` 归档与配置。

### Property 1: Eager / Lazy 等价性

*For any* 封装好的 BuF 字节流 `b`，令 `src = MemorySource(b)`，`obj_e = load_from_source(src, Eager)`、`obj_l = load_from_source(src, Lazy)` 后对 `obj_l` force-all：则 `manifest`、`raw_header`、`raw_trailer` 相等，且每个 Section 内容逐字节相等。

**Validates: Requirements 6.1, 6.2, 6.3, 8.1**

### Property 2: Lazy 首次访问校验与缓存

*For any* Lazy 加载的 artifact 与任一 section_id：首次 `section(id)` 触发对 source 的恰好一次 read_at 并校验 digest；再次 `section(id)` 命中缓存不再 read_at；返回内容与 Eager 一致。

**Validates: Requirements 3.3, 3.4, 8.1**

### Property 3: 源读取失败的阶段标注

*For any* 一个在第 k 次 read_at 后失败的 source：Eager 加载在 load 栈内失败时 `phase=eager` 且码为 `LDR_SOURCE_READ_FAILED`；Lazy 加载在 fetch 阶段因 source 失败时 `phase=lazy` 且码为 `LDR_LAZY_SOURCE_UNAVAILABLE`。

**Validates: Requirements 4.1, 4.2, 5.1, 5.2, 5.3, 8.1**

### Property 4: Lazy 摘要篡改被检测

*For any* 封装好的字节流，对其某个 Section payload 区间任一字节翻转后承载于 MemorySource：Lazy 加载本身成功（骨架未变），但对被篡改 Section 的首次 `fetch` 返回 `LDR_LAZY_DIGEST_MISMATCH`（phase=lazy），且不缓存。

**Validates: Requirements 3.5, 8.1**

### Property 5: read_at 字节数契约

*For any* `(offset, length)` 落在 `[0, total)` 内的请求，MemorySource.read_at 返回恰好 `length` 字节；越界请求返回 Err（被加载层映射为 `LDR_SOURCE_READ_FAILED`）。

**Validates: Requirements 1.2, 1.3, 4.3, 8.1**

## Testing Strategy

### 测试金字塔

```
        ┌──────────────────────────────────────┐
        │ 端到端：MemorySource → Eager/Lazy 等价 │
        │ + 失败 Source → Lazy 不可用            │
        └──────────────────────────────────────┘
      ┌────────────────────────────────────────────┐
      │  PBT (5 properties × ≥500x)                 │
      └────────────────────────────────────────────┘
   ┌───────────────────────────────────────────────────┐
   │ 单元：MemorySource read_at 边界、SectionLoader fetch │
   │ /缓存、阶段标注、parse_skeleton                       │
   └───────────────────────────────────────────────────┘
```

### 单元测试

- `source`：MemorySource read_at 正常/越界/零长；length。
- `strategy`：SectionLoader fetch 触发读取并缓存；缓存命中不读；digest 失配不缓存；is_loaded/loaded_ids。
- `source_load`：Eager 成功且 sections 齐全；Lazy 骨架成功且 sections 空、fetch 后填充；失败 source 的阶段标注。
- `fayger-buf::parse_skeleton`：骨架解析得到完整 manifest 与空 sections；篡改骨架被结构校验拒。

### PBT 生成器（fayger-loader 测试内）

```rust
// 复用 fayger_buf::testing::arb_buf_object + M1 seal 产出字节
fn arb_sealed_bytes() -> impl Strategy<Value = Vec<u8>>;
// 一个可配置"在第 k 次 read_at 失败"的 FaultySource（测试用）
struct FaultySource { inner: Vec<u8>, fail_after: usize, calls: Cell<usize> }
```

### 端到端测试

`tests/source_e2e.rs`：seal 一个两段 BuF → MemorySource → Eager 与 Lazy 加载断言等价 → 用一个总是失败的 Source 做 Lazy 加载的骨架读取失败（eager 标注）→ 用一个"骨架可读、Section 读取失败"的 Source 验证 Lazy fetch 的 `LDR_LAZY_SOURCE_UNAVAILABLE`（lazy 标注）。

### 反例归档 / CI

沿用前序里程碑：每个 PBT 文件 `failure_persistence = WithSource("regressions")`，`.regressions` 进 git。

## Dependency Versions

M5 不引入新的第三方运行时依赖。`fayger-loader` 与 `fayger-buf` 依赖不变。

## Open Questions

- **Q1**：Lazy 模式是否应校验 crc32？
  - 决定：不校验。crc32 覆盖整文件含 Section 段体，Lazy 未读全；强行校验需读全，违背 Lazy 初衷。crc32 是 Eager（完整字节）职责。Lazy 的完整性由 manifest_digest（骨架）+ 每段 digest（首次访问）保证。
- **Q2**：`BuFSource` 用 `&self` 还是 `&mut self` 的 read_at？
  - 决定：`&self`。随机访问读是只读语义；MemorySource 无内部可变状态。SectionLoader 的缓存可变性在 SectionLoader 自身（`&mut self` 的 fetch），不要求 source 可变。FaultySource 的调用计数用内部可变（Cell）以保持 `&self`。
- **Q3**：是否统一 LoadedArtifact 与 M2 的 BuFObject/M1 的 BuFObject？
  - 决定：不统一。LoadedArtifact 是 source 加载的产出（含策略与按需 loader），与 M1 的 BuFObject（全内存）形态不同。Eager 模式内部用 BuFObject，但对外暴露 LoadedArtifact 统一两种策略的访问入口。后续集成里程碑再议运行层如何消费 LoadedArtifact。
