//! 命令行参数解析（M9）。
//!
//! [`parse_args`] 是纯函数：把 argv（不含程序名）解析为 [`Command`] 或 [`CliError::Usage`]，
//! 永不 panic。子命令集合小且固定，自实现解析便于被 PBT 直接验证全域性。

use crate::error::CliError;

/// 一次 CLI 调用解析出的子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// 打印版本与构建信息。
    Version,
    /// 探测宿主环境并打印选中的适配器与授予能力。
    Info,
    /// 加载并校验一个 BuF，打印其 Manifest 摘要。
    Inspect {
        /// BuF 文件路径。
        path: String,
    },
    /// 加载并运行一个 BuF，打印运行报告。
    Run {
        /// BuF 文件路径。
        path: String,
    },
    /// 加载一个 BuF 并经真实适配层在当前宿主上运行，打印产生的 HostSyscall。
    Bridge {
        /// BuF 文件路径。
        path: String,
    },
    /// 解析并装配一个 FayManifest（JSON），打印依赖解析报告。
    Assemble {
        /// FayManifest JSON 文件路径。
        path: String,
        /// 可选的静态已签名注册表在线索引 JSON 文件路径。
        registry_index: Option<String>,
        /// 可选的离线已验签缓存索引 JSON 文件路径。
        offline_cache: Option<String>,
        /// 注册表信任锚 JSON 文件路径（配合 `--registry-index` 或 `--offline-cache`）。
        trust_anchors: Option<String>,
    },
    /// 运行 `Runtime_Interface` 语言无关黑盒一致性测试套件并输出合规报告。
    Conformance {
        /// 是否以格式化 JSON 输出报告。
        json: bool,
    },
    /// Evaluate one CAP operation against a FayManifest and driver manifest.
    Evaluate {
        /// FayManifest JSON path.
        manifest: String,
        /// CAP driver manifest JSON path.
        driver: String,
        /// Driver resource id.
        resource: String,
        /// Driver operation id.
        operation: String,
    },
    /// 把若干 section 文件封装为一个可加载的 BuF。
    Pack {
        /// 入口名。
        entry: String,
        /// section 规格列表。
        sections: Vec<PackSpec>,
        /// 输出文件路径。
        out: String,
    },
    /// 在内存中构造并运行一个示例 BuF（安装自检）。
    Demo,
    /// Run the complete local drone telemetry MVP demonstration.
    DemoDrone,
    /// Run the end-to-end authenticated live iFay controlled workspace slice.
    DemoLiveIfay,
    /// Run an authenticated live iFay controlled execution bundle from explicit files.
    RunLive {
        /// FayManifest JSON path.
        manifest: String,
        /// Production CAP EvaluationRequest JSON path.
        request: String,
        /// CAP trust anchors JSON path.
        anchors: String,
        /// Signed Ego_Package directory path.
        ego_package: String,
        /// Trusted Ego Ed25519 subkey public key hex.
        ego_pubkey: String,
        /// Sealed task BuF path.
        buf: String,
        /// Controlled sandbox workspace directory path.
        workspace: String,
        /// Audit approval ticket string.
        ticket: String,
    },
    /// 输出标准化设备运行时环境描述文件（`fayger-device-profile/1.0`）。
    Profile {
        /// 是否以 JSON 输出。
        json: bool,
        /// 可选状态目录（读取已配置的宿主资源池）。
        state_dir: Option<String>,
    },
    /// 内置 `FayID` 身份与 `Auth Exchange` 凭据保险箱操作。
    Wallet {
        /// 子动作。
        subcommand: WalletSubcommand,
    },
    /// 一键构建并密封可运行的 `.buf` `iFay` 数字灵魂或 `coFay` 公共席位包。
    SoulCreate {
        /// `ifay` 或 `cofay`。
        kind: String,
        /// 灵魂/席位显示名称。
        name: String,
        /// 输出 `.buf` 路径。
        out: String,
        /// 可选逗号分隔的请求能力列表。
        capabilities: Option<Vec<String>>,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否以 JSON 输出回执。
        json: bool,
    },
    /// 启动 `faygerd` 守护引擎与内嵌 `FayGer Dashboard` Web 控制台（支持别名 `serve`）。
    Up {
        /// 监听地址（默认 `127.0.0.1:7979`）。
        addr: String,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 可选回环 `GMChain` JSON-RPC 端点（默认 `http://127.0.0.1:8546`）。
        gmchain_endpoint: Option<String>,
    },
    /// 在命令行中与已挂载 `Fay` 进行 `ICP`（`.ctx`）对话并展示终端文本降级结果。
    Chat {
        /// 目标 `fay_id`。
        fay_id: String,
        /// 对话消息或快捷宿主指令（如 `/read`、`/write`）。
        message: String,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否直接输出原始 `.ctx` JSON。
        json: bool,
    },
    /// 打印用法帮助。
    Help {
        /// 可选的子命令主题。
        topic: Option<String>,
    },
}

