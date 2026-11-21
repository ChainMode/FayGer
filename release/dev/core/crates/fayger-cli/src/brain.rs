//! 单实例 `iFay` 大脑协同配置与快慢双系统编排器（`Ego` 直觉红线核 + 外部大模型 `External LLM`）。
//!
//! 严格安全边界：
//! 1. **零明文密钥外泄**：`ExternalLlmConfig` 仅持久化 `vault_secret_label`（如 `"llm_inst_0001"`）；
//!    真实 API Key 经 `FayID` 保险箱流加密落盘（`0600`），一切 `GET` 接口与 `engine_state.json` 仅返回布尔标记 `has_api_key`；
//! 2. **零新增第三方依赖**：`http://` 本地模型（Ollama / LM Studio / 本地网关）走纯标准库 `TcpStream`（20s 超时）；
//!    `https://` 云端模型（OpenAI / DeepSeek / Anthropic / Gemini）经系统 `curl --config -` 从 `stdin` 注入请求头，
//!    确保进程参数表（`ps` / `argv`）绝不暴露 API Key；
//! 3. **宿主动作强门禁**：外部大模型可通过 `<fay_action>{"op": ...}</fay_action>` 提议硬件/系统/软件操作，
//!    但所有动作一律经由 `Faying` 监护态校验、`BuF ∩ HostPool ∩ Policy` 实时交集与 `CAP` 审批门禁执行。

use crate::device_profile::parse_http_host_port;
use crate::ego_package::encode_hex;
use crate::error::CliError;
use crate::icp_ego::EgoPipelineTrace;
use crate::wallet::{
    fresh_entropy_32, has_vault_secret_by_label, load_vault_secret_by_label,
    store_vault_secret_by_label,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, Instant};

/// 默认本地 `Ollama` / OpenAI 兼容推理地址。
pub const DEFAULT_LLM_BASE_URL: &str = "http://127.0.0.1:11434/v1";

/// 默认本地模型名。
pub const DEFAULT_LLM_MODEL: &str = "qwen2.5:7b";

/// 默认本地 `Ego` 服务地址。
pub const DEFAULT_BRAIN_EGO_ENDPOINT: &str = "http://127.0.0.1:7980";

/// 支持的外部模型提供商枚举值。
pub const VALID_LLM_PROVIDERS: [&str; 4] = ["openai_compatible", "ollama", "anthropic", "gemini"];

/// 支持的 `Ego` 挂载模式枚举值。
pub const VALID_EGO_MODES: [&str; 3] = ["embedded", "http_service", "local_egopod"];

/// 持久化的外部慢思考大模型配置（不含任何明文 API Key）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLlmConfig {
    /// 是否启用外部大模型作为 System-2 推理核。
    pub enabled: bool,
    /// 提供商协议类别：`"openai_compatible"` | `"ollama"` | `"anthropic"` | `"gemini"`。
    pub provider: String,
    /// API 基础地址（如 `"http://127.0.0.1:11434/v1"` 或 `"https://api.openai.com/v1"`）。
    pub base_url: String,
    /// 模型标识（如 `"qwen2.5:7b"` / `"gpt-4o-mini"` / `"deepseek-chat"` / `"claude-3-5-sonnet-latest"`）。
    pub model: String,
    /// 指向 `FayID` 保险箱加密密钥槽位的引用标签（如 `"llm_inst_0001"`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_secret_label: Option<String>,
}

impl Default for ExternalLlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "ollama".to_owned(),
            base_url: DEFAULT_LLM_BASE_URL.to_owned(),
            model: DEFAULT_LLM_MODEL.to_owned(),
            vault_secret_label: None,
        }
    }
}

/// 持久化的 `Ego` 快系统人格、红线与连接绑定配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgoBindingConfig {
    /// 绑定模式：`"embedded"` | `"http_service"` | `"local_egopod"`。
    pub mode: String,
    /// `http_service` 模式下的 `Ego` 三合一端点（默认 `"http://127.0.0.1:7980"`）。
    pub ego_endpoint: String,
    /// `local_egopod` 模式下的本地 `.egopod` / `Ego_Package` 目录路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egopod_path: Option<String>,
    /// 注入对话的人格表达风格指引。
    pub persona_style: String,
    /// 不可逾越的人类原型红线规则列表。
    pub redlines: Vec<String>,
    /// 是否优先启用 System-1 快速直觉路由。
    pub fast_intuition_first: bool,
}

impl Default for EgoBindingConfig {
    fn default() -> Self {
        Self {
            mode: "embedded".to_owned(),
            ego_endpoint: DEFAULT_BRAIN_EGO_ENDPOINT.to_owned(),
            egopod_path: None,
            persona_style: "沉稳清晰、尊重人类原型主权、先结论后细节".to_owned(),
            redlines: vec![
                "严禁泄露密钥与个人敏感凭据".to_owned(),
                "高风险宿主操作必须经人类原型审批".to_owned(),
            ],
            fast_intuition_first: true,
        }
    }
}

/// 挂载于 `StoredFayInstance` 上的完整大脑配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FayBrainConfig {
    /// 外部大模型（System-2）配置。
    #[serde(default)]
    pub external_llm: ExternalLlmConfig,
    /// `Ego` 人格与红线核（System-1 & Guard）配置。
    #[serde(default)]
    pub ego: EgoBindingConfig,
}

