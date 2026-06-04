# Requirements Document

## Introduction

本文是 Fayger 第一阶段开发的 **M9 里程碑**需求文档。M9 把前八个里程碑沉淀的库能力（加载链 / 运行层 / 适配层 / 资源配额 / 可观测性）封装为一个**可安装、可执行的运行基座**——即像 Docker 客户端或 JRE 那样「下载即用」的命令行程序 `fayger`，并提供跨平台的分发与安装通道。

在 M9 之前，工作区内全部是 library crate（供别的 Rust 代码调用），没有任何可执行入口、没有打包产物。M9 补齐这一步：

- **可执行入口（`fayger` CLI）**：一个二进制程序，把 loader→runtime→（observability/adapter 查询）串成端到端流程，提供子命令：
  - `fayger version`：打印版本与构建信息。
  - `fayger info`：探测当前宿主环境、选中内置适配器、打印其描述符与授予能力。
  - `fayger inspect <file.buf>`：加载并校验一个 BuF，打印其 Manifest 摘要（schema / runtime_min / entry / sections / capabilities / quotas / 签名状态 / 弃用提示）。
  - `fayger run <file.buf>`：把一个 BuF 加载为实例并走完生命周期，订阅事件总线，打印发出的生命周期/配额事件与最终状态。
  - `fayger pack`：把若干 section 文件封装（seal）为一个可加载的 BuF 制品。
  - `fayger demo`：在内存中构造一个示例 BuF 并运行（安装后自检，类似 `docker run hello-world`）。
  - `fayger help [cmd]` / `--help` / `-h`：用法帮助。
- **分发与安装通道**：跨三平台的 release 构建脚本、一键安装脚本（`curl ... | sh`）、GitHub Release 工作流（交叉编译 + 校验和），以及安装文档。

M9 **不**交付：真实 OS 资源计量绑定（沿用 M7 的注入式用量，CLI 不强制注入）；运行层与适配层的统一桥接（`run` 走运行层参考实现 + MockAdapter，适配层仅由 `info` 用于宿主/适配器查询，二者的统一是后续集成里程碑）；包管理器原生包的最终发布（M9 产出可被打包的二进制与校验和，Homebrew formula / .deb / .msi 的实际上架属后续运营工作，但 M9 提供可直接放入 PATH 的安装脚本）；GUI。

完成 M9 后，用户可以在 macOS / Linux / Windows 上获得一个 `fayger` 可执行文件并放入 PATH，运行 `fayger version` / `fayger demo` 验证安装、用 `fayger run app.buf` 运行一个 BuF——Fayger 第一次具备「运行基座」的可安装形态。

## Glossary

- **CLI（`fayger`）**：M9 交付的可执行程序，运行基座的对外入口。
- **Command**：一次 CLI 调用解析出的子命令（Version / Info / Inspect / Run / Pack / Demo / Help）。
- **CliError**：CLI 执行失败的统一错误，至少区分「用法错误」（Usage）与「执行失败」（Failure，含底层 `FaygerError`）。
- **退出码（Exit Code）**：进程返回码。约定：成功 `0`；用法错误 `2`；执行失败（加载/运行错误）`1`。
- **纯核心（Pure Core）**：不触达文件系统/网络/时钟的可测函数集合：参数解析、`inspect_object`、`run_object`、`pack_bytes`、宿主画像与策略构造。FS 包装函数（读文件/写文件）只是薄封装。
- **默认加载策略（Default LoadPolicy）**：CLI 内置的版本协商策略（schema 支持区间 + 运行层提供版本），用于 `inspect` / `run`。
- **宿主画像构造（Host Profiling）**：CLI 依据编译期/运行期平台信息（`std::env::consts::OS`/`ARCH`）确定性地构造一个 `HostEnvironment`，供 `info` 选择内置适配器。
- **RunReport**：`run_object` 的结构化结果，含最终生命周期状态、派发指令数、发出的事件计数（生命周期/配额）。
- **安装脚本（install.sh）**：可被 `curl ... | sh` 执行的脚本，把 `fayger` 放入用户 PATH（默认 `~/.fayger/bin`）。

## Requirements

### Requirement 1: 可执行入口与子命令解析

**User Story:** 作为终端用户，我希望像运行 `docker` / `java` 一样运行 `fayger` 并带子命令，以便完成查看版本、运行 BuF 等操作。

#### Acceptance Criteria

1. THE M9 SHALL 交付一个名为 `fayger` 的二进制目标（bin target）。
2. THE CLI SHALL 把命令行参数解析为一个 Command；可识别子命令至少包括 `version`、`info`、`inspect`、`run`、`pack`、`demo`、`help`。
3. WHEN 未提供任何参数或提供 `help` / `--help` / `-h`，THE CLI SHALL 输出用法帮助。
4. WHEN 提供了未知子命令或缺失必需参数，THE CLI SHALL 返回用法错误（CliError::Usage），并以退出码 `2` 结束。
5. THE 参数解析 SHALL 为纯函数（不触达文件系统/网络），对同一参数向量产出同一 Command 或同一用法错误。

### Requirement 2: version 与 info 子命令

