# Fayger 快速开始

**Fayger**（`v0.3.0`）是面向 Fay（`iFay` 与 `coFay`）的跨平台三位一体虚拟运行环境（`faygerd` 守护进程 + `fayger` CLI + 内嵌 **FayGer Dashboard** 可视化控制台）。它负责接收可分发的 **BuF（Built-up Fay）** 制品与 **FayManifest** 声明式装配清单，在 macOS、Linux、Windows 桌面、服务器与嵌入式宿主终端之间提供统一的加载校验、**Ego** 与外部大模型双系统协同、原生操作系统权限探测与硬件/软件 **CAP** 受控执行、以及连接 **公誉链（GMChain）** 的 **美誉值（MeriToken，`μ`）** 展示、入账与优先权消减闭环。

> 术语边界：本文档中**宿主（Host）**仅指被附身的终端设备或应用进程；自然人一律称为**人类原型（Human Prime）**。

## 一键下载安装并启动控制台（macOS / Linux / Windows）

**macOS（Apple Silicon / Intel）与 Linux（x86_64 / ARM64）：**

```bash
curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.sh | sh
export PATH="$HOME/.fayger/bin:$PATH"
fayger up --open
```

**Windows（PowerShell x86_64）：**

```powershell
irm https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.ps1 | iex
fayger up --open
```

执行 `fayger up --open` 后，本机浏览器将自动打开 `http://127.0.0.1:7979` 控制台，人类原型可在六大工作台标签页中一键申请宿主系统权限、创建或加载 `iFay` 数字灵魂、配置本地/云端外部大模型与 `Ego` 人格红线、拨动 **Faying** 上岗拨杆、审批高风险终端硬件/桌面软件操控，以及管理 **美誉值（MeriToken）**。

## 核心三层架构

1. **加载层（Loader Layer）**：解析五段式 `BuF` 制品，校验摘要、Ed25519 签名与运行接口版本约束。
2. **运行层（Runtime Layer）**：通过语言中立的 `Runtime_Interface` 管理生命周期迁移（`Loaded -> Initialized -> Running -> Suspended -> Terminated/Failed`）与资源配额隔离，并由 `fayger-conformance` 黑盒一致性套件验证跨实现行为等价性。
3. **适配层（Adapter Layer）**：将平台无关的 `Universal_Instruction` 与受控宿主请求映射为宿主能力，在显式 **Faying** 监护、`BuF ∩ HostPool ∩ PerFayPolicy` 实时交集与生产级 **CAP** 授权门禁通过后执行真实硬件、操作系统与桌面应用操控。

## 常用命令行入口

```bash
# 启动三位一体守护引擎并自动在浏览器打开内嵌控制台
fayger up --open

# 查看运行基座版本与宿主环境画像
fayger version
fayger info
fayger profile --json

# 运行 Runtime_Interface 语言无关黑盒一致性套件
fayger conformance

# 使用静态已签名注册表索引解析并装配 FayManifest
fayger assemble app.faymanifest.json \
  --registry-index registry-index.json \
  --trust-anchors trust-anchors.json

# 运行端到端受控活体 iFay 闭环演示
fayger demo live-ifay
```

完整架构与分章设计请阅读同目录下的 `blueprint/` 章节。