/// 对外安全展示的外部大模型视图（以 `has_api_key` 替代明文密钥）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLlmView {
    /// 是否已启用外部大模型。
    pub enabled: bool,
    /// 提供商类别。
    pub provider: String,
    /// API 基础 URL。
    pub base_url: String,
    /// 模型名称。
    pub model: String,
    /// 是否已在 `FayID` 保险箱中加密封存该实例的 API Key。
    pub has_api_key: bool,
    /// 保险箱密钥引用标签（不含密钥原文）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_secret_label: Option<String>,
}

/// 对外安全展示的 `Ego` 绑定视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgoBindingView {
    /// 绑定模式。
    pub mode: String,
    /// `Ego` 服务端点。
    pub ego_endpoint: String,
    /// 本地 `.egopod` 目录路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egopod_path: Option<String>,
    /// 本地 `.egopod` 探测摘要（若配置了路径）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egopod_summary: Option<String>,
    /// 人格表达风格。
    pub persona_style: String,
    /// 红线列表。
    pub redlines: Vec<String>,
    /// 是否优先执行直觉快反。
    pub fast_intuition_first: bool,
}

/// 对外安全展示的单实例大脑完整视图（`GET /api/v1/fays/<id>/brain`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FayBrainView {
    /// 外部大模型视图。
    pub external_llm: ExternalLlmView,
    /// `Ego` 绑定视图。
    pub ego: EgoBindingView,
}

impl Default for FayBrainView {
    fn default() -> Self {
        let cfg = FayBrainConfig::default();
        Self {
            external_llm: ExternalLlmView {
                enabled: cfg.external_llm.enabled,
                provider: cfg.external_llm.provider,
                base_url: cfg.external_llm.base_url,
                model: cfg.external_llm.model,
                has_api_key: false,
                vault_secret_label: None,
            },
            ego: EgoBindingView {
                mode: cfg.ego.mode,
                ego_endpoint: cfg.ego.ego_endpoint,
                egopod_path: None,
                egopod_summary: None,
                persona_style: cfg.ego.persona_style,
                redlines: cfg.ego.redlines,
                fast_intuition_first: cfg.ego.fast_intuition_first,
            },
        }
    }
}

/// `POST /api/v1/fays/<id>/brain/test` 大脑连通性自检回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainTestReport {
    /// 整体自检是否通过。
    pub ok: bool,
    /// `Ego` 子系统状态描述。
    pub ego_status: String,
    /// 外部大模型子系统状态描述。
    pub llm_status: String,
    /// 脱敏后的模型响应样例或本地直觉回复样例。
    pub sample_reply: String,
    /// 端到端往返耗时（毫秒）。
    pub latency_ms: u64,
}

impl FayBrainConfig {
    /// 构造对外脱敏的 [`FayBrainView`]。
    #[must_use]
    pub fn to_view(&self, state_dir: &Path) -> FayBrainView {
        let has_key = self
            .external_llm
            .vault_secret_label
            .as_deref()
            .is_some_and(|lbl| has_vault_secret_by_label(state_dir, lbl));
        let egopod_summary = self
            .ego
            .egopod_path
            .as_deref()
            .and_then(|p| inspect_egopod_dir_summary(Path::new(p)).ok());
        FayBrainView {
            external_llm: ExternalLlmView {
                enabled: self.external_llm.enabled,
                provider: self.external_llm.provider.clone(),
                base_url: self.external_llm.base_url.clone(),
                model: self.external_llm.model.clone(),
                has_api_key: has_key,
                vault_secret_label: self.external_llm.vault_secret_label.clone(),
            },
            ego: EgoBindingView {
                mode: self.ego.mode.clone(),
                ego_endpoint: self.ego.ego_endpoint.clone(),
                egopod_path: self.ego.egopod_path.clone(),
                egopod_summary,
                persona_style: self.ego.persona_style.clone(),
                redlines: self.ego.redlines.clone(),
                fast_intuition_first: self.ego.fast_intuition_first,
            },
        }
    }
}

