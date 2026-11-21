//! `faygerd` 本地 HTTP/JSON 守护进程与嵌入式 `FayGer Dashboard` 服务（纯安全 Rust `std::net::TcpListener`）。
//!
//! 零重型异步 Web 框架依赖：基于标准库阻塞套接字 + 线程池连接处理，默认监听 `127.0.0.1:7979`，
//! 同时暴露 [`spawn_daemon`] 后台句柄供端到端集成测试在 `127.0.0.1:0` 临时端口上完整验证全部 REST 契约。

use crate::dashboard_html::dashboard_index_html;
use crate::engine::{FayGerEngine, RequestedHostAction};
use crate::error::CliError;
use crate::merit::{parse_positive_mu_to_raw, MeritAddInput, MeritConsumeInput};
use crate::soul::SoulKind;
use crate::wallet::{
    derive_cofay, derive_ifay, exchange_auth_grant, init_wallet, issue_dynamic_code, show_wallet,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// 默认 `fayger up` 监听地址。
pub const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:7979";

/// 后台运行的 `faygerd` 守护句柄（用于集成测试与可控嵌入式生命周期）。
#[derive(Debug)]
pub struct FaygerDaemonHandle {
    local_addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl FaygerDaemonHandle {
    /// 返回实际绑定的本地套接字地址（支持传入 `127.0.0.1:0` 后读取分配的真实端口）。
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 返回基础 HTTP URL（如 `"http://127.0.0.1:54321"`）。
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.local_addr)
    }

    /// 优雅停止后台 `faygerd` 监听线程并等待退出。
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.local_addr, Duration::from_millis(100));
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for FaygerDaemonHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 在后台线程启动 `faygerd` 并立即返回 [`FaygerDaemonHandle`]。
pub fn spawn_daemon(bind_addr: &str, state_dir: &Path) -> Result<FaygerDaemonHandle, CliError> {
    spawn_daemon_with_gmchain(bind_addr, state_dir, None)
}

/// 在后台线程启动 `faygerd`，可选覆盖回环 `GMChain` JSON-RPC 端点。
pub fn spawn_daemon_with_gmchain(
    bind_addr: &str,
    state_dir: &Path,
    gmchain_endpoint: Option<&str>,
) -> Result<FaygerDaemonHandle, CliError> {
    let mut engine = FayGerEngine::open(state_dir)?;
    if let Some(ep) = gmchain_endpoint {
        let _ = engine.update_merit_config(Some(ep))?;
    }
    let listener = TcpListener::bind(bind_addr).map_err(|err| CliError::Io {
        path: bind_addr.to_owned(),
        detail: format!("failed to bind faygerd on {bind_addr}: {err}"),
    })?;
    let local_addr = listener.local_addr().map_err(|err| CliError::Io {
        path: bind_addr.to_owned(),
        detail: err.to_string(),
    })?;
    let shared_engine = Arc::new(Mutex::new(engine));
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);

    let join_handle = thread::spawn(move || {
        accept_loop(&listener, &shared_engine, &shutdown_clone);
    });

    Ok(FaygerDaemonHandle {
        local_addr,
        shutdown,
        join_handle: Some(join_handle),
    })
}

/// 在当前线程阻塞运行 `faygerd`（供 `fayger up` / `fayger serve` 使用）。
pub fn serve_blocking(
    bind_addr: &str,
    state_dir: &Path,
    banner_out: &mut dyn std::io::Write,
) -> Result<(), CliError> {
    serve_blocking_with_gmchain(bind_addr, state_dir, None, banner_out)
}