**User Story:** 作为运维人员，我希望查看运行基座版本与当前宿主的适配情况，以便确认安装与环境。

#### Acceptance Criteria

1. WHEN 执行 `version`，THE CLI SHALL 打印 Fayger 运行基座版本（来自 crate 版本）与运行层提供的 Runtime_Interface 版本。
2. WHEN 执行 `info`，THE CLI SHALL 依据当前平台确定性地构造一个 HostEnvironment，并在内置适配器注册表上选择匹配的适配器。
3. WHEN `info` 成功选中适配器，THE CLI SHALL 打印其 `platform_id` 与授予能力集合（granted capabilities）。
4. WHERE 当前平台无任何内置适配器匹配，THE CLI SHALL 报告该情况并以执行失败退出码 `1` 结束（不 panic）。
5. THE 宿主画像构造 SHALL 为确定性的（同一平台同一输入产出同一画像）。

### Requirement 3: inspect 子命令

**User Story:** 作为开发者，我希望查看一个 BuF 制品的元数据，以便了解它声明了什么。

#### Acceptance Criteria

1. WHEN 执行 `inspect <file>`，THE CLI SHALL 读取该文件并用默认加载策略经加载链加载为 BuFObject（Parse → Verify(Digest) → Negotiate(Version)）。
2. WHEN 加载成功，THE CLI SHALL 打印 Manifest 摘要：`schema_version`、`runtime_interface_min`、`entry`、section 列表（每段 id / kind / length / visibility）、capabilities、quotas（若有）、签名是否存在、弃用提示（若有）。
3. WHEN 文件不存在或不可读，THE CLI SHALL 返回执行失败并以退出码 `1` 结束（不 panic）。
4. WHEN 加载链校验失败（结构/摘要/版本），THE CLI SHALL 把底层 `FaygerError` 的 `code` 与关键 `context` 呈现给用户，并以退出码 `1` 结束。
5. THE `inspect_object`（对已加载 BuFObject 产出摘要文本）SHALL 为纯函数，且对任意合法 BuFObject 的 section 数与各 section id 的呈现与 Manifest 完全一致。

### Requirement 4: run 子命令

**User Story:** 作为终端用户，我希望运行一个 BuF 并看到它的生命周期与事件，以便确认运行基座工作正常。

#### Acceptance Criteria

1. WHEN 执行 `run <file>`，THE CLI SHALL 加载该 BuF，在运行层参考实现上创建实例并依次执行 `init → start → 推进执行直至程序耗尽 → terminate`。
2. WHILE 执行实例，THE CLI SHALL 订阅运行层内置事件总线，收集发出的生命周期与配额事件。
3. WHEN 运行结束，THE CLI SHALL 打印一个 RunReport：最终生命周期状态、派发的指令数、发出的生命周期事件数与配额事件数。
4. THE 正常运行结束后实例的最终状态 SHALL 为 `Terminated`。
5. THE 派发的指令数 SHALL 等于该 BuF 的 section 数（运行层从每个 section 派生一条确定性指令）。
6. WHEN 加载失败，THE CLI SHALL 以退出码 `1` 结束并呈现底层错误；`run_object`（对已加载 BuFObject 执行并产出 RunReport）SHALL 为纯函数（不触达 FS）。

### Requirement 5: pack 与 demo 子命令

**User Story:** 作为开发者，我希望用 CLI 把若干内容文件封装为一个可运行的 BuF，并能一键自检安装。

#### Acceptance Criteria

1. WHEN 执行 `pack --entry <name> --section <id>:<kind>=<path> [...] -o <out>`，THE CLI SHALL 读取每个 section 文件、构造 Manifest（schema/runtime_min 取默认）、封装（seal）并序列化为字节流写入 `<out>`。
2. THE `pack_bytes`（由 entry 与「(id, kind, payload bytes)」列表产出 BuF 字节流）SHALL 为纯函数（字节进、字节出，不触达 FS），且其产出经加载链 `load` 加载成功。
3. WHEN `pack` 的某个 `<id>:<kind>=<path>` 形式非法或 kind 不在 {code, data, asset}，THE CLI SHALL 返回用法错误并以退出码 `2` 结束。
4. WHEN 执行 `demo`，THE CLI SHALL 在内存中构造一个示例 BuF（不读取任何文件）、加载并运行，打印其 RunReport，最终状态为 `Terminated`。
5. THE pack 产出的 BuF 经 `inspect_object` 呈现的 section id 序列 SHALL 与 pack 输入的 id 序列一致（顺序保持）。

### Requirement 6: 退出码与错误呈现

**User Story:** 作为脚本/CI 使用者，我希望 CLI 的退出码可据以判断成败，以便编排自动化。

#### Acceptance Criteria

1. WHEN 命令成功执行，THE CLI SHALL 以退出码 `0` 结束。
2. WHEN 发生用法错误（未知命令/缺参/参数格式错误），THE CLI SHALL 以退出码 `2` 结束。
3. WHEN 发生执行失败（文件不可读、加载链失败、运行失败、无匹配适配器），THE CLI SHALL 以退出码 `1` 结束。
4. THE 从 CliError 到退出码的映射 SHALL 为确定性的纯函数。
5. THE CLI SHALL NOT 因任何用户输入（含畸形参数、损坏文件、超界版本）而 panic；所有失败路径以 CliError 表达并映射为退出码。

