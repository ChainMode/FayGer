//! Fayger 运行基座命令行（M9）。
//!
//! 这个 crate 把 M0–M8 的库能力封装为一个可安装、可执行的 `fayger` 命令行程序的核心。
//! 设计为**纯核心 + 薄壳**：参数解析、inspect/run/pack、宿主画像、退出码映射都是不触达
//! 文件系统/网络/时钟的纯函数（见 [`mod@args`] / [`mod@commands`] / [`mod@host_profile`] /
//! [`mod@error`]）；[`mod@fs_ops`] 是仅有的两个触达文件系统的薄封装；[`run_cli`] 把二者拼接。
//!
//! 二进制入口 `bin/fayger.rs` 只是 `argv → run_cli → process::exit(exit_code)` 的极薄壳。
//!
//! 详见 `release/dev/core/MILESTONES/M9-cli-distribution/design.md`。
//!
//! # Example
//!
//! ```
//! let mut out = Vec::new();
//! let args = vec!["demo".to_owned()];
//! fayger_cli::run_cli(&args, &mut out).expect("demo runs");
//! let text = String::from_utf8(out).expect("utf8");
//! assert!(text.contains("Terminated"));
//! ```

#![doc(html_root_url = "https://docs.rs/fayger-cli/0.0.0")]

pub mod args;
pub mod commands;
pub mod error;
pub mod fs_ops;
pub mod host_profile;

pub use args::{parse_args, usage_text, Command, PackSpec};
pub use commands::{
    bridge_object, default_policy, demo_object, inspect_object, pack_bytes, render_bridge_report,
    run_object, RunReport,
};
pub use error::{exit_code_of, CliError, ExitCode};
pub use host_profile::{detect_host_environment, info_text};

use std::io::Write;

/// Fayger 运行基座版本（来自 crate 版本）。
pub const FAYGER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 运行层提供的 Runtime_Interface 版本（与 CLI 默认策略一致）。
pub const RUNTIME_INTERFACE_VERSION: &str = "1.0.0";

/// 解析并执行一次 CLI 调用，把人类可读输出写入 `out`。
///
/// # Errors
/// 用法错误返回 [`CliError::Usage`]；文件 I/O 失败返回 [`CliError::Io`]；库层失败返回
/// [`CliError::Failure`]。调用方用 [`exit_code_of`] 把结果映射为退出码。
pub fn run_cli(args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let command = parse_args(args)?;
    dispatch(&command, out)
}

/// 按已解析命令执行。FS 读写在此与纯核心拼接。
fn dispatch(command: &Command, out: &mut dyn Write) -> Result<(), CliError> {
    match command {
        Command::Help { topic } => {
            writeln_out(out, &usage_text(topic.as_deref()))?;
            Ok(())
        }
        Command::Version => {
            writeln_out(
                out,
                &format!("fayger {FAYGER_VERSION}\nruntime-interface {RUNTIME_INTERFACE_VERSION}"),
            )?;
            Ok(())
        }
        Command::Info => {
            let text = info_text()?;
            write_out(out, &text)
        }
        Command::Inspect { path } => {
            let bytes = fs_ops::read_file(path)?;
            let obj = fayger_loader::load(&bytes, &default_policy())?;
            let text = inspect_object(&obj);
            write_out(out, &text)
        }
        Command::Run { path } => {
            let bytes = fs_ops::read_file(path)?;
            let obj = fayger_loader::load(&bytes, &default_policy())?;
            let report = run_object(obj)?;
            writeln_out(out, &report.render())
        }
        Command::Bridge { path } => {
            let bytes = fs_ops::read_file(path)?;
            let obj = fayger_loader::load(&bytes, &default_policy())?;
            let report = bridge_object(obj)?;
            write_out(out, &render_bridge_report(&report))
        }
        Command::Demo => {
            let report = run_object(demo_object())?;
            writeln_out(
                out,
                &format!("Running in-memory demo BuF...\n{}", report.render()),
            )
        }
        Command::Pack {
            entry,
            sections,
            out: out_path,
        } => {
            // 读取每个 section 文件，拼成 (id, kind, payload) 列表。
            let mut specs: Vec<(String, String, Vec<u8>)> = Vec::with_capacity(sections.len());
            for s in sections {
                let payload = fs_ops::read_file(&s.path)?;
                specs.push((s.id.clone(), s.kind.clone(), payload));
            }
            let bytes = pack_bytes(entry, &specs)?;
            fs_ops::write_file(out_path, &bytes)?;
            writeln_out(
                out,
                &format!(
                    "Packed {} section(s) into {out_path} ({} bytes)",
                    sections.len(),
                    bytes.len()
                ),
            )
        }
    }
}

/// 写出文本（不追加换行；调用方文本通常已含尾随换行）。
fn write_out(out: &mut dyn Write, text: &str) -> Result<(), CliError> {
    out.write_all(text.as_bytes()).map_err(|e| CliError::Io {
        path: "<stdout>".to_owned(),
        detail: e.to_string(),
    })
}

/// 写出文本并追加换行。
fn writeln_out(out: &mut dyn Write, text: &str) -> Result<(), CliError> {
    write_out(out, text)?;
    write_out(out, "\n")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{exit_code_of, run_cli, CliError, ExitCode};

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn run_to_string(items: &[&str]) -> (Result<(), CliError>, String) {
        let mut out = Vec::new();
        let r = run_cli(&argv(items), &mut out);
        (r, String::from_utf8(out).expect("utf8"))
    }

    #[test]
    fn version_prints_version() {
        let (r, text) = run_to_string(&["version"]);
        assert!(r.is_ok());
        assert!(text.contains("fayger"));
        assert!(text.contains("runtime-interface"));
    }

    #[test]
    fn help_prints_usage() {
        let (r, text) = run_to_string(&["help"]);
        assert!(r.is_ok());
        assert!(text.contains("用法"));
    }

    #[test]
    fn empty_args_prints_usage() {
        let (r, text) = run_to_string(&[]);
        assert!(r.is_ok());
        assert!(text.contains("fayger"));
    }

    #[test]
    fn demo_runs_and_terminates() {
        let (r, text) = run_to_string(&["demo"]);
        assert!(r.is_ok());
        assert!(text.contains("Terminated"));
        assert!(text.contains("dispatched"));
    }

    #[test]
    fn info_prints_host_or_fails_cleanly() {
        // 在测试平台（linux/macos/windows）应成功并打印 platform_id。
        let (r, text) = run_to_string(&["info"]);
        assert!(r.is_ok());
        assert!(text.contains("Host environment"));
    }

    #[test]
    fn unknown_command_is_usage_exit_two() {
        let (r, _) = run_to_string(&["bogus"]);
        assert!(matches!(r, Err(CliError::Usage(_))));
        assert_eq!(exit_code_of(&r), ExitCode::Usage);
    }

    #[test]
    fn inspect_missing_file_is_io_failure() {
        let (r, _) = run_to_string(&["inspect", "/no/such/file.buf"]);
        assert!(matches!(r, Err(CliError::Io { .. })));
        assert_eq!(exit_code_of(&r), ExitCode::Failure);
    }
}