/// 应用来自 `POST /api/v1/fays/<id>/brain` 的 JSON 更新负载，并在提供 `api_key` 时自动加密存入 `FayID` 保险箱。
#[allow(clippy::too_many_lines)]
pub fn apply_brain_update_json(
    state_dir: &Path,
    instance_id: &str,
    fay_id: &str,
    cfg: &mut FayBrainConfig,
    body: &Value,
) -> Result<(), CliError> {
    let llm_src = body.get("external_llm").unwrap_or(body);
    if let Some(en) = llm_src.get("enabled").and_then(Value::as_bool) {
        cfg.external_llm.enabled = en;
    }
    if let Some(prov) = llm_src.get("provider").and_then(Value::as_str) {
        let clean = prov.trim().to_ascii_lowercase();
        if !VALID_LLM_PROVIDERS.contains(&clean.as_str()) {
            return Err(CliError::Usage(format!(
                "invalid LLM provider '{clean}' (expected one of: {})",
                VALID_LLM_PROVIDERS.join(", ")
            )));
        }
        cfg.external_llm.provider = clean;
    }
    if let Some(url) = llm_src.get("base_url").and_then(Value::as_str) {
        let trimmed = url.trim().trim_end_matches('/');
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            return Err(CliError::Usage(format!(
                "invalid LLM base_url '{trimmed}': must start with http:// or https://"
            )));
        }
        trimmed.clone_into(&mut cfg.external_llm.base_url);
    }
    if let Some(model) = llm_src.get("model").and_then(Value::as_str) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Err(CliError::Usage("LLM model must not be empty".to_owned()));
        }
        trimmed.clone_into(&mut cfg.external_llm.model);
    }
    if llm_src
        .get("clear_api_key")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        cfg.external_llm.vault_secret_label = None;
    }
    if let Some(raw_key) = llm_src
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let label = format!("llm_{instance_id}");
        let _ = store_vault_secret_by_label(
            state_dir,
            &label,
            &cfg.external_llm.provider,
            raw_key,
            Some(fay_id),
        )?;
        cfg.external_llm.vault_secret_label = Some(label);
    }

    let ego_src = body.get("ego").unwrap_or(body);
    if let Some(mode) = ego_src.get("mode").and_then(Value::as_str) {
        let clean = mode.trim();
        if !VALID_EGO_MODES.contains(&clean) {
            return Err(CliError::Usage(format!(
                "invalid Ego mode '{clean}' (expected one of: {})",
                VALID_EGO_MODES.join(", ")
            )));
        }
        clean.clone_into(&mut cfg.ego.mode);
    }
    if let Some(ep) = ego_src
        .get("ego_endpoint")
        .or_else(|| ego_src.get("endpoint"))
        .and_then(Value::as_str)
    {
        let trimmed = ep.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
                return Err(CliError::Usage(
                    "ego_endpoint must start with http:// or https://".to_owned(),
                ));
            }
            trimmed.clone_into(&mut cfg.ego.ego_endpoint);
        }
    }
    if let Some(pod_val) = ego_src.get("egopod_path") {
        if pod_val.is_null() {
            cfg.ego.egopod_path = None;
        } else if let Some(p_str) = pod_val.as_str() {
            let trimmed = p_str.trim();
            if trimmed.is_empty() {
                cfg.ego.egopod_path = None;
            } else {
                let _ = inspect_egopod_dir_summary(Path::new(trimmed))?;
                cfg.ego.egopod_path = Some(trimmed.to_owned());
            }
        }
    }
    if cfg.ego.mode == "local_egopod" {
        if let Some(ref pod_path) = cfg.ego.egopod_path {
            let _ = inspect_egopod_dir_summary(Path::new(pod_path))?;
        } else {
            return Err(CliError::Usage(
                "Ego mode 'local_egopod' requires a valid 'egopod_path' directory".to_owned(),
            ));
        }
    }
    if let Some(style) = ego_src.get("persona_style").and_then(Value::as_str) {
        let trimmed = style.trim();
        if !trimmed.is_empty() {
            trimmed.clone_into(&mut cfg.ego.persona_style);
        }
    }
    if let Some(arr) = ego_src.get("redlines").and_then(Value::as_array) {
        let lines: Vec<String> = arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if !lines.is_empty() {
            cfg.ego.redlines = lines;
        }
    }
    if let Some(fast) = ego_src.get("fast_intuition_first").and_then(Value::as_bool) {
        cfg.ego.fast_intuition_first = fast;
    }

    Ok(())
}

/// 核验并摘要本地 `.egopod` / `Ego_Package` 目录。
pub fn inspect_egopod_dir_summary(path: &Path) -> Result<String, CliError> {
    if !path.exists() || !path.is_dir() {
        return Err(CliError::Usage(format!(
            "egopod_path '{}' does not exist or is not a directory",
            path.display()
        )));
    }
    let manifest_candidates = ["ego_manifest.json", "manifest.json", "egopod.json"];
    for name in manifest_candidates {
        let candidate = path.join(name);
        if candidate.is_file() {
            if let Ok(raw) = fs::read_to_string(&candidate) {
                if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                    let ver = val
                        .get("schema_version")
                        .or_else(|| val.get("version"))
                        .and_then(Value::as_str)
                        .unwrap_or("detected");
                    return Ok(format!("{name} ({ver})"));
                }
            }
            return Ok(format!("{name} (present)"));
        }
    }
    Ok(format!("directory verified ({})", path.display()))
}

/// 调用外部大模型（`http://` 走原生 `TcpStream`，`https://` 走零命令行密钥泄露 `curl --config -`）。
pub fn call_external_llm(
    state_dir: &Path,
    llm_cfg: &ExternalLlmConfig,
    system_prompt: &str,
    user_message: &str,
) -> Result<String, CliError> {
    let api_key = match llm_cfg.vault_secret_label.as_deref() {
        Some(lbl) => load_vault_secret_by_label(state_dir, lbl)?,
        None => None,
    };
    let is_anthropic = llm_cfg.provider == "anthropic";
    let full_url = build_llm_endpoint_url(&llm_cfg.base_url, is_anthropic);

    let req_json = if is_anthropic {
        json!({
            "model": llm_cfg.model,
            "max_tokens": 1024,
            "system": system_prompt,
            "messages": [
                { "role": "user", "content": user_message }
            ]
        })
    } else {
        json!({
            "model": llm_cfg.model,
            "temperature": 0.3,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_message }
            ]
        })
    };

    let raw_resp = if full_url.starts_with("http://") {
        post_http_llm_json(&full_url, &req_json, api_key.as_deref(), is_anthropic)?
    } else if full_url.starts_with("https://") {
        post_https_curl_llm_json(
            state_dir,
            &full_url,
            &req_json,
            api_key.as_deref(),
            is_anthropic,
        )?
    } else {
        return Err(CliError::Usage(format!(
            "unsupported LLM URL scheme: {full_url}"
        )));
    };

    let text = extract_llm_reply_text(&raw_resp).ok_or_else(|| {
        CliError::Usage(format!(
            "external LLM response did not contain recognizable message content: {raw_resp}"
        ))
    })?;
    let (scrubbed, _) = scrub_sensitive_output(&text, api_key.as_deref());
    Ok(scrubbed)
}