/// `fayger wallet` 子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletSubcommand {
    /// 初始化本地 `FayID` 助记词与根身份保险箱。
    Init {
        /// 钱包标签。
        label: String,
        /// 可选显式助记词短语。
        seed_phrase: Option<String>,
        /// 是否强制覆盖已存在的保险箱。
        force: bool,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否输出 JSON。
        json: bool,
    },
    /// 查看对外脱敏的保险箱摘要（绝不回显明文 `Human ID` 或原始凭据）。
    Show {
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否输出 JSON。
        json: bool,
    },
    /// 派生个人 `iFay` 身份（`ifay_...`）。
    DeriveIfay {
        /// 名称。
        name: String,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否输出 JSON。
        json: bool,
    },
    /// 派生公共席位 `coFay` 身份（`cofay_...` + `RoleCharter` 1.0）。
    DeriveCofay {
        /// 席位名称。
        name: String,
        /// 公开岗位头衔。
        role: Option<String>,
        /// 职权范围说明。
        scope: Option<String>,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否输出 JSON。
        json: bool,
    },
    /// 签发防关联动态码（`dync_...`）。
    DynamicCode {
        /// 用途说明。
        purpose: String,
        /// 有效期秒数。
        ttl_secs: u64,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否输出 JSON。
        json: bool,
    },
    /// 封存传统 API Key / Token 并兑换限时 `Authorization Grant`。
    ExchangeGrant {
        /// 服务商标识。
        provider: String,
        /// 原始凭据密钥。
        secret: String,
        /// 可选绑定 `fay_id`。
        fay_id: Option<String>,
        /// 作用域列表。
        scopes: Vec<String>,
        /// 有效期秒数。
        ttl_secs: u64,
        /// 可选状态目录。
        state_dir: Option<String>,
        /// 是否输出 JSON。
        json: bool,
    },
}

/// `pack` 的单条 section 规格：`<id>:<kind>=<path>`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSpec {
    /// section 标识。
    pub id: String,
    /// section 类型（code / data / asset）。
    pub kind: String,
    /// 内容文件路径。
    pub path: String,
}

/// `pack --section` 可接受的 kind 取值。
pub const VALID_KINDS: [&str; 3] = ["code", "data", "asset"];

/// 解析 argv（不含程序名）为 [`Command`]。
///
/// # Errors
/// 未知子命令、缺失必需参数或参数格式错误时返回 [`CliError::Usage`]。
pub fn parse_args(args: &[String]) -> Result<Command, CliError> {
    let Some(first) = args.first() else {
        return Ok(Command::Help { topic: None });
    };
    match first.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help {
            topic: args.get(1).cloned(),
        }),
        "version" | "--version" | "-V" => Ok(Command::Version),
        "info" => Ok(Command::Info),
        "demo" => match args.get(1).map(String::as_str) {
            None => Ok(Command::Demo),
            Some("drone") if args.len() == 2 => Ok(Command::DemoDrone),
            Some("live-ifay") if args.len() == 2 => Ok(Command::DemoLiveIfay),
            Some(other) => Err(CliError::Usage(format!("unknown demo scenario: {other}"))),
        },
        "inspect" => {
            let path = require_positional(args, "inspect <file>")?;
            Ok(Command::Inspect { path })
        }
        "run" => {
            let path = require_positional(args, "run <file>")?;
            Ok(Command::Run { path })
        }
        "bridge" => {
            let path = require_positional(args, "bridge <file>")?;
            Ok(Command::Bridge { path })
        }
        "assemble" => parse_assemble(&args[1..]),
        "conformance" => parse_conformance(&args[1..]),
        "evaluate" => parse_evaluate(&args[1..]),
        "run-live" => parse_run_live(&args[1..]),
        "pack" => parse_pack(&args[1..]),
        "profile" => parse_profile(&args[1..]),
        "wallet" => parse_wallet(&args[1..]),
        "soul" => parse_soul(&args[1..]),
        "up" | "serve" => parse_up(&args[1..]),
        "chat" => parse_chat(&args[1..]),
        other => Err(CliError::Usage(format!("unknown command: {other}"))),
    }
}

/// 解析 `assemble <file.json> [--registry-index <idx.json>] [--offline-cache <cache.json>] [--trust-anchors <anchors.json>]`。
fn parse_assemble(rest: &[String]) -> Result<Command, CliError> {
    let Some(path) = rest.first().filter(|s| !s.starts_with('-')).cloned() else {
        return Err(CliError::Usage(
            "missing argument: assemble <file>".to_owned(),
        ));
    };
    let mut registry_index = None;
    let mut offline_cache = None;
    let mut trust_anchors = None;
    let mut i = 1;
    while i < rest.len() {
        let (slot, usage) = match rest[i].as_str() {
            "--registry-index" => (&mut registry_index, "--registry-index <index.json>"),
            "--offline-cache" => (&mut offline_cache, "--offline-cache <cache.json>"),
            "--trust-anchors" => (&mut trust_anchors, "--trust-anchors <anchors.json>"),
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected assemble argument: {other}"
                )));
            }
        };
        if slot.is_some() {
            return Err(CliError::Usage(format!(
                "duplicate assemble argument: {}",
                rest[i]
            )));
        }
        *slot = Some(next_value(rest, i, usage)?);
        i += 2;
    }
    if (registry_index.is_some() || offline_cache.is_some()) && trust_anchors.is_none() {
        return Err(CliError::Usage(
            "assemble with --registry-index or --offline-cache requires --trust-anchors <anchors.json>"
                .to_owned(),
        ));
    }
    Ok(Command::Assemble {
        path,
        registry_index,
        offline_cache,
        trust_anchors,
    })
}

/// 解析 `conformance [--json]`。
fn parse_conformance(rest: &[String]) -> Result<Command, CliError> {
    match rest {
        [] => Ok(Command::Conformance { json: false }),
        [flag] if flag == "--json" => Ok(Command::Conformance { json: true }),
        [other, ..] => Err(CliError::Usage(format!(
            "unexpected conformance argument: {other}"
        ))),
    }
}

