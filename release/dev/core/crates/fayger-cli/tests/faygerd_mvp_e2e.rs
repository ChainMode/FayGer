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
    parse_args, run_cli, spawn_daemon, spawn_daemon_with_gmchain, verify_audit_chain, Command,
    HashChainedAuditEntry, DEVICE_PROFILE_SCHEMA_VERSION, ICP_CTX_SCHEMA_VERSION,
    MERIT_CREDENTIAL_SCHEMA_VERSION,
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

    // 1. Embedded Dashboard HTML check (6 tabs, offline-ready, no external CDN)
    let (html_code, html) = http_call(addr, "GET", "/", None);
    assert_eq!(html_code, 200);
    assert!(html.contains("FayGer Local Runtime Console"));
    assert!(html.contains("设备环境与资源池"));
    assert!(html.contains("身份与凭据保险箱"));
    assert!(html.contains("G0 一键急刹"));
    assert!(html.contains("ICP 富交互对话与 CAP 审批"));
    assert!(html.contains("双 Fay 心灵感应协作间"));
    assert!(html.contains("6. 美誉值与优先权 (MeriToken)"));
    assert!(html.contains("OS 原生权限探针与一键申请面板"));
    assert!(html.contains("配置大脑：Ego 与外部大模型"));
    assert!(html.contains("终端硬件 / 操作系统 / 桌面软件操控台"));
    assert!(html.contains("Panel A — 美誉值展示与分级凭证出示"));
    assert!(html.contains("Panel B — 添加美誉值"));
    assert!(html.contains("Panel C — 消费美誉值"));
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
            gmchain_endpoint: None,
            open_browser: false,
        }
    );
    assert_eq!(
        parse_args(&[
            "up".to_owned(),
            "--gmchain-endpoint".to_owned(),
            "http://127.0.0.1:8546".to_owned(),
            "--open".to_owned(),
        ])
        .unwrap(),
        Command::Up {
            addr: "127.0.0.1:7979".to_owned(),
            state_dir: None,
            gmchain_endpoint: Some("http://127.0.0.1:8546".to_owned()),
            open_browser: true,
        }
    );
    assert_eq!(
        parse_args(&["serve".to_owned()]).unwrap(),
        Command::Up {
            addr: "127.0.0.1:7979".to_owned(),
            state_dir: None,
            gmchain_endpoint: None,
            open_browser: false,
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

#[test]
fn meritoken_local_fallback_faying_guard_floor_protection_and_credentials_e2e() {
    let state_dir = fresh_state_dir("merit-local-e2e");
    // Reserve & close an ephemeral loopback port so local fallback is 100% deterministic.
    let closed_listener = TcpListener::bind("127.0.0.1:0").expect("bind closed port");
    let closed_addr = closed_listener.local_addr().expect("closed addr");
    drop(closed_listener);

    let mut daemon = spawn_daemon_with_gmchain(
        "127.0.0.1:0",
        &state_dir,
        Some(&format!("http://{closed_addr}")),
    )
    .expect("spawn faygerd");
    let addr = daemon.local_addr();

    // Non-loopback GMChain endpoint must be rejected with 400
    let (bad_cfg_code, _) = http_json(
        addr,
        "POST",
        "/api/v1/merit/config",
        Some(&json!({"gmchain_endpoint": "http://10.0.0.8:8546"})),
    );
    assert_eq!(bad_cfg_code, 400);

    let (cfg_code, cfg_val) = http_json(addr, "GET", "/api/v1/merit/config", None);
    assert_eq!(cfg_code, 200);
    assert_eq!(cfg_val["sync_mode"], "local_pocket_fallback");

    // 1. Create & load an iFay soul (starts in ROGUE)
    let (create_code, ifay_view) = http_json(
        addr,
        "POST",
        "/api/v1/fays/create",
        Some(&json!({"kind": "ifay", "name": "MeritPrimeSoul"})),
    );
    assert_eq!(create_code, 201);
    assert_eq!(ifay_view["faying_state"], "ROGUE");
    let instance_id = ifay_view["instance_id"].as_str().unwrap().to_owned();
    let fay_id = ifay_view["fay_id"].as_str().unwrap().to_owned();

    // 2. ROGUE allows GET /api/v1/fays/<instance_id>/merit (200), but blocks add & consume (403)!
    let (get_rogue_code, initial_merit) = http_json(
        addr,
        "GET",
        &format!("/api/v1/fays/{instance_id}/merit"),
        None,
    );
    assert_eq!(get_rogue_code, 200);
    assert_eq!(initial_merit["cur_merit_raw"], 5_000_000);
    assert_eq!(initial_merit["min_merit_floor_raw"], 2_718_281);
    assert_eq!(initial_merit["consumable_merit_raw"], 2_281_719);
    assert_eq!(initial_merit["sync_mode"], "local_pocket_fallback");

    let (add_rogue_code, add_rogue_err) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/merit/add"),
        Some(&json!({
            "title": "Blocked Contribution",
            "domain_chain_id": "gmc-open-science",
            "amount_mu": 40,
            "horizon_days": 90,
            "summary": "Must be rejected in ROGUE"
        })),
    );
    assert_eq!(add_rogue_code, 403);
    assert!(add_rogue_err["error"].as_str().unwrap().contains("ROGUE"));

    let (consume_rogue_code, consume_rogue_err) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/merit/consume"),
        Some(&json!({
            "priority_kind": "voting_priority",
            "amount_mu": 1,
            "reason": "Blocked in ROGUE",
            "proposal_id": "prop-001"
        })),
    );
    assert_eq!(consume_rogue_code, 403);
    assert!(consume_rogue_err["error"]
        .as_str()
        .unwrap()
        .contains("ROGUE"));

    // 3. Switch Faying ON -> POST merit/add (+40 μ) -> POST merit/consume (-12 μ voting_priority)
    let (faying_code, _) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/faying"),
        Some(&json!({"grade": "G2", "ttl_secs": 600, "heartbeat_timeout_secs": 120})),
    );
    assert_eq!(faying_code, 200);

    let (add_code, add_res) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/merit/add"),
        Some(&json!({
            "title": "Reproducible Open-Science Benchmark",
            "domain_chain_id": "gmc-open-science",
            "amount_mu": 40,
            "horizon_days": 90,
            "summary": "Verified artifact hash & peer attestation"
        })),
    );
    assert_eq!(add_code, 200);
    assert_eq!(add_res["cur_merit_raw"], 45_000_000);
    let floor_after_add = add_res["min_merit_floor_raw"].as_i64().unwrap();
    assert!(floor_after_add >= 2_718_281);
    let cur_after_add = add_res["cur_merit_raw"].as_i64().unwrap();

    let (consume_code, consume_res) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/merit/consume"),
        Some(&json!({
            "priority_kind": "voting_priority",
            "amount_mu": 12,
            "reason": "Weighted governance vote on open protocol upgrade",
            "proposal_id": "prop-001"
        })),
    );
    assert_eq!(consume_code, 200);
    assert_eq!(consume_res["before_cur_merit_raw"], cur_after_add);
    assert_eq!(
        consume_res["after_cur_merit_raw"],
        cur_after_add - 12_000_000
    );
    assert_eq!(consume_res["cur_merit_raw"], 33_000_000);
    // Constitutional invariant: minMerit floor stays strictly unchanged during priority consumption!
    assert_eq!(consume_res["min_merit_floor_raw"], floor_after_add);
    assert_eq!(
        consume_res["consumable_merit_raw"],
        33_000_000 - floor_after_add
    );

    // 4. Over-consuming past (curMerit - minMerit) returns 400 while preserving minMerit!
    // Note: 31 μ < curMerit (33 μ), yet 31 μ > consumable_merit!
    let (over_code, over_err) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/merit/consume"),
        Some(&json!({
            "priority_kind": "cofay_service_priority",
            "amount_mu": 31,
            "reason": "Attempt to dip below minMerit floor"
        })),
    );
    assert_eq!(over_code, 400);
    assert!(over_err["error"].as_str().unwrap().contains("minMerit"));

    // 5. L1 / L2 / L3 credentials generate cleanly with JCS SHA-256 commitments
    for tier in ["L1_PUBLIC", "L2_BAND", "L3_ZK_BLINDED"] {
        let (cred_code, cred) = http_json(
            addr,
            "POST",
            &format!("/api/v1/fays/{instance_id}/merit/credential"),
            Some(&json!({"privacy_tier": tier})),
        );
        assert_eq!(cred_code, 200);
        assert_eq!(cred["schema_version"], MERIT_CREDENTIAL_SCHEMA_VERSION);
        assert_eq!(cred["privacy_tier"], tier);
        assert_eq!(cred["commitment_sha256"].as_str().unwrap().len(), 64);
        if tier == "L3_ZK_BLINDED" {
            let raw_cred = serde_json::to_string(&cred).unwrap();
            assert!(!raw_cred.contains(&fay_id));
        }
    }

    // 6. ICP chat mentioning 美誉 / 优先权 / μ includes structured MeritStandingCard (.ctx)
    let (chat_code, chat_turn) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{instance_id}/chat"),
        Some(&json!({"message": "请展示我当前的美誉值与可消减优先权 μ"})),
    );
    assert_eq!(chat_code, 200);
    let has_merit_card = chat_turn["response_ctx"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["kind"] == "merit_standing_card");
    assert!(has_merit_card);

    // 7. Audit chain verification remains true and records all merit stages
    let (audit_status, aud) = http_json(addr, "GET", "/api/v1/audit", None);
    assert_eq!(audit_status, 200);
    assert_eq!(aud["verified"], true);
    let parsed_entries: Vec<HashChainedAuditEntry> =
        serde_json::from_value(aud["entries"].clone()).expect("deserialize audit entries");
    assert!(verify_audit_chain(&parsed_entries));
    assert!(parsed_entries
        .iter()
        .any(|e| e.stage == "merit.contribution_added"));
    assert!(parsed_entries
        .iter()
        .any(|e| e.stage == "merit.priority_consumed"));
    assert!(parsed_entries
        .iter()
        .any(|e| e.stage == "merit.credential_presented"));

    daemon.shutdown();
    let _ = fs::remove_dir_all(&state_dir);
}

