# Fayger Core (Rust 实现)

Fayger 第一阶段开发的主参考实现，作为 Cargo workspace 组织。

## Crates

| Crate | 状态 | 说明 |
| --- | --- | --- |
| `fayger-error` | M0 任务 2 / M1 任务 2 / M2 任务 2 / M8 任务 1 | 统一错误模型（蓝图 §6.1）；跨层错误因果链（§6.2–§6.3） |
| `fayger-buf` | M0 任务 3-16 / M1 任务 3-4 | BuF 制品格式、Parser、Serializer、PBT 与快照测试 |
| `fayger-loader` | M1 任务 5-12 / M4 任务 1-11 / M5 任务 1-11 / M6 任务 1-10 | 加载链：Verify(Digest/Signature/Version) + BuF_Source / Eager-Lazy + Profile 部分加载 |
| `fayger-runtime` | M2 任务 3-12 / M7 任务 1-9 / M8 任务 5-6,9-10 | 运行层骨架：Runtime_Interface、生命周期、Mock Adapter；资源监测 + 配额超限暂停 + 故障隔离；内置事件总线接入 |
| `fayger-adapter` | M3 任务 3-12 | 适配层：Universal_Instruction 8 类指令集、Platform_Adapter、能力裁剪 |
| `fayger-observability` | M8 任务 2-4,8 | 可观测性事件总线：五类事件、订阅过滤、调试开关、逻辑时钟（蓝图 §6.5–§6.8） |
| `fayger-cli` | M9 任务 1-11 / M10 任务 9 | 运行基座命令行：可安装的 `fayger` 可执行程序（run / inspect / pack / info / demo / bridge / version） |
| `fayger-bridge` | M10 任务 1-10 | 运行层↔适配层统一桥接：指令翻译、经适配层派发为 HostSyscall、跨层错误链 |

更多 crate（`fayger-conformance` 等）会随后续里程碑加入。

## 运行基座 CLI（`fayger`）

M9 起，工作区提供一个可安装的运行基座可执行程序 `fayger`（类比 Docker 客户端 / JRE）：

```bash
# 构建
cargo build --release -p fayger-cli --bin fayger

# 运行
./target/release/fayger version     # 版本信息
./target/release/fayger demo        # 内存示例 BuF 运行自检（类似 docker run hello-world）
./target/release/fayger info        # 探测宿主、选中适配器、列出授予能力
./target/release/fayger inspect app.buf   # 查看 BuF 元数据
./target/release/fayger run app.buf       # 运行一个 BuF
./target/release/fayger bridge app.buf    # 经真实适配层在当前宿主上运行（打印 HostSyscall）
./target/release/fayger pack --entry main --section code:code=main.bin -o app.buf  # 封装 BuF
```

安装（放入 PATH，默认 `~/.fayger/bin`）与三平台分发见 [`docs/INSTALL.md`](docs/INSTALL.md)；本地脚本在 `scripts/`（`build-release.sh` / `install.sh`），打 tag 触发的三平台 release 工作流见 `.github/workflows/release.yml`。退出码约定：成功 `0`、用法错误 `2`、执行失败 `1`。

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
cargo test -p fayger-loader

# 只跑某一个集成测试文件（每个文件对应一项 PBT 或快照测试）
cargo test --test round_trip      # [M0] Property 1 + Property 2 往返一致性
cargo test --test corruption      # [M0] Property 3 字节流破坏被检测
cargo test --test missing_field   # [M0] Property 4 必填字段缺失被拒
cargo test --test section_index   # [M0] Property 5 Section_Index 自相矛盾被检测
cargo test --test fixtures        # [M0/M1] 快照测试（各 crate 各自的 fixtures）

# M1（fayger-loader）的 PBT 与快照
cargo test -p fayger-loader --test seal_load            # [M1] Property 1 封装-加载往返
cargo test -p fayger-loader --test digest_tamper        # [M1] Property 2/3 摘要 / crc32 篡改被检测
cargo test -p fayger-loader --test version_negotiation  # [M1] Property 4 版本协商决策表
cargo test -p fayger-loader --test semver_order         # [M1] Property 5 SemVer 全序
cargo test -p fayger-loader --test fixtures             # [M1] sealed.buf 快照