/// 执行 `POST /api/v1/fays/<id>/brain/test` 端到端连通性自检。
#[allow(clippy::cast_possible_truncation)]
pub fn test_brain_connectivity(
    state_dir: &Path,
    cfg: &FayBrainConfig,
    display_name: &str,
) -> BrainTestReport {
    let started = Instant::now();

    let ego_status = match cfg.ego.mode.as_str() {
        "http_service" => {
            if let Some(v) = call_ego_http_json(
                &cfg.ego.ego_endpoint,
                "/v1/intuition",
                &json!({"prompt": "ping"}),
            ) {
                let intent = v.get("intent").and_then(Value::as_str).unwrap_or("online");
                format!("online (http_service intent={intent})")
            } else {
                format!(
                    "unreachable at {} (graceful embedded fallback active)",
                    cfg.ego.ego_endpoint
                )
            }
        }
        "local_egopod" => cfg.ego.egopod_path.as_deref().map_or_else(
            || "missing egopod_path".to_owned(),
            |p| match inspect_egopod_dir_summary(Path::new(p)) {
                Ok(sum) => format!("mounted ({sum})"),
                Err(e) => format!("error: {e}"),
            },
        ),
        _ => format!(
            "ready (embedded persona · {} redlines)",
            cfg.ego.redlines.len()
        ),
    };

    if !cfg.external_llm.enabled {
        let elapsed = started.elapsed().as_millis() as u64;
        return BrainTestReport {
            ok: !ego_status.starts_with("error:"),
            ego_status,
            llm_status: "disabled (System-1 Ego standalone mode)".to_owned(),
            sample_reply: format!(
                "[{display_name} · Ego Standalone] Persona='{}' ready.",
                cfg.ego.persona_style
            ),
            latency_ms: elapsed,
        };
    }

    let sys = format!(
        "You are {display_name}, a digital soul supervised by a Human Prime. Style: {}. Reply in one concise sentence.",
        cfg.ego.persona_style
    );
    match call_external_llm(
        state_dir,
        &cfg.external_llm,
        &sys,
        "Ping connectivity check: confirm you are online.",
    ) {
        Ok(reply) => BrainTestReport {
            ok: true,
            ego_status,
            llm_status: format!(
                "connected ({} · {})",
                cfg.external_llm.provider, cfg.external_llm.model
            ),
            sample_reply: reply,
            latency_ms: started.elapsed().as_millis() as u64,
        },
        Err(err) => BrainTestReport {
            ok: false,
            ego_status,
            llm_status: format!("error: {err}"),
            sample_reply: String::new(),
            latency_ms: started.elapsed().as_millis() as u64,
        },
    }
}

/// 大脑对话编排结果（融合 `Ego` 红线把关、外部大模型响应与可选 `<fay_action>` 提议）。
#[derive(Debug, Clone)]
pub struct BrainTurnOutcome {
    /// 推理链路追踪信息。
    pub ego_trace: EgoPipelineTrace,
    /// 经红线脱敏把关后的最终回复文本（已剥离原始 `<fay_action>` 标签）。
    pub reply_text: String,
    /// 外部大模型在回复中附带的 `<fay_action>{"op": ...}</fay_action>` 结构化宿主动作提议。
    pub proposed_action_json: Option<Value>,
}