### Requirement 7: 跨平台分发与安装

**User Story:** 作为终端用户，我希望在 macOS / Linux / Windows 上方便地安装 `fayger`，以便像装 Docker 客户端那样获得运行基座。

#### Acceptance Criteria

1. THE M9 SHALL 提供一个 release 构建脚本，产出当前平台的优化二进制（`cargo build --release` 的 `fayger`）。
2. THE M9 SHALL 提供一个 GitHub Release 工作流，在打 tag 时为 Linux / macOS / Windows 构建 `fayger` 二进制并随产物附带 SHA-256 校验和。
3. THE M9 SHALL 提供一个一键安装脚本（`curl ... | sh`），把 `fayger` 放入用户 PATH（默认 `~/.fayger/bin`）并提示 PATH 配置。
4. THE M9 SHALL 在文档中说明各平台的安装步骤（脚本安装、从源码 `cargo install --path` 安装）。
5. THE 分发产物 SHALL 可被签名/校验：随二进制提供 SHA-256 校验和文件，文档说明校验方法。

### Requirement 8: 测试与质量

**User Story:** 作为 Fayger 项目的维护者，我希望 M9 与前序里程碑保持同一质量基线。

#### Acceptance Criteria

1. THE M9 实现 SHALL 至少包含以下属性测试：参数解析的全域性（任意 argv 要么得到 Command 要么得到用法错误，且已知子命令恒被识别）、`pack_bytes → load → run` 往返（运行成功且最终 Terminated、派发指令数等于 section 数）、`inspect_object` 与 Manifest 一致（section id 序列一致）、CliError→退出码映射确定性。
2. THE M9 实现 SHALL 提供至少一个端到端用例（纯内存，无 FS）：`demo` 路径或 `pack_bytes` 产出经 `load` 加载并 `run_object` 运行，断言 RunReport 的最终状态与派发数。
3. THE M9 实现的所有 PBT 与单元测试 SHALL 在 CI 友好的方式下可重复执行（无外部依赖、无网络、无文件系统副作用）；触达 FS 的子命令逻辑以薄封装隔离，纯核心可在内存中测试。
4. WHEN PBT 发现失败用例，THE 测试基础设施 SHALL 自动记录最小化反例（沿用前序里程碑的 `.regressions` 归档机制）。
5. THE M9 实现 SHALL 与前序里程碑共用同一 CI（fmt / clippy / test / doc 四项检查，三平台），且 M0–M8 既有测试在 M9 引入后仍全部通过。

### Requirement 9: M9 范围边界

**User Story:** 作为 Fayger 项目的维护者，我希望明确 M9 不交付哪些能力，以避免实现越界。

#### Acceptance Criteria

1. THE M9 的 `run` SHALL 走运行层参考实现（M2 的 `Runtime` + `MockAdapter` 派生执行），SHALL NOT 实现运行层与适配层的统一桥接（那是后续集成里程碑）。
2. THE M9 SHALL NOT 引入真实 OS 资源计量；`run` 不强制注入用量（`demo` 可选地注入用量以演示配额事件）。
3. THE M9 SHALL NOT 修改 M0–M8 已固化的库 crate 的对外契约（字节布局、加载链、错误码语义、运行层/适配层/观测接口）；M9 仅新增 `fayger-cli` crate 与分发产物，并以加性方式消费既有 API。
4. THE M9 SHALL NOT 发布到任何第三方包仓库（crates.io / Homebrew core / apt / winget）；M9 只产出可分发的二进制、校验和与安装脚本，实际上架属后续运营。
5. THE M9 的 CLI 解析 SHALL NOT 依赖大型第三方参数库（保持依赖精简、解析可被 PBT 直接验证）；采用工作区内自实现的小型解析。

## 约束与假设

- **目标语言**：M9 仅交付 Rust 实现。
- **crate 归属**：M9 新增 `fayger-cli` crate（同时含 `lib`（纯核心 + FS 薄封装）与 `bin`（名为 `fayger` 的薄入口））。
- **依赖方向**：`fayger-cli` 依赖 `fayger-buf` / `fayger-error` / `fayger-loader` / `fayger-runtime` / `fayger-adapter` / `fayger-observability`；不被任何库 crate 反向依赖。
- **新增依赖**：M9 不引入新的第三方运行时依赖（参数解析自实现）；`proptest` 沿用既有 dev-dependency。
- **MSRV 与工具链**：沿用 M0 锁定的 Rust stable 与 `rust-toolchain.toml`，不漂移。
- **不 panic**：CLI 全路径以 `Result` + CliError 表达失败；`main` 仅做收集参数 → 调用纯核心 → 写出 → 映射退出码。
- **可测建模**：FS/进程交互隔离在薄封装层；参数解析、inspect/run/pack 核心、退出码映射、宿主画像构造均为纯函数，由单元测试与 PBT 覆盖。