/// Parses the four named inputs for the MVP CAP evaluator.
fn parse_evaluate(rest: &[String]) -> Result<Command, CliError> {
    let mut manifest = None;
    let mut driver = None;
    let mut resource = None;
    let mut operation = None;
    let mut i = 0;

    while i < rest.len() {
        let (slot, usage) = match rest[i].as_str() {
            "--manifest" => (&mut manifest, "--manifest <faymanifest.json>"),
            "--driver" => (&mut driver, "--driver <driver.json>"),
            "--resource" => (&mut resource, "--resource <resource-id>"),
            "--operation" => (&mut operation, "--operation <operation-id>"),
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected evaluate argument: {other}"
                )));
            }
        };
        if slot.is_some() {
            return Err(CliError::Usage(format!(
                "duplicate evaluate argument: {}",
                rest[i]
            )));
        }
        *slot = Some(next_value(rest, i, usage)?);
        i += 2;
    }

    Ok(Command::Evaluate {
        manifest: manifest.ok_or_else(|| {
            CliError::Usage("evaluate requires --manifest <faymanifest.json>".to_owned())
        })?,
        driver: driver.ok_or_else(|| {
            CliError::Usage("evaluate requires --driver <driver.json>".to_owned())
        })?,
        resource: resource.ok_or_else(|| {
            CliError::Usage("evaluate requires --resource <resource-id>".to_owned())
        })?,
        operation: operation.ok_or_else(|| {
            CliError::Usage("evaluate requires --operation <operation-id>".to_owned())
        })?,
    })
}

/// Parses named inputs for `fayger run-live`.
#[allow(clippy::too_many_lines)]
fn parse_run_live(rest: &[String]) -> Result<Command, CliError> {
    let mut manifest = None;
    let mut request = None;
    let mut anchors = None;
    let mut ego_package = None;
    let mut ego_pubkey = None;
    let mut buf = None;
    let mut workspace = None;
    let mut ticket = None;
    let mut i = 0;

    while i < rest.len() {
        let (slot, usage) = match rest[i].as_str() {
            "--manifest" => (&mut manifest, "--manifest <faymanifest.json>"),
            "--request" => (&mut request, "--request <cap-request.json>"),
            "--anchors" => (&mut anchors, "--anchors <trust-anchors.json>"),
            "--ego-package" => (&mut ego_package, "--ego-package <dir>"),
            "--ego-pubkey" => (&mut ego_pubkey, "--ego-pubkey <hex64>"),
            "--buf" => (&mut buf, "--buf <task.buf>"),
            "--workspace" => (&mut workspace, "--workspace <dir>"),
            "--ticket" => (&mut ticket, "--ticket <approval-ticket>"),
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected run-live argument: {other}"
                )));
            }
        };
        if slot.is_some() {
            return Err(CliError::Usage(format!(
                "duplicate run-live argument: {}",
                rest[i]
            )));
        }
        *slot = Some(next_value(rest, i, usage)?);
        i += 2;
    }

    Ok(Command::RunLive {
        manifest: manifest.ok_or_else(|| {
            CliError::Usage("run-live requires --manifest <faymanifest.json>".to_owned())
        })?,
        request: request.ok_or_else(|| {
            CliError::Usage("run-live requires --request <cap-request.json>".to_owned())
        })?,
        anchors: anchors.ok_or_else(|| {
            CliError::Usage("run-live requires --anchors <trust-anchors.json>".to_owned())
        })?,
        ego_package: ego_package
            .ok_or_else(|| CliError::Usage("run-live requires --ego-package <dir>".to_owned()))?,
        ego_pubkey: ego_pubkey
            .ok_or_else(|| CliError::Usage("run-live requires --ego-pubkey <hex64>".to_owned()))?,
        buf: buf.ok_or_else(|| CliError::Usage("run-live requires --buf <task.buf>".to_owned()))?,
        workspace: workspace
            .ok_or_else(|| CliError::Usage("run-live requires --workspace <dir>".to_owned()))?,
        ticket: ticket.ok_or_else(|| {
            CliError::Usage("run-live requires --ticket <approval-ticket>".to_owned())
        })?,
    })
}

/// 取第 1 个位置参数（索引 1）。缺失或多余 → Usage。
fn require_positional(args: &[String], usage: &str) -> Result<String, CliError> {
    match (args.get(1), args.get(2)) {
        (Some(p), None) => Ok(p.clone()),
        (None, _) => Err(CliError::Usage(format!("missing argument: {usage}"))),
        (Some(_), Some(extra)) => Err(CliError::Usage(format!(
            "unexpected argument: {extra} ({usage})"
        ))),
    }
}

/// 解析 `pack` 的 flag：`--entry <name>`、`--section <id>:<kind>=<path>`（可多次）、`-o <out>`。
fn parse_pack(rest: &[String]) -> Result<Command, CliError> {
    let mut entry: Option<String> = None;
    let mut out: Option<String> = None;
    let mut sections: Vec<PackSpec> = Vec::new();

    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--entry" => {
                let v = next_value(rest, i, "--entry <name>")?;
                entry = Some(v);
                i += 2;
            }
            "-o" | "--out" => {
                let v = next_value(rest, i, "-o <out>")?;
                out = Some(v);
                i += 2;
            }
            "--section" => {
                let v = next_value(rest, i, "--section <id>:<kind>=<path>")?;
                sections.push(parse_pack_spec(&v)?);
                i += 2;
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected pack argument: {other}"
                )));
            }
        }
    }

    let entry = entry.ok_or_else(|| CliError::Usage("pack requires --entry <name>".to_owned()))?;
    let out = out.ok_or_else(|| CliError::Usage("pack requires -o <out>".to_owned()))?;
    if sections.is_empty() {
        return Err(CliError::Usage(
            "pack requires at least one --section <id>:<kind>=<path>".to_owned(),
        ));
    }
    Ok(Command::Pack {
        entry,
        sections,
        out,
    })
}