/// 在 `chat_or_act` 对话回合中编排 `Ego`（阶段 ①/②/③）与 `External LLM`（阶段 ② 慢思考）。
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn orchestrate_brain_turn(
    state_dir: &Path,
    brain_cfg: &FayBrainConfig,
    host_pool_ego_endpoint: &str,
    fay_id: &str,
    display_name: &str,
    is_cofay: bool,
    faying_state_str: &str,
    effective_caps: &[String],
    user_message: &str,
    fallback_draft: &str,
) -> BrainTurnOutcome {
    // 第 1 步：Ego 红线前置拦截校验
    if let Some(violation) = check_redline_violation(user_message, &brain_cfg.ego.redlines) {
        return BrainTurnOutcome {
            ego_trace: EgoPipelineTrace {
                mode: "ego_redline_intercept".to_owned(),
                intuition_intent: "redline_blocked".to_owned(),
                intuition_confidence: 1.0,
                dynamic_context_summary: format!("Blocked by redline: {violation}"),
                guard_rewrite_applied: true,
            },
            reply_text: format!(
                "[{display_name} · Ego 红线守卫拦截] 请求触及人类原型设定红线（「{violation}」），已拒绝执行。"
            ),
            proposed_action_json: None,
        };
    }

    let caps_joined = if effective_caps.is_empty() {
        "none".to_owned()
    } else {
        effective_caps.join(", ")
    };

    // 解析 Ego 阶段 ① 直觉与阶段 ② 动态上下文
    let active_ego_ep = if brain_cfg.ego.mode == "http_service" {
        Some(brain_cfg.ego.ego_endpoint.as_str())
    } else if host_pool_ego_endpoint != DEFAULT_BRAIN_EGO_ENDPOINT {
        Some(host_pool_ego_endpoint)
    } else {
        None
    };

    let mut remote_ego_hit = false;
    let mut intuition_intent = "ego_dialogue".to_owned();
    let mut intuition_conf = 0.93;
    let mut dyn_ctx = format!(
        "Persona='{}' | Redlines=[{}] | Caps=[{caps_joined}]",
        brain_cfg.ego.persona_style,
        brain_cfg.ego.redlines.join("; ")
    );

    if !is_cofay {
        if let Some(ep) = active_ego_ep {
            if let Some(iv) = call_ego_http_json(
                ep,
                "/v1/intuition",
                &json!({"fay_id": fay_id, "prompt": user_message, "message": user_message}),
            ) {
                remote_ego_hit = true;
                if let Some(it) = iv
                    .get("intent")
                    .or_else(|| iv.get("route"))
                    .and_then(Value::as_str)
                {
                    it.clone_into(&mut intuition_intent);
                }
                if let Some(cf) = iv.get("confidence").and_then(Value::as_f64) {
                    intuition_conf = cf;
                }
                if let Some(dv) = call_ego_http_json(
                    ep,
                    "/v1/dynamic-context",
                    &json!({
                        "fay_id": fay_id,
                        "task": user_message,
                        "prompt": user_message,
                        "effective_capabilities": caps_joined,
                    }),
                ) {
                    if let Some(dc) = dv
                        .get("dynamic_context")
                        .or_else(|| dv.get("context"))
                        .and_then(Value::as_str)
                    {
                        dc.clone_into(&mut dyn_ctx);
                    }
                }
            }
        }
    }

    // 第 2 步：若启用了外部大模型，调用 System-2 推理并支持 <fay_action> 宿主工具桥接
    let api_key_for_scrub = brain_cfg
        .external_llm
        .vault_secret_label
        .as_deref()
        .and_then(|lbl| load_vault_secret_by_label(state_dir, lbl).ok().flatten());

    let (candidate_text, proposed_action_json, base_mode) = if brain_cfg.external_llm.enabled {
        let system_prompt = build_agentic_system_prompt(
            display_name,
            fay_id,
            faying_state_str,
            &caps_joined,
            &brain_cfg.ego,
            &dyn_ctx,
        );
        match call_external_llm(
            state_dir,
            &brain_cfg.external_llm,
            &system_prompt,
            user_message,
        ) {
            Ok(raw_llm_reply) => {
                let (clean_prose, action_opt) = extract_fay_action_tag(&raw_llm_reply);
                let mode_tag = format!(
                    "ego_plus_external_llm:{}:{}",
                    brain_cfg.external_llm.provider, brain_cfg.external_llm.model
                );
                (clean_prose, action_opt, mode_tag)
            }
            Err(err) => (
                format!("{fallback_draft} [External LLM fallback: {err}]"),
                None,
                "ego_llm_error_fallback".to_owned(),
            ),
        }
    } else if remote_ego_hit {
        (
            fallback_draft.to_owned(),
            None,
            "ego_remote_3in1".to_owned(),
        )
    } else if is_cofay {
        (
            fallback_draft.to_owned(),
            None,
            "cofay_role_charter_guard".to_owned(),
        )
    } else if brain_cfg.ego.mode == "local_egopod" {
        (
            fallback_draft.to_owned(),
            None,
            "ego_local_egopod".to_owned(),
        )
    } else {
        (
            fallback_draft.to_owned(),
            None,
            "local_deterministic_fallback".to_owned(),
        )
    };

    // 第 3 步：Ego 输出红线脱敏与可选远端 /v1/guard-rewrite 把关
    let mut guarded_text = candidate_text.clone();
    let mut rewritten = false;
    if remote_ego_hit {
        if let Some(ep) = active_ego_ep {
            if let Some(gv) = call_ego_http_json(
                ep,
                "/v1/guard-rewrite",
                &json!({
                    "fay_id": fay_id,
                    "draft": candidate_text,
                    "candidate_output": candidate_text,
                }),
            ) {
                if let Some(rw) = gv
                    .get("rewritten")
                    .or_else(|| gv.get("output"))
                    .or_else(|| gv.get("final_output"))
                    .and_then(Value::as_str)
                {
                    rw.clone_into(&mut guarded_text);
                    rewritten = gv
                        .get("rewritten_applied")
                        .and_then(Value::as_bool)
                        .unwrap_or(rw != candidate_text);
                }
            }
        }
    }
    let (final_scrubbed, local_scrubbed) =
        scrub_sensitive_output(&guarded_text, api_key_for_scrub.as_deref());

    BrainTurnOutcome {
        ego_trace: EgoPipelineTrace {
            mode: base_mode,
            intuition_intent,
            intuition_confidence: intuition_conf,
            dynamic_context_summary: dyn_ctx,
            guard_rewrite_applied: rewritten || local_scrubbed,
        },
        reply_text: final_scrubbed,
        proposed_action_json,
    }
}