# M4（fayger-loader 签名）的 PBT 与端到端
cargo test -p fayger-loader --test sign_load              # [M4] Property 1/5 签名-加载往返 / 强制模式
cargo test -p fayger-loader --test sig_decision           # [M4] Property 2 签名验证决策表
cargo test -p fayger-loader --test sig_tamper             # [M4] Property 3 签名作用域篡改被检测
cargo test -p fayger-loader --test truststore_visibility  # [M4] Property 4 可信根更新可见性
cargo test -p fayger-loader --test signature_e2e          # [M4] 端到端：keygen → seal_and_sign → load_signed

# M5（fayger-loader 多源 + 延迟加载）的 PBT 与端到端
cargo test -p fayger-loader --test eager_lazy             # [M5] Property 1 Eager/Lazy 等价
cargo test -p fayger-loader --test lazy_fetch             # [M5] Property 2 Lazy 首访校验与缓存
cargo test -p fayger-loader --test source_faults          # [M5] Property 3/5 源失败阶段标注 / read_at 契约
cargo test -p fayger-loader --test lazy_tamper            # [M5] Property 4 Lazy 段摘要篡改被检测
cargo test -p fayger-loader --test source_e2e             # [M5] 端到端：MemorySource → Eager/Lazy + 失败 Source

# M6（fayger-loader profile 部分加载）的 PBT 与端到端
cargo test -p fayger-loader --test selection              # [M6] Property 1/5 选择决策 / Required-Optional
cargo test -p fayger-loader --test partial_load           # [M6] Property 2/3 内容一致 / 被跳过不可访问
cargo test -p fayger-loader --test partial_signature      # [M6] Property 4 部分加载下签名一致性
cargo test -p fayger-loader --test profile_e2e            # [M6] 端到端：Required+Optional+约束 BuF profile 加载

# M2（fayger-runtime）的 PBT 与端到端
cargo test -p fayger-runtime --test lifecycle           # [M2] Property 1 生命周期合法迁移/非法拒绝
cargo test -p fayger-runtime --test registry            # [M2] Property 2/3 注册完备性 / 路由决策
cargo test -p fayger-runtime --test failure             # [M2] Property 4 失败保留
cargo test -p fayger-runtime --test dispatch            # [M2] Property 5 暂停停止派发/恢复继续
cargo test -p fayger-runtime --test end_to_end          # [M2] 端到端：加载链 → 运行层完整生命周期

# M3（fayger-adapter）的 PBT 与端到端
cargo test -p fayger-adapter --test selection           # [M3] Property 1 适配器选择决策性
cargo test -p fayger-adapter --test capability          # [M3] Property 2 能力裁剪集合代数
cargo test -p fayger-adapter --test unsupported         # [M3] Property 3 不支持指令处理
cargo test -p fayger-adapter --test required            # [M3] Property 4 必需能力缺失拒绝
cargo test -p fayger-adapter --test cross_adapter       # [M3] Property 5 跨适配器行为一致性
cargo test -p fayger-adapter --test end_to_end          # [M3] 端到端：宿主画像 → 选中 → 裁剪 → 派发

# M7（fayger-runtime 资源监测 + 配额 + 故障隔离）的 PBT 与端到端
cargo test -p fayger-runtime --test isolation           # [M7] Property 1/5 数据区隔离 / 故障隔离
cargo test -p fayger-runtime --test quota_suspend       # [M7] Property 2 超限暂停且维度确定
cargo test -p fayger-runtime --test quota_noop          # [M7] Property 3/4 未超限/无配额不触发 / 非 Running 不迁移
cargo test -p fayger-runtime --test quota_e2e           # [M7] 端到端：两实例不同配额，一超限暂停、一正常继续

# M8（可观测性事件总线 + 跨层错误链）的 PBT 与端到端
cargo test -p fayger-error --test error_chain               # [M8] Property 4 错误因果链顺序保持
cargo test -p fayger-observability --test filter_dispatch   # [M8] Property 2/3/5 过滤 / 调试门控 / 取消订阅
cargo test -p fayger-runtime --test lifecycle_events        # [M8] Property 1 生命周期事件序列一致性
cargo test -p fayger-runtime --test observability_e2e       # [M8] 端到端：完整生命周期 + 超限暂停事件序列