#[test]
fn meritoken_live_gmchain_json_rpc_wire_roundtrip_e2e() {
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct MockChainAccount {
        cur_raw: i64,
        floor_raw: i64,
    }

    let gmc_listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gmc-node");
    let gmc_addr = gmc_listener.local_addr().expect("gmc addr");
    let stop_gmc = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_gmc);
    let methods_seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let methods_clone = Arc::clone(&methods_seen);

    let gmc_thread = thread::spawn(move || {
        let mut accounts: HashMap<String, MockChainAccount> = HashMap::new();
        let mut block_height: u64 = 100;

        for incoming in gmc_listener.incoming() {
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
            if content_len == 0 && !req_line.starts_with("POST /rpc ") {
                continue;
            }
            let mut body = vec![0u8; content_len];
            if content_len > 0 {
                let _ = reader.read_exact(&mut body);
            }
            let Ok(rpc_req) = serde_json::from_slice::<Value>(&body) else {
                continue;
            };
            assert_eq!(rpc_req["jsonrpc"], "2.0");
            let method = rpc_req["method"].as_str().unwrap_or("").to_owned();
            let params = &rpc_req["params"];
            methods_clone.lock().unwrap().push(method.clone());

            let rpc_resp = match method.as_str() {
                "gmc_registerExternalFay" => {
                    assert_eq!(params["role_label"], "FayGer Digital Soul");
                    let fid = params["fay_id"].as_str().unwrap().to_owned();
                    let acct = accounts.entry(fid.clone()).or_insert(MockChainAccount {
                        cur_raw: 8_000_000,
                        floor_raw: 2_718_281,
                    });
                    json!({
                        "jsonrpc": "2.0",
                        "id": rpc_req["id"],
                        "result": {
                            "canonical_fayid": fid,
                            "cur_merit_raw": acct.cur_raw,
                            "consumable_merit_raw": acct.cur_raw - acct.floor_raw,
                            "min_merit_floor_raw": acct.floor_raw,
                            "block_height": block_height,
                            "state_root_hex": format!("{block_height:064x}"),
                        }
                    })
                }
                "gmc_recordContribution" => {
                    let fid = params["fay_id"].as_str().unwrap().to_owned();
                    let amount_mu = params["amount_mu"].as_i64().unwrap();
                    let acct = accounts.entry(fid).or_insert(MockChainAccount {
                        cur_raw: 8_000_000,
                        floor_raw: 2_718_281,
                    });
                    let delta = amount_mu * 1_000_000;
                    acct.cur_raw += delta;
                    acct.floor_raw += delta / 10;
                    block_height += 1;
                    json!({
                        "jsonrpc": "2.0",
                        "id": rpc_req["id"],
                        "result": {
                            "batch_id": format!("gmc_batch_{block_height}"),
                            "cur_merit_raw": acct.cur_raw,
                            "consumable_merit_raw": acct.cur_raw - acct.floor_raw,
                            "min_merit_floor_raw": acct.floor_raw,
                            "block_height": block_height,
                            "state_root_hex": format!("{block_height:064x}"),
                        }
                    })
                }
                "gmc_consumePriority" => {
                    assert_eq!(params["approve"], true);
                    let fid = params["fay_id"].as_str().unwrap().to_owned();
                    let amt_str = params["amount_mu"].as_str().unwrap();
                    let amt_mu: i64 = amt_str.parse().unwrap();
                    let consumed_raw = amt_mu * 1_000_000;
                    let acct = accounts.get_mut(&fid).unwrap();
                    let consumable = acct.cur_raw - acct.floor_raw;
                    if consumed_raw > consumable {
                        json!({
                            "jsonrpc": "2.0",
                            "id": rpc_req["id"],
                            "error": {
                                "code": -32001,
                                "message": "exceeds consumable_merit_raw; minMerit floor protected"
                            }
                        })
                    } else {
                        let before = acct.cur_raw;
                        acct.cur_raw -= consumed_raw;
                        block_height += 1;
                        json!({
                            "jsonrpc": "2.0",
                            "id": rpc_req["id"],
                            "result": {
                                "consumption_id": format!("gmc_csm_{block_height}"),
                                "consumed_mu_raw": consumed_raw,
                                "before_cur_merit_raw": before,
                                "after_cur_merit_raw": acct.cur_raw,
                                "consumable_merit_raw": acct.cur_raw - acct.floor_raw,
                                "min_merit_floor_raw": acct.floor_raw,
                                "block_height": block_height,
                                "state_root_hex": format!("{block_height:064x}"),
                            }
                        })
                    }
                }
                other => panic!("unexpected RPC method: {other}"),
            };

            let resp_bytes = serde_json::to_vec(&rpc_resp).unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                resp_bytes.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&resp_bytes);
            let _ = stream.flush();
        }
    });

    let state_dir = fresh_state_dir("merit-live-rpc");
    let mut daemon = spawn_daemon("127.0.0.1:0", &state_dir).expect("spawn faygerd");
    let addr = daemon.local_addr();

    // Point FayGer to live mock gmc-node via POST /api/v1/merit/config
    let (cfg_code, cfg_res) = http_json(
        addr,
        "POST",
        "/api/v1/merit/config",
        Some(&json!({"gmchain_endpoint": format!("http://{gmc_addr}")})),
    );
    assert_eq!(cfg_code, 200);
    assert_eq!(cfg_res["online"], true);
    assert_eq!(cfg_res["sync_mode"], "gmchain_live_rpc");

    // Create coFay -> registers immediately on mock gmc-node (`cur_merit_raw = 8_000_000`)
    let (_, cofay) = http_json(
        addr,
        "POST",
        "/api/v1/fays/create",
        Some(&json!({"kind": "cofay", "name": "ScienceStewardSeat"})),
    );
    let inst_id = cofay["instance_id"].as_str().unwrap();
    assert_eq!(cofay["merit"]["sync_mode"], "gmchain_live_rpc");
    assert_eq!(cofay["merit"]["cur_merit_raw"], 8_000_000);

    // Activate Faying -> add +45 μ -> consume -15 μ compute_slot_priority
    let _ = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{inst_id}/faying"),
        Some(&json!({"grade": "G2"})),
    );
    let (add_code, added) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{inst_id}/merit/add"),
        Some(&json!({
            "title": "Public Infra Validator Benchmark",
            "domain_chain_id": "gmc-public-infra",
            "amount_mu": 45,
            "horizon_days": 180,
            "summary": "Live RPC mint verification"
        })),
    );
    assert_eq!(add_code, 200);
    assert_eq!(added["batch_id"], "gmc_batch_101");
    assert_eq!(added["cur_merit_raw"], 53_000_000);
    assert_eq!(added["sync_mode"], "gmchain_live_rpc");
    let live_floor = added["min_merit_floor_raw"].as_i64().unwrap();

    let (csm_code, consumed) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{inst_id}/merit/consume"),
        Some(&json!({
            "priority_kind": "compute_slot_priority",
            "amount_mu": 15,
            "reason": "Priority host compute dispatch",
            "proposal_id": "slot-req-09"
        })),
    );
    assert_eq!(csm_code, 200);
    assert_eq!(consumed["consumption_id"], "gmc_csm_102");
    assert_eq!(consumed["before_cur_merit_raw"], 53_000_000);
    assert_eq!(consumed["after_cur_merit_raw"], 38_000_000);
    assert_eq!(consumed["min_merit_floor_raw"], live_floor);
    assert_eq!(consumed["last_block_height"], 102);

    // Over-consuming past live consumable_merit_raw returns 400
    let (over_live_code, _) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{inst_id}/merit/consume"),
        Some(&json!({
            "priority_kind": "voting_priority",
            "amount_mu": 36,
            "reason": "Exceeds live consumable pool"
        })),
    );
    assert_eq!(over_live_code, 400);

    let seen = methods_seen.lock().unwrap().clone();
    assert!(seen.contains(&"gmc_registerExternalFay".to_owned()));
    assert!(seen.contains(&"gmc_recordContribution".to_owned()));
    assert!(seen.contains(&"gmc_consumePriority".to_owned()));

    let (_, aud) = http_json(addr, "GET", "/api/v1/audit", None);
    assert_eq!(aud["verified"], true);

    daemon.shutdown();
    stop_gmc.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect(gmc_addr);
    let _ = gmc_thread.join();
    let _ = fs::remove_dir_all(&state_dir);
}

