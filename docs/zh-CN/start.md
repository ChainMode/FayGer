# Fayger 快速开始

**Fayger** 是面向 Fay（`iFay` 与 `coFay`）的多平台虚拟运行环境（Virtual Runtime Environment）。它负责接收可分发的 **BuF（Built-up Fay）** 制品与 **FayManifest** 声明式装配清单，在桌面、服务器、浏览器与嵌入式宿主终端之间提供统一的加载校验、运行生命周期管理、适配层能力裁剪与可审计执行闭环。

> 术语边界：本文档中**宿主（Host）**仅指被附身的终端设备或应用进程；自然人一律称为**人类原型（Human Prime）**。

## 核心三层架构

1. **加载层（Loader Layer）**：解析五段式 `BuF` 制品，校验摘要、Ed25519 签名与运行接口版本约束。
2. **运行层（Runtime Layer）**：通过语言中立的 `Runtime_Interface` 管理生命周期迁移（`Loaded -> Initialized -> Running -> Suspended -> Terminated/Failed`）与资源配额隔离，并由 `fayger-conformance` 黑盒一致性套件验证跨实现行为等价性。
3. **适配层（Adapter Layer）**：将平台无关的 `Universal_Instruction` 映射为宿主能力，并在生产级 CAP 授权门禁与已签名 `Ego_Package` 双层验签通过后执行受控工作区操作。

## 常用命令行入口

```bash
# 查看运行基座版本与宿主环境画像
fayger version
fayger info

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