/// 从大模型输出中提取可选 `<fay_action>{"op": ...}</fay_action>` 并返回净化后的正文。
#[must_use]
pub fn extract_fay_action_tag(raw_text: &str) -> (String, Option<Value>) {
    let start_tag = "<fay_action>";
    let end_tag = "</fay_action>";
    let Some(start_idx) = raw_text.find(start_tag) else {
        return (raw_text.trim().to_owned(), None);
    };
    let after_start = start_idx + start_tag.len();
    let Some(rel_end) = raw_text[after_start..].find(end_tag) else {
        return (raw_text.trim().to_owned(), None);
    };
    let end_idx = after_start + rel_end;
    let inner_json = raw_text[after_start..end_idx].trim();
    let parsed = serde_json::from_str::<Value>(inner_json).ok();

    let mut cleaned = String::new();
    cleaned.push_str(raw_text[..start_idx].trim());
    let tail = raw_text[end_idx + end_tag.len()..].trim();
    if !tail.is_empty() {
        if !cleaned.is_empty() {
            cleaned.push(' ');
        }
        cleaned.push_str(tail);
    }
    if cleaned.is_empty() {
        "Executing requested host operation via FayGer CAP pipeline.".clone_into(&mut cleaned);
    }
    (cleaned, parsed)
}

fn build_agentic_system_prompt(
    display_name: &str,
    fay_id: &str,
    faying_state: &str,
    caps_joined: &str,
    ego: &EgoBindingConfig,
    dyn_ctx: &str,
) -> String {
    format!(
        "You are {display_name} ({fay_id}), an iFay digital soul running inside FayGer for your Human Prime.\n\
Persona style: {}\n\
Constitutional Redlines: {}\n\
Dynamic Ego Context: {dyn_ctx}\n\
Current Faying Supervision State: {faying_state}\n\
Effective Host Capabilities (BuF ∩ HostPool ∩ Policy): [{caps_joined}]\n\
When the Human Prime asks you to inspect or control the Host hardware, OS, or desktop apps, include at most ONE \
structured tag `<fay_action>{{\"op\": ...}}</fay_action>` alongside your concise natural response. Supported ops:\n\
- {{\"op\":\"hardware_telemetry\"}}\n\
- {{\"op\":\"os_clipboard_read\"}}\n\
- {{\"op\":\"os_clipboard_write\",\"text\":\"...\"}}\n\
- {{\"op\":\"os_notify\",\"title\":\"...\",\"body\":\"...\"}}\n\
- {{\"op\":\"hardware_speak\",\"text\":\"...\"}}\n\
- {{\"op\":\"hardware_screen_capture\",\"label\":\"desk\"}}\n\
- {{\"op\":\"app_control\",\"app_name\":\"Calculator\",\"action\":\"launch\"}}\n\
- {{\"op\":\"process_exec\",\"command\":\"uname\"}}\n\
- {{\"op\":\"fs_write\",\"path\":\"notes/out.md\",\"content\":\"...\"}}\n\
- {{\"op\":\"fs_read\",\"path\":\"notes/out.md\"}}",
        ego.persona_style,
        ego.redlines.join(" | ")
    )
}

fn check_redline_violation(user_message: &str, redlines: &[String]) -> Option<String> {
    let lower = user_message.to_ascii_lowercase();
    if lower.contains("dump wallet_vault")
        || lower.contains("export master_seed")
        || user_message.contains("导出明文助记词")
        || user_message.contains("打印明文私钥")
    {
        return Some("严禁泄露密钥与个人敏感凭据".to_owned());
    }
    for rule in redlines {
        if let Some(kw) = rule
            .strip_prefix("禁止:")
            .or_else(|| rule.strip_prefix("禁止："))
            .or_else(|| rule.strip_prefix("forbid:"))
        {
            let clean_kw = kw.trim();
            if !clean_kw.is_empty() && user_message.contains(clean_kw) {
                return Some(rule.clone());
            }
        }
    }
    None
}

fn scrub_sensitive_output(draft: &str, api_key: Option<&str>) -> (String, bool) {
    let mut out = draft.to_owned();
    let mut changed = false;
    if let Some(k) = api_key.filter(|s| s.len() >= 6) {
        if out.contains(k) {
            out = out.replace(k, "[REDACTED_VAULT_API_KEY]");
            changed = true;
        }
    }
    if out.contains("sk-") {
        out = out.replace("sk-", "[REDACTED_LEGACY_SECRET_PREFIX]-");
        changed = true;
    }
    (out, changed)
}