# M9（fayger-cli 运行基座）的 PBT 与端到端
cargo test -p fayger-cli --test args_props                  # [M9] Property 1/4 解析全域性 / 退出码映射
cargo test -p fayger-cli --test roundtrip_props             # [M9] Property 2/3/5 pack→load→run / inspect 一致 / 事件一致
cargo test -p fayger-cli --test cli_e2e                     # [M9] 端到端：run_cli demo/version/help + 内存往返

# M10（fayger-bridge 运行层↔适配层桥接）的 PBT 与端到端
cargo test -p fayger-bridge --test mapping_props            # [M10] Property 1 指令翻译确定性与字段保持
cargo test -p fayger-bridge --test run_props                # [M10] Property 2 桥接运行 syscall 数等于 section 数
cargo test -p fayger-bridge --test dispatch_props           # [M10] Property 3/4/5 能力裁剪 / 不支持 / 跨层错误链
cargo test -p fayger-bridge --test bridge_e2e               # [M10] 端到端：加载链 → 桥接运行 + 跨层错误链
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

### M1 PBT 概览（fayger-loader）

M1 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M1-load-chain/design.md` §"Correctness Properties"），分摊在 4 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/seal_load.rs` | P1（封装-加载往返 + 字节级强往返） | 500 | Requirements 5.4 / 9.4 / 11.1 |
| `tests/digest_tamper.rs` | P2（摘要篡改被检测）+ P3（crc32 篡改被检测） | 500 | Requirements 2.2 / 3.2 / 4.2 / 11.1 |
| `tests/version_negotiation.rs` | P4（版本协商决策表） | 500 | Requirements 7.x / 8.x / 9.3 / 11.1 |
| `tests/semver_order.rs` | P5（SemVer 全序） | 500 | Requirements 6.1–6.4 / 11.1 |

加上 `fayger-loader` 的 `tests/fixtures.rs`（`sealed.buf` 加载成功 + 逐字节篡改必失败），完整覆盖 M1 的加载链正确性约束。

### M4 PBT 概览（fayger-loader 签名）

M4 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M4-signature/design.md` §"Correctness Properties"），分摊在 4 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/sign_load.rs` | P1（签名-加载往返）+ P5（强制签名模式） | 500 | Requirements 6.1 / 6.2 / 3.1 / 3.2 |
| `tests/sig_decision.rs` | P2（签名验证决策表） | 500 | Requirements 3.1–3.6 |
| `tests/sig_tamper.rs` | P3（签名作用域篡改被检测） | 500 | Requirements 2.1 / 2.3 / 3.5 |
| `tests/truststore_visibility.rs` | P4（可信根更新可见性） | 500 | Requirements 4.3 / 4.4 |

加上 `tests/signature_e2e.rs`（生成密钥对 → seal_and_sign → 含/不含公钥的 TrustStore load_signed），完整覆盖 M4 的签名校验正确性约束。签名算法为 ed25519，签名作用域为 `Header || Manifest_without_signature || Section_Index`。

### M5 PBT 概览（fayger-loader 多源 + 延迟加载）

M5 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M5-source-lazy/design.md` §"Correctness Properties"），分摊在 4 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/eager_lazy.rs` | P1（Eager/Lazy 等价性） | 500 | Requirements 6.1–6.3 |
| `tests/lazy_fetch.rs` | P2（Lazy 首访校验与缓存） | 500 | Requirements 3.3 / 3.4 |
| `tests/source_faults.rs` | P3（源失败阶段标注）+ P5（read_at 契约） | 500 | Requirements 4.1–4.3 / 5.1–5.3 / 1.2 / 1.3 |
| `tests/lazy_tamper.rs` | P4（Lazy 段摘要篡改被检测） | 500 | Requirements 3.5 |

加上 `tests/source_e2e.rs`（MemorySource 上 Eager/Lazy 等价、失败 Source 的阶段标注），完整覆盖 M5 的多源与延迟加载正确性约束。`BuF_Source` 仅要求随机访问读 + 总长度；Lazy 模式下每段偏移由 manifest 声明长度顺序累加得到（受 manifest_digest 保护）。

