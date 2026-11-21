//! `ICP`（`Conception Annotation` `.ctx`）多端自适应消息信封与 `Ego` 快慢双系统三合一客户端。
//!
//! 本模块实现：
//! 1. **`ICP` `.ctx` 1.0 结构化信封（[`CtxEnvelope`]）**：在 Web 控制台渲染为富交互卡片、键值表与
//!    `CAP` 实时审批按钮；在命令行终端通过 [`render_ctx_terminal`] 自适应无损降级为清晰结构化文本；
//! 2. **`Ego` 三合一标准接口客户端（[`run_ego_three_in_one`]）**：当本机或远端配置了在线 `Ego` 服务
//!    （默认 `http://127.0.0.1:7980/v1/{intuition,dynamic-context,guard-rewrite}`）时自动走真实三合一调用；
//!    当 `Ego` 服务离线时自动无缝回退到确定性本地直觉与红线把关，保证 `fayger up` 100% 独立可用。

use crate::device_profile::parse_http_host_port;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// `ICP` `.ctx` 信封 schema 版本。
pub const ICP_CTX_SCHEMA_VERSION: &str = "icp.ctx/1.0";

/// `Ego` 三合一推理链路追踪摘要。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EgoPipelineTrace {
    /// 实际运行模式：`"ego_remote_3in1"` 或 `"local_deterministic_fallback"`。
    pub mode: String,
    /// 阶段 ① 直觉快反识别出的意图标签。
    pub intuition_intent: String,
    /// 阶段 ① 直觉置信度 `[0.0, 1.0]`。
    pub intuition_confidence: f64,
    /// 阶段 ② 注入的主人红线与动态上下文摘要。
    pub dynamic_context_summary: String,
    /// 阶段 ③ 输出把关改写是否介入。
    pub guard_rewrite_applied: bool,
}

/// `.ctx` 键值表行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CtxKeyValue {
    /// 字段键名。
    pub key: String,
    /// 字段取值。
    pub value: String,
}

/// `ICP` `.ctx` 富交互观念标注节点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CtxAnnotation {
    /// 状态徽章。
    StatusBadge {
        /// 标签名。
        label: String,
        /// 状态值。
        value: String,
        /// 视觉语义：`ok` / `warn` / `danger` / `info`。
        tone: String,
    },
    /// 结构化键值表格。
    KvTable {
        /// 表格标题。
        title: String,
        /// 行列表。
        rows: Vec<CtxKeyValue>,
    },
    /// 受控沙箱产物/文件预览卡片。
    ArtifactPreview {
        /// 卡片标题。
        title: String,
        /// 沙箱内相对路径。
        path: String,
        /// 内容摘要预览。
        content_preview: String,
        /// 内容 SHA-256 摘要。
        sha256: String,
    },
    /// 高风险 `CAP` 宿主操作审批卡片（Web 端渲染 Allow / Deny 按钮，CLI 端降级为审批指引）。
    CapApprovalCard {
        /// 待审批单号（`apr_...`）。
        approval_id: String,
        /// 发起请求的 `Fay` 标识。
        fay_id: String,
        /// 请求的宿主驱动操作名。
        operation: String,
        /// 操作目标摘要。
        target: String,
        /// 风险等级（`high` / `critical`）。
        criticality: String,
        /// 拦截原因说明。
        reason: String,
        /// 卡片状态：`pending` / `approved` / `denied` / `revoked_g0`。
        status: String,
    },
    /// 警示横幅（如 `Rogue Fay` 停机拦截提示）。
    AlertBanner {
        /// 警示级别：`warning` / `error` / `info`。
        level: String,
        /// 标题。
        title: String,
        /// 详细说明。
        detail: String,
    },
    /// `MeriToken`（`美誉值`, `μ`）实时账户与可消费优先权状态卡片。
    MeritStandingCard {
        /// 所属 `fay_id`。
        fay_id: String,
        /// 当前实时美誉值（如 `"45.00 μ"`）。
        cur_merit_mu: String,
        /// 可消费活跃池 `curMerit - minMerit`（如 `"38.28 μ"`）。
        consumable_merit_mu: String,
        /// 历史贡献保底值 `minMerit`（如 `"6.72 μ"`，只增不减）。
        min_merit_floor_mu: String,
        /// 同步模式（`"gmchain_live_rpc"` 或 `"local_pocket_fallback"`）。
        sync_mode: String,
        /// 最新公誉链区块高度。
        block_height: u64,
        /// 最新状态根十六进制。
        state_root_hex: String,
    },
}