/// 取 `flag` 后紧跟的值（索引 i+1）。缺失 → Usage。
fn next_value(rest: &[String], i: usize, usage: &str) -> Result<String, CliError> {
    rest.get(i + 1)
        .cloned()
        .ok_or_else(|| CliError::Usage(format!("missing value for {usage}")))
}

/// 解析 `<id>:<kind>=<path>`。
fn parse_pack_spec(spec: &str) -> Result<PackSpec, CliError> {
    // 先按 '=' 分出 path，再按 ':' 分出 id 与 kind。
    let (id_kind, path) = spec
        .split_once('=')
        .ok_or_else(|| CliError::Usage(format!("invalid --section (missing '='): {spec}")))?;
    let (id, kind) = id_kind
        .split_once(':')
        .ok_or_else(|| CliError::Usage(format!("invalid --section (missing ':'): {spec}")))?;
    if id.is_empty() {
        return Err(CliError::Usage(format!(
            "invalid --section (empty id): {spec}"
        )));
    }
    if path.is_empty() {
        return Err(CliError::Usage(format!(
            "invalid --section (empty path): {spec}"
        )));
    }
    if !VALID_KINDS.contains(&kind) {
        return Err(CliError::Usage(format!(
            "invalid --section kind '{kind}' (expected one of code|data|asset)"
        )));
    }
    Ok(PackSpec {
        id: id.to_owned(),
        kind: kind.to_owned(),
        path: path.to_owned(),
    })
}

/// 解析 `profile [--json] [--state-dir <dir>]`。
fn parse_profile(rest: &[String]) -> Result<Command, CliError> {
    let mut json = false;
    let mut state_dir = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--json" => {
                json = true;
                i += 1;
            }
            "--state-dir" => {
                state_dir = Some(next_value(rest, i, "--state-dir <dir>")?);
                i += 2;
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected profile argument: {other}"
                )));
            }
        }
    }
    Ok(Command::Profile { json, state_dir })
}

/// 解析 `wallet <init|show|derive-ifay|derive-cofay|dynamic-code|exchange-grant> ...`。
#[allow(clippy::too_many_lines)]
fn parse_wallet(rest: &[String]) -> Result<Command, CliError> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(CliError::Usage(
            "wallet requires subcommand: init|show|derive-ifay|derive-cofay|dynamic-code|exchange-grant"
                .to_owned(),
        ));
    };
    let tail = &rest[1..];
    match sub {
        "init" => {
            let mut label = "Primary Human Vault".to_owned();
            let mut seed_phrase = None;
            let mut force = false;
            let mut state_dir = None;
            let mut json = false;
            let mut i = 0;
            while i < tail.len() {
                match tail[i].as_str() {
                    "--name" | "--label" => {
                        label = next_value(tail, i, "--name <label>")?;
                        i += 2;
                    }
                    "--seed-phrase" => {
                        seed_phrase = Some(next_value(tail, i, "--seed-phrase <phrase>")?);
                        i += 2;
                    }
                    "--force" => {
                        force = true;
                        i += 1;
                    }
                    "--state-dir" => {
                        state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                        i += 2;
                    }
                    "--json" => {
                        json = true;
                        i += 1;
                    }
                    other => {
                        return Err(CliError::Usage(format!(
                            "unexpected wallet init argument: {other}"
                        )));
                    }
                }
            }
            Ok(Command::Wallet {
                subcommand: WalletSubcommand::Init {
                    label,
                    seed_phrase,
                    force,
                    state_dir,
                    json,
                },
            })
        }
        "show" => {
            let mut state_dir = None;
            let mut json = false;
            let mut i = 0;
            while i < tail.len() {
                match tail[i].as_str() {
                    "--state-dir" => {
                        state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                        i += 2;
                    }
                    "--json" => {
                        json = true;
                        i += 1;
                    }
                    other => {
                        return Err(CliError::Usage(format!(
                            "unexpected wallet show argument: {other}"
                        )));
                    }
                }
            }
            Ok(Command::Wallet {
                subcommand: WalletSubcommand::Show { state_dir, json },
            })
        }
        "derive-ifay" => {
            let mut name = None;
            let mut state_dir = None;
            let mut json = false;
            let mut i = 0;
            while i < tail.len() {
                match tail[i].as_str() {
                    "--name" => {
                        name = Some(next_value(tail, i, "--name <name>")?);
                        i += 2;
                    }
                    "--state-dir" => {
                        state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                        i += 2;
                    }
                    "--json" => {
                        json = true;
                        i += 1;
                    }
                    other => {
                        return Err(CliError::Usage(format!(
                            "unexpected wallet derive-ifay argument: {other}"
                        )));
                    }
                }
            }
            Ok(Command::Wallet {
                subcommand: WalletSubcommand::DeriveIfay {
                    name: name.ok_or_else(|| {
                        CliError::Usage("wallet derive-ifay requires --name <name>".to_owned())
                    })?,
                    state_dir,
                    json,
                },
            })
        }
        "derive-cofay" => {
            let mut name = None;
            let mut role = None;
            let mut scope = None;
            let mut state_dir = None;
            let mut json = false;
            let mut i = 0;
            while i < tail.len() {
                match tail[i].as_str() {
                    "--name" => {
                        name = Some(next_value(tail, i, "--name <name>")?);
                        i += 2;
                    }
                    "--role" => {
                        role = Some(next_value(tail, i, "--role <role-title>")?);
                        i += 2;
                    }
                    "--scope" => {
                        scope = Some(next_value(tail, i, "--scope <scope>")?);
                        i += 2;
                    }
                    "--state-dir" => {
                        state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                        i += 2;
                    }
                    "--json" => {
                        json = true;
                        i += 1;
                    }
                    other => {
                        return Err(CliError::Usage(format!(
                            "unexpected wallet derive-cofay argument: {other}"
                        )));
                    }
                }
            }
            Ok(Command::Wallet {
                subcommand: WalletSubcommand::DeriveCofay {
                    name: name.ok_or_else(|| {
                        CliError::Usage("wallet derive-cofay requires --name <name>".to_owned())
                    })?,
                    role,
                    scope,
                    state_dir,
                    json,
                },
            })
        }
        "dynamic-code" => {
            let mut purpose = "verify_with_fayid".to_owned();
            let mut ttl_secs = 300u64;
            let mut state_dir = None;
            let mut json = false;
            let mut i = 0;
            while i < tail.len() {
                match tail[i].as_str() {
                    "--purpose" => {
                        purpose = next_value(tail, i, "--purpose <purpose>")?;
                        i += 2;
                    }
                    "--ttl-secs" => {
                        let raw = next_value(tail, i, "--ttl-secs <secs>")?;
                        ttl_secs = raw.parse::<u64>().map_err(|_| {
                            CliError::Usage(format!("invalid --ttl-secs integer: {raw}"))
                        })?;
                        i += 2;
                    }
                    "--state-dir" => {
                        state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                        i += 2;
                    }
                    "--json" => {
                        json = true;
                        i += 1;
                    }
                    other => {
                        return Err(CliError::Usage(format!(
                            "unexpected wallet dynamic-code argument: {other}"
                        )));
                    }
                }
            }
            Ok(Command::Wallet {
                subcommand: WalletSubcommand::DynamicCode {
                    purpose,
                    ttl_secs,
                    state_dir,
                    json,
                },
            })
        }
        "exchange-grant" => parse_wallet_exchange(tail),
        other => Err(CliError::Usage(format!(
            "unknown wallet subcommand: {other}"
        ))),
    }
}

