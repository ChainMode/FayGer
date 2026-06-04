# 安装 Fayger 运行基座（`fayger`）

`fayger` 是 Fay 多平台虚拟运行环境的命令行运行基座——就像 Docker 客户端或 JRE 那样，安装后即可在终端运行 BuF 制品。本文介绍三种获取方式：预编译二进制、一键脚本、从源码安装。

支持平台：macOS、Linux、Windows。

---

## 方式一：下载预编译二进制（推荐）

每个 `v*` 版本的 GitHub Release 都附带三平台二进制与对应的 SHA-256 校验和：

| 平台 | 文件 |
| --- | --- |
| Linux x86_64 | `fayger-x86_64-unknown-linux-gnu` |
| macOS (Apple Silicon) | `fayger-aarch64-apple-darwin` |
| Windows x86_64 | `fayger-x86_64-pc-windows-msvc.exe` |

步骤（以 macOS 为例）：

```bash
# 1. 从 Release 页面下载二进制与校验和
#    fayger-aarch64-apple-darwin
#    fayger-aarch64-apple-darwin.sha256

# 2. 校验完整性
shasum -a 256 -c fayger-aarch64-apple-darwin.sha256

# 3. 安装到 PATH（用随附的安装脚本，见方式二），或手动：
mkdir -p ~/.fayger/bin
cp fayger-aarch64-apple-darwin ~/.fayger/bin/fayger
chmod +x ~/.fayger/bin/fayger
export PATH="$HOME/.fayger/bin:$PATH"   # 建议写入 ~/.zshrc / ~/.bashrc

# 4. 验证
fayger version
fayger demo
```

Windows（PowerShell）校验：

```powershell
Get-FileHash .\fayger-x86_64-pc-windows-msvc.exe -Algorithm SHA256
# 与 .sha256 文件内容比对
```

---

## 方式二：一键脚本安装

仓库 `release/dev/core/scripts/` 下提供两个脚本：

- `build-release.sh`：本地构建优化版 `fayger` 并打印产物路径与校验和。
- `install.sh`：把一个已构建/已下载的二进制复制到 `~/.fayger/bin` 并提示 PATH 配置。

本地构建并安装：

```bash
cd release/dev/core
./scripts/build-release.sh          # 产出 target/release/fayger
./scripts/install.sh                # 安装到 ~/.fayger/bin
```

安装一个已下载的二进制：

```bash
FAYGER_BIN=/path/to/fayger ./scripts/install.sh
# 或
./scripts/install.sh /path/to/fayger
```

安装目录可用 `FAYGER_HOME` 覆盖（默认 `~/.fayger`）。脚本不联网下载——下载请用方式一。

---

## 方式三：从源码安装

需要 Rust stable（≥ 1.83，仓库已用 `rust-toolchain.toml` 锁定）。

```bash
cd release/dev/core
cargo install --path crates/fayger-cli --bin fayger
# 产物进入 ~/.cargo/bin/fayger（确保它在 PATH 中）
fayger version
```

或仅构建不安装：

```bash
cargo build --release -p fayger-cli --bin fayger
./target/release/fayger version
```

---

## 验证安装

```bash
fayger version     # 打印运行基座与 Runtime_Interface 版本
fayger demo        # 在内存中构造并运行一个示例 BuF（类似 docker run hello-world）
fayger info        # 探测当前宿主、选中适配器、列出授予能力
```

`fayger demo` 成功时会打印一份运行报告，最终状态为 `Terminated`。

---

## 常用命令

```bash
# 查看一个 BuF 的元数据
fayger inspect app.buf

# 运行一个 BuF
fayger run app.buf

# 把若干内容文件封装为一个可运行的 BuF
fayger pack --entry main \
  --section code:code=main.bin \
  --section res:asset=logo.png \
  -o app.buf

# 帮助
fayger help
fayger help pack
```

退出码约定：成功 `0`、用法错误 `2`、执行失败（文件不可读 / 加载校验失败 / 无匹配适配器）`1`。