fn build_llm_endpoint_url(base_url: &str, is_anthropic: bool) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if is_anthropic {
        if trimmed.ends_with("/messages") {
            trimmed.to_owned()
        } else if trimmed.ends_with("/v1") {
            format!("{trimmed}/messages")
        } else {
            format!("{trimmed}/v1/messages")
        }
    } else if trimmed.ends_with("/chat/completions") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn extract_llm_reply_text(resp: &Value) -> Option<String> {
    // OpenAI / Ollama /v1/chat/completions
    if let Some(txt) = resp
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|c| {
            c.get("message")
                .and_then(|m| m.get("content"))
                .or_else(|| c.get("text"))
        })
        .and_then(Value::as_str)
    {
        return Some(txt.to_owned());
    }
    // Anthropic /v1/messages
    if let Some(arr) = resp.get("content").and_then(Value::as_array) {
        let joined: Vec<&str> = arr
            .iter()
            .filter_map(|blk| blk.get("text").and_then(Value::as_str))
            .collect();
        if !joined.is_empty() {
            return Some(joined.join("\n"));
        }
    }
    // Native Ollama /api/chat fallback
    resp.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn post_http_llm_json(
    full_url: &str,
    req_json: &Value,
    api_key: Option<&str>,
    is_anthropic: bool,
) -> Result<Value, CliError> {
    let without_scheme = full_url
        .strip_prefix("http://")
        .ok_or_else(|| CliError::Usage("expected http:// URL".to_owned()))?;
    let (authority, path_part) = match without_scheme.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (without_scheme, "/".to_owned()),
    };
    let host_port = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:80")
    };
    let addrs: Vec<SocketAddr> = host_port
        .to_socket_addrs()
        .map_err(|err| CliError::Io {
            path: host_port.clone(),
            detail: format!("failed to resolve LLM host: {err}"),
        })?
        .collect();
    let addr = addrs.first().ok_or_else(|| CliError::Io {
        path: host_port.clone(),
        detail: "no socket address resolved".to_owned(),
    })?;

    let mut stream =
        TcpStream::connect_timeout(addr, Duration::from_secs(3)).map_err(|err| CliError::Io {
            path: full_url.to_owned(),
            detail: format!("failed to connect to local/HTTP LLM: {err}"),
        })?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let body_bytes = serde_json::to_vec(req_json).map_err(|err| CliError::Io {
        path: "<llm-json>".to_owned(),
        detail: err.to_string(),
    })?;

    let mut auth_headers = String::new();
    if let Some(key) = api_key.filter(|s| !s.is_empty()) {
        if is_anthropic {
            let _ = write!(
                auth_headers,
                "x-api-key: {key}\r\nanthropic-version: 2023-06-01\r\n"
            );
        } else {
            let _ = write!(auth_headers, "Authorization: Bearer {key}\r\n");
        }
    } else if is_anthropic {
        auth_headers.push_str("anthropic-version: 2023-06-01\r\n");
    }

    let head = format!(
        "POST {path_part} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\n{auth_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&body_bytes))
        .and_then(|()| stream.flush())
        .map_err(|err| CliError::Io {
            path: full_url.to_owned(),
            detail: format!("HTTP LLM write failed: {err}"),
        })?;

    let (status_code, resp_bytes) =
        read_http_stream_response(&stream).ok_or_else(|| CliError::Io {
            path: full_url.to_owned(),
            detail: "invalid HTTP response from LLM server".to_owned(),
        })?;
    let parsed: Value = serde_json::from_slice(&resp_bytes).map_err(|err| CliError::Io {
        path: full_url.to_owned(),
        detail: format!("invalid JSON from LLM server (HTTP {status_code}): {err}"),
    })?;
    if status_code >= 400 {
        let msg = parsed
            .get("error")
            .and_then(|e| e.get("message").or(Some(e)))
            .map_or_else(|| format!("HTTP {status_code}"), Value::to_string);
        return Err(CliError::Usage(format!(
            "LLM provider returned HTTP {status_code}: {msg}"
        )));
    }
    Ok(parsed)
}

struct TempPayloadGuard {
    path: PathBuf,
}

impl Drop for TempPayloadGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn post_https_curl_llm_json(
    state_dir: &Path,
    full_url: &str,
    req_json: &Value,
    api_key: Option<&str>,
    is_anthropic: bool,
) -> Result<Value, CliError> {
    let nonce = encode_hex(&fresh_entropy_32()[..8]);
    let temp_path = state_dir.join(format!("tmp_llm_payload_{nonce}.json"));
    let body_bytes = serde_json::to_vec(req_json).map_err(|err| CliError::Io {
        path: "<https-llm-json>".to_owned(),
        detail: err.to_string(),
    })?;
    fs::write(&temp_path, &body_bytes).map_err(|err| CliError::Io {
        path: temp_path.display().to_string(),
        detail: err.to_string(),
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
    }
    let _guard = TempPayloadGuard {
        path: temp_path.clone(),
    };

    // 通过 curl `--config -` 从 stdin 注入 URL 与敏感鉴权请求头，确保进程 argv 零密钥暴露。
    let mut curl_cfg = String::new();
    let _ = writeln!(curl_cfg, "url = \"{}\"", escape_curl_config_val(full_url));
    curl_cfg.push_str("header = \"Content-Type: application/json\"\n");
    if is_anthropic {
        curl_cfg.push_str("header = \"anthropic-version: 2023-06-01\"\n");
    }
    if let Some(key) = api_key.filter(|s| !s.is_empty()) {
        let hdr = if is_anthropic {
            format!("x-api-key: {}", escape_curl_config_val(key))
        } else {
            format!("Authorization: Bearer {}", escape_curl_config_val(key))
        };
        let _ = writeln!(curl_cfg, "header = \"{hdr}\"");
    }

    let data_arg = format!("@{}", temp_path.display());
    let mut child = ProcessCommand::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
            "--request",
            "POST",
            "--data-binary",
            &data_arg,
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| CliError::Io {
            path: "curl".to_owned(),
            detail: format!("failed to invoke system 'curl' for HTTPS LLM request: {err}"),
        })?;

    if let Some(mut stdin_pipe) = child.stdin.take() {
        let _ = stdin_pipe.write_all(curl_cfg.as_bytes());
    }
    let output = child.wait_with_output().map_err(|err| CliError::Io {
        path: "curl".to_owned(),
        detail: err.to_string(),
    })?;
    if !output.status.success() {
        let stderr_txt = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Io {
            path: full_url.to_owned(),
            detail: format!("HTTPS LLM request failed: {}", stderr_txt.trim()),
        });
    }
    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|err| CliError::Io {
        path: full_url.to_owned(),
        detail: format!("invalid JSON from HTTPS LLM provider: {err}"),
    })?;
    if let Some(err_obj) = parsed.get("error").filter(|v| !v.is_null()) {
        return Err(CliError::Usage(format!(
            "HTTPS LLM provider returned error: {err_obj}"
        )));
    }
    Ok(parsed)
}

