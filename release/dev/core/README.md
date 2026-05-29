# Fayger Core (Rust 实现)

Fayger 第一阶段开发的主参考实现，作为 Cargo workspace 组织。

## Crates

| Crate | 状态 | 说明 |
| --- | --- | --- |
| `fayger-error` | M0 任务 2 | 统一错误模型（蓝图 §6.1） |
| `fayger-buf` | M0 任务 3-16 | BuF 制品格式、Parser、Serializer、PBT 与快照测试 |

更多 crate（`fayger-loader`、`fayger-runtime`、`fayger-conformance` 等）会随后续里程碑加入。

## 开发环境

需要 Rust stable（≥ 1.83）。`rust-toolchain.toml` 已锁定 `stable` 通道与 `rustfmt` / `clippy` 组件。

如果是首次签出，`rustup` 会按需自动下载 toolchain。

## 如何本地运行测试

下面四条命令与 `.github/workflows/ci-m0.yml` 三平台 CI 一致，全部在 `release/dev/core/` 目录下执行：

```bash
# 1. 格式化检查
cargo fmt --all -- --check

# 2. 静态分析（warnings 全部视为错误）
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 3. 全量测试（单元测试 + 集成测试 + Doc-tests）
cargo test --workspace --all-features

# 4. 文档构建（warnings 全部视为错误）
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

合并到 `release/dev` 之前，这四条必须全绿。

### 单独运行某一类测试

```bash
# 只跑某一个 crate 的单元测试
cargo test -p fayger-buf
cargo test -p fayger-error

# 只跑某一个集成测试文件（每个文件对应一项 PBT 或快照测试）
cargo test --test round_trip      # Property 1 + Property 2 往返一致性
cargo test --test corruption      # Property 3 字节流破坏被检测
cargo test --test missing_field   # Property 4 必填字段缺失被拒
cargo test --test section_index   # Property 5 Section_Index 自相矛盾被检测
cargo test --test fixtures        # 快照测试（minimal.buf / two_sections.buf）
```

### 增加 PBT cases 数

PBT 默认每条 property 跑 200~500 次。本地排查疑难反例时可以临时调大：

```bash
# 通过环境变量提高 cases 数（仅本次进程生效）
PROPTEST_CASES=2000 cargo test --workspace --all-features

