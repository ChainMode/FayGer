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

#![doc(html_root_url = "https://docs.rs/fayger-cli/0.3.0")]

pub mod args;
pub mod brain;
pub mod commands;
pub mod daemon;
pub mod dashboard_html;
pub mod device_profile;
pub mod ego_package;
pub mod engine;
pub mod error;
pub mod fs_ops;
pub mod host_profile;
pub mod icp_ego;
pub mod live_ifay;
pub mod merit;
pub mod os_control;
pub mod soul;
pub mod wallet;

pub use args::{parse_args, usage_text, Command, PackSpec, WalletSubcommand};
pub use brain::{
    BrainTestReport, EgoBindingConfig, ExternalLlmConfig, FayBrainConfig, FayBrainView,
};
pub use commands::{
    assemble_manifest, assemble_manifest_with_registry, bridge_object, conformance_command,
    default_policy, demo_drone_command, demo_object, evaluate_manifest_command, inspect_object,
    pack_bytes, render_bridge_report, render_merit_standing_summary, run_object, RunReport,
};
pub use daemon::{
    serve_blocking, serve_blocking_full, serve_blocking_with_gmchain, spawn_daemon,
    spawn_daemon_with_gmchain, FaygerDaemonHandle, DEFAULT_DAEMON_ADDR,
};
pub use device_profile::{
    detect_device_runtime_profile, negotiate_embodiment_slices, profile_command,
    DeviceRuntimeProfile, EmbodimentPlan, HostResourcePoolConfig, DEVICE_PROFILE_SCHEMA_VERSION,
};
pub use ego_package::{
    jcs_canonicalize, sign_ego_manifest_value, verify_and_mount_ego_package, EgoIntegrity,
    EgoSignature, FidelityGatePolicy, FidelityReport, VerifiedEgoMount, EGO_SIGNING_DOMAIN,
};
pub use engine::{
    CapApprovalItem, DashboardSnapshot, FayGerEngine, FayInstanceView, FayingState,
    RequestedHostAction, TpCognitiveEntry, TpSharedSession,
};
pub use error::{exit_code_of, CliError, ExitCode};
pub use host_profile::{detect_host_environment, info_text};
pub use icp_ego::{
    render_ctx_terminal, run_ego_three_in_one, CtxAnnotation, CtxEnvelope, ICP_CTX_SCHEMA_VERSION,
};
pub use live_ifay::{
    demo_live_ifay_command, execute_live_ifay, verify_audit_chain, AuditChainBuilder,
    HashChainedAuditEntry, LiveIfayInputs, LiveIfayOutcome, TaskProgram, TaskStep,
    LIVE_IFAY_AUDIT_SCHEMA_VERSION, TASK_IR_SCHEMA_VERSION,
};
pub use merit::{
    MeriTokenPocketState, MeritAddInput, MeritAddReceipt, MeritConfigView, MeritConsumeInput,
    MeritConsumeReceipt, MeritCredentialPresentation, MeritLedgerEntry, MeritStandingView,
    DEFAULT_GMCHAIN_ENDPOINT, INITIAL_CONSUMABLE_MERIT_RAW, INITIAL_CUR_MERIT_RAW,
    MERIT_CREDENTIAL_SCHEMA_VERSION, MERIT_SCALE, MIN_MERIT_E_RAW,
};
pub use os_control::{
    probe_os_permissions, OsPermissionItem, OsPermissionRequestReceipt,
    ALLOWED_PROCESS_EXEC_COMMANDS,
};
pub use soul::{
    create_soul_buf_bytes, extract_soul_from_buf, SoulKind, SoulPackageMetadata,
    SOUL_PACKAGE_SCHEMA_VERSION,
};
pub use wallet::{
    derive_cofay, derive_ifay, exchange_auth_grant, init_wallet, issue_dynamic_code, show_wallet,
    AuthExchangeGrant, DerivedCofayRecord, DerivedIfayRecord, DynamicCodeTicket, RoleCharter,
    WalletInitReceipt, WalletPublicView,
};

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
#[allow(clippy::too_many_lines)]
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
        Command::Assemble {
            path,
            registry_index,
            offline_cache,
            trust_anchors,
        } => {
            let json = read_utf8_file(path, "FayManifest")?;
            let text = if let Some(anchors_path) = trust_anchors {
                let anchors_json = read_utf8_file(anchors_path, "registry trust anchors")?;
                let online_json = registry_index
                    .as_deref()
                    .map(|p| read_utf8_file(p, "registry index"))
                    .transpose()?;
                let cached_json = offline_cache
                    .as_deref()
                    .map(|p| read_utf8_file(p, "offline cache index"))
                    .transpose()?;
                assemble_manifest_with_registry(
                    &json,
                    online_json.as_deref(),
                    cached_json.as_deref(),
                    &anchors_json,
                )?
            } else {
                assemble_manifest(&json)?
            };
            writeln_out(out, &text)
        }
        Command::Conformance { json } => {
            let text = conformance_command(*json)?;
            writeln_out(out, &text)
        }
        Command::Evaluate {
            manifest,
            driver,
            resource,
            operation,
        } => {
            let faymanifest_bytes = fs_ops::read_file(manifest)?;
            let driver_bytes = fs_ops::read_file(driver)?;
            let faymanifest_json =
                String::from_utf8(faymanifest_bytes).map_err(|error| CliError::Io {
                    path: manifest.clone(),
                    detail: format!("FayManifest is not valid UTF-8: {error}"),
                })?;
            let driver_json = String::from_utf8(driver_bytes).map_err(|error| CliError::Io {
                path: driver.clone(),
                detail: format!("driver manifest is not valid UTF-8: {error}"),
            })?;
            let text =
                evaluate_manifest_command(&faymanifest_json, &driver_json, resource, operation)?;
            writeln_out(out, &text)
        }
        Command::Demo => {
            let report = run_object(demo_object())?;
            writeln_out(
                out,
                &format!("Running in-memory demo BuF...\n{}", report.render()),
            )
        }
        Command::DemoDrone => {
            let text = demo_drone_command()?;
            writeln_out(out, &text)
        }
        Command::DemoLiveIfay => {
            let text = demo_live_ifay_command()?;
            write_out(out, &text)
        }
        Command::RunLive {
            manifest,
            request,
            anchors,
            ego_package,
            ego_pubkey,
            buf,
            workspace,
            ticket,
        } => {
            let faymanifest_json = read_utf8_file(manifest, "FayManifest")?;
            let cap_request_json = read_utf8_file(request, "CAP request")?;
            let trust_anchors_json = read_utf8_file(anchors, "CAP trust anchors")?;
            let buf_bytes = fs_ops::read_file(buf)?;
            let outcome = execute_live_ifay(&LiveIfayInputs {
                faymanifest_json: &faymanifest_json,
                cap_request_json: &cap_request_json,
                trust_anchors_json: &trust_anchors_json,
                ego_package_dir: std::path::Path::new(ego_package),
                ego_public_key_hex: ego_pubkey,
                buf_bytes: &buf_bytes,
                workspace_root: std::path::Path::new(workspace),
                audit_approval_ticket: ticket,
                fidelity_policy: &FidelityGatePolicy::production_default(),
            })?;
            write_out(out, &outcome.render()?)
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
        Command::Profile { json, state_dir } => {
            let resolved = state_dir.as_deref().map(std::path::PathBuf::from);
            let text = profile_command(*json, resolved.as_deref())?;
            if *json {
                writeln_out(out, &text)
            } else {
                write_out(out, &text)
            }
        }
        Command::Wallet { subcommand } => dispatch_wallet(subcommand, out),
        Command::SoulCreate {
            kind,
            name,
            out: out_path,
            capabilities,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let soul_kind = SoulKind::parse(kind)?;
            let (bytes, meta) =
                create_soul_buf_bytes(&dir, soul_kind, name, capabilities.as_deref())?;
            if let Some(parent) = std::path::Path::new(out_path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).map_err(|err| CliError::Io {
                    path: parent.display().to_string(),
                    detail: err.to_string(),
                })?;
            }
            fs_ops::write_file(out_path, &bytes)?;
            let mut engine = FayGerEngine::open(&dir)?;
            let mounted = engine.mount_buf_bytes(&bytes, out_path, name)?;
            if *json {
                let payload = serde_json::json!({
                    "out_path": out_path,
                    "bytes": bytes.len(),
                    "fay_id": meta.fay_id,
                    "kind": meta.kind,
                    "faying_state": mounted.faying_state,
                    "requested_capabilities": mounted.requested_capabilities,
                    "effective_capabilities": mounted.effective_capabilities,
                });
                let pretty =
                    serde_json::to_string_pretty(&payload).map_err(|err| CliError::Io {
                        path: "<soul-json>".to_owned(),
                        detail: err.to_string(),
                    })?;
                writeln_out(out, &pretty)
            } else {
                writeln_out(
                    out,
                    &format!(
                        "Created runnable {} soul package\n  fay_id:        {}\n  name:          {}\n  out:           {out_path} ({} bytes)\n  faying_state:  {} (flip Faying switch in `fayger up` to enable host actions)\n  requested:     {}",
                        meta.kind.as_str(),
                        meta.fay_id,
                        meta.display_name,
                        bytes.len(),
                        mounted.faying_state.as_str(),
                        mounted.requested_capabilities.join(", ")
                    ),
                )
            }
        }
        Command::Up {
            addr,
            state_dir,
            gmchain_endpoint,
            open_browser,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            serve_blocking_full(addr, &dir, gmchain_endpoint.as_deref(), *open_browser, out)
        }
        Command::Chat {
            fay_id,
            message,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let mut engine = FayGerEngine::open(&dir)?;
            let turn = engine.chat_or_act(fay_id, message, None)?;
            if *json {
                let pretty = serde_json::to_string_pretty(&turn.response_ctx).map_err(|err| {
                    CliError::Io {
                        path: "<chat-json>".to_owned(),
                        detail: err.to_string(),
                    }
                })?;
                writeln_out(out, &pretty)
            } else {
                write_out(out, &turn.terminal_text)
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn dispatch_wallet(sub: &WalletSubcommand, out: &mut dyn Write) -> Result<(), CliError> {
    match sub {
        WalletSubcommand::Init {
            label,
            seed_phrase,
            force,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let receipt = init_wallet(&dir, label, seed_phrase.as_deref(), *force)?;
            if *json {
                write_json_pretty(out, &receipt)
            } else {
                writeln_out(
                    out,
                    &format!(
                        "Initialized FayID Wallet Vault\n  label:                 {}\n  human_id (masked):     {}\n  human_id_commitment:   {}\n  recovery_mnemonic_once: {}\n  (Save mnemonic offline; subsequent `wallet show` never prints it.)",
                        receipt.label,
                        receipt.human_id_masked,
                        receipt.human_id_commitment,
                        receipt.recovery_mnemonic_once
                    ),
                )
            }
        }
        WalletSubcommand::Show { state_dir, json } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let view = show_wallet(&dir)?;
            if *json {
                write_json_pretty(out, &view)
            } else {
                write_out(out, &view.render_text())
            }
        }
        WalletSubcommand::DeriveIfay {
            name,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let rec = derive_ifay(&dir, name)?;
            if *json {
                write_json_pretty(out, &rec)
            } else {
                writeln_out(
                    out,
                    &format!(
                        "Derived iFay identity\n  ifay_id:        {}\n  name:           {}\n  public_key_hex: {}\n  discovery_mode: {}",
                        rec.ifay_id, rec.name, rec.public_key_hex, rec.discovery_mode
                    ),
                )
            }
        }
        WalletSubcommand::DeriveCofay {
            name,
            role,
            scope,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let rec = derive_cofay(
                &dir,
                name,
                role.as_deref().unwrap_or(""),
                scope.as_deref().unwrap_or(""),
            )?;
            if *json {
                write_json_pretty(out, &rec)
            } else {
                writeln_out(
                    out,
                    &format!(
                        "Derived coFay seat identity\n  cofay_id:       {}\n  name:           {}\n  role_title:     {}\n  charter_schema: {}",
                        rec.cofay_id,
                        rec.name,
                        rec.role_charter.role_title,
                        rec.role_charter.schema_version
                    ),
                )
            }
        }
        WalletSubcommand::DynamicCode {
            purpose,
            ttl_secs,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let ticket = issue_dynamic_code(&dir, purpose, *ttl_secs)?;
            if *json {
                write_json_pretty(out, &ticket)
            } else {
                writeln_out(
                    out,
                    &format!(
                        "Issued unlinkable Dynamic Code\n  dynamic_code:    {}\n  purpose:         {}\n  expires_at_unix: {}",
                        ticket.dynamic_code, ticket.purpose, ticket.expires_at_unix
                    ),
                )
            }
        }
        WalletSubcommand::ExchangeGrant {
            provider,
            secret,
            fay_id,
            scopes,
            ttl_secs,
            state_dir,
            json,
        } => {
            let dir = resolve_state_dir(state_dir.as_deref());
            let grant = exchange_auth_grant(
                &dir,
                provider,
                secret,
                fay_id.as_deref(),
                scopes.clone(),
                *ttl_secs,
            )?;
            if *json {
                write_json_pretty(out, &grant)
            } else {
                writeln_out(
                    out,
                    &format!(
                        "Issued Auth Exchange Authorization Grant\n  grant_id:           {}\n  provider:           {}\n  scopes:             {}\n  secret_fingerprint: {}\n  expires_at_unix:    {}",
                        grant.grant_id,
                        grant.provider,
                        grant.scopes.join(", "),
                        grant.secret_fingerprint,
                        grant.expires_at_unix
                    ),
                )
            }
        }
    }
}

fn resolve_state_dir(explicit: Option<&str>) -> std::path::PathBuf {
    if let Some(p) = explicit {
        return std::path::PathBuf::from(p);
    }
    if let Ok(env_dir) = std::env::var("FAYGER_STATE_DIR") {
        if !env_dir.trim().is_empty() {
            return std::path::PathBuf::from(env_dir);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return std::path::PathBuf::from(home).join(".fayger").join("state");
        }
    }
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        if !user_profile.trim().is_empty() {
            return std::path::PathBuf::from(user_profile)
                .join(".fayger")
                .join("state");
        }
    }
    std::path::PathBuf::from(".fayger-state")
}

fn write_json_pretty<T: serde::Serialize>(out: &mut dyn Write, val: &T) -> Result<(), CliError> {
    let pretty = serde_json::to_string_pretty(val).map_err(|err| CliError::Io {
        path: "<json-stdout>".to_owned(),
        detail: err.to_string(),
    })?;
    writeln_out(out, &pretty)
}

/// 读取 UTF-8 文本文件（在薄壳边界上转换编码错误）。
fn read_utf8_file(path: &str, label: &str) -> Result<String, CliError> {
    let bytes = fs_ops::read_file(path)?;
    String::from_utf8(bytes).map_err(|err| CliError::Io {
        path: path.to_owned(),
        detail: format!("{label} is not valid UTF-8: {err}"),
    })
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

    #[test]
    fn conformance_runs_in_text_and_json_modes() {
        let (r_txt, txt) = run_to_string(&["conformance"]);
        assert!(r_txt.is_ok());
        assert!(txt.contains("CONFORMANT"));
        assert!(txt.contains("7/7 required properties passed"));

        let (r_json, js) = run_to_string(&["conformance", "--json"]);
        assert!(r_json.is_ok());
        assert!(js.contains("\"verdict\": \"conformant\""));
    }
}