### M6 PBT 概览（fayger-loader profile 部分加载）

M6 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M6-profile-loading/design.md` §"Correctness Properties"），分摊在 3 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/selection.rs` | P1（选择决策性）+ P5（Required 拒绝 / Optional 跳过） | 500 | Requirements 2.x / 3.x |
| `tests/partial_load.rs` | P2（被选中内容与全选一致）+ P3（被跳过不可访问） | 500 | Requirements 4.2–4.5 |
| `tests/partial_signature.rs` | P4（部分加载下签名一致性） | 500 | Requirements 5.2–5.5 |

加上 `tests/profile_e2e.rs`（code Required + gpu/huge Optional 带约束的 BuF，用满足/不满足约束的 profile 加载，断言选中/跳过/失败与签名一致），完整覆盖 M6 的部分加载正确性约束。选择约束来自 capabilities / features / max_size（`target_class` 仅描述性，不参与选择）；被跳过的 Optional Section 不被读取；签名作用域不含 Section 段体，故不同 profile 子集签名结果相同。

### M2 PBT 概览（fayger-runtime）

M2 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M2-runtime-skeleton/design.md` §"Correctness Properties"），分摊在 4 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/lifecycle.rs` | P1（生命周期合法迁移 / 非法拒绝 / 终态保护） | 500 | Requirements 5.1–5.8 / 9.1 |
| `tests/registry.rs` | P2（注册完备性集合代数）+ P3（路由决策与确定性） | 500 | Requirements 2.3 / 2.4 / 3.2–3.6 |
| `tests/failure.rs` | P4（失败保留 + FailureInfo 完整） | 500 | Requirements 7.1–7.4 |
| `tests/dispatch.rs` | P5（暂停停止派发 / 恢复继续，收到的恒为 program 前缀） | 500 | Requirements 6.1–6.4 |

加上 `tests/end_to_end.rs`（用 M1 加载链产出的 BuF 走完 `Loaded→Initialized→Running→Suspended→Running→Terminated` 并断言指令派发序列），完整覆盖 M2 的运行层骨架正确性约束。

### M3 PBT 概览（fayger-adapter）

M3 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M3-adapter-layer/design.md` §"Correctness Properties"），分摊在 5 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/selection.rs` | P1（适配器选择决策性 + 确定性） | 500 | Requirements 4.2–4.4 |
| `tests/capability.rs` | P2（能力裁剪集合代数） | 500 | Requirements 5.1–5.4 |
| `tests/unsupported.rs` | P3（不支持指令处理） | 500 | Requirements 2.5 / 7.3 / 7.4 |
| `tests/required.rs` | P4（必需能力缺失拒绝） | 500 | Requirements 6.1–6.3 / 7.1 / 7.2 |
| `tests/cross_adapter.rs` | P5（跨适配器行为一致性） | 500 | Requirements 8.1 / 8.2 |

加上 `tests/end_to_end.rs`（构造宿主画像 → 选中内置适配器 → 能力裁剪 → 派发并断言通过/拒绝/翻译），完整覆盖 M3 的适配层正确性约束。

### M7 PBT 概览（fayger-runtime 资源监测 + 配额 + 故障隔离）

M7 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M7-resource-quota/design.md` §"Correctness Properties"），分摊在 3 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/isolation.rs` | P1（运行时数据区隔离）+ P5（故障隔离） | 500 | Requirements 1.1–1.3 / 5.1–5.4 / 7.1 / 7.4 |
| `tests/quota_suspend.rs` | P2（超限暂停且维度确定） | 500 | Requirements 3.2 / 3.3 / 4.1–4.3 / 7.1 |
| `tests/quota_noop.rs` | P3（未超限/无配额不触发）+ P4（非 Running 不自动迁移） | 500 | Requirements 2.3 / 2.4 / 4.4 / 4.5 / 8.6 / 7.1 |