# 或者只针对某个 PBT 文件
PROPTEST_CASES=5000 cargo test --test corruption
```

完整可调环境变量参考 [proptest 文档](https://docs.rs/proptest/latest/proptest/test_runner/struct.Config.html)。

## 如何运行 PBT 与查看反例归档

### M0 PBT 概览

M0 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M0-buf-core/design.md` §"Correctness Properties"），分摊在 4 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/round_trip.rs` | P1（结构等价）+ P2（字节级强往返） | 500 | Requirements 5.1 / 5.2 / 5.3 |
| `tests/corruption.rs` | P3（字节流破坏被检测） | 500 | Requirements 7.1 / 3.3 / 3.4 / 3.5 / 3.7 / 3.8 |
| `tests/missing_field.rs` | P4（必填字段缺失被拒） | 500 | Requirements 4.4 / 7.1 |
| `tests/section_index.rs` | P5（Section_Index 自相矛盾被检测） | 200 | Requirements 3.6 / 7.1 |

加上 `tests/fixtures.rs` 中的快照测试（minimal.buf + two_sections.buf 双向字节级一致），完整覆盖 M0 的核心正确性约束。

### 反例归档机制

每个 PBT 文件都把 `failure_persistence` 显式设为：

```rust
proptest::test_runner::FileFailurePersistence::WithSource("regressions")
```

这意味着 proptest 一旦发现反例，会自动把最小化 seed 写到与测试源文件**同目录**下的 `<test_name>.regressions` 文件，例如：

```
release/dev/core/crates/fayger-buf/tests/
├── corruption.rs
├── corruption.regressions       ← 这里
├── missing_field.rs
├── round_trip.rs
└── section_index.rs
```

这些 `.regressions` 文件**已加入 git**——一旦某反例被归档，未来每次 `cargo test` 都会先重放它，以防同类回归再次溜过。

### 重现一个具体反例

如果 CI 报告 `tests/<name>.rs` 失败：

1. 打开 `release/dev/core/crates/fayger-buf/tests/<name>.regressions`，里面每行是一条历史反例（含 seed 与 shrink 后的最小化值）。
2. 本地直接执行：

   ```bash
   cargo test --test <name>
   ```

   proptest 会优先重放 `.regressions` 中的所有 seed；只要它们在你本地复现失败，就证明本地代码尚未修复该反例。

3. 修复后再跑一次，全绿即代表反例已不再触发。该归档行不会被自动清理，作为长期回归护栏保留。

### M0 已知反例（已修复）

`corruption.regressions` 中保留有一条历史反例：

```
position = 10959327567306175650, bit = 0
```

这条反例由任务 16 的快照测试合并前发现：彼时 serializer 在写 Header 时把 raw_header.format_version "纠正"成 `FORMAT_VERSION_M0`，导致一旦 fixture 的 raw bytes 中 format_version 字段是其他值（这条反例正是这种情形），字节级强往返 P2 就破。修复方式是 commit `3bb6829`：serializer 直接保留 `raw_header.format_version` 不动（M0 只在 parser 层校验）。归档保留这条反例，后续任何对 Header 序列化路径的改动都会被它"绊"一下，提醒不要重蹈覆辙。

## Fixtures 快照测试

`tests/fixtures/` 下两个手工构造的合法 BuF 字节流：

| 文件 | 大小 | 描述 |
| --- | --- | --- |
| `minimal.buf` | 156 字节 | 0 个 sections 的最小合法 BuF |
| `two_sections.buf` | 736 字节 | 2 个 sections（Code + Asset）的代表性 BuF |

`tests/fixtures.rs` 对每个 fixture 跑两条断言：

1. `parse(fixture_bytes) == 预期 BuFObject`（解析结构正确）
2. `serialize(预期 BuFObject) == fixture_bytes`（序列化字节级回写）

合起来即"字节 → 对象 → 字节"双向一致。

如果 BuF 格式有意演进（例如 M1+ 引入新字段），fixture 需要刷新：

```bash
cargo test --test fixtures -- --ignored regenerate_fixtures
```

跑完会原地覆写 `tests/fixtures/*.buf`。**人工 review `git diff`** 后再 commit，确认变化是预期的格式演进而不是回归。

## M0 测试统计（截至当前）

| 类别 | 数量 |
| --- | --- |
| `fayger-buf` 单元测试 | 107 |
| `fayger-error` 单元测试 | 16 |
| 集成测试：`corruption` | 4 |
| 集成测试：`fixtures`（含 1 个 `#[ignore]` 重生成器） | 2 + 1 ignored |
| 集成测试：`missing_field` | 5 |
| 集成测试：`round_trip` | 3 |
| 集成测试：`section_index` | 1 |
| Doc-tests | 2 |
| **总计** | **140 passed + 1 ignored** |

PBT 每次完整 run 约 **2200 cases**（500 + 500 + 500 + 500 + 200，分摊在 4 个 PBT 文件）。本地 debug 模式约 5–10 秒；CI release 模式 smoke 跑一次约 3–5 秒。

## 里程碑状态

**M0（BuF 核心）已完成**：17 个任务全部落地，5 条 Correctness Properties 经 PBT 持续验证，四项 CI 检查（fmt / clippy / test / doc）三平台全绿。详见 [`MILESTONES/M0-buf-core/`](MILESTONES/M0-buf-core/)：

- `requirements.md` — 8 条需求与 42 条 EARS 验收
- `design.md` — 架构、模块拆分、5 条 Correctness Properties
- `tasks.md` — 17 个任务 / 7 个并行波次

后续里程碑（加载链、运行层、适配层等）将以 M0 的 BuF 可执行规范为基础展开。

## 分支约定

- `main`：蓝图与稳定 release。
- `release/dev`：长期开发分支，每个里程碑的合并目标。
- `feat/m0-N-<topic>`：M0 单个任务的功能分支，从 `release/dev` 切出，完成后通过 `git merge --no-ff` 合回。
- 每个 commit message 必须含尾注 `Refs: tasks.md 任务 N；Requirements X.Y` 以便回溯。
