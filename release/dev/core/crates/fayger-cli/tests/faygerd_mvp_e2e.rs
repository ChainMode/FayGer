//! End-to-end integration tests for `FayGer` Three-in-One Engine (`faygerd` + CLI + Dashboard).
//!
//! Spins up `faygerd` on an ephemeral `127.0.0.1:0` port and exercises:
//! - `GET /` embedded offline-ready `FayGer Dashboard` HTML (5 tabs, zero CDN);
//! - `GET /api/v1/host/profile` (`fayger-device-profile/1.0`) & Host Resource Pool updates;
//! - `FayID` Wallet Vault (`init`, `show`, `derive-ifay`, `derive-cofay`, `dynamic-code`, `exchange-grant`);
//! - Multi-tenant `.buf` soul creation & mounting (`iFay` + `coFay`);
//! - Live `BuF ∩ HostPool ∩ PerFayPolicy` two-layer capability intersection;
//! - `ROGUE -> FAYING -> G0 EMERGENCY BRAKE -> ROGUE` lifecycle & heartbeat timeout;
//! - Built-in `CAP` Gatekeeper routine sandbox allow + high-criticality `ICP` `.ctx` approval hold/approve;
//! - Live `Ego` 3-in-1 service HTTP integration + standalone offline fallback;
//! - CLI subcommands (`profile`, `wallet`, `soul create`, `chat`) & `.ctx` terminal degradation;
//! - Local `TP` (`Telepathy`) Shared Cognitive Space between `iFay` and `coFay` + SHA-256 audit chain.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::too_many_lines
)]

use fayger_cli::{
    parse_args, run_cli, spawn_daemon, Command, DEVICE_PROFILE_SCHEMA_VERSION,
    ICP_CTX_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

fn fresh_state_dir(tag: &str) -> PathBuf {
    let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("faygerd-e2e-{tag}-{}-{seq}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir state_dir");
    dir
}

fn http_call(addr: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).expect("connect faygerd");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let payload = body.map_or_else(Vec::new, |v| serde_json::to_vec(v).expect("json body"));
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(req.as_bytes()).expect("write head");
    if !payload.is_empty() {
        stream.write_all(&payload).expect("write body");
    }
    stream.flush().expect("flush");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    let text = String::from_utf8(raw).expect("utf8 response");
    let (headers, resp_body) = text.split_once("\r\n\r\n").expect("http split");
    let status_line = headers.lines().next().expect("status line");
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .expect("status code token")
        .parse()
        .expect("u16 status");
    (code, resp_body.to_owned())
}

fn http_json(addr: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let (code, text) = http_call(addr, method, path, body);
    let val: Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("invalid JSON from {method} {path} (status {code}): {e}\n{text}")
    });
    (code, val)
}

fn run_cli_capture(items: &[&str]) -> String {
    let args: Vec<String> = items.iter().map(|s| (*s).to_owned()).collect();
    let mut out = Vec::new();
    run_cli(&args, &mut out).unwrap_or_else(|e| panic!("CLI failed for {items:?}: {e}"));
    String::from_utf8(out).expect("utf8 cli output")
}