加上 `tests/quota_e2e.rs`（实例 A `memory_bytes=100` 超限暂停 + 实例 B 无配额继续 step，断言互不影响），完整覆盖 M7 的资源监测正确性约束。配额从 `manifest.quotas` 读取；超限判定按固定顺序 Cpu→Memory→Io、用严格大于（达到上限不算越界）、只判定声明（`Some`）的维度；用量经 `observe_usage(id, usage)` 注入（确定性，真实 OS 计量为后续工作），超限仅在 `Running` 状态自动迁移到 `Suspended` 并经 `quota_exceeded(id)` 查询返回 `QuotaExceeded`；每实例 `InstanceData` 为独立 owned 数据，对一个实例的操作不触及其它实例。

### M8 PBT 概览（可观测性事件总线 + 跨层错误链）

M8 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M8-observability/design.md` §"Correctness Properties"），分摊在 3 个集成测试文件中（跨 `fayger-error` / `fayger-observability` / `fayger-runtime` 三个 crate）：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `fayger-error/tests/error_chain.rs` | P4（错误因果链顺序保持） | 500 | Requirements 5.5 / 8.1 / 8.4 |
| `fayger-observability/tests/filter_dispatch.rs` | P2（过滤）+ P3（调试门控）+ P5（取消订阅/隔离） | 500 | Requirements 2.x / 3.x / 1.2 / 8.1 |
| `fayger-runtime/tests/lifecycle_events.rs` | P1（生命周期事件序列一致性） | 500 | Requirements 4.1–4.4 / 6.2 / 8.1 |

加上 `fayger-runtime/tests/observability_e2e.rs`（完整生命周期 + 一次超限暂停，订阅断言 Lifecycle from/to 序列、含 `Running→Suspended`、至少一条 `Quota`、时间戳单调非减），完整覆盖 M8 的可观测性与错误链正确性约束。EventBus 为同步、确定性、单线程：事件时间戳为内部逻辑时钟（单调非减，仅在被投递事件上前进；被调试开关丢弃的 `Debug` 既不投递也不递增）；订阅按注册顺序确定性投递；跨层错误经 `FaygerError::wrap` 逐层包装，`chain`/`layer_chain`/`root_cause`/`depth` 由外到内遍历且不改写被包装层字段；运行层在迁移/故障/用量时发布事件，无订阅者时不改变 M2–M7 既有行为。

### M9 PBT 概览（fayger-cli 运行基座）

M9 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M9-cli-distribution/design.md` §"Correctness Properties"），分摊在 2 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/args_props.rs` | P1（解析全域性 + 稳定识别）+ P4（退出码映射确定性） | 500 | Requirements 1.2 / 1.4 / 1.5 / 6.x |
| `tests/roundtrip_props.rs` | P2（pack→load→run 往返）+ P3（inspect 与 Manifest 一致）+ P5（事件与生命周期一致） | 500 | Requirements 3.5 / 4.x / 5.2 / 5.5 |

加上 `tests/cli_e2e.rs`（`run_cli` 跑 demo/version/help + 未知命令退出码 + pack_bytes→load→run 纯内存往返），完整覆盖 M9 的 CLI 正确性约束。CLI 设计为「纯核心 + 薄壳」：参数解析、inspect/run/pack、宿主画像、退出码映射均为不触达 FS 的纯函数（PBT 直接验证「任意 argv 不 panic、要么 Command 要么 Usage」与「pack 出的 BuF 必然可加载并运行到 Terminated、派发指令数等于 section 数」）；文件读写隔离在 `fs_ops` 两个薄封装里；`bin/fayger.rs` 只做 `argv → run_cli → process::exit(exit_code)`。

### M10 PBT 概览（fayger-bridge 运行层↔适配层桥接）

M10 共有 **5 条 Correctness Properties**（详见 `MILESTONES/M10-runtime-adapter-bridge/design.md` §"Correctness Properties"），分摊在 3 个集成测试文件中：

| 文件 | Property | cases | Validates |
| --- | --- | --- | --- |
| `tests/mapping_props.rs` | P1（指令翻译确定性与字段保持） | 500 | Requirements 1.1–1.6 |
| `tests/run_props.rs` | P2（桥接运行 syscall 数等于 section 数） | 500 | Requirements 2.2 / 2.3 / 2.4 / 2.6 |
| `tests/dispatch_props.rs` | P3（能力裁剪）+ P4（不支持指令）+ P5（跨层错误链） | 500 | Requirements 3.x / 4.x |

加上 `tests/bridge_e2e.rs`（加载链产物 → `run_bridged` 在 native-desktop 上 syscall 序列与 section 一致；browser 上需 proc 能力的派发 → `wrap_dispatch_failure` → `[Runtime, Adapter]`），完整覆盖 M10 的桥接正确性约束。M10 打通三层主路径：BuF → 运行层执行 → 适配层选择/裁剪/翻译 → `HostSyscall`。关键分层约束（蓝图 §8.8）：运行层不依赖适配层；桥接经运行层 `Adapter` trait 接缝完成，依赖方向单向无环（`fayger-bridge` → `fayger-runtime` / `fayger-adapter`）。能力裁剪经 `AdapterRegistry::dispatch` 与适配层同一语义；跨层失败经 `wrap_dispatch_failure` 构造 `RT_INSTANCE_FAILED` cause `ADP_*` 的 `[Runtime, Adapter]` 因果链。

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

**M1（加载链）已完成**：12 个任务全部落地，新增 `fayger-loader` crate，交付 Verify(Digest)（manifest_digest + 每段 section digest + crc32）与 Negotiate(Version)（schema 范围 + 弃用 + runtime 最低版本），并以 `load(bytes, policy)` 串联为加载链。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M1-load-chain/`](MILESTONES/M1-load-chain/)：