/// `ICP` `.ctx` 多端自适应消息信封。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CtxEnvelope {
    /// 固定为 [`ICP_CTX_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 消息唯一标识（`ctx_...`）。
    pub message_id: String,
    /// 秒级 Unix 时间戳。
    pub timestamp_unix: u64,
    /// 响应所属 `Fay` 标识。
    pub fay_id: String,
    /// `ifay` 或 `cofay`。
    pub fay_kind: String,
    /// 产生该消息时的上岗状态（`ROGUE` 或 `FAYING`）。
    pub faying_state: String,
    /// 核心人类可读回复正文。
    pub summary_text: String,
    /// `Ego` 三合一推理链路追踪。
    pub ego_trace: EgoPipelineTrace,
    /// 结构化 `.ctx` 标注组件列表。
    pub annotations: Vec<CtxAnnotation>,
}

/// 将 `ICP` `.ctx` 富交互信封优雅降级渲染为适合命令行终端的纯文本视图。
#[must_use]
pub fn render_ctx_terminal(env: &CtxEnvelope) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "┌─ ICP .ctx [{}] · {} ({}) · state={}",
        env.message_id, env.fay_id, env.fay_kind, env.faying_state
    );
    let _ = writeln!(
        out,
        "│ Ego: {} | intent={} (conf={:.2}) | guard_rewrite={}",
        env.ego_trace.mode,
        env.ego_trace.intuition_intent,
        env.ego_trace.intuition_confidence,
        env.ego_trace.guard_rewrite_applied
    );
    let _ = writeln!(out, "│ {}", env.summary_text);

    for ann in &env.annotations {
        match ann {
            CtxAnnotation::StatusBadge { label, value, tone } => {
                let _ = writeln!(out, "│ [BADGE:{tone}] {label}: {value}");
            }
            CtxAnnotation::KvTable { title, rows } => {
                let _ = writeln!(out, "│ ── {title} ──");
                for row in rows {
                    let _ = writeln!(out, "│   {:<22} : {}", row.key, row.value);
                }
            }
            CtxAnnotation::ArtifactPreview {
                title,
                path,
                content_preview,
                sha256,
            } => {
                let _ = writeln!(
                    out,
                    "│ [ARTIFACT] {title} ({path}) sha256={}",
                    &sha256[..12.min(sha256.len())]
                );
                let _ = writeln!(out, "│   {content_preview}");
            }
            CtxAnnotation::CapApprovalCard {
                approval_id,
                operation,
                target,
                criticality,
                reason,
                status,
                ..
            } => {
                let _ = writeln!(
                    out,
                    "│ [CAP APPROVAL CARD · {status}] id={approval_id} op={operation} target={target} criticality={criticality}"
                );
                let _ = writeln!(out, "│   reason: {reason}");
                if status == "pending" {
                    let _ = writeln!(
                        out,
                        "│   action: approve/deny in FayGer Dashboard or POST /api/v1/cap/approvals/{approval_id}/decide"
                    );
                }
            }
            CtxAnnotation::AlertBanner {
                level,
                title,
                detail,
            } => {
                let _ = writeln!(out, "│ [!{}: {title}] {detail}", level.to_uppercase());
            }
            CtxAnnotation::MeritStandingCard {
                fay_id,
                cur_merit_mu,
                consumable_merit_mu,
                min_merit_floor_mu,
                sync_mode,
                block_height,
                state_root_hex,
            } => {
                let short_root = &state_root_hex[..12.min(state_root_hex.len())];
                let _ = writeln!(
                    out,
                    "│ [MERITOKEN STANDING · {sync_mode}] fay={fay_id} height=#{block_height} root={short_root}…"
                );
                let _ = writeln!(out, "│   curMerit(t)                : {cur_merit_mu}");
                let _ = writeln!(
                    out,
                    "│   consumable (cur - min)     : {consumable_merit_mu}"
                );
                let _ = writeln!(out, "│   minMerit floor (protected) : {min_merit_floor_mu}");
            }
        }
    }
    let _ = writeln!(
        out,
        "└────────────────────────────────────────────────────────"
    );
    out
}