#[test]
fn faygerd_full_mvp_closed_loop_over_ephemeral_tcp() {
    let state_dir = fresh_state_dir("full-loop");
    let cofay_buf_path = state_dir.join("external_project_seat.buf");

    // Pre-build a runnable coFay .buf via `fayger soul create` CLI
    let create_out = run_cli_capture(&[
        "soul",
        "create",
        "--kind",
        "cofay",
        "--name",
        "CoreReleaseChair",
        "--out",
        cofay_buf_path.to_str().unwrap(),
        "--state-dir",
        state_dir.to_str().unwrap(),
    ]);
    assert!(create_out.contains("cofay_"));
    assert!(create_out.contains("ROGUE"));

    let mut daemon = spawn_daemon("127.0.0.1:0", &state_dir).expect("spawn faygerd");
    let addr = daemon.local_addr();

    // 1. Embedded Dashboard HTML check (5 tabs, offline-ready, no external CDN)
    let (html_code, html) = http_call(addr, "GET", "/", None);
    assert_eq!(html_code, 200);
    assert!(html.contains("FayGer Local Runtime Console"));
    assert!(html.contains("设备环境与资源池"));
    assert!(html.contains("身份与凭据保险箱"));
    assert!(html.contains("G0 一键急刹"));
    assert!(html.contains("ICP 富交互对话与 CAP 审批"));
    assert!(html.contains("双 Fay 心灵感应协作间"));
    assert!(!html.contains("https://cdn."));

    // 2. Device Runtime Environment Profile (`fayger-device-profile/1.0`)
    let (prof_code, prof) = http_json(addr, "GET", "/api/v1/host/profile", None);
    assert_eq!(prof_code, 200);
    assert_eq!(prof["schema_version"], DEVICE_PROFILE_SCHEMA_VERSION);
    assert!(prof["host_id"].as_str().unwrap().starts_with("host_"));
    assert!(prof["cpu_cores"].as_u64().unwrap() >= 1);
    assert!(prof["memory_mb"].as_u64().unwrap() >= 512);
    assert_eq!(prof["ego_slot"]["mode"], "local_deterministic_fallback");

    // 3. Wallet show, dynamic code, and Auth Exchange (no plaintext Human ID or raw secret leaked)
    let (dync_code, dync) = http_json(
        addr,
        "POST",
        "/api/v1/wallet/dynamic-code",
        Some(&json!({"purpose": "platform_onboarding", "ttl_secs": 600})),
    );
    assert_eq!(dync_code, 201);
    assert!(dync["dynamic_code"].as_str().unwrap().starts_with("dync_"));

    let demo_cred = "<example-placeholder-credential-778899>";
    let (grant_code, grant) = http_json(
        addr,
        "POST",
        "/api/v1/wallet/exchange-grant",
        Some(&json!({
            "provider": "openai",
            "secret": demo_cred,
            "scopes": ["responses.create"],
            "ttl_secs": 1800
        })),
    );
    assert_eq!(grant_code, 201);
    assert!(grant["grant_id"].as_str().unwrap().starts_with("ag_"));

    let (w_code, wallet_view) = http_json(addr, "GET", "/api/v1/wallet", None);
    assert_eq!(w_code, 200);
    let wallet_str = serde_json::to_string(&wallet_view).unwrap();
    assert!(!wallet_str.contains(demo_cred));
    assert!(wallet_view["human_id_masked"]
        .as_str()
        .unwrap()
        .contains("..."));

    // 4. Create & mount a personal iFay via REST API (multi-tenant alongside the CLI-created coFay)
    let (ifay_code, ifay_view) = http_json(
        addr,
        "POST",
        "/api/v1/fays/create",
        Some(&json!({"kind": "ifay", "name": "AlicePersonalSoul"})),
    );
    assert_eq!(ifay_code, 201);
    let ifay_id = ifay_view["fay_id"].as_str().unwrap().to_owned();
    assert_eq!(ifay_view["faying_state"], "ROGUE");

    let (_, fays_list) = http_json(addr, "GET", "/api/v1/fays", None);
    let fays_arr = fays_list.as_array().unwrap();
    assert_eq!(fays_arr.len(), 2);
    let cofay_id = fays_arr.iter().find(|f| f["kind"] == "cofay").unwrap()["fay_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // 5. Two-layer capability intersection: BuF ∩ HostPool ∩ PerFayPolicy
    // Remove `media.camera` from Host Resource Pool -> effective_capabilities must drop `media.camera`
    let (pool_code, _) = http_json(
        addr,
        "PUT",
        "/api/v1/host/pool",
        Some(&json!({
            "granted_capabilities": ["fs.read", "fs.write", "process.exec", "net.http"]
        })),
    );
    assert_eq!(pool_code, 200);
    let (_, after_pool) = http_json(addr, "GET", &format!("/api/v1/fays/{ifay_id}"), None);
    let eff_after_pool: Vec<&str> = after_pool["effective_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(!eff_after_pool.contains(&"media.camera"));
    assert!(eff_after_pool.contains(&"process.exec"));

    // Shrink Per-Fay Policy to only `fs.read` and `fs.write` -> `process.exec` drops from intersection
    let (policy_status, after_policy) = http_json(
        addr,
        "PUT",
        &format!("/api/v1/fays/{ifay_id}/policy"),
        Some(&json!({
            "policy_capabilities": ["fs.read", "fs.write", "media.camera"]
        })),
    );
    assert_eq!(policy_status, 200);
    let eff_after_policy: Vec<&str> = after_policy["effective_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(eff_after_policy, vec!["fs.read", "fs.write"]);

    // 6. ROGUE state strictly blocks CAP external host operations
    let (_, rogue_attempt) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/write diary.txt blocked-in-rogue"})),
    );
    assert_eq!(
        rogue_attempt["response_ctx"]["schema_version"],
        ICP_CTX_SCHEMA_VERSION
    );
    assert!(rogue_attempt["terminal_text"]
        .as_str()
        .unwrap()
        .contains("DENY_ROGUE_FAY"));

    // 7. Flip Faying Activation Switch ON -> ROGUE -> FAYING
    let (fay_on_code, faying_view) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/faying"),
        Some(&json!({"grade": "G2", "ttl_secs": 600, "heartbeat_timeout_secs": 120})),
    );
    assert_eq!(fay_on_code, 200);
    assert_eq!(faying_view["faying_state"], "FAYING");

    // Routine sandbox write & read succeed
    let (_, write_ok) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/write notes/plan.txt ship-batch-1"})),
    );
    assert!(write_ok["terminal_text"]
        .as_str()
        .unwrap()
        .contains("ALLOW_CONTROLLED"));

    let (_, read_ok) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/read notes/plan.txt"})),
    );
    assert!(read_ok["terminal_text"]
        .as_str()
        .unwrap()
        .contains("ship-batch-1"));

    // Unpermitted `process.exec` (removed from Per-Fay policy earlier) is denied by intersection
    let (_, exec_denied) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/exec workspace.ls"})),
    );
    assert!(exec_denied["terminal_text"]
        .as_str()
        .unwrap()
        .contains("DENY_CAPABILITY_INTERSECTION"));

    // 8. Grant `process.exec` in Per-Fay Policy -> High-criticality op holds for Human Prime approval
    let _ = http_json(
        addr,
        "PUT",
        &format!("/api/v1/fays/{ifay_id}/policy"),
        Some(&json!({"policy_capabilities": ["fs.read", "fs.write", "process.exec"]})),
    );
    let (_, exec_hold) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/exec workspace.ls"})),
    );
    assert!(exec_hold["terminal_text"]
        .as_str()
        .unwrap()
        .contains("HOLD_FOR_HUMAN_CONFIRMATION"));

    let (_, approvals) = http_json(addr, "GET", "/api/v1/cap/approvals", None);
    let first_apr_id = approvals[0]["approval_id"].as_str().unwrap().to_owned();
    let (dec_code, decided) = http_json(
        addr,
        "POST",
        &format!("/api/v1/cap/approvals/{first_apr_id}/decide"),
        Some(&json!({"approve": true})),
    );
    assert_eq!(dec_code, 200);
    assert_eq!(decided["status"], "approved");

    // 9. Queue another high-risk CAP request, then hit RED G0 Emergency Brake -> revokes pending & blocks!
    let _ = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/exec workspace.digest"})),
    );
    let (_, approvals2) = http_json(addr, "GET", "/api/v1/cap/approvals", None);
    let second_apr_id = approvals2
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["status"] == "pending")
        .unwrap()["approval_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let (brake_code, braked) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/emergency-brake"),
        Some(&json!({"reason": "Emergency G0 disconnect test"})),
    );
    assert_eq!(brake_code, 200);
    assert_eq!(braked["faying_state"], "ROGUE");

    // Pending approval was automatically revoked by G0 brake; trying to approve it returns 403
    let (late_approve_code, _) = http_json(
        addr,
        "POST",
        &format!("/api/v1/cap/approvals/{second_apr_id}/decide"),
        Some(&json!({"approve": true})),
    );
    assert_eq!(late_approve_code, 403);

    // Subsequent CAP call is immediately rejected in ROGUE state
    let (_, post_brake_read) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{ifay_id}/chat"),
        Some(&json!({"message": "/read notes/plan.txt"})),
    );
    assert!(post_brake_read["terminal_text"]
        .as_str()
        .unwrap()
        .contains("DENY_ROGUE_FAY"));

    // 10. Local Telepathy (TP) Shared Cognitive Space between personal iFay & project coFay
    let (tp_code, tp_session) = http_json(
        addr,
        "POST",
        "/api/v1/tp/sessions",
        Some(&json!({
            "initiator_fay_id": ifay_id,
            "peer_fay_id": cofay_id,
            "topic": "Release Architecture Handoff"
        })),
    );
    assert_eq!(tp_code, 201);
    let tp_id = tp_session["session_id"].as_str().unwrap().to_owned();

    let (share_code, shared_entry) = http_json(
        addr,
        "POST",
        &format!("/api/v1/tp/sessions/{tp_id}/share"),
        Some(&json!({
            "author_fay_id": ifay_id,
            "category": "decision_artifact",
            "key": "target_tag",
            "structured_value": {"tag": "v0.3.0", "approved_by_chair": true}
        })),
    );
    assert_eq!(share_code, 201);
    assert_eq!(shared_entry["revision"], 1);

    // Peer coFay directly sees shared TP cognitive entry during ICP dialogue without prompt-telephone loss
    let (_, cofay_chat) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{cofay_id}/chat"),
        Some(&json!({"message": "Check shared TP status"})),
    );
    assert!(cofay_chat["terminal_text"]
        .as_str()
        .unwrap()
        .contains("tp:target_tag"));

    // 11. Tamper-evident SHA-256 hash chain verification
    let (aud_code, aud) = http_json(addr, "GET", "/api/v1/audit", None);
    assert_eq!(aud_code, 200);
    assert_eq!(aud["verified"], true);
    assert!(aud["count"].as_u64().unwrap() >= 10);

    daemon.shutdown();
    let _ = fs::remove_dir_all(&state_dir);
}