- `requirements.md` — 12 条需求
- `design.md` — 加载链编排、摘要覆盖区间、SemVer 全序、5 条 Correctness Properties
- `tasks.md` — 12 个任务 / 7 个并行波次

**M2（运行层骨架）已完成**：12 个任务全部落地，新增 `fayger-runtime` crate，交付 Runtime_Interface 契约、Runtime_Implementation 注册完备性与路由、BuF_Instance 生命周期状态机、最小 Universal_Instruction 与 Mock Adapter。一个 BuF 现在可以被加载为实例并走完 `Loaded→Initialized→Running→Suspended→Terminated/Failed` 生命周期、在执行时向适配层派发指令。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M2-runtime-skeleton/`](MILESTONES/M2-runtime-skeleton/)：

- `requirements.md` — 10 条需求
- `design.md` — 三层定位、状态机、注册/路由、Mock 执行、5 条 Correctness Properties
- `tasks.md` — 12 个任务 / 8 个并行波次

**M3（适配层）已完成**：12 个任务全部落地，新增 `fayger-adapter` crate，交付 Universal_Instruction 8 类指令集、Platform_Adapter 契约与四类内置适配器（NativeDesktop / Server / Browser / InApp）、Host_Environment 检测与适配器选择、能力裁剪（`granted = requested ∩ available ∩ policy`）与指令派发。同一份 BuF 现在可以在桌面/服务器/浏览器/In-App 四类宿主上被适配执行，受限宿主上能力被裁剪、不支持指令被拒绝。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M3-adapter-layer/`](MILESTONES/M3-adapter-layer/)：

- `requirements.md` — 11 条需求
- `design.md` — 适配层架构、能力裁剪集合代数、四类内置适配器能力矩阵、5 条 Correctness Properties
- `tasks.md` — 12 个任务 / 8 个并行波次

**M4（签名校验）已完成**：11 个任务全部落地，在 `fayger-loader` 中新增 Verify(Signature) 阶段与安全横切组件，交付 ed25519 签名验证、TrustStore（可信根 + 强制签名开关 + 更新可见性）、`seal_and_sign` 生产侧封装签名、以及携带 TrustStore 的加载链入口 `load_signed`。加载链现在具备来源与完整性双重护栏（Verify(Digest) 防篡改、Verify(Signature) 防伪造来源、强制模式拒绝无签名制品）。签名作用域为 `Header || Manifest_without_signature || Section_Index`。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M4-signature/`](MILESTONES/M4-signature/)：

- `requirements.md` — 9 条需求
- `design.md` — 签名作用域、签名验证决策表、TrustStore 更新时序、5 条 Correctness Properties
- `tasks.md` — 11 个任务 / 8 个并行波次

**M5（多源存储 + 延迟加载）已完成**：11 个任务全部落地，在 `fayger-loader` 中引入 `BuF_Source` 随机访问存储抽象（含内置 `MemorySource`）、`Eager` / `Lazy` 两种 `LoadStrategy`、Lazy 模式的 `SectionLoader`（首次访问读取 + digest 校验 + 缓存）与阶段错误标注（`context.phase ∈ {eager, lazy}`）。同一份 BuF 现在既可一次性 Eager 加载、也可只读可信骨架后按段 Lazy 取回，两种模式语义等价。为支持 Lazy，`fayger-buf` 新增 `parse_skeleton`（仅骨架解析，不读 Section 段体）。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M5-source-lazy/`](MILESTONES/M5-source-lazy/)：