/// `Ego` 三合一推理输出（供 `FayGer` 引擎组装回复与执行把关）。
#[derive(Debug, Clone)]
pub struct EgoThreeInOneResult {
    /// 推理追踪摘要。
    pub trace: EgoPipelineTrace,
    /// 经阶段 ① 直觉与阶段 ③ 红线把关润色后的建议回复文本。
    pub guarded_reply: String,
}

/// 调用 `Ego` 快慢双系统三合一接口（`/v1/intuition` → `/v1/dynamic-context` → `/v1/guard-rewrite`），
/// 若 `ego_endpoint` 不可达则自动降级为确定性本地实现。
pub fn run_ego_three_in_one(
    ego_endpoint: &str,
    fay_id: &str,
    display_name: &str,
    is_cofay: bool,
    user_message: &str,
    effective_caps: &[String],
    draft_outcome_summary: &str,
) -> EgoThreeInOneResult {
    let caps_joined = if effective_caps.is_empty() {
        "none".to_owned()
    } else {
        effective_caps.join(",")
    };

    // coFay 使用公开 RoleCharter 驱动，不挂载私人 Ego，但仍执行统一边界把关。
    if is_cofay {
        let (guarded, rewritten) = apply_local_redline_guard(draft_outcome_summary);
        return EgoThreeInOneResult {
            trace: EgoPipelineTrace {
                mode: "cofay_role_charter_guard".to_owned(),
                intuition_intent: classify_local_intent(user_message).to_owned(),
                intuition_confidence: 0.98,
                dynamic_context_summary: format!(
                    "RoleCharter seat={display_name} ({fay_id}) | effective_caps=[{caps_joined}]"
                ),
                guard_rewrite_applied: rewritten,
            },
            guarded_reply: guarded,
        };
    }

    // 尝试在线连接本地/远端 Ego 三合一服务
    if let Some(remote_res) = try_remote_ego_3in1(
        ego_endpoint,
        fay_id,
        display_name,
        user_message,
        &caps_joined,
        draft_outcome_summary,
    ) {
        return remote_res;
    }

    // 离线确定性本地 System-1 + 红线把关回退
    let intent = classify_local_intent(user_message);
    let (guarded, rewritten) = apply_local_redline_guard(draft_outcome_summary);
    EgoThreeInOneResult {
        trace: EgoPipelineTrace {
            mode: "local_deterministic_fallback".to_owned(),
            intuition_intent: intent.to_owned(),
            intuition_confidence: 0.91,
            dynamic_context_summary: format!(
                "Owner persona={display_name} | redlines=[hide_raw_hid, cap_intersection_strict] | caps=[{caps_joined}]"
            ),
            guard_rewrite_applied: rewritten,
        },
        guarded_reply: guarded,
    }
}