#[test]
fn faygerd_connects_to_live_ego_3in1_service_when_online() {
    // Spin up a lightweight mock Ego 3-in-1 service on 127.0.0.1:0
    let ego_listener = TcpListener::bind("127.0.0.1:0").expect("bind mock ego");
    let ego_addr = ego_listener.local_addr().expect("ego addr");
    let stop_ego = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_ego);

    let ego_thread = thread::spawn(move || {
        for incoming in ego_listener.incoming() {
            if stop_clone.load(Ordering::SeqCst) {
                break;
            }
            let Ok(mut stream) = incoming else {
                continue;
            };
            let mut reader = BufReader::new(&stream);
            let mut req_line = String::new();
            if reader.read_line(&mut req_line).unwrap_or(0) == 0 {
                continue;
            }
            let mut content_len = 0usize;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" || h == "\n" {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.trim().eq_ignore_ascii_case("content-length") {
                        content_len = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; content_len];
            if content_len > 0 {
                let _ = reader.read_exact(&mut body);
            }
            let resp_json = if req_line.contains("/v1/intuition") {
                json!({"intent": "ego_fast_system1", "confidence": 0.99})
            } else if req_line.contains("/v1/dynamic-context") {
                json!({"dynamic_context": "Injected owner concise architectural tone"})
            } else {
                json!({"rewritten": "[Ego-Guarded] Clean personalized response!", "rewritten_applied": true})
            };
            let bytes = serde_json::to_vec(&resp_json).unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&bytes);
            let _ = stream.flush();
        }
    });

    let state_dir = fresh_state_dir("live-ego");
    let mut daemon = spawn_daemon("127.0.0.1:0", &state_dir).expect("spawn faygerd");
    let addr = daemon.local_addr();

    let _ = http_json(
        addr,
        "PUT",
        "/api/v1/host/pool",
        Some(&json!({"ego_endpoint": format!("http://{ego_addr}")})),
    );
    let (_, prof) = http_json(addr, "GET", "/api/v1/host/profile", None);
    assert_eq!(prof["ego_slot"]["online"], true);
    assert_eq!(prof["ego_slot"]["mode"], "ego_remote_3in1");

    let (_, created) = http_json(
        addr,
        "POST",
        "/api/v1/fays/create",
        Some(&json!({"kind": "ifay", "name": "LiveEgoSoul"})),
    );
    let fay_id = created["fay_id"].as_str().unwrap();
    let (_, turn) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "Summarize my day"})),
    );
    assert_eq!(turn["response_ctx"]["ego_trace"]["mode"], "ego_remote_3in1");
    assert_eq!(
        turn["response_ctx"]["ego_trace"]["intuition_intent"],
        "ego_fast_system1"
    );
    assert_eq!(
        turn["response_ctx"]["summary_text"],
        "[Ego-Guarded] Clean personalized response!"
    );

    daemon.shutdown();
    stop_ego.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect(ego_addr);
    let _ = ego_thread.join();
    let _ = fs::remove_dir_all(&state_dir);
}