#[test]
fn faygerd_v0_3_0_os_permissions_brain_external_llm_ego_and_real_host_drivers_e2e() {
    use std::sync::Mutex;

    std::env::set_var("FAYGER_HEADLESS_PERMISSION_TEST", "1");
    std::env::set_var("FAYGER_MUTE_AUDIO", "1");
    std::env::set_var("FAYGER_SANDBOX_CLIPBOARD_ONLY", "1");

    // Spin up a local loopback mock OpenAI-compatible External LLM server (`127.0.0.1:0`)
    let llm_listener = TcpListener::bind("127.0.0.1:0").expect("bind mock external llm");
    let llm_addr = llm_listener.local_addr().expect("llm addr");
    let stop_llm = Arc::new(AtomicBool::new(false));
    let stop_llm_clone = Arc::clone(&stop_llm);
    let seen_auth_headers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_auth_clone = Arc::clone(&seen_auth_headers);

    let llm_thread = thread::spawn(move || {
        for incoming in llm_listener.incoming() {
            if stop_llm_clone.load(Ordering::SeqCst) {
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
            let mut auth_header = String::new();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" || h == "\n" {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.trim().eq_ignore_ascii_case("content-length") {
                        content_len = v.trim().parse().unwrap_or(0);
                    } else if k.trim().eq_ignore_ascii_case("authorization") {
                        auth_header = v.trim().to_owned();
                    }
                }
            }
            seen_auth_clone.lock().unwrap().push(auth_header);
            let mut body = vec![0u8; content_len];
            if content_len > 0 {
                let _ = reader.read_exact(&mut body);
            }
            let req_json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
            let body_str = String::from_utf8_lossy(&body);

            let reply_content = if body_str.contains("Ping connectivity check") {
                "BRAIN_PING_OK — External LLM online!".to_owned()
            } else if body_str.contains("帮我检查宿主硬件") {
                // Deliberately include a sk- token in mock output to verify Ego Guard redacts it,
                // plus a structured <fay_action> tag to trigger live HardwareTelemetry!
                "好的，已通过 Ego 对齐为您采集宿主硬件遥测（测试脱敏 sk-live9988776655443322） <fay_action>{\"op\":\"hardware_telemetry\"}</fay_action>".to_owned()
            } else {
                format!("Mock LLM handled model={}", req_json["model"])
            };

            let resp_json = json!({
                "id": "chatcmpl-fayger-test",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": reply_content
                    }
                }]
            });
            let resp_bytes = serde_json::to_vec(&resp_json).unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                resp_bytes.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&resp_bytes);
            let _ = stream.flush();
        }
    });

    let state_dir = fresh_state_dir("v030-os-brain-drivers");
    let mut daemon = spawn_daemon("127.0.0.1:0", &state_dir).expect("spawn faygerd");
    let addr = daemon.local_addr();

    // 1. OS Permissions Probe & Native Request Endpoints
    let (perm_code, perms) = http_json(addr, "GET", "/api/v1/os/permissions", None);
    assert_eq!(perm_code, 200);
    let perm_arr = perms.as_array().unwrap();
    assert_eq!(perm_arr.len(), 12);
    assert!(perm_arr.iter().any(|p| p["capability"] == "os.clipboard"));
    assert!(perm_arr
        .iter()
        .any(|p| p["capability"] == "hardware.screen"));
    assert!(perm_arr.iter().any(|p| p["capability"] == "app.control"));

    let (req_perm_code, req_perm_receipt) = http_json(
        addr,
        "POST",
        "/api/v1/os/permissions/request",
        Some(&json!({
            "capability": "hardware.telemetry",
            "open_settings": false,
            "grant_to_host_pool": true
        })),
    );
    assert_eq!(req_perm_code, 200);
    assert_eq!(req_perm_receipt["capability"], "hardware.telemetry");
    assert_eq!(req_perm_receipt["granted_to_host_pool"], true);

    // 2. Create iFay soul (starts in ROGUE)
    let (create_status, ifay) = http_json(
        addr,
        "POST",
        "/api/v1/fays/create",
        Some(&json!({"kind": "ifay", "name": "BrainAndHostSoul"})),
    );
    assert_eq!(create_status, 201);
    let inst_id = ifay["instance_id"].as_str().unwrap().to_owned();
    let fay_id = ifay["fay_id"].as_str().unwrap().to_owned();

    // 3. Configure External LLM + Ego Brain with encrypted API key in FayID Vault
    let secret_api_key = "<test-vaulted-llm-key-alpha-991122>";
    let (brain_save_code, brain_view) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{inst_id}/brain"),
        Some(&json!({
            "external_llm": {
                "enabled": true,
                "provider": "openai_compatible",
                "base_url": format!("http://{llm_addr}/v1"),
                "model": "qwen2.5-coder",
                "temperature": 0.2,
                "max_tokens": 512,
                "api_key": secret_api_key
            },
            "ego": {
                "mode": "embedded",
                "persona_style": "严谨、尊重人类原型主权",
                "redlines": ["禁止泄露密钥", "高危动作必走 CAP 审批"],
                "fast_intuition_first": false
            }
        })),
    );
    assert_eq!(brain_save_code, 200);
    assert_eq!(brain_view["external_llm"]["enabled"], true);
    assert_eq!(brain_view["external_llm"]["has_api_key"], true);

    // Verify zero plaintext leakage across GET /brain, GET /state, GET /wallet, engine_state.json, and wallet_vault.json!
    let (_, brain_get) = http_json(addr, "GET", &format!("/api/v1/fays/{inst_id}/brain"), None);
    assert_eq!(brain_get["external_llm"]["has_api_key"], true);
    assert!(!serde_json::to_string(&brain_get)
        .unwrap()
        .contains(secret_api_key));
    let (_, full_state) = http_json(addr, "GET", "/api/v1/state", None);
    assert!(!serde_json::to_string(&full_state)
        .unwrap()
        .contains(secret_api_key));
    let engine_disk = fs::read_to_string(state_dir.join("engine_state.json")).unwrap();
    assert!(!engine_disk.contains(secret_api_key));
    let vault_disk = fs::read_to_string(state_dir.join("wallet_vault.json")).unwrap();
    assert!(!vault_disk.contains(secret_api_key));

    // 4. Test Brain Connectivity (`POST /api/v1/fays/<id>/brain/test`)
    let (test_code, test_rep) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{inst_id}/brain/test"),
        Some(&json!({})),
    );
    assert_eq!(test_code, 200);
    assert_eq!(test_rep["ok"], true);
    assert!(test_rep["ego_status"].as_str().unwrap().contains("ready"));
    assert!(test_rep["llm_status"]
        .as_str()
        .unwrap()
        .contains("connected"));
    assert!(test_rep["sample_reply"]
        .as_str()
        .unwrap()
        .contains("BRAIN_PING_OK"));
    let seen_headers = seen_auth_headers.lock().unwrap().clone();
    assert!(seen_headers.contains(&format!("Bearer {secret_api_key}")));

    // 5. Verify ROGUE strictly blocks hardware/OS drivers (`HardwareTelemetry`, `OsClipboardWrite`, `OsNotify`)
    for blocked_cmd in [
        "/hardware",
        "/clipboard-write rogue-blocked",
        "/notify T|rogue",
    ] {
        let (_, rogue_turn) = http_json(
            addr,
            "POST",
            &format!("/api/v1/fays/{fay_id}/chat"),
            Some(&json!({"message": blocked_cmd})),
        );
        assert!(
            rogue_turn["terminal_text"]
                .as_str()
                .unwrap()
                .contains("DENY_ROGUE_FAY"),
            "ROGUE must block {blocked_cmd}"
        );
    }

    // 6. Activate Faying ON -> execute normal-risk OS/Hardware drivers + Brain `<fay_action>` orchestration!
    let (faying_code, _) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/faying"),
        Some(&json!({"grade": "G2"})),
    );
    assert_eq!(faying_code, 200);

    // Chat via Ego + External LLM -> External LLM emits `<fay_action>{"op":"hardware_telemetry"}</fay_action>`
    // AND Ego Guard scrubs `sk-live9988776655443322`!
    let (brain_turn_code, brain_turn) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "帮我检查宿主硬件电池与内存状态"})),
    );
    assert_eq!(brain_turn_code, 200);
    let summary_txt = brain_turn["response_ctx"]["summary_text"].as_str().unwrap();
    assert!(!summary_txt.contains("sk-live9988776655443322"));
    assert!(summary_txt.contains("[REDACTED_LEGACY_SECRET_PREFIX]"));
    assert!(summary_txt.contains("Hardware Telemetry:"));
    assert!(brain_turn["response_ctx"]["ego_trace"]["mode"]
        .as_str()
        .unwrap()
        .contains("ego_plus_external_llm:openai_compatible:qwen2.5-coder"));

    // Clipboard Write & Read, Desktop Notify, Speaker TTS execute immediately in FAYING
    let (_, clip_w) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/clipboard-write FayGer-Clipboard-Payload-030"})),
    );
    assert!(clip_w["terminal_text"]
        .as_str()
        .unwrap()
        .contains("ALLOW_CONTROLLED"));

    let (_, clip_r) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/clipboard-read"})),
    );
    assert!(clip_r["terminal_text"]
        .as_str()
        .unwrap()
        .contains("FayGer-Clipboard-Payload-030"));

    let (_, notify_res) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/notify FayGer|Notification verified"})),
    );
    assert!(notify_res["terminal_text"]
        .as_str()
        .unwrap()
        .contains("ALLOW_CONTROLLED"));

    let (_, speak_res) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/speak 你好人类原型"})),
    );
    assert!(speak_res["terminal_text"]
        .as_str()
        .unwrap()
        .contains("ALLOW_CONTROLLED"));

    // 7. High-risk drivers (`AppControl`, `HardwareScreenCapture`, `ProcessExec`) MUST hold for CAP Human Prime confirmation!
    let (_, app_hold) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({
            "message": "",
            "host_action": {
                "op": "app_control",
                "app_name": "Calculator",
                "action": "verify_installed"
            }
        })),
    );
    assert!(app_hold["terminal_text"]
        .as_str()
        .unwrap()
        .contains("HOLD_FOR_HUMAN_CONFIRMATION"));

    let (_, screen_hold) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/screenshot e2e_desk"})),
    );
    assert!(screen_hold["terminal_text"]
        .as_str()
        .unwrap()
        .contains("HOLD_FOR_HUMAN_CONFIRMATION"));

    let (_, uname_hold) = http_json(
        addr,
        "POST",
        &format!("/api/v1/fays/{fay_id}/chat"),
        Some(&json!({"message": "/exec uname"})),
    );
    assert!(uname_hold["terminal_text"]
        .as_str()
        .unwrap()
        .contains("HOLD_FOR_HUMAN_CONFIRMATION"));

    // Approve all 3 pending high-risk CAP items (testing both `/api/v1/approvals/<id>/decide` and `/api/v1/cap/approvals/<id>/decide`)
    let (_, pending_list) = http_json(addr, "GET", "/api/v1/cap/approvals", None);
    let pending_ids: Vec<String> = pending_list
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["status"] == "pending")
        .map(|a| a["approval_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(pending_ids.len(), 3);

    for (idx, apr_id) in pending_ids.iter().enumerate() {
        let decide_path = if idx == 0 {
            format!("/api/v1/approvals/{apr_id}/decide")
        } else {
            format!("/api/v1/cap/approvals/{apr_id}/decide")
        };
        let (d_code, decided_item) =
            http_json(addr, "POST", &decide_path, Some(&json!({"approve": true})));
        assert_eq!(d_code, 200);
        assert_eq!(decided_item["status"], "approved");
        assert!(decided_item["execution_summary"].as_str().is_some());
    }

    // 8. Verify SHA-256 hash-chained audit trail integrity across all new stages
    let (audit_http_code, audit_payload) = http_json(addr, "GET", "/api/v1/audit", None);
    assert_eq!(audit_http_code, 200);
    assert_eq!(audit_payload["verified"], true);
    let entries: Vec<HashChainedAuditEntry> =
        serde_json::from_value(audit_payload["entries"].clone()).expect("parse audit");
    assert!(verify_audit_chain(&entries));
    assert!(entries.iter().any(|e| e.stage == "os.permission_requested"));
    assert!(entries.iter().any(|e| e.stage == "brain.configured"));

    daemon.shutdown();
    stop_llm.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect(llm_addr);
    let _ = llm_thread.join();
    let _ = fs::remove_dir_all(&state_dir);
}