fn parse_wallet_exchange(tail: &[String]) -> Result<Command, CliError> {
    let mut provider = None;
    let mut secret = None;
    let mut fay_id = None;
    let mut scopes: Vec<String> = Vec::new();
    let mut ttl_secs = 3600u64;
    let mut state_dir = None;
    let mut json = false;
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--provider" => {
                provider = Some(next_value(tail, i, "--provider <provider>")?);
                i += 2;
            }
            "--secret" => {
                secret = Some(next_value(tail, i, "--secret <legacy-secret>")?);
                i += 2;
            }
            "--fay-id" => {
                fay_id = Some(next_value(tail, i, "--fay-id <fay-id>")?);
                i += 2;
            }
            "--scopes" => {
                let raw = next_value(tail, i, "--scopes <csv>")?;
                scopes = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                i += 2;
            }
            "--ttl-secs" => {
                let raw = next_value(tail, i, "--ttl-secs <secs>")?;
                ttl_secs = raw
                    .parse::<u64>()
                    .map_err(|_| CliError::Usage(format!("invalid --ttl-secs integer: {raw}")))?;
                i += 2;
            }
            "--state-dir" => {
                state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected wallet exchange-grant argument: {other}"
                )));
            }
        }
    }
    Ok(Command::Wallet {
        subcommand: WalletSubcommand::ExchangeGrant {
            provider: provider.ok_or_else(|| {
                CliError::Usage("wallet exchange-grant requires --provider <provider>".to_owned())
            })?,
            secret: secret.ok_or_else(|| {
                CliError::Usage("wallet exchange-grant requires --secret <secret>".to_owned())
            })?,
            fay_id,
            scopes,
            ttl_secs,
            state_dir,
            json,
        },
    })
}

/// 解析 `soul create --kind ifay|cofay --name <name> --out <path.buf> [--capabilities <csv>] [--state-dir <dir>] [--json]`。
fn parse_soul(rest: &[String]) -> Result<Command, CliError> {
    match rest.first().map(String::as_str) {
        Some("create") => {}
        Some(other) => {
            return Err(CliError::Usage(format!(
                "unknown soul subcommand: {other} (expected 'create')"
            )));
        }
        None => {
            return Err(CliError::Usage(
                "soul requires subcommand: soul create --kind ifay|cofay --name <name> --out <path.buf>"
                    .to_owned(),
            ));
        }
    }
    let tail = &rest[1..];
    let mut kind = None;
    let mut name = None;
    let mut out = None;
    let mut capabilities = None;
    let mut state_dir = None;
    let mut json = false;
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--kind" => {
                let k = next_value(tail, i, "--kind ifay|cofay")?;
                if k != "ifay" && k != "cofay" {
                    return Err(CliError::Usage(format!(
                        "invalid --kind '{k}' (expected 'ifay' or 'cofay')"
                    )));
                }
                kind = Some(k);
                i += 2;
            }
            "--name" => {
                name = Some(next_value(tail, i, "--name <name>")?);
                i += 2;
            }
            "--out" | "-o" => {
                out = Some(next_value(tail, i, "--out <path.buf>")?);
                i += 2;
            }
            "--capabilities" => {
                let raw = next_value(tail, i, "--capabilities <csv>")?;
                let parsed: Vec<String> = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                capabilities = Some(parsed);
                i += 2;
            }
            "--state-dir" => {
                state_dir = Some(next_value(tail, i, "--state-dir <dir>")?);
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected soul create argument: {other}"
                )));
            }
        }
    }
    Ok(Command::SoulCreate {
        kind: kind
            .ok_or_else(|| CliError::Usage("soul create requires --kind ifay|cofay".to_owned()))?,
        name: name
            .ok_or_else(|| CliError::Usage("soul create requires --name <name>".to_owned()))?,
        out: out
            .ok_or_else(|| CliError::Usage("soul create requires --out <path.buf>".to_owned()))?,
        capabilities,
        state_dir,
        json,
    })
}

