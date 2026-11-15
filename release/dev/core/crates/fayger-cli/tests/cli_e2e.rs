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
fn drone_demo_runs_the_vertical_slice_with_one_command() {
    let (r, text) = run(&["demo", "drone"]);
    assert!(r.is_ok(), "drone demo should succeed: {r:?}");
    assert!(text.contains("drone-controller-ifay"));
    assert!(text.contains("ALLOW_SIMULATION"));
    assert!(text.contains("simulated_success"));
    assert!(text.contains("\"telemetry_status\": \"nominal\""));
    assert!(text.contains("\"production_verified\": false"));
    assert!(text.contains("\"fayger_version\": \"0.3.0-rc.1\""));
    assert!(text.contains("\"release_channel\": \"prerelease\""));
}

#[test]
fn live_ifay_demo_runs_authenticated_slice_with_one_command() {
    let (r, text) = run(&["demo", "live-ifay"]);
    assert!(r.is_ok(), "live-ifay demo should succeed: {r:?}");
    assert!(text.contains("workspace-guardian-ifay"));
    assert!(text.contains("production_verified: true"));
    assert!(text.contains("AllowControlledExecution"));
    assert!(text.contains("\"audit_chain_verified\": true"));
    assert!(text.contains("\"fayger_version\": \"0.3.0-rc.1\""));
}

#[test]
fn version_reports_versions() {
    let (r, text) = run(&["version"]);
    assert!(r.is_ok());
    assert!(text.contains("fayger 0.3.0-rc.1"));
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

#[test]
fn assemble_drone_example_reports_phase_2() {
    // 指向 fayger-faymanifest crate 内的蓝图无人机样例（相对路径，避免内嵌 JSON）。
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fayger-faymanifest/examples/drone-controller.faymanifest.json"
    );
    let (r, text) = run(&["assemble", path]);
    assert!(r.is_ok(), "assemble should succeed: {r:?}");
    assert_eq!(exit_code_of(&r), ExitCode::Success);
    assert!(text.contains("FayManifest assembly report"));
    assert!(text.contains("resolvedPhase: 2"));
    assert!(text.contains("@ego-models/drone-pilot-v1"));
    assert!(text.contains("stubbed"));
}

#[test]
fn assemble_missing_file_is_io_failure() {
    let (r, _) = run(&["assemble", "/no/such/manifest.json"]);
    assert!(matches!(r, Err(CliError::Io { .. })));
    assert_eq!(exit_code_of(&r), ExitCode::Failure);
}

#[test]
fn evaluate_drone_telemetry_emits_auditable_demo_record() {
    let faymanifest = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fayger-faymanifest/examples/drone-controller.faymanifest.json"
    );
    let driver = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fayger-cap/examples/drone.driver.json"
    );
    let (r, text) = run(&[
        "evaluate",
        "--manifest",
        faymanifest,
        "--driver",
        driver,
        "--resource",
        "flight_controller",
        "--operation",
        "read_telemetry",
    ]);

    assert!(r.is_ok(), "evaluate should succeed: {r:?}");
    assert!(text.contains("iFay command evaluation"));
    assert!(text.contains("ALLOW_SIMULATION"));
    assert!(text.contains("simulated_success"));
    assert!(text.contains("\"telemetry_status\": \"nominal\""));
    assert!(text.contains("\"production_verified\": false"));
    assert!(text.contains("CAP-D-1"));
}

#[test]
fn conformance_cli_e2e_verifies_reference_runtime() {
    let (r, text) = run(&["conformance"]);
    assert!(r.is_ok(), "conformance should succeed: {r:?}");
    assert_eq!(exit_code_of(&r), ExitCode::Success);
    assert!(text.contains("Fayger Runtime_Interface Conformance Report"));
    assert!(text.contains("CONFORMANT"));
    assert!(text.contains("7/7 required properties passed"));
}

#[test]
fn assemble_with_signed_static_registry_and_offline_cache() {
    use ed25519_dalek::SigningKey;
    use fayger_cli::assemble_manifest_with_registry;
    use fayger_faymanifest::{sign_registry_index_value, REGISTRY_INDEX_SCHEMA_VERSION};
    use serde_json::json;

    use std::fmt::Write as _;
    let sk = SigningKey::from_bytes(&[19u8; 32]);
    let pk_hex =
        sk.verifying_key()
            .as_bytes()
            .iter()
            .fold(String::with_capacity(64), |mut acc, b| {
                let _ = write!(acc, "{b:02x}");
                acc
            });
    let anchors_json = serde_json::to_string(&json!({
        "anchors": [{
            "key_id": "reg-root-1",
            "signer_fay_id": "org_ifay_registry",
            "public_key_hex": pk_hex,
            "trusted_index_signer": true
        }]
    }))
    .expect("anchors");
    let digest = "b".repeat(64);
    let mut index_val = json!({
        "schema_version": REGISTRY_INDEX_SCHEMA_VERSION,
        "generated_at": "2026-11-16T00:00:00Z",
        "modules": {"sensor": ["1.2.0"], "fayid": ["1.0.0"]},
        "protocols": {"cap_protocol": ["1.0.0"], "faying_protocol": ["1.1.0"]},
        "packages": {
            "@cap-drivers/gpio": {
                "releases": [{"version": "1.0.0", "artifact_sha256": digest}],
                "signer_fay_id": "org_ifay_drivers",
                "provenance_verified": true
            }
        }
    });
    sign_registry_index_value(&mut index_val, "reg-root-1", "org_ifay_registry", &sk)
        .expect("sign");
    let index_json = serde_json::to_string(&index_val).expect("index json");
    let manifest_json = r#"{
        "name": "iot-ifay", "version": "1.0.0", "description": "d", "vendor": "v",
        "modules": [{"id": "sensor", "version": "^1.0.0"}],
        "protocols": [{"id": "cap_protocol"}],
        "controlMode": "command",
        "drivers": [{"name": "gpio", "type": "device", "driverPackage": "@cap-drivers/gpio"}]
    }"#;

    let online_text =
        assemble_manifest_with_registry(manifest_json, Some(&index_json), None, &anchors_json)
            .expect("online assemble");
    assert!(online_text.contains("online_verified"));
    assert!(online_text.contains("[core-governed]"));
    assert!(online_text.contains("resolved -> 1.2.0"));

    let cached_text =
        assemble_manifest_with_registry(manifest_json, None, Some(&index_json), &anchors_json)
            .expect("cached assemble");
    assert!(cached_text.contains("cached_verified (offline degraded)"));
}
