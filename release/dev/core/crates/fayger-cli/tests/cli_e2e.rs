//! Feature: fayger-m9, 端到端：run_cli 各命令（纯内存，无 FS）。
//!
//! 覆盖 demo / version / help 成功路径与未知命令的用法错误退出码，以及 pack_bytes → load →
//! run_object 的纯内存往返。
//!
//! Validates: Requirements 8.2

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_cli::{
    default_policy, exit_code_of, pack_bytes, run_cli, run_object, CliError, ExitCode,
};
use fayger_loader::load;
use fayger_runtime::LifecycleState;

fn run(items: &[&str]) -> (Result<(), CliError>, String) {
    let argv: Vec<String> = items.iter().map(|s| (*s).to_owned()).collect();
    let mut out = Vec::new();
    let r = run_cli(&argv, &mut out);
    (r, String::from_utf8(out).expect("utf8"))
}

#[test]
fn demo_runs_to_terminated() {
    let (r, text) = run(&["demo"]);
    assert!(r.is_ok());
    assert_eq!(exit_code_of(&r), ExitCode::Success);
    assert!(text.contains("Terminated"));
    assert!(text.contains("dispatched:       2"));
}

#[test]
fn version_reports_versions() {
    let (r, text) = run(&["version"]);
    assert!(r.is_ok());
    assert!(text.contains("fayger 0.0.0"));
    assert!(text.contains("runtime-interface 1.0.0"));
}

#[test]
fn help_lists_commands() {
    let (r, text) = run(&["help"]);
    assert!(r.is_ok());
    assert!(text.contains("run"));
    assert!(text.contains("inspect"));
    assert!(text.contains("pack"));
}

#[test]
fn bogus_command_exits_two() {
    let (r, _) = run(&["totally-not-a-command"]);
    assert!(matches!(r, Err(CliError::Usage(_))));
    assert_eq!(exit_code_of(&r), ExitCode::Usage);
}

#[test]
fn pack_load_run_in_memory_round_trip() {
    let bytes = pack_bytes(
        "main",
        &[
            ("code".to_owned(), "code".to_owned(), b"hello-fay".to_vec()),
            ("data".to_owned(), "data".to_owned(), vec![1, 2, 3, 4]),
        ],
    )
    .expect("pack");
    let obj = load(&bytes, &default_policy()).expect("load");
    let report = run_object(obj).expect("run");
    assert_eq!(report.final_state, LifecycleState::Terminated);
    assert_eq!(report.dispatched, 2);
}
