# Fayger 快速开始

**Fayger**（`v0.3.1`）是面向 Fay（`iFay` 与 `coFay`）的跨平台三位一体虚拟运行环境（`faygerd` 守护进程 + `fayger` CLI + 内嵌 **FayGer Dashboard** 与 `/mobile` 随身监护控制台）。它负责接收可分发的 **BuF（Built-up Fay）** 制品与 **FayManifest** 声明式装配清单，在 macOS、Linux、Windows 桌面、移动伴侣端、服务器与具身硬件宿主终端之间提供统一的加载校验、**Ego** 与外部大模型双系统协同、原生操作系统权限探测与硬件/软件 **CAP** 受控执行、以及连接 **公誉链（GMChain）** 的 **美誉值（MeriToken，`μ`）** 展示、入账与优先权消减闭环。

> 术语边界：本文档中**宿主（Host）**仅指被附身的终端设备或应用进程；自然人一律称为**人类原型（Human Prime）**。

## 桌面端安装与启动控制台（`v0.3.1` — macOS `.dmg` / Linux / Windows）

**macOS 图形化安装包（Apple Silicon）：**
- 从 GitHub Releases 下载 **`FayGer-macos-arm64.dmg`**，将 `FayGer.app` 拖入 `/Applications` 即可双击启动（打包脚本见 [`scripts/package-macos-dmg.sh`](../../scripts/package-macos-dmg.sh)）。

**macOS（Apple Silicon / Intel）与 Linux（x86_64 / ARM64）一键安装：**

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

执行 `fayger up --open` 后，本机浏览器将自动打开 `http://127.0.0.1:7979` 控制台，人类原型可在工作台中一键申请宿主系统权限、创建或加载 `iFay` 数字灵魂、配置本地/云端外部大模型与 `Ego` 人格红线、拨动 **Faying** 上岗拨杆、触发红色 **`G0` 紧急制动**、审批高风险终端硬件/桌面软件操控，以及管理 **美誉值（MeriToken）**。

## 移动端随身监护（`/mobile` PWA、Android `.apk` 与 iOS TestFlight）

人类原型可通过手机随时随地监护已挂载的 `iFay` / `coFay`、远程审批 **CAP** 请求并一键按下红色 **`G0` 紧急制动**（完整说明见 [`release/mobile/README.md`](../../release/mobile/README.md)）：

- **双端零安装 PWA**：在同一局域网用手机浏览器打开 `http://<宿主局域网IP>:7979/mobile` 并“添加到主屏幕”。
- **Android 原生安装包**：直接从 GitHub Releases 下载安装 **`fayger-mobile-guardian-android.apk`**，或由 `release/mobile/android/` 零依赖工程自行构建。
- **iOS TestFlight 外部公测套件**：使用 [`release/mobile/ios-testflight/`](../../release/mobile/ios-testflight/README.md) 一键归档并上传至 App Store Connect TestFlight（最高支持 `10,000` 名外部测试者）。

## 硬件厂商三档具身接入套件（无人机 / 遥控车 / 机械臂 / 数控机床 / 摄影摇臂）

物理设备厂商可通过 [`release/hardware-sdk/`](../../release/hardware-sdk/README.md) 将具身硬件作为受控**宿主（Host）**安全接入：

1. **Tier A（嵌入式 Linux 直装引擎）**：在 Raspberry Pi、Jetson、RK3588 等伴侣板上直接运行预编译 Linux 二进制并挂载 `systemd` 服务。
2. **Tier B（零依赖 C99 固件桥接 `FHBP/1.0`）**：在遥控车（RC Car）、数控机床（CNC）、电动摄影摇臂（Camera Jib）、无人机（Drone）与机械臂（Robot）的 MCU/RTOS/PLC 固件中引入单头文件 [`fayger_hw_bridge.h`](../../release/hardware-sdk/c99/fayger_hw_bridge.h)，在固件侧硬性执行物理安全包络预检、死人开关看门狗与毫秒级 `G0` 安全急停。
3. **Tier C（定制系统 Rust `HostAdapter` HAL）**：实现 5 函数 `HostAdapter` 硬件抽象层并通过 `fayger conformance` 黑盒套件验证。

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