- `requirements.md` — 9 条需求
- `design.md` — BuF_Source 抽象、骨架解析、Eager/Lazy 数据流、阶段标注、5 条 Correctness Properties
- `tasks.md` — 11 个任务 / 8 个并行波次

**M6（profile 部分加载）已完成**：10 个任务全部落地，在 `fayger-loader` 中引入 `LoadProfile`（capabilities / features / max_section_bytes / target_class）、确定性的 Section 选择谓词 `select_sections`（`Required` 缺失即失败、`Optional` 不满足时跳过并记录原因）与部分加载入口 `load_with_profile` / `load_with_profile_signed`（产出 `PartialArtifact`）。同一份 BuF 现在可仅读取与当前运行环境相关的 Section 子集；被跳过段不被读取、不可访问；签名作用域不含 Section 段体，故不同 profile 子集的签名校验结果一致。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M6-profile-loading/`](MILESTONES/M6-profile-loading/)：

- `requirements.md` — 9 条需求
- `design.md` — LoadProfile、选择谓词与决策表、部分加载下签名一致性、5 条 Correctness Properties
- `tasks.md` — 10 个任务 / 8 个并行波次

**M7（资源监测 + 配额 + 故障隔离）已完成**：9 个任务全部落地，在 `fayger-runtime` 中新增资源监测能力：`Runtime::load` 从 `manifest.quotas` 读取配额；新增 `observe_usage(id, usage)` 注入用量采样并在 `Running` 实例超出配额时自动迁移到 `Suspended`、记录 `QuotaExceeded`；新增 `quota(id)` / `usage(id)` / `quota_exceeded(id)` 查询。超限判定按固定顺序 Cpu→Memory→Io、用严格大于（达到上限不算越界）、只判定声明的维度；非 `Running` 状态注入超限用量仅记录 `last_usage` 而不迁移。每实例 `InstanceData` 为独立 owned 数据，单实例故障（`Failed`）或超限暂停不影响其它实例的状态与指令派发序列。只新增一个错误码 `RT_QUOTA_EXCEEDED`，未改动 M0–M6 的对外行为。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M7-resource-quota/`](MILESTONES/M7-resource-quota/)：

- `requirements.md` — 8 条需求
- `design.md` — 超限暂停数据流、维度顺序判定、隔离不变式、5 条 Correctness Properties
- `tasks.md` — 9 个任务 / 6 个并行波次

