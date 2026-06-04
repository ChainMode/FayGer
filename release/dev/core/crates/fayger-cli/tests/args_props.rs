//! Feature: fayger-m9, Property 1 + 4: 参数解析全域性 / 退出码映射确定性。
//!
//! Property 1：任意随机 argv，parse_args 要么返回 Command、要么返回 CliError::Usage，永不
//! panic；且以已知子命令名开头并带合法参数的 argv 恒被识别为对应 Command。
//! Property 4：exit_code_of 是确定性纯函数，Ok→0 / Usage→2 / Failure|Io→1。
//!
//! Validates: Requirements 1.2, 1.4, 1.5, 6.1, 6.2, 6.3, 6.4, 6.5, 8.1

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fayger_cli::CliError;
use fayger_cli::{exit_code_of, parse_args, Command, ExitCode};
use fayger_error::{ErrorCode, FaygerError, Layer};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

/// 随机 token：混入已知关键字、flag 与任意短字符串，制造多样 argv。
fn arb_token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("run".to_owned()),
        Just("inspect".to_owned()),
        Just("pack".to_owned()),
        Just("info".to_owned()),
        Just("demo".to_owned()),
        Just("version".to_owned()),
        Just("help".to_owned()),
        Just("--entry".to_owned()),
        Just("--section".to_owned()),
        Just("-o".to_owned()),
        Just("code:code=p".to_owned()),
        "[a-z]{0,6}",
    ]
}

fn arb_argv() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(arb_token(), 0..=8)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    })]

    /// Property 1：解析永不 panic，结果要么 Command 要么 Usage。
    #[test]
    fn parse_is_total(argv in arb_argv()) {
        match parse_args(&argv) {
            Ok(_cmd) => {}
            Err(CliError::Usage(_)) => {}
            Err(other) => prop_assert!(false, "parse must only yield Usage errors, got {other:?}"),
        }
    }

    /// Property 1（识别）：已知子命令带合法参数恒被识别。
    #[test]
    fn known_subcommands_recognized(path in "[a-z][a-z0-9_.]{0,8}") {
        prop_assert_eq!(parse_args(&["version".to_owned()]).unwrap(), Command::Version);
        prop_assert_eq!(parse_args(&["info".to_owned()]).unwrap(), Command::Info);
        prop_assert_eq!(parse_args(&["demo".to_owned()]).unwrap(), Command::Demo);
        prop_assert_eq!(
            parse_args(&["run".to_owned(), path.clone()]).unwrap(),
            Command::Run { path: path.clone() }
        );
        prop_assert_eq!(
            parse_args(&["inspect".to_owned(), path.clone()]).unwrap(),
            Command::Inspect { path }
        );
    }

    /// Property 4：exit_code_of 确定性三分支。
    #[test]
    fn exit_code_mapping_is_deterministic(which in 0u8..4) {
        let result: Result<(), CliError> = match which {
            0 => Ok(()),
            1 => Err(CliError::Usage("x".to_owned())),
            2 => Err(CliError::Failure(FaygerError::new(
                ErrorCode::LdrParseFail,
                Layer::Loader,
                "x",
            ))),
            _ => Err(CliError::Io {
                path: "p".to_owned(),
                detail: "d".to_owned(),
            }),
        };
        let expected = match which {
            0 => ExitCode::Success,
            1 => ExitCode::Usage,
            _ => ExitCode::Failure,
        };
        // 确定性：多次调用同结果。
        prop_assert_eq!(exit_code_of(&result), expected);
        prop_assert_eq!(exit_code_of(&result), expected);
    }
}