/// 解析 `up [--addr 127.0.0.1:7979] [--state-dir <dir>] [--gmchain-endpoint <http://127.0.0.1:8546>]`。
fn parse_up(rest: &[String]) -> Result<Command, CliError> {
    let mut addr = "127.0.0.1:7979".to_owned();
    let mut state_dir = None;
    let mut gmchain_endpoint = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--addr" => {
                addr = next_value(rest, i, "--addr <host:port>")?;
                i += 2;
            }
            "--state-dir" => {
                state_dir = Some(next_value(rest, i, "--state-dir <dir>")?);
                i += 2;
            }
            "--gmchain-endpoint" => {
                let raw = next_value(rest, i, "--gmchain-endpoint <http://127.0.0.1:8546>")?;
                let normalized = crate::merit::normalize_loopback_gmchain_endpoint(&raw)?;
                gmchain_endpoint = Some(normalized);
                i += 2;
            }
            other => {
                return Err(CliError::Usage(format!("unexpected up argument: {other}")));
            }
        }
    }
    Ok(Command::Up {
        addr,
        state_dir,
        gmchain_endpoint,
    })
}

/// 解析 `chat --fay <fay_id> --message <text> [--state-dir <dir>] [--json]`。
fn parse_chat(rest: &[String]) -> Result<Command, CliError> {
    let mut fay_id = None;
    let mut message = None;
    let mut state_dir = None;
    let mut json = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--fay" => {
                fay_id = Some(next_value(rest, i, "--fay <fay_id>")?);
                i += 2;
            }
            "--message" | "-m" => {
                message = Some(next_value(rest, i, "--message <text>")?);
                i += 2;
            }
            "--state-dir" => {
                state_dir = Some(next_value(rest, i, "--state-dir <dir>")?);
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unexpected chat argument: {other}"
                )));
            }
        }
    }
    Ok(Command::Chat {
        fay_id: fay_id.ok_or_else(|| CliError::Usage("chat requires --fay <fay_id>".to_owned()))?,
        message: message
            .ok_or_else(|| CliError::Usage("chat requires --message <text>".to_owned()))?,
        state_dir,
        json,
    })
}

/// 用法帮助文本（纯）。`topic` 为某子命令名时给出该命令的用法。
#[must_use]
pub fn usage_text(topic: Option<&str>) -> String {
    match topic {
        Some("up" | "serve") => "fayger up [--addr 127.0.0.1:7979] [--state-dir <dir>] [--gmchain-endpoint <http://127.0.0.1:8546>]\n    启动 faygerd 三位一体后台引擎与内嵌本地可视化控制台（FayGer Dashboard，联动 GMChain/MeriToken）。".to_owned(),
        Some("profile") => "fayger profile [--json] [--state-dir <dir>]\n    输出标准化设备运行时环境描述文件（fayger-device-profile/1.0）。".to_owned(),
        Some("wallet") => "fayger wallet <init|show|derive-ifay|derive-cofay|dynamic-code|exchange-grant> [--state-dir <dir>] [--json]\n    管理内置 FayID 助记词保险箱、不可关联动态码与 Auth Exchange 授权票据。".to_owned(),
        Some("soul") => "fayger soul create --kind ifay|cofay --name <name> --out <path.buf> [--state-dir <dir>] [--json]\n    一键创建并密封可运行的 iFay 数字灵魂运行包或 coFay 公共席位包（.buf）。".to_owned(),
        Some("chat") => "fayger chat --fay <fay_id> --message <text> [--state-dir <dir>] [--json]\n    在终端与已加载分身对话，展示 ICP (.ctx) 命令行结构化文本降级渲染。".to_owned(),
        Some("run") => "fayger run <file.buf>\n    加载并运行一个 BuF，打印运行报告。".to_owned(),
        Some("bridge") => {
            "fayger bridge <file.buf>\n    加载一个 BuF 并经真实适配层在当前宿主上运行，打印产生的 HostSyscall。".to_owned()
        }
        Some("inspect") => {
            "fayger inspect <file.buf>\n    加载并校验一个 BuF，打印 Manifest 摘要。".to_owned()
        }
        Some("assemble") => {
            "fayger assemble <faymanifest.json> [--registry-index <index.json>] [--offline-cache <cache.json>] [--trust-anchors <anchors.json>]\n    解析并装配一个 FayManifest：校验 schema、自动补充基础设施、判定解析阶段，并按静态已签名注册表索引（或离线缓存/占位）解析依赖。".to_owned()
        }
        Some("conformance") => {
            "fayger conformance [--json]\n    运行 Runtime_Interface 语言无关黑盒一致性测试套件并输出合规报告。".to_owned()
        }
        Some("evaluate") => "fayger evaluate --manifest <faymanifest.json> --driver <driver.json> --resource <resource-id> --operation <operation-id>\n    以本地演示策略评估一个 CAP 操作，输出人类摘要与机器可读审计记录；不是生产 Faying 授权。".to_owned(),
        Some("run-live") => "fayger run-live --manifest <faymanifest.json> --request <cap-request.json> --anchors <trust-anchors.json> --ego-package <dir> --ego-pubkey <hex64> --buf <task.buf> --workspace <dir> --ticket <approval-ticket>\n    在生产级 CAP 门控、已签名 Ego_Package 双层验签与受控工作区约束下运行活体 iFay 任务并输出防篡改哈希链审计。".to_owned(),
        Some("pack") => "fayger pack --entry <name> --section <id>:<kind>=<path> [...] -o <out>\n    把若干 section 文件封装为一个可加载的 BuF。kind ∈ {code,data,asset}。".to_owned(),
        Some("info") => "fayger info\n    探测宿主环境并打印选中的适配器与授予能力。".to_owned(),
        Some("demo") => "fayger demo [drone|live-ifay]\n    无参数时运行 BuF 安装自检；drone 运行无人机模拟演示；live-ifay 运行受控活体 iFay 端到端闭环。".to_owned(),
        Some("version") => "fayger version\n    打印运行基座版本与 Runtime_Interface 版本。".to_owned(),
        _ => TOP_LEVEL_USAGE.to_owned(),
    }
}

