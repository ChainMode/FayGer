# Fayger Core (Rust 实现)

Fayger 第一阶段开发的主参考实现，作为 Cargo workspace 组织。

## Crates

| Crate | 状态 | 说明 |
| --- | --- | --- |
| `fayger-error` | M0 任务 2 | 统一错误模型（蓝图 §6.1） |
| `fayger-buf` | M0 任务 3-10 | BuF 制品格式、Parser、Serializer |

更多 crate（`fayger-loader`、`fayger-runtime`、`fayger-conformance` 等）会随后续里程碑加入。

## 开发环境

需要 Rust stable（≥ 1.83）。`rust-toolchain.toml` 已锁定 `stable` 通道与 `rustfmt` / `clippy` 组件。

```bash
# 全部 CI 检查（与 GitHub Actions 一致）
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps
```

如果是首次签出，`rustup` 会按需自动下载 toolchain。

## 当前里程碑

正在进行 M0：BuF 核心。详见 [`MILESTONES/M0-buf-core/`](MILESTONES/M0-buf-core/)：

- `requirements.md` — 8 条需求与 36 条 EARS 验收
- `design.md` — 架构、模块拆分、5 条 Correctness Properties
- `tasks.md` — 17 个任务 / 7 个并行波次

## 分支约定

- `main`：蓝图与稳定 release。
- `release/dev`：长期开发分支。
- `feat/m0-N-<topic>`：M0 单个任务的功能分支，从 `release/dev` 切出，完成合回。

## 反例归档

`proptest` 在测试失败时会把最小化反例写入 `proptest-regressions/<test_name>.txt`，提交进 git 跟踪，确保已发现的反例不会回潮。M0 阶段的反例归档将随各 PBT 任务（任务 12-15）逐步出现。