fn try_remote_ego_3in1(
    ego_endpoint: &str,
    fay_id: &str,
    display_name: &str,
    user_message: &str,
    caps_joined: &str,
    draft_outcome_summary: &str,
) -> Option<EgoThreeInOneResult> {
    let intuition_val = http_post_json(
        ego_endpoint,
        "/v1/intuition",
        &json!({
            "fay_id": fay_id,
            "prompt": user_message,
            "message": user_message,
        }),
    )?;

    let intent = intuition_val
        .get("intent")
        .or_else(|| intuition_val.get("route"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| classify_local_intent(user_message))
        .to_owned();
    let confidence = intuition_val
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.94);

    let dyn_ctx_val = http_post_json(
        ego_endpoint,
        "/v1/dynamic-context",
        &json!({
            "fay_id": fay_id,
            "task": user_message,
            "prompt": user_message,
            "effective_capabilities": caps_joined,
        }),
    );
    let dyn_summary = dyn_ctx_val
        .as_ref()
        .and_then(|v| {
            v.get("dynamic_context")
                .or_else(|| v.get("context"))
                .or_else(|| v.get("system_prompt"))
                .and_then(Value::as_str)
        })
        .map_or_else(
            || format!("Remote Ego dynamic context for {display_name} | caps=[{caps_joined}]"),
            str::to_owned,
        );

    let guard_val = http_post_json(
        ego_endpoint,
        "/v1/guard-rewrite",
        &json!({
            "fay_id": fay_id,
            "draft": draft_outcome_summary,
            "candidate_output": draft_outcome_summary,
        }),
    );

    let (final_text, guard_applied) = guard_val.map_or_else(
        || apply_local_redline_guard(draft_outcome_summary),
        |gv| {
            let candidate = gv
                .get("rewritten")
                .or_else(|| gv.get("output"))
                .or_else(|| gv.get("final_output"))
                .and_then(Value::as_str)
                .unwrap_or(draft_outcome_summary);
            let remote_flag = gv
                .get("rewritten_applied")
                .or_else(|| gv.get("modified"))
                .and_then(Value::as_bool)
                .unwrap_or(candidate != draft_outcome_summary);
            let (locally_scrubbed, local_flag) = apply_local_redline_guard(candidate);
            (locally_scrubbed, remote_flag || local_flag)
        },
    );

    Some(EgoThreeInOneResult {
        trace: EgoPipelineTrace {
            mode: "ego_remote_3in1".to_owned(),
            intuition_intent: intent,
            intuition_confidence: confidence,
            dynamic_context_summary: dyn_summary,
            guard_rewrite_applied: guard_applied,
        },
        guarded_reply: final_text,
    })
}

fn http_post_json(base_endpoint: &str, path: &str, body: &Value) -> Option<Value> {
    let host_port = parse_http_host_port(base_endpoint)?;
    let addrs: Vec<SocketAddr> = host_port.to_socket_addrs().ok()?.collect();
    let addr = addrs.first()?;
    let mut stream = TcpStream::connect_timeout(addr, Duration::from_millis(150)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));

    let body_bytes = serde_json::to_vec(body).ok()?;
    let req_head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(req_head.as_bytes()).ok()?;
    stream.write_all(&body_bytes).ok()?;
    let _ = stream.flush();

    let mut raw_resp = Vec::new();
    stream.read_to_end(&mut raw_resp).ok()?;
    let resp_str = String::from_utf8(raw_resp).ok()?;
    let (headers, payload) = resp_str.split_once("\r\n\r\n")?;
    let status_line = headers.lines().next()?;
    if !status_line.contains(" 200 ") {
        return None;
    }
    serde_json::from_str(payload.trim()).ok()
}

/// 判断对话消息是否在询问或提及 `MeriToken`（`美誉` / `merit` / `优先权` / `μ`）。
#[must_use]
pub fn message_mentions_merit(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    msg.contains("美誉")
        || msg.contains("优先权")
        || msg.contains('μ')
        || lower.contains("merit")
        || lower.contains("priority")
}