/// 在当前线程阻塞运行 `faygerd`，支持通过 `--gmchain-endpoint` 指定本机回环 `GMChain` RPC 端点。
pub fn serve_blocking_with_gmchain(
    bind_addr: &str,
    state_dir: &Path,
    gmchain_endpoint: Option<&str>,
    banner_out: &mut dyn std::io::Write,
) -> Result<(), CliError> {
    let mut engine = FayGerEngine::open(state_dir)?;
    if let Some(ep) = gmchain_endpoint {
        let _ = engine.update_merit_config(Some(ep))?;
    }
    let gm_cfg = engine.merit_config();
    let listener = TcpListener::bind(bind_addr).map_err(|err| CliError::Io {
        path: bind_addr.to_owned(),
        detail: format!("failed to bind faygerd on {bind_addr}: {err}"),
    })?;
    let local_addr = listener.local_addr().map_err(|err| CliError::Io {
        path: bind_addr.to_owned(),
        detail: err.to_string(),
    })?;

    let banner = format!(
        "FayGer Three-in-One Daemon (faygerd v{}) is UP!\n  Dashboard UI:  http://{local_addr}\n  REST API Base: http://{local_addr}/api/v1\n  GMChain RPC:   {} [{}]\n  State Dir:     {}\nPress Ctrl+C to stop.\n",
        crate::FAYGER_VERSION,
        gm_cfg.rpc_url,
        gm_cfg.sync_mode,
        state_dir.display()
    );
    banner_out
        .write_all(banner.as_bytes())
        .and_then(|()| banner_out.flush())
        .map_err(|err| CliError::Io {
            path: "<stdout>".to_owned(),
            detail: err.to_string(),
        })?;

    let shared_engine = Arc::new(Mutex::new(engine));
    let shutdown = Arc::new(AtomicBool::new(false));
    accept_loop(&listener, &shared_engine, &shutdown);
    Ok(())
}

fn accept_loop(
    listener: &TcpListener,
    shared_engine: &Arc<Mutex<FayGerEngine>>,
    shutdown: &Arc<AtomicBool>,
) {
    for incoming in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = incoming else {
            continue;
        };
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let engine_ref = Arc::clone(shared_engine);
        thread::spawn(move || {
            handle_connection(stream, &engine_ref);
        });
    }
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct HttpResponse {
    status_code: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn html(html: &str) -> Self {
        Self {
            status_code: 200,
            content_type: "text/html; charset=utf-8",
            body: html.as_bytes().to_vec(),
        }
    }

    fn json_ok<T: Serialize>(val: &T) -> Self {
        match serde_json::to_vec_pretty(val) {
            Ok(bytes) => Self {
                status_code: 200,
                content_type: "application/json; charset=utf-8",
                body: bytes,
            },
            Err(err) => Self::json_err(500, &format!("serialization error: {err}")),
        }
    }

    fn json_created<T: Serialize>(val: &T) -> Self {
        let mut resp = Self::json_ok(val);
        resp.status_code = 201;
        resp
    }

    fn json_err(status_code: u16, message: &str) -> Self {
        let bytes = serde_json::to_vec(&json!({ "error": message }))
            .unwrap_or_else(|_| b"{\"error\":\"internal error\"}".to_vec());
        Self {
            status_code,
            content_type: "application/json; charset=utf-8",
            body: bytes,
        }
    }
}

fn handle_connection(mut stream: TcpStream, shared_engine: &Arc<Mutex<FayGerEngine>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let Some(req) = read_http_request(&stream) else {
        return;
    };
    let resp = if req.method == "OPTIONS" {
        HttpResponse {
            status_code: 204,
            content_type: "text/plain; charset=utf-8",
            body: Vec::new(),
        }
    } else {
        route_request(&req, shared_engine)
    };
    let _ = write_http_response(&mut stream, &resp);
}

fn read_http_request(stream: &TcpStream) -> Option<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_uppercase();
    let raw_target = parts.next()?;
    let path = raw_target.split('?').next().unwrap_or("/").to_owned();

    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        let n = reader.read_line(&mut header_line).ok()?;
        if n == 0 || header_line == "\r\n" || header_line == "\n" {
            break;
        }
        if let Some((name, val)) = header_line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = val.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }

    if content_length > 16 * 1024 * 1024 {
        return None;
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(HttpRequest { method, path, body })
}

fn write_http_response(stream: &mut TcpStream, resp: &HttpResponse) -> std::io::Result<()> {
    let reason = match resp.status_code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, PUT, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n",
        resp.status_code,
        resp.content_type,
        resp.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&resp.body)?;
    stream.flush()
}