**M8（可观测性事件总线 + 跨层错误链）已完成**：10 个任务全部落地，新增 `fayger-observability` crate（同步、确定性、单线程的 `EventBus`，承载 Lifecycle / Quota / Loader / Adapter / Debug 五类事件，支持按 `EventFilter` 订阅、取消订阅与调试开关，时间戳为逻辑时钟），并在 `fayger-error` 上加性新增跨层错误因果链能力（`wrap` / `chain` / `layer_chain` / `root_cause` / `depth`），在 `fayger-runtime` 上加性接入事件发布（生命周期迁移、注入故障、注入用量/超限暂停时发布对应事件）。Fayger 三层的失败现在可经统一错误链由外到内定位，运行过程可经事件总线被订阅观测；无订阅者时不改变 M2–M7 任何既有可观测行为。M8 不新增错误码，未改动 M0–M7 的对外行为。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M8-observability/`](MILESTONES/M8-observability/)：

- `requirements.md` — 9 条需求
- `design.md` — EventBus 数据流、过滤决策表、生命周期事件序列一致性、错误链不变式、5 条 Correctness Properties
- `tasks.md` — 10 个任务 / 7 个并行波次

**M9（运行基座 CLI + 跨平台分发）已完成**：11 个任务全部落地，新增 `fayger-cli` crate（含 `lib` 纯核心与名为 `fayger` 的 `bin`），把 M0–M8 的库能力封装为可安装、可执行的运行基座。`fayger` 提供 `version` / `info` / `inspect` / `run` / `pack` / `demo` / `help` 子命令：`run` 在运行层参考实现上加载并跑完一个 BuF 的完整生命周期、订阅事件总线并打印运行报告；`inspect` 打印 Manifest 摘要；`pack` 把若干 section 文件封装（seal）为可加载 BuF；`info` 探测宿主并选择内置适配器；`demo` 在内存中构造并运行示例 BuF（安装自检）。CLI 为「纯核心 + 薄壳」：解析/inspect/run/pack/退出码映射/宿主画像均为不触达 FS 的纯函数，FS 仅隔离在 `fs_ops`。配套交付跨平台分发：`scripts/build-release.sh` / `scripts/install.sh`、`.github/workflows/release.yml`（打 tag 触发三平台构建 + SHA-256 校验和）与 [`docs/INSTALL.md`](docs/INSTALL.md) 安装指南。M9 不修改 M0–M8 库 crate 的对外契约，仅以加性方式消费既有 API。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M9-cli-distribution/`](MILESTONES/M9-cli-distribution/)：

- `requirements.md` — 9 条需求
- `design.md` — 纯核心+薄壳架构、命令数据流、退出码映射、5 条 Correctness Properties
- `tasks.md` — 11 个任务 / 7 个并行波次

Fayger 现在具备**可安装的运行基座形态**：在 macOS / Linux / Windows 上获取 `fayger` 放入 PATH，即可 `fayger demo` 自检、`fayger run app.buf` 运行 BuF。

**M10（运行层↔适配层统一桥接）已完成**：10 个任务全部落地，新增 `fayger-bridge` 集成层 crate，把运行层执行流真正经由适配层（选择 → 能力裁剪 → 翻译）派发为 `HostSyscall`，打通「BuF → 运行层执行 → 适配层 → 宿主调用」这条贯穿三层的主路径。交付指令翻译（运行层最小指令 → 适配层 8 类指令）、`BridgeAdapter`（实现运行层 `Adapter` trait，记录每次派发结果）、端到端 `run_bridged`（在四类内置适配器上按宿主画像运行并产出报告）、以及 `wrap_dispatch_failure`（把 `ADP_*` 包装为 `RT_INSTANCE_FAILED` cause，构造 `[Runtime, Adapter]` 跨层错误链）。CLI 新增 `fayger bridge <file>` 子命令。关键分层约束被严格遵守：`fayger-runtime` 不依赖 `fayger-adapter`，桥接经运行层 `Adapter` trait 接缝完成，依赖方向单向无环。M10 不新增错误码、不引入新依赖、不修改 M0–M9 库 crate 的对外契约。5 条 Correctness Properties 经 PBT 持续验证。详见 [`MILESTONES/M10-runtime-adapter-bridge/`](MILESTONES/M10-runtime-adapter-bridge/)：

- `requirements.md` — 8 条需求
- `design.md` — 桥接数据流、翻译字段保持、跨层错误链不变式、5 条 Correctness Properties
- `tasks.md` — 10 个任务 / 7 个并行波次

至此三层（加载 / 运行 / 适配）与横切关注点（错误模型 / 可观测性）已贯通，并由可安装的 `fayger` 运行基座对外承载。后续可向第二阶段的执行性能（ExecutionStrategy / JIT / AOT）、分发与缓存方向展开。

## 分支约定

- `main`：蓝图与稳定 release。
- `release/dev`：长期开发分支，每个里程碑的合并目标。
- `feat/m0-N-<topic>`：M0 单个任务的功能分支，从 `release/dev` 切出，完成后通过 `git merge --no-ff` 合回。
- 每个 commit message 必须含尾注 `Refs: tasks.md 任务 N；Requirements X.Y` 以便回溯。