/// 顶层用法。
const TOP_LEVEL_USAGE: &str = "\
fayger - Fay 多平台虚拟运行环境（三位一体运行基座：faygerd + CLI + Dashboard）

用法：
    fayger <命令> [参数]

命令：
    up [--addr ...]      启动 faygerd 守护引擎与内嵌 FayGer Dashboard (http://127.0.0.1:7979)
    profile [--json]     输出标准化设备运行时环境描述文件 (fayger-device-profile/1.0)
    wallet <子命令>      管理内置 FayID 身份保险箱、动态码与 Auth Exchange 凭据兑换
    soul create ...      一键创建可运行的 iFay 数字灵魂包或 coFay 公共席位包 (.buf)
    chat ...             在终端与已加载 Fay 进行 ICP (.ctx) 对话并渲染降级文本
    run <file.buf>       加载并运行一个 BuF
    bridge <file.buf>    加载并经适配层在当前宿主上运行一个 BuF（打印 HostSyscall）
    inspect <file.buf>   加载并校验一个 BuF，打印 Manifest 摘要
    assemble <file.json> 解析并装配一个 FayManifest（支持已签名注册表索引与离线缓存）
    conformance [--json] 运行 Runtime_Interface 语言无关黑盒一致性测试套件
    evaluate ...         评估一个本地 CAP 演示操作并输出审计记录
    run-live ...         运行生产级 CAP + 已签名 Ego_Package 的受控活体 iFay 任务
    pack ...             把若干 section 文件封装为一个可加载的 BuF
    info                 探测宿主环境并打印选中的适配器与授予能力
    demo [drone|live-ifay] 运行安装自检、无人机模拟或受控活体 iFay 闭环
    version              打印版本信息
    help [命令]          打印用法帮助

示例：
    fayger up
    fayger profile --json
    fayger wallet init
    fayger soul create --kind ifay --name alice --out alice.buf
    fayger inspect alice.buf
    fayger run alice.buf
    fayger demo live-ifay";

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{parse_args, usage_text, Command, PackSpec};
    use crate::error::CliError;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn empty_args_is_help() {
        assert_eq!(parse_args(&[]).expect("ok"), Command::Help { topic: None });
    }

    #[test]
    fn help_aliases() {
        for a in ["help", "--help", "-h"] {
            assert!(matches!(
                parse_args(&argv(&[a])).expect("ok"),
                Command::Help { .. }
            ));
        }
        assert_eq!(
            parse_args(&argv(&["help", "run"])).expect("ok"),
            Command::Help {
                topic: Some("run".to_owned())
            }
        );
    }

    #[test]
    fn version_aliases() {
        for a in ["version", "--version", "-V"] {
            assert_eq!(parse_args(&argv(&[a])).expect("ok"), Command::Version);
        }
    }

    #[test]
    fn info_and_demo() {
        assert_eq!(parse_args(&argv(&["info"])).expect("ok"), Command::Info);
        assert_eq!(parse_args(&argv(&["demo"])).expect("ok"), Command::Demo);
        assert_eq!(
            parse_args(&argv(&["demo", "drone"])).expect("ok"),
            Command::DemoDrone
        );
    }

    #[test]
    fn inspect_and_run_take_path() {
        assert_eq!(
            parse_args(&argv(&["inspect", "a.buf"])).expect("ok"),
            Command::Inspect {
                path: "a.buf".to_owned()
            }
        );
        assert_eq!(
            parse_args(&argv(&["run", "a.buf"])).expect("ok"),
            Command::Run {
                path: "a.buf".to_owned()
            }
        );
    }

    #[test]
    fn bridge_takes_path_and_missing_is_usage() {
        assert_eq!(
            parse_args(&argv(&["bridge", "a.buf"])).expect("ok"),
            Command::Bridge {
                path: "a.buf".to_owned()
            }
        );
        assert!(matches!(
            parse_args(&argv(&["bridge"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn assemble_takes_path_and_missing_is_usage() {
        assert_eq!(
            parse_args(&argv(&["assemble", "m.json"])).expect("ok"),
            Command::Assemble {
                path: "m.json".to_owned(),
                registry_index: None,
                offline_cache: None,
                trust_anchors: None,
            }
        );
        assert_eq!(
            parse_args(&argv(&[
                "assemble",
                "m.json",
                "--registry-index",
                "idx.json",
                "--trust-anchors",
                "anchors.json"
            ]))
            .expect("ok"),
            Command::Assemble {
                path: "m.json".to_owned(),
                registry_index: Some("idx.json".to_owned()),
                offline_cache: None,
                trust_anchors: Some("anchors.json".to_owned()),
            }
        );
        assert!(matches!(
            parse_args(&argv(&["assemble"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse_args(&argv(&[
                "assemble",
                "m.json",
                "--registry-index",
                "idx.json"
            ])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn conformance_parses_default_and_json_flag() {
        assert_eq!(
            parse_args(&argv(&["conformance"])).expect("ok"),
            Command::Conformance { json: false }
        );
        assert_eq!(
            parse_args(&argv(&["conformance", "--json"])).expect("ok"),
            Command::Conformance { json: true }
        );
        assert!(matches!(
            parse_args(&argv(&["conformance", "--bogus"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn evaluate_requires_all_named_arguments() {
        assert_eq!(
            parse_args(&argv(&[
                "evaluate",
                "--manifest",
                "drone.faymanifest.json",
                "--driver",
                "dji.driver.json",
                "--resource",
                "flight_controller",
                "--operation",
                "read_telemetry",
            ]))
            .expect("valid evaluate args"),
            Command::Evaluate {
                manifest: "drone.faymanifest.json".to_owned(),
                driver: "dji.driver.json".to_owned(),
                resource: "flight_controller".to_owned(),
                operation: "read_telemetry".to_owned(),
            }
        );
        assert!(matches!(
            parse_args(&argv(&[
                "evaluate",
                "--manifest",
                "drone.faymanifest.json",
                "--driver",
                "dji.driver.json",
            ])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn inspect_missing_path_is_usage() {
        assert!(matches!(
            parse_args(&argv(&["inspect"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn run_extra_arg_is_usage() {
        assert!(matches!(
            parse_args(&argv(&["run", "a.buf", "extra"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn unknown_command_is_usage() {
        assert!(matches!(
            parse_args(&argv(&["frobnicate"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn pack_full_spec() {
        let cmd = parse_args(&argv(&[
            "pack",
            "--entry",
            "main",
            "--section",
            "code:code=a.bin",
            "--section",
            "res:asset=b.png",
            "-o",
            "app.buf",
        ]))
        .expect("ok");
        assert_eq!(
            cmd,
            Command::Pack {
                entry: "main".to_owned(),
                sections: vec![
                    PackSpec {
                        id: "code".to_owned(),
                        kind: "code".to_owned(),
                        path: "a.bin".to_owned()
                    },
                    PackSpec {
                        id: "res".to_owned(),
                        kind: "asset".to_owned(),
                        path: "b.png".to_owned()
                    },
                ],
                out: "app.buf".to_owned(),
            }
        );
    }

    #[test]
    fn pack_missing_entry_is_usage() {
        assert!(matches!(
            parse_args(&argv(&["pack", "--section", "c:code=a", "-o", "x"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn pack_missing_out_is_usage() {
        assert!(matches!(
            parse_args(&argv(&["pack", "--entry", "m", "--section", "c:code=a"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn pack_no_section_is_usage() {
        assert!(matches!(
            parse_args(&argv(&["pack", "--entry", "m", "-o", "x"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn pack_bad_kind_is_usage() {
        assert!(matches!(
            parse_args(&argv(&[
                "pack",
                "--entry",
                "m",
                "--section",
                "c:weird=a",
                "-o",
                "x"
            ])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn pack_malformed_section_is_usage() {
        for bad in ["noequals", "noColon=path", ":code=path", "id:code="] {
            assert!(
                matches!(
                    parse_args(&argv(&[
                        "pack",
                        "--entry",
                        "m",
                        "--section",
                        bad,
                        "-o",
                        "x"
                    ])),
                    Err(CliError::Usage(_))
                ),
                "expected usage error for {bad}"
            );
        }
    }

    #[test]
    fn usage_text_has_topics() {
        assert!(usage_text(None).contains("fayger"));
        assert!(usage_text(Some("run")).contains("run"));
        assert!(usage_text(Some("pack")).contains("section"));
        assert!(usage_text(Some("up")).contains("--gmchain-endpoint"));
    }

    #[test]
    fn up_accepts_loopback_gmchain_endpoint_and_rejects_remote() {
        assert_eq!(
            parse_args(&argv(&[
                "up",
                "--addr",
                "127.0.0.1:7979",
                "--gmchain-endpoint",
                "http://localhost:8546/rpc"
            ]))
            .expect("valid loopback"),
            Command::Up {
                addr: "127.0.0.1:7979".to_owned(),
                state_dir: None,
                gmchain_endpoint: Some("http://localhost:8546".to_owned()),
            }
        );
        assert!(matches!(
            parse_args(&argv(&[
                "up",
                "--gmchain-endpoint",
                "http://example.com:8546"
            ])),
            Err(CliError::Usage(_))
        ));
    }
}
