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
    /// 打印用法帮助。
    Help {
        /// 可选的子命令主题。
        topic: Option<String>,
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
        "demo" => Ok(Command::Demo),
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
        "pack" => parse_pack(&args[1..]),
        other => Err(CliError::Usage(format!("unknown command: {other}"))),
    }
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

/// 用法帮助文本（纯）。`topic` 为某子命令名时给出该命令的用法。
#[must_use]
pub fn usage_text(topic: Option<&str>) -> String {
    match topic {
        Some("run") => "fayger run <file.buf>\n    加载并运行一个 BuF，打印运行报告。".to_owned(),
        Some("bridge") => {
            "fayger bridge <file.buf>\n    加载一个 BuF 并经真实适配层在当前宿主上运行，打印产生的 HostSyscall。".to_owned()
        }
        Some("inspect") => {
            "fayger inspect <file.buf>\n    加载并校验一个 BuF，打印 Manifest 摘要。".to_owned()
        }
        Some("pack") => "fayger pack --entry <name> --section <id>:<kind>=<path> [...] -o <out>\n    把若干 section 文件封装为一个可加载的 BuF。kind ∈ {code,data,asset}。".to_owned(),
        Some("info") => "fayger info\n    探测宿主环境并打印选中的适配器与授予能力。".to_owned(),
        Some("demo") => "fayger demo\n    在内存中构造并运行一个示例 BuF（安装自检）。".to_owned(),
        Some("version") => "fayger version\n    打印运行基座版本与 Runtime_Interface 版本。".to_owned(),
        _ => TOP_LEVEL_USAGE.to_owned(),
    }
}

/// 顶层用法。
const TOP_LEVEL_USAGE: &str = "\
fayger - Fay 多平台虚拟运行环境（运行基座）

用法：
    fayger <命令> [参数]

命令：
    run <file.buf>       加载并运行一个 BuF
    bridge <file.buf>    加载并经适配层在当前宿主上运行一个 BuF（打印 HostSyscall）
    inspect <file.buf>   加载并校验一个 BuF，打印 Manifest 摘要
    pack ...             把若干 section 文件封装为一个可加载的 BuF
    info                 探测宿主环境并打印选中的适配器与授予能力
    demo                 在内存中构造并运行一个示例 BuF（安装自检）
    version              打印版本信息
    help [命令]          打印用法帮助

示例：
    fayger version
    fayger demo
    fayger inspect app.buf
    fayger run app.buf
    fayger pack --entry main --section code:code=main.bin -o app.buf";

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
    }
}