fn classify_local_intent(msg: &str) -> &'static str {
    let trimmed = msg.trim();
    if trimmed.starts_with("/write ") || trimmed.contains("fs.write") {
        "host_fs_write"
    } else if trimmed.starts_with("/read ") || trimmed.contains("fs.read") {
        "host_fs_read"
    } else if trimmed.starts_with("/exec ") || trimmed.contains("process.exec") {
        "host_process_exec"
    } else if trimmed.starts_with("/camera") || trimmed.contains("camera") {
        "host_camera_snapshot"
    } else if message_mentions_merit(trimmed) {
        "merit_standing_query"
    } else if trimmed.contains("tp") || trimmed.contains("telepathy") {
        "tp_collaboration_query"
    } else {
        "system1_direct_dialogue"
    }
}

fn apply_local_redline_guard(draft: &str) -> (String, bool) {
    if draft.contains("sk-") {
        let scrubbed = draft.replace("sk-", "[REDACTED_LEGACY_SECRET_PREFIX]-");
        return (scrubbed, true);
    }
    (draft.to_owned(), false)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        message_mentions_merit, render_ctx_terminal, run_ego_three_in_one, CtxAnnotation,
        CtxEnvelope, ICP_CTX_SCHEMA_VERSION,
    };

    #[test]
    fn offline_ego_falls_back_cleanly_and_renders_terminal_ctx() {
        let ego_res = run_ego_three_in_one(
            "http://127.0.0.1:1",
            "ifay_abcdefabcdefabcdefabcdefab",
            "Alice",
            false,
            "Hello with sk-12345",
            &["fs.read".to_owned()],
            "Draft containing sk-12345 token",
        );
        assert_eq!(ego_res.trace.mode, "local_deterministic_fallback");
        assert!(ego_res.trace.guard_rewrite_applied);
        assert!(!ego_res.guarded_reply.contains("sk-12345"));

        let env = CtxEnvelope {
            schema_version: ICP_CTX_SCHEMA_VERSION.to_owned(),
            message_id: "ctx_001".to_owned(),
            timestamp_unix: 1_763_600_000,
            fay_id: "ifay_abcdefabcdefabcdefabcdefab".to_owned(),
            fay_kind: "ifay".to_owned(),
            faying_state: "FAYING".to_owned(),
            summary_text: ego_res.guarded_reply,
            ego_trace: ego_res.trace,
            annotations: vec![
                CtxAnnotation::CapApprovalCard {
                    approval_id: "apr_01".to_owned(),
                    fay_id: "ifay_abcdefabcdefabcdefabcdefab".to_owned(),
                    operation: "process.exec".to_owned(),
                    target: "sandbox.ls".to_owned(),
                    criticality: "high".to_owned(),
                    reason: "High criticality host command requires Human Prime approval"
                        .to_owned(),
                    status: "pending".to_owned(),
                },
                CtxAnnotation::MeritStandingCard {
                    fay_id: "ifay_abcdefabcdefabcdefabcdefab".to_owned(),
                    cur_merit_mu: "33.00 μ".to_owned(),
                    consumable_merit_mu: "26.28 μ".to_owned(),
                    min_merit_floor_mu: "6.72 μ".to_owned(),
                    sync_mode: "local_pocket_fallback".to_owned(),
                    block_height: 2,
                    state_root_hex:
                        "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234"
                            .to_owned(),
                },
            ],
        };
        let term = render_ctx_terminal(&env);
        assert!(term.contains("[CAP APPROVAL CARD · pending]"));
        assert!(term.contains("apr_01"));
        assert!(term.contains("[MERITOKEN STANDING · local_pocket_fallback]"));
        assert!(term.contains("33.00 μ"));
        assert!(message_mentions_merit("我的美誉值还有多少？"));
        assert!(message_mentions_merit("Check my merit standing"));
        assert!(message_mentions_merit("投票优先权额度"));
        assert!(message_mentions_merit("Balance in μ"));
    }
}