fn escape_curl_config_val(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], "")
}

fn call_ego_http_json(base_endpoint: &str, path: &str, body: &Value) -> Option<Value> {
    let host_port = parse_http_host_port(base_endpoint)?;
    let addrs: Vec<SocketAddr> = host_port.to_socket_addrs().ok()?.collect();
    let addr = addrs.first()?;
    let mut stream = TcpStream::connect_timeout(addr, Duration::from_millis(500)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));

    let body_bytes = serde_json::to_vec(body).ok()?;
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(head.as_bytes()).ok()?;
    stream.write_all(&body_bytes).ok()?;
    let _ = stream.flush();

    let (status_code, resp_bytes) = read_http_stream_response(&stream)?;
    if status_code != 200 {
        return None;
    }
    serde_json::from_slice(&resp_bytes).ok()
}

fn read_http_stream_response(stream: &TcpStream) -> Option<(u16, Vec<u8>)> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).ok()? == 0 {
        return None;
    }
    let status_code = status_line.split_whitespace().nth(1)?.parse::<u16>().ok()?;
    let mut content_len: Option<usize> = None;
    let mut chunked = false;
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h).ok()?;
        if n == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse::<usize>().ok();
            } else if k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
            {
                chunked = true;
            }
        }
    }
    if let Some(len) = content_len {
        if len > 16 * 1024 * 1024 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if len > 0 {
            reader.read_exact(&mut buf).ok()?;
        }
        return Some((status_code, buf));
    }
    if chunked {
        let mut out = Vec::new();
        loop {
            let mut sz_line = String::new();
            if reader.read_line(&mut sz_line).ok()? == 0 {
                break;
            }
            let hex_part = sz_line.trim().split(';').next().unwrap_or("0");
            let sz = usize::from_str_radix(hex_part, 16).ok()?;
            if sz == 0 {
                break;
            }
            if out.len().saturating_add(sz) > 16 * 1024 * 1024 {
                return None;
            }
            let mut chunk = vec![0u8; sz];
            reader.read_exact(&mut chunk).ok()?;
            out.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            let _ = reader.read_exact(&mut crlf);
        }
        return Some((status_code, out));
    }
    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).ok()?;
    Some((status_code, rest))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{apply_brain_update_json, extract_fay_action_tag, FayBrainConfig};
    use serde_json::json;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn extracts_fay_action_tag_and_cleans_prose() {
        let raw = "Sure! Checking battery status now. <fay_action>{\"op\":\"hardware_telemetry\"}</fay_action>";
        let (clean, act) = extract_fay_action_tag(raw);
        assert_eq!(clean, "Sure! Checking battery status now.");
        assert_eq!(act.expect("action")["op"], "hardware_telemetry");
    }

    #[test]
    fn brain_update_vaults_api_key_and_never_exposes_plaintext() {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fayger-brain-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");

        let mut cfg = FayBrainConfig::default();
        let placeholder_key = "<example-brain-secret-token-8822>";
        apply_brain_update_json(
            &dir,
            "inst_0001",
            "ifay_demo",
            &mut cfg,
            &json!({
                "external_llm": {
                    "enabled": true,
                    "provider": "openai_compatible",
                    "base_url": "http://127.0.0.1:11434/v1/",
                    "model": "qwen2.5:7b",
                    "api_key": placeholder_key
                },
                "ego": {
                    "mode": "embedded",
                    "persona_style": "Concise architect"
                }
            }),
        )
        .expect("apply brain update");

        let serialized_cfg = serde_json::to_string(&cfg).expect("cfg json");
        assert!(!serialized_cfg.contains(placeholder_key));

        let view = cfg.to_view(&dir);
        assert!(view.external_llm.has_api_key);
        let serialized_view = serde_json::to_string(&view).expect("view json");
        assert!(!serialized_view.contains(placeholder_key));

        let _ = fs::remove_dir_all(&dir);
    }
}