#[test]
fn cli_subcommands_profile_wallet_soul_and_chat_work_end_to_end() {
    let state_dir = fresh_state_dir("cli-suite");
    let state_str = state_dir.to_str().unwrap();

    // 1. fayger profile --json
    let prof_txt = run_cli_capture(&["profile", "--json", "--state-dir", state_str]);
    let prof_val: Value = serde_json::from_str(&prof_txt).expect("profile json");
    assert_eq!(prof_val["schema_version"], DEVICE_PROFILE_SCHEMA_VERSION);

    // 2. fayger wallet init|show|derive-ifay|derive-cofay|dynamic-code|exchange-grant
    let init_txt = run_cli_capture(&[
        "wallet",
        "init",
        "--name",
        "CLI Vault",
        "--state-dir",
        state_str,
    ]);
    assert!(init_txt.contains("Initialized FayID Wallet Vault"));

    let ifay_json = run_cli_capture(&[
        "wallet",
        "derive-ifay",
        "--name",
        "cli-ifay",
        "--state-dir",
        state_str,
        "--json",
    ]);
    assert!(ifay_json.contains("ifay_"));

    let cofay_txt = run_cli_capture(&[
        "wallet",
        "derive-cofay",
        "--name",
        "cli-seat",
        "--role",
        "Maintainer",
        "--state-dir",
        state_str,
    ]);
    assert!(cofay_txt.contains("cofay_"));

    let dync_txt = run_cli_capture(&[
        "wallet",
        "dynamic-code",
        "--purpose",
        "cli-check",
        "--state-dir",
        state_str,
    ]);
    assert!(dync_txt.contains("dync_"));

    let grant_txt = run_cli_capture(&[
        "wallet",
        "exchange-grant",
        "--provider",
        "github",
        "--secret",
        "<example-placeholder-cli-cred>",
        "--scopes",
        "repo:read",
        "--state-dir",
        state_str,
    ]);
    assert!(grant_txt.contains("ag_"));

    let show_txt = run_cli_capture(&["wallet", "show", "--state-dir", state_str]);
    assert!(!show_txt.contains("<example-placeholder-cli-cred>"));
    assert!(show_txt.contains("CLI Vault"));

    // 3. fayger soul create + inspect + run + chat (terminal .ctx degradation)
    let buf_out = state_dir.join("cli_soul.buf");
    let soul_json_str = run_cli_capture(&[
        "soul",
        "create",
        "--kind",
        "ifay",
        "--name",
        "CliRunner",
        "--out",
        buf_out.to_str().unwrap(),
        "--state-dir",
        state_str,
        "--json",
    ]);
    let soul_val: Value = serde_json::from_str(&soul_json_str).unwrap();
    let soul_fay_id = soul_val["fay_id"].as_str().unwrap();

    let inspect_txt = run_cli_capture(&["inspect", buf_out.to_str().unwrap()]);
    assert!(inspect_txt.contains("soul_metadata"));
    let run_txt = run_cli_capture(&["run", buf_out.to_str().unwrap()]);
    assert!(run_txt.contains("Terminated"));

    let chat_term = run_cli_capture(&[
        "chat",
        "--fay",
        soul_fay_id,
        "--message",
        "Hello from terminal!",
        "--state-dir",
        state_str,
    ]);
    assert!(chat_term.contains("ICP .ctx"));
    assert!(chat_term.contains("Runtime Context & Capability Intersection"));

    // 4. Verify `fayger up` / `fayger serve` parser mapping
    assert_eq!(
        parse_args(&[
            "up".to_owned(),
            "--addr".to_owned(),
            "127.0.0.1:9090".to_owned()
        ])
        .unwrap(),
        Command::Up {
            addr: "127.0.0.1:9090".to_owned(),
            state_dir: None,
        }
    );
    assert_eq!(
        parse_args(&["serve".to_owned()]).unwrap(),
        Command::Up {
            addr: "127.0.0.1:7979".to_owned(),
            state_dir: None,
        }
    );

    let _ = fs::remove_dir_all(&state_dir);
}