#[allow(clippy::too_many_lines)]
fn route_request(req: &HttpRequest, shared_engine: &Arc<Mutex<FayGerEngine>>) -> HttpResponse {
    if req.method == "GET" && (req.path == "/" || req.path == "/index.html") {
        return HttpResponse::html(dashboard_index_html());
    }

    let Ok(mut engine) = shared_engine.lock() else {
        return HttpResponse::json_err(500, "engine mutex poisoned");
    };

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/v1/health") => {
            let (entries, verified) = engine.audit_log();
            let head = entries
                .last()
                .map_or("", |e| e.entry_hash.as_str())
                .to_owned();
            HttpResponse::json_ok(&json!({
                "status": "ok",
                "fayger_version": crate::FAYGER_VERSION,
                "audit_chain_verified": verified,
                "chain_head_hash": head,
            }))
        }
        ("GET", "/api/v1/state") => map_cli_res(engine.snapshot(true), false),
        ("GET", "/api/v1/host/profile") => HttpResponse::json_ok(&engine.device_profile(true)),
        ("GET", "/api/v1/host/pool") => HttpResponse::json_ok(engine.host_pool()),
        ("PUT" | "POST", "/api/v1/host/pool") => {
            let body = parse_json_body(&req.body);
            let caps = body
                .get("granted_capabilities")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<BTreeSet<String>>()
                });
            let ego_ep = body
                .get("ego_endpoint")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let ext_llm = body
                .get("external_llm_endpoint")
                .map(|v| v.as_str().map(str::to_owned));
            let net_online = body.get("network_online").and_then(Value::as_bool);
            map_cli_res(
                engine.update_host_pool(caps, ego_ep, ext_llm, net_online),
                false,
            )
        }
        ("GET", "/api/v1/wallet") => map_cli_res(show_wallet(engine.state_dir()), false),
        ("POST", "/api/v1/wallet/init") => {
            let body = parse_json_body(&req.body);
            let label = body
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Primary Human Vault");
            let seed = body.get("seed_phrase").and_then(Value::as_str);
            let force = body.get("force").and_then(Value::as_bool).unwrap_or(false);
            map_cli_res(init_wallet(engine.state_dir(), label, seed, force), true)
        }
        ("POST", "/api/v1/wallet/derive-ifay") => {
            let body = parse_json_body(&req.body);
            let name = body.get("name").and_then(Value::as_str).unwrap_or("");
            map_cli_res(derive_ifay(engine.state_dir(), name), true)
        }
        ("POST", "/api/v1/wallet/derive-cofay") => {
            let body = parse_json_body(&req.body);
            let name = body.get("name").and_then(Value::as_str).unwrap_or("");
            let role = body.get("role_title").and_then(Value::as_str).unwrap_or("");
            let scope = body.get("scope").and_then(Value::as_str).unwrap_or("");
            map_cli_res(derive_cofay(engine.state_dir(), name, role, scope), true)
        }
        ("POST", "/api/v1/wallet/dynamic-code") => {
            let body = parse_json_body(&req.body);
            let purpose = body
                .get("purpose")
                .and_then(Value::as_str)
                .unwrap_or("verify_with_fayid");
            let ttl = body.get("ttl_secs").and_then(Value::as_u64).unwrap_or(300);
            map_cli_res(issue_dynamic_code(engine.state_dir(), purpose, ttl), true)
        }
        ("POST", "/api/v1/wallet/exchange-grant") => {
            let body = parse_json_body(&req.body);
            let provider = body.get("provider").and_then(Value::as_str).unwrap_or("");
            let secret = body.get("secret").and_then(Value::as_str).unwrap_or("");
            let bound_fay_id = body.get("bound_fay_id").and_then(Value::as_str);
            let scopes = extract_string_vec(&body, "scopes");
            let ttl = body.get("ttl_secs").and_then(Value::as_u64).unwrap_or(3600);
            map_cli_res(
                exchange_auth_grant(
                    engine.state_dir(),
                    provider,
                    secret,
                    bound_fay_id,
                    scopes,
                    ttl,
                ),
                true,
            )
        }
        ("GET", "/api/v1/fays") => map_cli_res(engine.list_fays(), false),
        ("POST", "/api/v1/fays/create") => {
            let body = parse_json_body(&req.body);
            let kind_str = body.get("kind").and_then(Value::as_str).unwrap_or("ifay");
            let Ok(kind) = SoulKind::parse(kind_str) else {
                return HttpResponse::json_err(400, "invalid kind (expected 'ifay' or 'cofay')");
            };
            let name = body.get("name").and_then(Value::as_str).unwrap_or("");
            let caps = body
                .get("requested_capabilities")
                .map(|_| extract_string_vec(&body, "requested_capabilities"));
            map_cli_res(
                engine.create_and_load_soul(kind, name, caps.as_deref()),
                true,
            )
        }
        ("POST", "/api/v1/fays/load") => {
            let body = parse_json_body(&req.body);
            let path = body.get("path").and_then(Value::as_str).unwrap_or("");
            if path.trim().is_empty() {
                return HttpResponse::json_err(400, "missing 'path' to .buf package");
            }
            map_cli_res(engine.load_buf_from_path(path.trim()), true)
        }
        ("GET", "/api/v1/cap/approvals") => HttpResponse::json_ok(&engine.list_cap_approvals()),
        ("GET", "/api/v1/tp/sessions") => HttpResponse::json_ok(&engine.list_tp_sessions()),
        ("POST", "/api/v1/tp/sessions") => {
            let body = parse_json_body(&req.body);
            let initiator = body
                .get("initiator_fay_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let peer = body
                .get("peer_fay_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let topic = body.get("topic").and_then(Value::as_str).unwrap_or("");
            let perms = extract_string_vec(&body, "permissions");
            map_cli_res(
                engine.create_tp_session(initiator, peer, topic, perms),
                true,
            )
        }
        ("GET", "/api/v1/merit/config") => HttpResponse::json_ok(&engine.merit_config()),
        ("POST" | "PUT", "/api/v1/merit/config") => {
            let body = parse_json_body(&req.body);
            let ep = body.get("gmchain_endpoint").and_then(Value::as_str);
            map_cli_res(engine.update_merit_config(ep), false)
        }
        ("GET", "/api/v1/audit") => {
            let (entries, verified) = engine.audit_log();
            HttpResponse::json_ok(&json!({
                "verified": verified,
                "count": entries.len(),
                "entries": entries,
            }))
        }
        _ => route_parameterized(req, &mut engine),
    }
}

