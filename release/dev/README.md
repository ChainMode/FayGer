# Fayger 开发版（release/dev）

本目录是 Fayger 第一阶段（蓝图见 `/docs/zh-CN/blueprint/`）的开发实现。

## 设计原则

Fayger 是 Fay 生态的虚拟运行环境，覆盖桌面、服务器、浏览器、嵌入式（无人机/摄像头）与 In-App 等多种宿主。可移植性优先：BuF 编译一次，Fayger 在每个宿主上让其执行起来。

主参考实现使用 **Rust**，理由：

- 一份代码可通过编译目标差异化覆盖桌面 / 服务器 / 嵌入式 / WASM。
- 内存安全 + 零开销，适合性能受限场景。
- 生态完善：`ciborium` / `ed25519-dalek` / `proptest` 等。

各平台的 Adapter 用最自然的语言：
- 桌面 / 服务器：Rust。
- 浏览器：TypeScript + Rust(WASM) 通过 wasm-bindgen 桥接。
- In-App：FFI 绑定（Java / Swift / .NET 等）。

## 目录结构

```
release/dev/
├── core/                        Rust 核心：Loader + Runtime（多平台共用）
│   ├── crates/
│   │   ├── fayger-buf/          BuF 格式、Parser、Serializer
│   │   ├── fayger-loader/       加载链、签名、版本协商（M1+）
│   │   ├── fayger-runtime/      Runtime_Interface、生命周期（M2+）
│   │   ├── fayger-error/        统一错误模型
│   │   └── fayger-conformance/  跨语言一致性测试套件（M2+）
│   └── targets/                 各平台输出二进制 / 库
├── adapters/                    各平台 Adapter
│   ├── native/                  Linux / macOS / Windows
│   ├── server/                  无 GUI
│   ├── browser/                 TS + Rust(WASM)
│   └── inapp/                   FFI 绑定
├── conformance/                 与语言无关的一致性测试用例
└── tools/                       命令行工具
    ├── buf-cli/                 打包 / 校验 / 签名
    └── buf-inspect/             BuF 结构查看器
```

后续里程碑会按需填充各目录。

## 分支策略

```
main                              蓝图、文档、稳定 release
  └── release/dev                 长期开发分支，所有里程碑在此推进
        ├── feat/m0-buf-parser    单个里程碑的功能分支（合回 release/dev）
        ├── feat/m0-buf-serializer
        └── ...
```

合并规则：

- `feat/* → release/dev`：每完成一个任务（含 PBT 通过）即合回。
- `release/dev → main`：每完成一个里程碑（M0、M1...）合一次，配 tag。

## 里程碑路线（暂定）

| 里程碑 | 主题 | 蓝图覆盖 |
| --- | --- | --- |
| M0 | BuF 核心：格式 + Parser + Serializer + 往返一致性 | Req 1.1 / 1.2 / 1.4 / 2.1 / 2.2 / 2.3 / 2.4 |
| M1 | 加载链：结构校验 + Section digest + 版本协商 | Req 1.3 / 1.5 / 10.x |
| M2 | Runtime_Interface 骨架 + 生命周期 + Mock Adapter | Req 3.x / 4.x |
| M3 | Universal_Instruction + Adapter 注册 + 能力裁剪 | Req 5.x / 6.x |
| M4 | 签名 + 强制签名 + TrustStore | Req 11.x |
| M5 | BuF_Source 抽象 + Eager/Lazy | Req 13.x |
| M6 | LoadProfile + 部分加载 | Req 12.x |
| M7 | 资源监测 + 配额 + 故障隔离 | Req 7.x |
| M8 | 可观测性事件总线 + 错误链 | Req 8.x / 9.x |

里程碑顺序与范围会根据实际进度调整。