#[test]
fn faying_heartbeat_timeout_automatically_reverts_instance_to_rogue() {
    let state_dir = fresh_state_dir("hb-timeout");
    let mut daemon = spawn_daemon("127.0.0.1:0", &state_dir).expect("spawn faygerd");
    let addr = daemon.local_addr();

    let (_, created) = http_json(
        addr,
        "POST",
        "/api/v1/fays/create",
        Some(&json!({"kind": "ifay", "name": "HeartbeatSoul"})),
    );
    let fay_id = created["fay_id"].as_str().unwrap();

    // Activate Faying with a short 2-second heartbeat timeout
    let (_, activated) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/faying"),
        Some(&json!({"grade": "G2", "ttl_secs": 60, "heartbeat_timeout_secs": 2})),
    );
    assert_eq!(activated["faying_state"], "FAYING");

    // Wait 3.1s without sending heartbeat -> must auto-revert to ROGUE and block CAP!
    thread::sleep(Duration::from_millis(3100));

    let (_, after_timeout) = http_json(addr, "GET", &format!("/api/v1/fays/{fay_id}"), None);
    assert_eq!(after_timeout["faying_state"], "ROGUE");

    let (_, blocked) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/write late.txt denied"})),
    );
    assert!(blocked["terminal_text"]
        .as_str()
        .unwrap()
        .contains("DENY_ROGUE_FAY"));

    daemon.shutdown();
    let _ = fs::remove_dir_all(&state_dir);
}