#[allow(clippy::too_many_lines)]
fn route_parameterized(req: &HttpRequest, engine: &mut FayGerEngine) -> HttpResponse {
    let segs: Vec<&str> = req
        .path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    match (req.method.as_str(), segs.as_slice()) {
        ("GET", ["api", "v1", "fays", fay_id]) => {
            let id = percent_decode(fay_id);
            map_cli_res(engine.get_fay_view(&id), false)
        }
        ("GET", ["api", "v1", "fays", fay_id, "messages"]) => {
            let id = percent_decode(fay_id);
            match engine.get_fay_view(&id) {
                Ok(v) => HttpResponse::json_ok(&v.chat_history),
                Err(e) => cli_err_to_http(&e),
            }
        }
        ("PUT" | "POST", ["api", "v1", "fays", fay_id, "policy"]) => {
            let id = percent_decode(fay_id);
            let body = parse_json_body(&req.body);
            let caps_list = if body.get("policy_capabilities").is_some() {
                extract_string_vec(&body, "policy_capabilities")
            } else {
                extract_string_vec(&body, "allowed_capabilities")
            };
            let set: BTreeSet<String> = caps_list.into_iter().collect();
            map_cli_res(engine.update_fay_policy(&id, &set), false)
        }
        ("POST", ["api", "v1", "fays", fay_id, "faying"]) => {
            let id = percent_decode(fay_id);
            let body = parse_json_body(&req.body);
            let grade = body.get("grade").and_then(Value::as_str);
            let ttl = body.get("ttl_secs").and_then(Value::as_u64);
            let hb = body.get("heartbeat_timeout_secs").and_then(Value::as_u64);
            map_cli_res(engine.activate_faying(&id, grade, ttl, hb), false)
        }
        ("POST", ["api", "v1", "fays", fay_id, "heartbeat"]) => {
            let id = percent_decode(fay_id);
            map_cli_res(engine.record_heartbeat(&id), false)
        }
        ("POST", ["api", "v1", "fays", fay_id, "emergency-brake"]) => {
            let id = percent_decode(fay_id);
            let body = parse_json_body(&req.body);
            let reason = body.get("reason").and_then(Value::as_str);
            map_cli_res(engine.trigger_emergency_brake(&id, reason), false)
        }
        ("POST", ["api", "v1", "fays", fay_id, "chat"]) => {
            let id = percent_decode(fay_id);
            let body = parse_json_body(&req.body);
            let message = body.get("message").and_then(Value::as_str).unwrap_or("");
            let explicit_action: Option<RequestedHostAction> = body
                .get("host_action")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            map_cli_res(engine.chat_or_act(&id, message, explicit_action), false)
        }
        ("GET", ["api", "v1", "fays", instance_id, "merit"]) => {
            let id = percent_decode(instance_id);
            map_cli_res(engine.get_fay_merit(&id), false)
        }
        ("POST", ["api", "v1", "fays", instance_id, "merit", "add"]) => {
            let id = percent_decode(instance_id);
            let body = parse_json_body(&req.body);
            let title = body
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let domain_chain_id = body
                .get("domain_chain_id")
                .and_then(Value::as_str)
                .unwrap_or("gmc-open-science")
                .to_owned();
            let Some(amount_mu) = body.get("amount_mu").and_then(Value::as_i64).or_else(|| {
                body.get("amount_mu")
                    .and_then(Value::as_str)
                    .and_then(|s| s.trim().parse::<i64>().ok())
            }) else {
                return HttpResponse::json_err(400, "missing or invalid 'amount_mu' integer");
            };
            let horizon_days = body
                .get("horizon_days")
                .and_then(Value::as_u64)
                .unwrap_or(90);
            let summary = body
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let input = MeritAddInput {
                title,
                domain_chain_id,
                amount_mu,
                horizon_days,
                summary,
            };
            map_cli_res(engine.add_fay_merit(&id, &input), false)
        }
        ("POST", ["api", "v1", "fays", instance_id, "merit", "consume"]) => {
            let id = percent_decode(instance_id);
            let body = parse_json_body(&req.body);
            let priority_kind = body
                .get("priority_kind")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let Some(amount_val) = body.get("amount_mu") else {
                return HttpResponse::json_err(400, "missing 'amount_mu'");
            };
            let (amount_raw, amount_mu_wire) = match parse_positive_mu_to_raw(amount_val) {
                Ok(pair) => pair,
                Err(err) => return cli_err_to_http(&err),
            };
            let reason = body
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let proposal_id = body
                .get("proposal_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let input = MeritConsumeInput {
                priority_kind,
                amount_raw,
                amount_mu_wire,
                reason,
                proposal_id,
            };
            map_cli_res(engine.consume_fay_merit(&id, &input), false)
        }
        ("POST", ["api", "v1", "fays", instance_id, "merit", "credential"]) => {
            let id = percent_decode(instance_id);
            let body = parse_json_body(&req.body);
            let privacy_tier = match body.get("privacy_tier") {
                Some(v) => v.as_str().unwrap_or(""),
                None => "L1_PUBLIC",
            };
            map_cli_res(
                engine.present_fay_merit_credential(&id, privacy_tier),
                false,
            )
        }
        ("POST", ["api", "v1", "cap", "approvals", approval_id, "decide"]) => {
            let id = percent_decode(approval_id);
            let body = parse_json_body(&req.body);
            let approve = body
                .get("approve")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| {
                    body.get("decision")
                        .and_then(Value::as_str)
                        .is_some_and(|d| {
                            d.eq_ignore_ascii_case("approve") || d.eq_ignore_ascii_case("allow")
                        })
                });
            map_cli_res(engine.decide_cap_approval(&id, approve), false)
        }
        ("POST", ["api", "v1", "tp", "sessions", session_id, "share"]) => {
            let sid = percent_decode(session_id);
            let body = parse_json_body(&req.body);
            let author = body
                .get("author_fay_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let category = body
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("memory_fact");
            let key = body.get("key").and_then(Value::as_str).unwrap_or("");
            let structured_value = body
                .get("structured_value")
                .cloned()
                .unwrap_or_else(|| json!({}));
            map_cli_res(
                engine.share_tp_entry(&sid, author, category, key, structured_value),
                true,
            )
        }
        ("POST", ["api", "v1", "tp", "sessions", session_id, "close"]) => {
            let sid = percent_decode(session_id);
            map_cli_res(engine.close_tp_session(&sid), false)
        }
        _ => HttpResponse::json_err(
            404,
            &format!("unknown endpoint: {} {}", req.method, req.path),
        ),
    }
}

fn parse_json_body(raw: &[u8]) -> Value {
    if raw.is_empty() {
        return json!({});
    }
    serde_json::from_slice(raw).unwrap_or_else(|_| json!({}))
}

fn extract_string_vec(body: &Value, key: &str) -> Vec<String> {
    body.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn map_cli_res<T: Serialize>(res: Result<T, CliError>, created: bool) -> HttpResponse {
    match res {
        Ok(val) => {
            if created {
                HttpResponse::json_created(&val)
            } else {
                HttpResponse::json_ok(&val)
            }
        }
        Err(err) => cli_err_to_http(&err),
    }
}

fn cli_err_to_http(err: &CliError) -> HttpResponse {
    match err {
        CliError::Usage(msg) => HttpResponse::json_err(400, msg),
        CliError::Failure(fe) => {
            let status = if fe.code == fayger_error::ErrorCode::AdpCapabilityDenied {
                403
            } else {
                400
            };
            HttpResponse::json_err(status, &fe.to_string())
        }
        CliError::Io { path, detail } => {
            HttpResponse::json_err(400, &format!("io error on {path}: {detail}"))
        }
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
