//! 内嵌离线可用的 `FayGer Dashboard` 单页可视化控制台（零外部 CDN 依赖）。
//!
//! 提供六大核心工作台标签页：
//! 1. 设备环境与资源池（`Overview & Device Profile`）
//! 2. 身份与凭据保险箱（`FayID Wallet & Auth Exchange`）
//! 3. 分身实例与上岗监护（`Running Fays & Faying Monitor` + 红色 `G0` 一键急刹 + `MeriToken` 胶囊）
//! 4. `ICP` 富交互对话与 `CAP` 审批（`ICP Interactive Console`）
//! 5. 双 `Fay` 心灵感应协作间（`Telepathy (TP) Shared Workspace`）
//! 6. 美誉值与优先权（`MeriToken & Priority` — `GMChain` 展示、贡献入账与优先权消减闭环）

/// 返回完整的 `FayGer Dashboard` 单页应用 HTML 文本。
#[must_use]
pub const fn dashboard_index_html() -> &'static str {
    DASHBOARD_HTML
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>FayGer Dashboard — Digital Soul Runtime Engine</title>
<style>
  :root {
    --bg: #0b1120;
    --panel: #111827;
    --panel-alt: #1e293b;
    --border: #334155;
    --text: #f1f5f9;
    --muted: #94a3b8;
    --accent: #38bdf8;
    --accent-hover: #0ea5e9;
    --ok: #10b981;
    --warn: #f59e0b;
    --danger: #ef4444;
    --danger-dark: #b91c1c;
    --mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family: var(--sans);
    line-height: 1.5;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    padding: 14px 24px;
    background: #0f172a;
    border-bottom: 1px solid var(--border);
    gap: 12px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .brand-logo {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: linear-gradient(135deg, #0284c7, #6366f1);
    display: grid;
    place-items: center;
    font-weight: 800;
    font-family: var(--mono);
  }
  .brand h1 {
    font-size: 18px;
    margin: 0;
  }
  .brand small {
    color: var(--muted);
    font-size: 12px;
  }
  .status-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .pill {
    font-size: 12px;
    font-family: var(--mono);
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--panel);
  }
  .pill.ok { border-color: var(--ok); color: #6ee7b7; }
  .pill.warn { border-color: var(--warn); color: #fcd34d; }
  .pill.danger { border-color: var(--danger); color: #fca5a5; }
  nav.tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 24px;
    background: #0f172a;
    border-bottom: 1px solid var(--border);
  }
  nav.tabs button {
    background: transparent;
    color: var(--muted);
    border: 1px solid transparent;
    padding: 8px 14px;
    border-radius: 8px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
  }
  nav.tabs button:hover {
    color: var(--text);
    background: var(--panel);
  }
  nav.tabs button.active {
    color: #0b1120;
    background: var(--accent);
  }
  main {
    max-width: 1360px;
    margin: 0 auto;
    padding: 20px 24px 60px;
  }
  .tab-pane { display: none; }
  .tab-pane.active { display: block; }
  .grid-2 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(380px, 1fr));
    gap: 16px;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 16px 18px;
    margin-bottom: 16px;
  }
  .card h2, .card h3 {
    margin: 0 0 10px 0;
    font-size: 16px;
  }
  .muted { color: var(--muted); font-size: 13px; }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    margin: 8px 0;
  }
  input[type="text"], input[type="password"], input[type="number"], select, textarea {
    background: #0b1120;
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 7px 10px;
    font-size: 13px;
    font-family: inherit;
  }
  textarea { width: 100%; min-height: 68px; font-family: var(--mono); }
  button.btn {
    background: var(--accent);
    color: #0b1120;
    border: none;
    border-radius: 7px;
    padding: 7px 14px;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  button.btn:hover { background: var(--accent-hover); }
  button.btn.secondary {
    background: var(--panel-alt);
    color: var(--text);
    border: 1px solid var(--border);
  }
  button.btn.ok { background: var(--ok); color: #052e16; }
  button.btn.brake {
    background: var(--danger);
    color: #fff;
    font-size: 13px;
    padding: 8px 16px;
    border: 2px solid #fca5a5;
    box-shadow: 0 0 14px rgba(239, 68, 68, 0.45);
  }
  button.btn.brake:hover { background: var(--danger-dark); }
  .cap-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-family: var(--mono);
    font-size: 12px;
    padding: 3px 9px;
    border-radius: 6px;
    background: #0b1120;
    border: 1px solid var(--border);
    margin: 2px;
  }
  .cap-chip.effective {
    border-color: var(--ok);
    color: #6ee7b7;
    background: rgba(16, 185, 129, 0.12);
  }
  pre.code {
    background: #070b14;
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    overflow-x: auto;
    font-family: var(--mono);
    font-size: 12px;
    margin: 8px 0 0;
  }
  .fay-card {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px;
    background: #0f172a;
    margin-bottom: 12px;
  }
  .fay-card.faying { border-left: 4px solid var(--ok); }
  .fay-card.rogue { border-left: 4px solid var(--danger); }
  .ctx-card {
    background: #0f172a;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 14px;
    margin-bottom: 12px;
  }
  .ctx-approval {
    border: 1px solid var(--warn);
    background: rgba(245, 158, 11, 0.08);
    border-radius: 8px;
    padding: 10px 12px;
    margin-top: 8px;
  }
  .ctx-alert {
    border: 1px solid var(--danger);
    background: rgba(239, 68, 68, 0.1);
    border-radius: 8px;
    padding: 10px 12px;
    margin-top: 8px;
  }
  table.kv {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
    font-size: 12px;
  }
  table.kv td, table.kv th {
    border: 1px solid var(--border);
    padding: 5px 8px;
    text-align: left;
    font-family: var(--mono);
  }
  .metric-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 12px;
    margin: 10px 0 14px;
  }
  .metric-box {
    background: #0f172a;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 14px;
  }
  .metric-box .metric-val {
    font-size: 24px;
    font-weight: 800;
    font-family: var(--mono);
    margin: 4px 0;
  }
  .merit-pill {
    border-color: #38bdf8;
    color: #7dd3fc;
    background: rgba(56, 189, 248, 0.12);
  }
  #toast {
    position: fixed;
    right: 20px;
    bottom: 20px;
    background: #1e293b;
    color: var(--text);
    border: 1px solid var(--accent);
    padding: 10px 16px;
    border-radius: 8px;
    display: none;
    z-index: 999;
    font-size: 13px;
  }
</style>
</head>
<body>
<header>
  <div class="brand">
    <div class="brand-logo">FG</div>
    <div>
      <h1>FayGer Local Runtime Console</h1>
      <small>Multi-Tenant Digital Soul Engine · CAP Host Gatekeeper · ICP (.ctx) · Telepathy (TP) · GMChain MeriToken (μ)</small>
    </div>
  </div>
  <div class="status-pills">
    <span class="pill" id="hdr-version">fayger v0.3.1</span>
    <span class="pill ok" id="hdr-audit">Audit Chain: verifying...</span>
    <span class="pill" id="hdr-ego">Ego Slot: ...</span>
    <span class="pill merit-pill" id="hdr-merit">GMChain: ...</span>
    <a href="/mobile" target="_blank" class="pill ok" style="text-decoration:none">📱 手机监护端 (/mobile)</a>
    <button class="btn secondary" onclick="refreshAll()">Refresh</button>
  </div>
</header>

<nav class="tabs">
  <button class="active" data-tab="tab-overview" onclick="switchTab('tab-overview', this)">1. 系统权限与设备环境与资源池 (OS &amp; Host Pool)</button>
  <button data-tab="tab-wallet" onclick="switchTab('tab-wallet', this)">2. 身份与凭据保险箱 (FayID Wallet)</button>
  <button id="nav-tab-fays" data-tab="tab-fays" onclick="switchTab('tab-fays', this)">3. 分身实例、大脑配置与上岗监护 (G0 一键急刹)</button>
  <button data-tab="tab-icp" onclick="switchTab('tab-icp', this)">4. ICP 富交互对话与 CAP 审批 · 终端软硬件操控</button>
  <button data-tab="tab-tp" onclick="switchTab('tab-tp', this)">5. 双 Fay 心灵感应协作间 (Telepathy TP)</button>
  <button id="nav-tab-merit" data-tab="tab-merit" onclick="switchTab('tab-merit', this)">6. 美誉值与优先权 (MeriToken)</button>
  <button id="nav-tab-hardware" data-tab="tab-hardware" onclick="switchTab('tab-hardware', this)">7. 硬件终端附身舱 (Hardware)</button>
</nav>

<main>
  <!-- TAB 1: Overview & Device Profile -->
  <section id="tab-overview" class="tab-pane active">
    <div class="grid-2">
      <div class="card">
        <h2>Host Resource Pool (两层授权链第 ① 层：本机授予 FayGer 的资源池)</h2>
        <p class="muted">设备所有人在此决定向 FayGer 引擎开放哪些宿主能力。任何分身实例实际运行能力严格受限于 <code>BuF ∩ HostPool ∩ PerFayPolicy</code>。</p>
        <div id="pool-caps-box" class="row"></div>
        <div class="row">
          <label class="muted">Ego 3-in-1 Endpoint:</label>
          <input type="text" id="pool-ego-ep" style="width:260px" value="http://127.0.0.1:7980" />
          <label class="muted"><input type="checkbox" id="pool-net-online" checked /> Network Online</label>
        </div>
        <div class="row">
          <button class="btn" onclick="saveHostPool()">Save Host Resource Pool</button>
        </div>
      </div>

      <div class="card">
        <h2>Device Runtime Environment Profile (<code>fayger-device-profile/1.0</code>)</h2>
        <p class="muted">终端切片附体（Partial Embodiment）协商所提交的标准环境描述文件：</p>
        <pre class="code" id="device-profile-json">{}</pre>
      </div>
    </div>

    <div class="card">
      <h2>OS 原生权限探针与一键申请面板 (Native OS Permissions &amp; Privacy Gate)</h2>
      <p class="muted">实时探测宿主操作系统剪贴板、通知中心、电池/硬件传感器、语音播报、屏幕录制、桌面应用唤起与摄像头/麦克风就绪状态。绝不绕过系统门禁：可一键触发合法系统探测或直达系统隐私设置页授权，并自动纳入本机 <code>HostResourcePool</code>。</p>
      <div id="os-permissions-feedback" style="margin-bottom:8px"></div>
      <div id="os-permissions-box"></div>
    </div>
  </section>

  <!-- TAB 2: FayID Wallet & Auth Exchange -->
  <section id="tab-wallet" class="tab-pane">
    <div class="grid-2">
      <div class="card">
        <h2>FayID Identity Vault (助记词根身份 · 对外零明文暴露)</h2>
        <div id="wallet-summary" class="muted">Loading vault...</div>
        <div id="mnemonic-banner" class="ctx-approval" style="display:none"></div>
        <div class="row" style="margin-top:12px">
          <input type="text" id="wallet-label" placeholder="Vault Label (e.g. Primary Human Vault)" style="width:240px" />
          <button class="btn" onclick="initWallet()">Initialize Vault</button>
        </div>
        <hr style="border-color:var(--border);margin:14px 0" />
        <h3>Generate Unlinkable Dynamic Code (<code>dync_...</code>)</h3>
        <div class="row">
          <input type="text" id="dync-purpose" placeholder="Purpose (e.g. platform_verify)" value="verify_with_fayid" />
          <input type="number" id="dync-ttl" value="300" style="width:90px" />
          <button class="btn secondary" onclick="issueDynamicCode()">Generate Dynamic Code</button>
        </div>
        <div id="dync-list"></div>
      </div>

      <div class="card">
        <h2>Derive iFay / coFay &amp; Auth Exchange (传统凭据兑换授权票据)</h2>
        <div class="row">
          <input type="text" id="derive-name" placeholder="Avatar / Seat Name" />
          <select id="derive-kind">
            <option value="ifay">Personal iFay (Stealth by default)</option>
            <option value="cofay">Public Role coFay (+ RoleCharter 1.0)</option>
          </select>
          <button class="btn" onclick="deriveIdentityOnly()">Derive ID</button>
        </div>
        <hr style="border-color:var(--border);margin:14px 0" />
        <h3>Auth Exchange (封存 API Key / Token -> 签发限时 Authorization Grant)</h3>
        <div class="row">
          <input type="text" id="ex-provider" placeholder="Provider (e.g. openai, github)" value="openai" style="width:150px" />
          <input type="password" id="ex-secret" placeholder="Legacy API Key / Token (sealed locally)" style="width:220px" />
        </div>
        <div class="row">
          <input type="text" id="ex-scopes" placeholder="Scopes CSV" value="chat.completions,embeddings" style="width:220px" />
          <button class="btn ok" onclick="exchangeGrant()">Exchange -> Authorization Grant</button>
        </div>
        <div id="grants-list"></div>
      </div>
    </div>
  </section>

  <!-- TAB 3: Running Fays & Faying Monitor -->
  <section id="tab-fays" class="tab-pane">
    <div class="card">
      <h2>One-Click Create Soul Package (<code>.buf</code>) or Mount Existing BuF</h2>
      <div class="row">
        <select id="create-soul-kind">
          <option value="ifay">iFay Digital Soul (.buf)</option>
          <option value="cofay">coFay Public Seat (.buf + RoleCharter)</option>
        </select>
        <input type="text" id="create-soul-name" placeholder="Name (e.g. Personal Architect / Release Chair)" style="width:260px" />
        <button class="btn" onclick="createAndMountSoul()">Create &amp; Mount Soul (Starts in ROGUE)</button>
        <span class="muted">|</span>
        <input type="text" id="load-buf-path" placeholder="/path/to/package.buf" style="width:230px" />
        <button class="btn secondary" onclick="loadBufPath()">Mount .buf File</button>
      </div>
    </div>

    <div class="card">
      <h2>配置大脑：Ego 与外部大模型 (Per-iFay External LLM &amp; Ego Brain Config)</h2>
      <p class="muted">为选定分身绑定本地 <code>Ego</code>（快直觉路由 + 人格红线注入 + 输出脱敏守卫）与外部大模型（支持本机 Ollama、OpenAI、DeepSeek、Gemini、Anthropic）。API Key 自动加密封存入本机 <code>FayID</code> 保险箱，绝不明文落盘或回传前端。</p>
      <div class="row">
        <label class="muted">目标 Fay:</label>
        <select id="brain-fay-select" onchange="loadSelectedFayBrainIntoForm()"></select>
        <label class="muted">一键预设:</label>
        <select id="brain-preset" onchange="applyBrainPreset(this.value)">
          <option value="">-- 选择模型服务商预设 --</option>
          <option value="ollama">Ollama 本机免费模型 (127.0.0.1:11434 · qwen2.5:7b)</option>
          <option value="openai">OpenAI Compatible (api.openai.com · gpt-4o-mini)</option>
          <option value="deepseek">DeepSeek (api.deepseek.com · deepseek-chat)</option>
          <option value="gemini">Google Gemini (generativelanguage.googleapis.com · gemini-2.5-flash)</option>
          <option value="anthropic">Anthropic Claude (api.anthropic.com · claude-3-5-sonnet-latest)</option>
        </select>
        <span class="pill" id="brain-apikey-badge">API Key: 未配置</span>
      </div>
      <div class="grid-2" style="margin-top:8px">
        <div class="fay-card" style="margin-bottom:0">
          <h3>External LLM (复杂推理与 <code>&lt;fay_action&gt;</code> 工具编排层)</h3>
          <div class="row">
            <label><input type="checkbox" id="brain-llm-enabled" /> 启用外部大模型协同推理</label>
            <select id="brain-llm-provider">
              <option value="ollama">ollama (Local HTTP)</option>
              <option value="openai_compatible">openai_compatible</option>
              <option value="gemini">gemini</option>
              <option value="anthropic">anthropic</option>
            </select>
          </div>
          <div class="row">
            <label class="muted" style="width:82px">Base URL:</label>
            <input type="text" id="brain-llm-url" value="http://127.0.0.1:11434/v1" style="flex:1" />
          </div>
          <div class="row">
            <label class="muted" style="width:82px">Model:</label>
            <input type="text" id="brain-llm-model" value="qwen2.5:7b" style="width:180px" />
            <label class="muted">Temp:</label>
            <input type="number" id="brain-llm-temp" value="0.4" step="0.1" min="0" max="2" style="width:68px" />
            <label class="muted">MaxTok:</label>
            <input type="number" id="brain-llm-maxtok" value="1024" step="128" min="64" max="32768" style="width:82px" />
          </div>
          <div class="row">
            <label class="muted" style="width:82px">API Key:</label>
            <input type="password" id="brain-llm-cred" placeholder="<API_KEY_PLACEHOLDER> 留空保持不变；输入后自动加密存入 FayID 保险箱" style="flex:1" />
          </div>
        </div>

        <div class="fay-card" style="margin-bottom:0">
          <h3>Ego 个体化小脑 (三合一：直觉路由 · 动态上下文 · 红线守卫)</h3>
          <div class="row">
            <label class="muted" style="width:86px">运行模式:</label>
            <select id="brain-ego-mode">
              <option value="embedded">embedded (内置确定性三合一管线)</option>
              <option value="http_service">http_service (独立 Ego 守护服务 :7980)</option>
              <option value="local_egopod">local_egopod (本地签名 .egopod 包挂载)</option>
            </select>
            <label><input type="checkbox" id="brain-ego-fast" checked /> 快直觉优先</label>
          </div>
          <div class="row">
            <label class="muted" style="width:86px">Ego URL:</label>
            <input type="text" id="brain-ego-endpoint" value="http://127.0.0.1:7980" style="width:190px" />
            <input type="text" id="brain-egopod-path" placeholder="可选 /path/to/ego_pack" style="flex:1" />
          </div>
          <div class="row">
            <label class="muted" style="width:86px">人格风格:</label>
            <input type="text" id="brain-ego-style" value="沉稳、简洁、严格尊重人类原型主权与宪章边界" style="flex:1" />
          </div>
          <div class="row">
            <label class="muted" style="width:86px">红线约束:</label>
            <input type="text" id="brain-ego-redlines" value="绝不泄露根身份助记词或原始 API Key, 高风险操作必须经 CAP 人类原型确认" style="flex:1" />
          </div>
        </div>
      </div>
      <div class="row" style="margin-top:12px">
        <button class="btn ok" onclick="saveFayBrain()">保存大脑配置 (Save Brain Config)</button>
        <button class="btn secondary" onclick="testFayBrain()">测试大脑连通性 (Test Ego + External LLM)</button>
      </div>
      <pre class="code" id="brain-test-result" style="display:none"></pre>
    </div>

    <div id="fays-container"></div>

    <div class="card">
      <h2>Human View — Tamper-Evident SHA-256 Hash-Chained Audit Feed</h2>
      <div id="audit-feed"></div>
    </div>
  </section>

  <!-- TAB 4: ICP Interactive Console & CAP Approvals -->
  <section id="tab-icp" class="tab-pane">
    <div class="grid-2">
      <div class="card">
        <h2>ICP (<code>.ctx</code>) Adaptive Chat &amp; Host Driver Trigger</h2>
        <div class="row" style="justify-content:space-between">
          <div class="row">
            <label class="muted">Active Fay:</label>
            <select id="chat-fay-select" onchange="renderSelectedChat()"></select>
          </div>
          <span class="pill" id="chat-brain-pill">Brain: Ego Embedded</span>
        </div>
        <div class="fay-card" style="padding:10px 12px;margin:8px 0">
          <div class="muted" style="font-weight:700;margin-bottom:6px">终端硬件 / 操作系统 / 桌面软件操控台 (一键实测 · Faying 门禁 + CAP 高风险审批):</div>
          <div class="row" style="gap:6px">
            <button class="btn secondary" onclick="runQuickHostPrompt('/hardware')">硬件电池与传感器遥测</button>
            <button class="btn secondary" onclick="runQuickHostPrompt('/clipboard-write Hello from FayGer iFay')">写入系统剪贴板</button>
            <button class="btn secondary" onclick="runQuickHostPrompt('/clipboard-read')">读取系统剪贴板</button>
            <button class="btn secondary" onclick="runQuickHostPrompt('/notify FayGer|iFay 已连接宿主通知中心')">发送原生桌面通知</button>
            <button class="btn secondary" onclick="runQuickHostPrompt('/speak 你好，我是受人类原型监护的数字灵魂')">扬声器语音播报</button>
            <button class="btn secondary" style="border-color:var(--warn);color:#fcd34d" onclick="runQuickHostPrompt('/screenshot desk')">抓取真实屏幕截图 (CAP审批)</button>
            <button class="btn secondary" style="border-color:var(--warn);color:#fcd34d" onclick="runQuickHostPrompt('/open Calculator')">唤起桌面软件 Calculator (CAP审批)</button>
            <button class="btn secondary" style="border-color:var(--warn);color:#fcd34d" onclick="runQuickHostPrompt('/exec uname')">执行受控终端命令 uname (CAP审批)</button>
          </div>
          <div class="row" style="gap:6px;margin-top:6px">
            <span class="muted">沙箱与美誉快捷指令:</span>
            <button class="btn secondary" onclick="fillPrompt('/write notes/todo.md Ship FayGer v0.3.1')">Routine Write</button>
            <button class="btn secondary" onclick="fillPrompt('/read notes/todo.md')">Routine Read</button>
            <button class="btn secondary" onclick="fillPrompt('/camera desk_view')">High-Risk Camera (CAP Hold)</button>
            <button class="btn secondary" onclick="fillPrompt('查询我的美誉值与投票优先权额度 μ')">Query MeriToken (μ)</button>
          </div>
        </div>
        <textarea id="chat-input" placeholder="输入自然语言（支持外部大模型自动编排 <fay_action>）或 /hardware, /clipboard-write, /notify, /speak, /screenshot, /open, /exec..."></textarea>
        <div class="row">
          <button class="btn" onclick="sendChat()">Send via ICP (.ctx)</button>
        </div>
        <div id="chat-stream" style="margin-top:14px"></div>
      </div>

      <div class="card">
        <h2>Pending &amp; Recent CAP Host Gatekeeper Approvals</h2>
        <p class="muted">高风险宿主驱动操作必须由 <code>Human Prime</code> 显式点击 Allow 才会真实执行（若中途触发红色急刹将自动作废）：</p>
        <div id="cap-approvals-box"></div>
      </div>
    </div>
  </section>

  <!-- TAB 5: Telepathy (TP) Shared Workspace -->
  <section id="tab-tp" class="tab-pane">
    <div class="grid-2">
      <div class="card">
        <h2>Open Authorized Telepathy (<code>TP</code>) Shared Cognitive Space</h2>
        <p class="muted">让本机两个已加载的分身（例如个人 <code>iFay</code> 与项目 <code>coFay</code>）在授权边界内直接共享结构化认知对象，消除转提示词传话失真。</p>
        <div class="row">
          <label class="muted">Initiator:</label>
          <select id="tp-init-fay"></select>
          <label class="muted">Peer:</label>
          <select id="tp-peer-fay"></select>
        </div>
        <div class="row">
          <input type="text" id="tp-topic" placeholder="Collaboration Topic" value="Architecture &amp; Release Coordination" style="width:280px" />
          <button class="btn" onclick="createTpSession()">Open TP Session</button>
        </div>
        <hr style="border-color:var(--border);margin:14px 0" />
        <h3>Share Structured Cognitive Entry</h3>
        <div class="row">
          <select id="tp-session-select"></select>
          <select id="tp-author-select"></select>
          <select id="tp-category">
            <option value="memory_fact">memory_fact</option>
            <option value="view_state">view_state</option>
            <option value="task_context">task_context</option>
            <option value="decision_artifact">decision_artifact</option>
          </select>
        </div>
        <div class="row">
          <input type="text" id="tp-key" placeholder="Semantic Key (e.g. sprint_goal)" value="milestone_status" style="width:220px" />
        </div>
        <textarea id="tp-json-val">{"milestone":"Batch-1","cap_gate":"verified","ready":true}</textarea>
        <div class="row">
          <button class="btn ok" onclick="shareTpEntry()">Share into Cognitive Space</button>
        </div>
      </div>

      <div class="card">
        <h2>Active TP Sessions &amp; Shared Cognitive State</h2>
        <div id="tp-sessions-box"></div>
      </div>
    </div>
  </section>

  <!-- TAB 6: MeriToken & Priority (GMChain Closed Loop) -->
  <section id="tab-merit" class="tab-pane">
    <div class="card">
      <div class="row" style="justify-content:space-between">
        <div class="row">
          <label class="muted">目标 Fay 实例:</label>
          <select id="merit-fay-select" onchange="renderMeritTab()"></select>
          <label class="muted">GMChain RPC (Loopback):</label>
          <input type="text" id="merit-rpc-ep" value="http://127.0.0.1:8546" style="width:220px" />
          <button class="btn secondary" onclick="syncGmchainStatus()">同步公誉链状态</button>
        </div>
        <div class="row">
          <span class="pill" id="merit-sync-badge">Local Pocket</span>
          <span class="pill ok">Non-Monetary · 0 手续费 · 0 转账</span>
        </div>
      </div>
      <div id="merit-rogue-banner" class="ctx-approval" style="display:none;margin-top:10px"></div>
    </div>

    <!-- Panel A: Display & Present -->
    <div class="card">
      <h2>Panel A — 美誉值展示与分级凭证出示 (Display &amp; Present)</h2>
      <div class="metric-grid" id="merit-metrics-grid"></div>
      <div class="row">
        <span class="muted">一键生成可验证美誉凭证 (<code>gmchain-merit-credential/1.0</code>):</span>
        <button class="btn secondary" onclick="presentMeritCredential('L1_PUBLIC')">L1 全公开明细 (L1_PUBLIC)</button>
        <button class="btn secondary" onclick="presentMeritCredential('L2_BAND')">L2 段位区间证明 (L2_BAND)</button>
        <button class="btn secondary" onclick="presentMeritCredential('L3_ZK_BLINDED')">L3 零知识盲化句柄 (L3_ZK_BLINDED)</button>
      </div>
      <pre class="code" id="merit-cred-preview" style="display:none"></pre>
    </div>

    <div class="grid-2">
      <!-- Panel B: Add MeriToken -->
      <div class="card">
        <h2>Panel B — 添加美誉值 (iFay 协作贡献见证入账)</h2>
        <p class="muted">提交经 <code>Faying</code> 受监护状态的 <code>iFay</code> / <code>coFay</code> 核验贡献，铸造入账活跃美誉值并单调抬升历史底值 <code>minMerit</code>。</p>
        <div class="row">
          <label class="muted" style="width:95px">贡献标题:</label>
          <input type="text" id="merit-add-title" value="开源治理协议基准实现与复现数据集" style="flex:1" />
        </div>
        <div class="row">
          <label class="muted" style="width:95px">领域链 ID:</label>
          <select id="merit-add-domain">
            <option value="gmc-open-science">gmc-open-science (开放科学协作链)</option>
            <option value="gmc-public-infra">gmc-public-infra (公共基础设施链)</option>
            <option value="gmc-safety-audit">gmc-safety-audit (安全与合规审计链)</option>
          </select>
        </div>
        <div class="row">
          <label class="muted" style="width:95px">入账值 +Δμ:</label>
          <input type="number" id="merit-add-amount" value="40" min="1" step="1" style="width:110px" />
          <label class="muted">衰减周期:</label>
          <select id="merit-add-horizon">
            <option value="30">30 天</option>
            <option value="90" selected>90 天</option>
            <option value="180">180 天</option>
          </select>
        </div>
        <div class="row">
          <label class="muted" style="width:95px">贡献摘要:</label>
          <input type="text" id="merit-add-summary" value="端到端可复现构件与 SHA-256 审计证据已封存" style="flex:1" />
        </div>
        <div class="row" style="margin-top:12px">
          <button class="btn ok" onclick="submitMeritAdd()">提交贡献并铸造入账 (+Δμ)</button>
        </div>
        <div id="merit-add-feedback" style="margin-top:8px"></div>
      </div>

      <!-- Panel C: Consume MeriToken -->
      <div class="card">
        <h2>Panel C — 消费美誉值 (行使投票优先权 / 服务优先权消减)</h2>
        <p class="muted">仅从可消费活跃池 <code>(curMerit - minMerit)</code> 中消减；历史贡献底值 <code>minMerit</code> 绝对只增不减、受宪章底值严格保护。</p>
        <div class="row">
          <label class="muted" style="width:95px">优先权类型:</label>
          <select id="merit-consume-kind" style="flex:1">
            <option value="voting_priority">投票优先权加权表决 (voting_priority)</option>
            <option value="cofay_service_priority">公共服务 coFay 优先通道 (cofay_service_priority)</option>
            <option value="compute_slot_priority">宿主稀缺算力优先调度 (compute_slot_priority)</option>
          </select>
        </div>
        <div class="row">
          <label class="muted" style="width:95px">关联提案 ID:</label>
          <input type="text" id="merit-consume-proposal" placeholder="如 prop-001" value="prop-001" style="width:150px" />
          <label class="muted">消减额度 -Δμ:</label>
          <input type="number" id="merit-consume-amount" value="12" min="0.1" step="0.5" style="width:110px" />
        </div>
        <div class="row">
          <label class="muted" style="width:95px">用途说明:</label>
          <input type="text" id="merit-consume-reason" value="对社区协议升级提案行使投票优先权表决" style="flex:1" />
        </div>
        <div class="row" style="margin-top:12px">
          <button class="btn" onclick="submitMeritConsume()">行使优先权并消减活跃美誉值 (-Δμ)</button>
        </div>
        <div id="merit-consume-feedback" style="margin-top:8px"></div>
      </div>
    </div>

    <div class="card">
      <h2>最近美誉值铸造入账与优先权消减流水 (Recent Merit &amp; Priority Ledger)</h2>
      <div id="merit-history-box"></div>
    </div>
  </section>

  <!-- TAB 7: Hardware Embodiment Hub (FHBP/1.0) -->
  <section id="tab-hardware" class="tab-pane">
    <div class="card">
      <div class="row" style="justify-content:space-between">
        <div class="row">
          <label class="muted">选择附身分身 (iFay / coFay):</label>
          <select id="hw-fay-select"></select>
          <span class="muted">|</span>
          <label class="muted">接入外部 C99 / FHBP 硬件节点:</label>
          <input type="text" id="hw-connect-ep" placeholder="http://127.0.0.1:9100" value="http://127.0.0.1:9100" style="width:210px" />
          <button class="btn secondary" onclick="connectExternalHardware()">接入节点 (FHBP/1.0)</button>
        </div>
        <div class="row">
          <button class="btn brake" onclick="haltAllHardwareG0()">🛑 全硬件 G0 紧急停机</button>
        </div>
      </div>
      <p class="muted" style="margin:6px 0 0">
        支持 Tier A（无人机 / ROS2 机器人嵌入式 Linux）与 Tier B（遥控车 / 五轴机床 / 电影摇臂 C99 <code>FHBP/1.0</code> 桥接）。所有动作严格受 <code>Faying</code> 监护态 + <code>CAP Physical Safety Envelope</code> 物理安全包络与高危人工审批保护。
      </p>
      <div id="hw-feedback" style="margin-top:8px"></div>
    </div>

    <div class="grid-2" id="hw-devices-grid"></div>
  </section>
</main>

<div id="toast"></div>

<script>
const KNOWN_CAPS = [
  "fs.read",
  "fs.write",
  "os.clipboard",
  "os.notify",
  "hardware.telemetry",
  "hardware.actuate",
  "hardware.audio",
  "hardware.screen",
  "app.control",
  "process.exec",
  "net.http",
  "media.camera",
  "media.mic"
];
let STATE = null;
let LAST_LOADED_BRAIN_FAY_ID = '';

function toast(msg) {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.style.display = 'block';
  setTimeout(() => { el.style.display = 'none'; }, 3200);
}

function switchTab(id, btn) {
  document.querySelectorAll('.tab-pane').forEach(p => p.classList.remove('active'));
  document.querySelectorAll('nav.tabs button').forEach(b => b.classList.remove('active'));
  document.getElementById(id).classList.add('active');
  btn.classList.add('active');
}

async function api(path, method = 'GET', body = null) {
  const opts = { method, headers: { 'Content-Type': 'application/json' } };
  if (body !== null) opts.body = JSON.stringify(body);
  const res = await fetch(path, opts);
  const data = await res.json();
  if (!res.ok) throw new Error(data.error || ('HTTP ' + res.status));
  return data;
}

async function refreshAll() {
  try {
    STATE = await api('/api/v1/state');
    renderHeader();
    renderOverview();
    renderWallet();
    renderFays();
    renderIcpTab();
    renderTpTab();
    renderMeritTab();
    renderHardwareTab();
  } catch (e) {
    toast('Error: ' + e.message);
  }
}

function renderHeader() {
  document.getElementById('hdr-version').textContent = 'fayger ' + STATE.fayger_version;
  const auditEl = document.getElementById('hdr-audit');
  auditEl.textContent = STATE.audit_chain_verified
    ? ('SHA-256 Chain OK · ' + STATE.chain_head_hash.slice(0, 10))
    : 'Audit Chain TAMPERED';
  auditEl.className = 'pill ' + (STATE.audit_chain_verified ? 'ok' : 'danger');
  const ego = STATE.device_profile.ego_slot;
  const egoEl = document.getElementById('hdr-ego');
  egoEl.textContent = 'Ego: ' + (ego.online ? 'ONLINE (3-in-1)' : 'Standalone Fallback');
  egoEl.className = 'pill ' + (ego.online ? 'ok' : 'warn');
  const mc = STATE.merit_config || {};
  const meritEl = document.getElementById('hdr-merit');
  meritEl.textContent = 'GMChain: ' + (mc.online ? 'Live RPC' : 'Local Pocket');
  meritEl.className = 'pill ' + (mc.online ? 'ok' : 'merit-pill');
}

function renderOverview() {
  const granted = new Set(STATE.host_pool.granted_capabilities || []);
  const box = document.getElementById('pool-caps-box');
  box.innerHTML = KNOWN_CAPS.map(c => `
    <label class="cap-chip ${granted.has(c) ? 'effective' : ''}">
      <input type="checkbox" class="pool-cap-cb" value="${c}" ${granted.has(c) ? 'checked' : ''} /> ${c}
    </label>
  `).join('');
  document.getElementById('pool-ego-ep').value = STATE.host_pool.ego_endpoint || 'http://127.0.0.1:7980';
  document.getElementById('pool-net-online').checked = !!STATE.host_pool.network_online;
  document.getElementById('device-profile-json').textContent = JSON.stringify(STATE.device_profile, null, 2);

  const perms = STATE.os_permissions || [];
  const permBox = document.getElementById('os-permissions-box');
  if (!perms.length) {
    permBox.innerHTML = '<p class="muted">OS permission probe unavailable.</p>';
  } else {
    permBox.innerHTML = `
      <table class="kv">
        <tr><th>宿主能力</th><th>安全门禁等级</th><th>OS 就绪状态</th><th>资源池授权</th><th>系统原生机制与说明</th><th>合法申请与隐私引导</th></tr>
        ${perms.map(p => {
          const stClass = p.status === 'ready' ? 'ok' : (p.status === 'needs_os_grant' ? 'warn' : 'danger');
          const tierLabel = p.risk_tier === 'high_cap_approval' ? '高危 · 需 CAP 审批' : '常规 · Faying 直通';
          return `
            <tr>
              <td><code>${esc(p.capability)}</code></td>
              <td><span class="pill ${p.risk_tier === 'high_cap_approval' ? 'warn' : 'ok'}">${tierLabel}</span></td>
              <td><span class="pill ${stClass}">${esc(p.status)}</span></td>
              <td>${p.in_host_pool ? '✓ 已纳入' : '✗ 未纳入'}</td>
              <td><b>${esc(p.os_mechanism)}</b><br/><span class="muted">${esc(p.detail)}</span></td>
              <td>
                <div class="row" style="margin:0;gap:6px">
                  <button class="btn secondary" onclick="requestOsPermission('${esc(p.capability)}', false)">申请系统权限</button>
                  ${p.settings_uri ? `<button class="btn secondary" onclick="requestOsPermission('${esc(p.capability)}', true)">打开系统隐私设置</button>` : ''}
                </div>
              </td>
            </tr>
          `;
        }).join('')}
      </table>
    `;
  }
}

async function requestOsPermission(capability, open_settings) {
  const fb = document.getElementById('os-permissions-feedback');
  try {
    const res = await api('/api/v1/os/permissions/request', 'POST', {
      capability,
      open_settings,
      grant_to_host_pool: true
    });
    fb.innerHTML = `<div class="ctx-approval" style="border-color:var(--ok)"><b>[${esc(res.capability)}] 系统权限探测/申请已执行</b>：状态=<code>${esc(res.status)}</code> · 动作=${esc(res.triggered_action)} ${res.settings_opened ? '· 已为您唤起系统隐私设置页' : ''}</div>`;
    toast('已探测并同步宿主权限: ' + capability);
    await refreshAll();
  } catch (e) {
    fb.innerHTML = `<div class="ctx-alert">${esc(e.message)}</div>`;
    toast(e.message);
  }
}

async function saveHostPool() {
  const caps = Array.from(document.querySelectorAll('.pool-cap-cb:checked')).map(el => el.value);
  const ego_endpoint = document.getElementById('pool-ego-ep').value;
  const network_online = document.getElementById('pool-net-online').checked;
  await api('/api/v1/host/pool', 'PUT', { granted_capabilities: caps, ego_endpoint, network_online });
  toast('Host Resource Pool updated');
  await refreshAll();
}

function renderWallet() {
  const w = STATE.wallet;
  const sum = document.getElementById('wallet-summary');
  if (!w.initialized) {
    sum.innerHTML = '<p>Vault is currently uninitialized. Click <b>Initialize Vault</b> or create a soul package.</p>';
  } else {
    sum.innerHTML = `
      <table class="kv">
        <tr><th>Label</th><td>${esc(w.label)}</td></tr>
        <tr><th>Human ID (Masked)</th><td>${esc(w.human_id_masked)} <span class="muted">(plaintext never exposed)</span></td></tr>
        <tr><th>Commitment</th><td>${esc(w.human_id_commitment.slice(0, 36))}...</td></tr>
        <tr><th>Derived iFays / coFays</th><td>${w.derived_ifays.length} iFay · ${w.derived_cofays.length} coFay</td></tr>
      </table>
    `;
  }
  document.getElementById('dync-list').innerHTML = (w.dynamic_codes || []).slice(-3).reverse().map(d => `
    <div class="cap-chip effective" style="display:block;margin-top:6px">
      <b>${esc(d.dynamic_code)}</b> · purpose=${esc(d.purpose)} · exp=${d.expires_at_unix}
    </div>
  `).join('');
  document.getElementById('grants-list').innerHTML = (w.auth_grants || []).slice(-4).reverse().map(g => `
    <div class="cap-chip" style="display:block;margin-top:6px">
      <b>${esc(g.grant_id)}</b> [${esc(g.provider)}] scopes=${esc(g.scopes.join(','))} fp=${esc(g.secret_fingerprint)}
    </div>
  `).join('');
}

async function initWallet() {
  const label = document.getElementById('wallet-label').value || 'Primary Human Vault';
  try {
    const receipt = await api('/api/v1/wallet/init', 'POST', { label, force: false });
    const banner = document.getElementById('mnemonic-banner');
    banner.style.display = 'block';
    banner.innerHTML = `<b>One-Time Recovery Mnemonic (Save Offline Now):</b><br/><code>${esc(receipt.recovery_mnemonic_once)}</code>`;
    toast('FayID Vault initialized');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function issueDynamicCode() {
  try {
    const purpose = document.getElementById('dync-purpose').value;
    const ttl_secs = Number(document.getElementById('dync-ttl').value || 300);
    await api('/api/v1/wallet/dynamic-code', 'POST', { purpose, ttl_secs });
    toast('Unlinkable Dynamic Code generated');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function deriveIdentityOnly() {
  try {
    const name = document.getElementById('derive-name').value || 'MyAvatar';
    const kind = document.getElementById('derive-kind').value;
    const ep = kind === 'cofay' ? '/api/v1/wallet/derive-cofay' : '/api/v1/wallet/derive-ifay';
    await api(ep, 'POST', { name });
    toast('Derived ' + kind + ' in FayID Vault');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function exchangeGrant() {
  try {
    const provider = document.getElementById('ex-provider').value;
    const secret = document.getElementById('ex-secret').value;
    const scopes = document.getElementById('ex-scopes').value.split(',').map(s => s.trim()).filter(Boolean);
    await api('/api/v1/wallet/exchange-grant', 'POST', { provider, secret, scopes, ttl_secs: 3600 });
    document.getElementById('ex-secret').value = '';
    toast('Sealed secret & issued Authorization Grant');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function createAndMountSoul() {
  try {
    const kind = document.getElementById('create-soul-kind').value;
    const name = document.getElementById('create-soul-name').value || (kind === 'cofay' ? 'Public Role Seat' : 'Personal Soul');
    await api('/api/v1/fays/create', 'POST', { kind, name });
    toast('Created & mounted .buf soul in ROGUE state');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function loadBufPath() {
  try {
    const path = document.getElementById('load-buf-path').value;
    await api('/api/v1/fays/load', 'POST', { path });
    toast('Mounted .buf in ROGUE state');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

function renderFays() {
  const container = document.getElementById('fays-container');
  const fays = STATE.fays || [];

  const brainSel = document.getElementById('brain-fay-select');
  const prevBrainSel = brainSel.value;
  brainSel.innerHTML = fays.map(f => `<option value="${esc(f.fay_id)}">${esc(f.display_name)} (${esc(f.kind.toUpperCase())}) — ${esc(f.instance_id)}</option>`).join('');
  if (prevBrainSel && fays.some(f => f.fay_id === prevBrainSel)) {
    brainSel.value = prevBrainSel;
  }
  if (brainSel.value && brainSel.value !== LAST_LOADED_BRAIN_FAY_ID) {
    loadSelectedFayBrainIntoForm();
  } else {
    refreshBrainKeyBadge();
  }

  if (!fays.length) {
    container.innerHTML = '<div class="card muted">No Fay instances mounted yet. Create an iFay or coFay above!</div>';
  } else {
    container.innerHTML = fays.map(f => {
      const isFaying = f.faying_state === 'FAYING';
      const polSet = new Set(f.policy_capabilities || []);
      const effSet = new Set(f.effective_capabilities || []);
      const policyChecks = f.requested_capabilities.map(c => `
        <label class="cap-chip ${effSet.has(c) ? 'effective' : ''}">
          <input type="checkbox" onchange="toggleFayCap('${f.fay_id}', '${c}', this.checked)" ${polSet.has(c) ? 'checked' : ''} />
          ${c} ${effSet.has(c) ? '✓' : '✗'}
        </label>
      `).join('');
      const m = f.merit || { cur_merit_formatted: '5.00', consumable_merit_formatted: '2.28', min_merit_floor_formatted: '2.72' };
      const b = f.brain || { external_llm: { enabled: false }, ego: { mode: 'embedded' } };
      const brainBadge = (b.external_llm && b.external_llm.enabled)
        ? `Brain: Ego(${b.ego.mode}) + ${b.external_llm.provider}/${b.external_llm.model}`
        : `Brain: Ego(${b.ego ? b.ego.mode : 'embedded'})`;
      return `
        <div class="fay-card ${isFaying ? 'faying' : 'rogue'}">
          <div class="row" style="justify-content:space-between">
            <div>
              <strong>${esc(f.display_name)}</strong>
              <span class="pill">${esc(f.kind.toUpperCase())}</span>
              <code class="muted">${esc(f.fay_id)}</code>
              <span class="pill ${isFaying ? 'ok' : 'danger'}">${esc(f.faying_state)}</span>
              <span class="pill ${b.external_llm && b.external_llm.enabled ? 'ok' : ''}">${esc(brainBadge)}</span>
              <span class="pill merit-pill">MeriToken: ${esc(m.cur_merit_formatted)} μ (可消减 ${esc(m.consumable_merit_formatted)} μ · 底值 ${esc(m.min_merit_floor_formatted)} μ)</span>
            </div>
            <div class="row">
              <button class="btn secondary" onclick="jumpToBrainConfig('${f.fay_id}')">配置大脑</button>
              <button class="btn secondary" onclick="jumpToMeritTab('${f.fay_id}')">美誉值与优先权 (Tab 6)</button>
              ${isFaying
                ? `<button class="btn secondary" onclick="sendHeartbeat('${f.fay_id}')">Heartbeat</button>`
                : `<button class="btn ok" onclick="activateFaying('${f.fay_id}')">Flip Faying Switch ON (G2)</button>`
              }
              <button class="btn brake" onclick="triggerBrake('${f.fay_id}')">G0 一键急刹 -&gt; Rogue Fay</button>
            </div>
          </div>
          <div class="muted" style="margin-top:6px">
            Embodiment: <code>${esc(f.embodiment.embodiment_mode)}</code> (${esc(f.embodiment.mounted_slices.join(', '))})
            · Pending Replica Deltas: <b>${f.pending_sync_deltas}</b>
            ${f.last_rogue_reason ? ` · Rogue Reason: <span>${esc(f.last_rogue_reason)}</span>` : ''}
          </div>
          <div style="margin-top:8px">
            <span class="muted">BuF ∩ HostPool ∩ PerFayPolicy Live Intersection:</span><br/>
            ${policyChecks}
          </div>
        </div>
      `;
    }).join('');
  }

  const auditBox = document.getElementById('audit-feed');
  const entries = (STATE.audit_entries || []).slice(-12).reverse();
  auditBox.innerHTML = `
    <table class="kv">
      <tr><th>Seq</th><th>Stage</th><th>Entry SHA-256</th><th>Summary</th></tr>
      ${entries.map(e => `
        <tr>
          <td>#${e.seq}</td>
          <td>${esc(e.stage)}</td>
          <td>${esc(e.entry_hash.slice(0, 16))}…</td>
          <td>${esc(JSON.stringify(e.summary))}</td>
        </tr>
      `).join('')}
    </table>
  `;
}

async function toggleFayCap(fayId, cap, checked) {
  const fay = (STATE.fays || []).find(x => x.fay_id === fayId);
  if (!fay) return;
  const set = new Set(fay.policy_capabilities || []);
  if (checked) set.add(cap); else set.delete(cap);
  await api(`/api/v1/fays/${encodeURIComponent(fayId)}/policy`, 'PUT', {
    policy_capabilities: Array.from(set)
  });
  toast('Updated Per-Fay Policy & recomputed intersection');
  await refreshAll();
}

async function activateFaying(fayId) {
  await api(`/api/v1/fays/${encodeURIComponent(fayId)}/faying`, 'POST', { grade: 'G2', ttl_secs: 3600, heartbeat_timeout_secs: 300 });
  toast('Faying Mandate active -> FAYING');
  await refreshAll();
}

async function sendHeartbeat(fayId) {
  await api(`/api/v1/fays/${encodeURIComponent(fayId)}/heartbeat`, 'POST', {});
  toast('Heartbeat renewed');
  await refreshAll();
}

async function triggerBrake(fayId) {
  await api(`/api/v1/fays/${encodeURIComponent(fayId)}/emergency-brake`, 'POST', {
    reason: 'Human Prime clicked G0 Red Emergency Brake in Dashboard'
  });
  toast('G0 EMERGENCY BRAKE ENGAGED -> ROGUE FAY');
  await refreshAll();
}

function jumpToBrainConfig(fayId) {
  const btn = document.getElementById('nav-tab-fays');
  switchTab('tab-fays', btn);
  const sel = document.getElementById('brain-fay-select');
  if (sel && fayId) {
    sel.value = fayId;
    loadSelectedFayBrainIntoForm();
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }
}

function refreshBrainKeyBadge() {
  const fayId = document.getElementById('brain-fay-select').value;
  const fay = (STATE && STATE.fays || []).find(f => f.fay_id === fayId);
  const badge = document.getElementById('brain-apikey-badge');
  if (!fay || !fay.brain || !fay.brain.external_llm) {
    badge.textContent = 'API Key: 未配置';
    badge.className = 'pill';
    return;
  }
  if (fay.brain.external_llm.has_api_key) {
    badge.textContent = '已加密存入保险箱 ✓';
    badge.className = 'pill ok';
  } else {
    badge.textContent = 'API Key: 未配置 (Ollama 本地免密)';
    badge.className = 'pill warn';
  }
}

function loadSelectedFayBrainIntoForm() {
  const fayId = document.getElementById('brain-fay-select').value;
  const fay = (STATE && STATE.fays || []).find(f => f.fay_id === fayId);
  if (!fay || !fay.brain) return;
  LAST_LOADED_BRAIN_FAY_ID = fayId;
  const llm = fay.brain.external_llm || {};
  const ego = fay.brain.ego || {};
  document.getElementById('brain-llm-enabled').checked = !!llm.enabled;
  document.getElementById('brain-llm-provider').value = llm.provider || 'ollama';
  document.getElementById('brain-llm-url').value = llm.base_url || 'http://127.0.0.1:11434/v1';
  document.getElementById('brain-llm-model').value = llm.model || 'qwen2.5:7b';
  document.getElementById('brain-llm-temp').value = llm.temperature ?? 0.4;
  document.getElementById('brain-llm-maxtok').value = llm.max_tokens ?? 1024;
  document.getElementById('brain-llm-cred').value = '';
  document.getElementById('brain-ego-mode').value = ego.mode || 'embedded';
  document.getElementById('brain-ego-endpoint').value = ego.ego_endpoint || ego.endpoint || 'http://127.0.0.1:7980';
  document.getElementById('brain-egopod-path').value = ego.egopod_path || '';
  document.getElementById('brain-ego-style').value = ego.persona_style || '';
  document.getElementById('brain-ego-redlines').value = (ego.redlines || []).join(', ');
  document.getElementById('brain-ego-fast').checked = ego.fast_intuition_first !== false;
  refreshBrainKeyBadge();
}

function applyBrainPreset(preset) {
  if (!preset) return;
  document.getElementById('brain-llm-enabled').checked = true;
  if (preset === 'ollama') {
    document.getElementById('brain-llm-provider').value = 'ollama';
    document.getElementById('brain-llm-url').value = 'http://127.0.0.1:11434/v1';
    document.getElementById('brain-llm-model').value = 'qwen2.5:7b';
  } else if (preset === 'openai') {
    document.getElementById('brain-llm-provider').value = 'openai_compatible';
    document.getElementById('brain-llm-url').value = 'https://api.openai.com/v1';
    document.getElementById('brain-llm-model').value = 'gpt-4o-mini';
  } else if (preset === 'deepseek') {
    document.getElementById('brain-llm-provider').value = 'openai_compatible';
    document.getElementById('brain-llm-url').value = 'https://api.deepseek.com/v1';
    document.getElementById('brain-llm-model').value = 'deepseek-chat';
  } else if (preset === 'gemini') {
    document.getElementById('brain-llm-provider').value = 'gemini';
    document.getElementById('brain-llm-url').value = 'https://generativelanguage.googleapis.com/v1beta/openai';
    document.getElementById('brain-llm-model').value = 'gemini-2.5-flash';
  } else if (preset === 'anthropic') {
    document.getElementById('brain-llm-provider').value = 'anthropic';
    document.getElementById('brain-llm-url').value = 'https://api.anthropic.com/v1';
    document.getElementById('brain-llm-model').value = 'claude-3-5-sonnet-latest';
  }
}

async function saveFayBrain() {
  const fayId = document.getElementById('brain-fay-select').value;
  if (!fayId) return toast('请先创建并选择目标 Fay');
  const apiKeyRaw = document.getElementById('brain-llm-cred').value.trim();
  const extPayload = {
    enabled: document.getElementById('brain-llm-enabled').checked,
    provider: document.getElementById('brain-llm-provider').value,
    base_url: document.getElementById('brain-llm-url').value.trim(),
    model: document.getElementById('brain-llm-model').value.trim(),
    temperature: Number(document.getElementById('brain-llm-temp').value || 0.4),
    max_tokens: Number(document.getElementById('brain-llm-maxtok').value || 1024)
  };
  if (apiKeyRaw) extPayload.api_key = apiKeyRaw;
  const egopodPath = document.getElementById('brain-egopod-path').value.trim();
  const egoPayload = {
    mode: document.getElementById('brain-ego-mode').value,
    ego_endpoint: document.getElementById('brain-ego-endpoint').value.trim(),
    egopod_path: egopodPath || null,
    persona_style: document.getElementById('brain-ego-style').value.trim(),
    redlines: document.getElementById('brain-ego-redlines').value.split(/[,\n]/).map(s => s.trim()).filter(Boolean),
    fast_intuition_first: document.getElementById('brain-ego-fast').checked
  };
  try {
    await api(`/api/v1/fays/${encodeURIComponent(fayId)}/brain`, 'POST', {
      external_llm: extPayload,
      ego: egoPayload
    });
    document.getElementById('brain-llm-cred').value = '';
    toast('大脑配置已保存 (API Key 已密封入保险箱)');
    await refreshAll();
    loadSelectedFayBrainIntoForm();
  } catch (e) {
    toast(e.message);
  }
}

async function testFayBrain() {
  const fayId = document.getElementById('brain-fay-select').value;
  if (!fayId) return toast('请先创建并选择目标 Fay');
  const box = document.getElementById('brain-test-result');
  box.style.display = 'block';
  box.textContent = '正在探测 Ego 与外部大模型连通性...';
  try {
    const rep = await api(`/api/v1/fays/${encodeURIComponent(fayId)}/brain/test`, 'POST', {});
    box.textContent = JSON.stringify(rep, null, 2);
    toast(rep.ok ? '大脑连通性测试通过 ✓' : '大脑连通性测试返回告警');
  } catch (e) {
    box.textContent = 'Error: ' + e.message;
    toast(e.message);
  }
}

function renderIcpTab() {
  const sel = document.getElementById('chat-fay-select');
  const prev = sel.value;
  const fays = STATE.fays || [];
  sel.innerHTML = fays.map(f => `<option value="${esc(f.fay_id)}">${esc(f.display_name)} (${esc(f.faying_state)}) — ${esc(f.fay_id.slice(0, 14))}…</option>`).join('');
  if (prev && fays.some(f => f.fay_id === prev)) sel.value = prev;
  renderSelectedChat();

  const aprBox = document.getElementById('cap-approvals-box');
  const aprs = STATE.cap_approvals || [];
  if (!aprs.length) {
    aprBox.innerHTML = '<p class="muted">No high-risk CAP requests yet.</p>';
  } else {
    aprBox.innerHTML = aprs.map(a => `
      <div class="ctx-approval">
        <div><b>${esc(a.approval_id)}</b> · <code>${esc(a.required_capability)}</code> · status=<b>${esc(a.status.toUpperCase())}</b></div>
        <div class="muted">Fay: ${esc(a.fay_id)} · params_sha256=${esc(a.params_sha256.slice(0, 16))}…</div>
        ${a.status === 'pending' ? `
          <div class="row" style="margin-top:8px">
            <button class="btn ok" onclick="decideCap('${a.approval_id}', true)">Allow (Human Prime Attest)</button>
            <button class="btn brake" onclick="decideCap('${a.approval_id}', false)">Deny</button>
          </div>
        ` : `<div class="muted">${esc(a.execution_summary || '')}</div>`}
      </div>
    `).join('');
  }
}

function renderSelectedChat() {
  const fayId = document.getElementById('chat-fay-select').value;
  const stream = document.getElementById('chat-stream');
  const brainPill = document.getElementById('chat-brain-pill');
  const fay = (STATE.fays || []).find(f => f.fay_id === fayId);
  if (!fay) {
    brainPill.textContent = 'Brain: Unmounted';
    brainPill.className = 'pill';
    stream.innerHTML = '<p class="muted">Create or mount a Fay first.</p>';
    return;
  }
  const b = fay.brain || {};
  if (b.external_llm && b.external_llm.enabled) {
    brainPill.textContent = `Brain: Ego(${b.ego ? b.ego.mode : 'embedded'}) + External LLM (${b.external_llm.provider}/${b.external_llm.model})`;
    brainPill.className = 'pill ok';
  } else {
    brainPill.textContent = `Brain: Ego(${b.ego ? b.ego.mode : 'embedded'}) Standalone`;
    brainPill.className = 'pill warn';
  }
  const turns = (fay.chat_history || []).slice().reverse();
  stream.innerHTML = turns.map(t => {
    const c = t.response_ctx;
    const anns = (c.annotations || []).map(renderAnnotation).join('');
    return `
      <div class="ctx-card">
        <div class="muted">You: <b>${esc(t.user_input)}</b></div>
        <div style="margin-top:6px">${esc(c.summary_text)}</div>
        <div class="muted" style="font-size:11px;margin-top:4px">
          ICP <code>${esc(c.schema_version)}</code> · Brain Pipeline: <code>${esc(c.ego_trace.mode)}</code> (intent=${esc(c.ego_trace.intuition_intent)}) · ctx=${esc(c.ego_trace.dynamic_context_summary || '')}
        </div>
        ${anns}
        <details style="margin-top:8px">
          <summary class="muted" style="cursor:pointer;font-size:12px">CLI Degraded Text Preview (.ctx -&gt; terminal)</summary>
          <pre class="code">${esc(t.terminal_text)}</pre>
        </details>
      </div>
    `;
  }).join('');
}

function renderAnnotation(a) {
  if (a.kind === 'status_badge') {
    return `<span class="pill ${esc(a.tone)}" style="display:inline-block;margin-top:6px">${esc(a.label)}: ${esc(a.value)}</span>`;
  }
  if (a.kind === 'kv_table') {
    return `<table class="kv"><tr><th colspan="2">${esc(a.title)}</th></tr>${(a.rows || []).map(r => `<tr><td>${esc(r.key)}</td><td>${esc(r.value)}</td></tr>`).join('')}</table>`;
  }
  if (a.kind === 'artifact_preview') {
    return `<div style="margin-top:8px"><span class="muted">${esc(a.title)} (<code>${esc(a.path)}</code>):</span><pre class="code">${esc(a.content_preview)}</pre></div>`;
  }
  if (a.kind === 'alert_banner') {
    return `<div class="ctx-alert"><b>${esc(a.title)}</b><br/>${esc(a.detail)}</div>`;
  }
  if (a.kind === 'cap_approval_card') {
    return `
      <div class="ctx-approval">
        <b>CAP Approval Card [${esc(a.status.toUpperCase())}]</b> · <code>${esc(a.operation)}</code> on <code>${esc(a.target)}</code><br/>
        <span class="muted">${esc(a.reason)}</span>
        ${a.status === 'pending' ? `
          <div class="row">
            <button class="btn ok" onclick="decideCap('${a.approval_id}', true)">Allow</button>
            <button class="btn secondary" onclick="decideCap('${a.approval_id}', false)">Deny</button>
          </div>
        ` : ''}
      </div>
    `;
  }
  if (a.kind === 'merit_standing_card') {
    return `
      <div class="ctx-approval" style="border-color:#38bdf8;background:rgba(56,189,248,0.08)">
        <div class="row" style="justify-content:space-between;margin:0">
          <b>MeriToken Standing Card (<code>${esc(a.sync_mode)}</code> · Height #${a.block_height})</b>
          <button class="btn secondary" onclick="jumpToMeritTab('${esc(a.fay_id)}')">打开 Tab 6</button>
        </div>
        <div style="margin-top:4px;font-family:var(--mono);font-size:12px">
          curMerit: <b>${esc(a.cur_merit_mu)}</b> · 可消费活跃池 (cur - min): <b>${esc(a.consumable_merit_mu)}</b> · 保底 minMerit: <b>${esc(a.min_merit_floor_mu)}</b>
        </div>
      </div>
    `;
  }
  return '';
}

function fillPrompt(txt) {
  document.getElementById('chat-input').value = txt;
}

async function runQuickHostPrompt(txt) {
  document.getElementById('chat-input').value = txt;
  await sendChat();
}

async function sendChat() {
  const fayId = document.getElementById('chat-fay-select').value;
  if (!fayId) return toast('Mount a Fay instance first');
  const message = document.getElementById('chat-input').value;
  try {
    await api(`/api/v1/fays/${encodeURIComponent(fayId)}/chat`, 'POST', { message });
    document.getElementById('chat-input').value = '';
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function decideCap(approvalId, approve) {
  try {
    await api(`/api/v1/cap/approvals/${encodeURIComponent(approvalId)}/decide`, 'POST', { approve });
    toast(approve ? 'CAP Action Approved & Executed' : 'CAP Action Denied');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

function renderTpTab() {
  const fays = STATE.fays || [];
  const opts = fays.map(f => `<option value="${esc(f.fay_id)}">${esc(f.display_name)} (${esc(f.kind)})</option>`).join('');
  const initSel = document.getElementById('tp-init-fay');
  const peerSel = document.getElementById('tp-peer-fay');
  const authSel = document.getElementById('tp-author-select');
  initSel.innerHTML = opts;
  peerSel.innerHTML = opts;
  authSel.innerHTML = opts;
  if (fays.length >= 2 && peerSel.selectedIndex === 0) peerSel.selectedIndex = 1;

  const sessions = STATE.tp_sessions || [];
  const sesSel = document.getElementById('tp-session-select');
  sesSel.innerHTML = sessions.filter(s => s.status === 'active').map(s => `<option value="${esc(s.session_id)}">${esc(s.session_id)} · ${esc(s.topic)}</option>`).join('');

  const box = document.getElementById('tp-sessions-box');
  if (!sessions.length) {
    box.innerHTML = '<p class="muted">No Telepathy sessions yet. Mount two Fays (e.g. an iFay + a coFay) and open a shared space.</p>';
  } else {
    box.innerHTML = sessions.map(s => `
      <div class="fay-card">
        <div class="row" style="justify-content:space-between">
          <div><b>${esc(s.topic)}</b> <code>${esc(s.session_id)}</code> <span class="pill ${s.status === 'active' ? 'ok' : 'warn'}">${esc(s.status)}</span></div>
          ${s.status === 'active' ? `<button class="btn secondary" onclick="closeTp('${s.session_id}')">Close</button>` : ''}
        </div>
        <div class="muted">${esc(s.initiator_fay_id)} &lt;==TP==&gt; ${esc(s.peer_fay_id)} · rev=${s.revision}</div>
        ${(s.shared_entries || []).map(e => `
          <pre class="code">[rev#${e.revision} · ${esc(e.category)} · key=${esc(e.key)} · sha256=${esc(e.value_sha256.slice(0, 12))}…]\n${esc(JSON.stringify(e.structured_value, null, 2))}</pre>
        `).join('')}
      </div>
    `).join('');
  }
}

async function createTpSession() {
  try {
    const initiator_fay_id = document.getElementById('tp-init-fay').value;
    const peer_fay_id = document.getElementById('tp-peer-fay').value;
    const topic = document.getElementById('tp-topic').value;
    await api('/api/v1/tp/sessions', 'POST', { initiator_fay_id, peer_fay_id, topic });
    toast('Opened TP Shared Cognitive Space');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function shareTpEntry() {
  try {
    const sessionId = document.getElementById('tp-session-select').value;
    if (!sessionId) return toast('Open an active TP session first');
    const author_fay_id = document.getElementById('tp-author-select').value;
    const category = document.getElementById('tp-category').value;
    const key = document.getElementById('tp-key').value;
    const structured_value = JSON.parse(document.getElementById('tp-json-val').value);
    await api(`/api/v1/tp/sessions/${encodeURIComponent(sessionId)}/share`, 'POST', {
      author_fay_id, category, key, structured_value
    });
    toast('Shared structured state into TP Space');
    await refreshAll();
  } catch (e) { toast(e.message); }
}

async function closeTp(sessionId) {
  await api(`/api/v1/tp/sessions/${encodeURIComponent(sessionId)}/close`, 'POST', {});
  toast('Closed TP session');
  await refreshAll();
}

function jumpToMeritTab(fayId) {
  const btn = document.getElementById('nav-tab-merit');
  switchTab('tab-merit', btn);
  const sel = document.getElementById('merit-fay-select');
  if (sel && fayId) {
    sel.value = fayId;
    renderMeritTab();
  }
}

function renderMeritTab() {
  const fays = STATE.fays || [];
  const sel = document.getElementById('merit-fay-select');
  const prev = sel.value;
  sel.innerHTML = fays.map(f => `<option value="${esc(f.fay_id)}">${esc(f.display_name)} (${esc(f.kind.toUpperCase())} · ${esc(f.faying_state)}) — ${esc(f.instance_id)}</option>`).join('');
  if (prev && fays.some(f => f.fay_id === prev)) sel.value = prev;

  const mc = STATE.merit_config || {};
  const epInput = document.getElementById('merit-rpc-ep');
  if (mc.gmchain_endpoint && document.activeElement !== epInput) {
    epInput.value = mc.gmchain_endpoint;
  }

  const grid = document.getElementById('merit-metrics-grid');
  const rogueBanner = document.getElementById('merit-rogue-banner');
  const histBox = document.getElementById('merit-history-box');
  const badge = document.getElementById('merit-sync-badge');

  const fay = fays.find(f => f.fay_id === sel.value) || fays[0];
  if (!fay) {
    badge.textContent = mc.online ? 'GMChain Live RPC' : 'Local Pocket';
    badge.className = 'pill ' + (mc.online ? 'ok' : 'warn');
    rogueBanner.style.display = 'none';
    grid.innerHTML = '<div class="muted">尚未挂载任何 Fay 实例。请先在 Tab 3 一键创建或加载一个 iFay / coFay！</div>';
    histBox.innerHTML = '<p class="muted">暂无美誉值流水。</p>';
    return;
  }

  const m = fay.merit || {};
  const isLive = m.sync_mode === 'gmchain_live_rpc';
  badge.textContent = isLive ? 'GMChain Live RPC' : 'Local Pocket';
  badge.className = 'pill ' + (isLive ? 'ok' : 'warn');

  if (fay.faying_state !== 'FAYING') {
    rogueBanner.style.display = 'block';
    rogueBanner.innerHTML = `
      <div class="row" style="justify-content:space-between;margin:0">
        <div>
          <b>当前分身处于 ROGUE 隔离停机状态</b>：允许只读展示与出示凭证，但<b>添加美誉值 (Add)</b> 与<b>消费优先权 (Consume)</b> 要求先开启 <code>Faying</code> 上岗监护！
        </div>
        <button class="btn ok" onclick="activateFaying('${esc(fay.fay_id)}')">一键激活 Faying 上岗监护 (G2)</button>
      </div>
    `;
  } else {
    rogueBanner.style.display = 'none';
  }

  const rootShort = (m.last_state_root_hex || '').slice(0, 18);
  grid.innerHTML = `
    <div class="metric-box">
      <div class="muted">当前美誉值 curMerit(t)</div>
      <div class="metric-val" style="color:#38bdf8">${esc(m.cur_merit_formatted)} μ</div>
      <div class="muted">raw=${m.cur_merit_raw} (SCALE=10⁶)</div>
    </div>
    <div class="metric-box">
      <div class="muted">可消费活跃池 (curMerit - minMerit)</div>
      <div class="metric-val" style="color:#10b981">${esc(m.consumable_merit_formatted)} μ</div>
      <div class="muted">可用于投票/服务/算力优先权消减</div>
    </div>
    <div class="metric-box">
      <div class="muted">历史贡献底值 minMerit (只增不减)</div>
      <div class="metric-val" style="color:#f59e0b">${esc(m.min_merit_floor_formatted)} μ</div>
      <div class="muted">初始常数 e ≈ 2.72 μ · 消费绝不触碰底值</div>
    </div>
    <div class="metric-box">
      <div class="muted">最新区块高度 &amp; StateRoot</div>
      <div class="metric-val">#${m.last_block_height ?? 0}</div>
      <div class="muted"><code>${esc(rootShort)}…</code></div>
    </div>
  `;

  const items = (m.history || []).slice().reverse();
  if (!items.length) {
    histBox.innerHTML = '<p class="muted">当前尚无入账或优先权消费记录（初始宪章口袋：5.00 μ，底值 e = 2.72 μ，可消减 2.28 μ）。</p>';
  } else {
    histBox.innerHTML = `
      <table class="kv">
        <tr><th>单号</th><th>类型</th><th>领域链 / 优先权</th><th>变动 Δμ</th><th>变动后 curMerit</th><th>受保护 minMerit</th><th>高度 / 来源</th><th>说明</th></tr>
        ${items.map(it => {
          const isAdd = it.entry_kind === 'contribution_added';
          const afterMu = (Number(it.after_cur_merit_raw) / 1000000).toFixed(2);
          const floorMu = (Number(it.min_merit_floor_raw) / 1000000).toFixed(2);
          return `
            <tr>
              <td>${esc(it.record_id)}</td>
              <td><span class="pill ${isAdd ? 'ok' : 'warn'}">${isAdd ? '贡献铸造入账' : '优先权消减'}</span></td>
              <td>${esc(it.domain_or_priority)}${it.proposal_id ? ` (<code>${esc(it.proposal_id)}</code>)` : ''}</td>
              <td><b>${esc(it.delta_formatted)}</b></td>
              <td>${afterMu} μ</td>
              <td>${floorMu} μ</td>
              <td>#${it.block_height} · ${esc(it.source)}</td>
              <td>${esc(it.title)}</td>
            </tr>
          `;
        }).join('')}
      </table>
    `;
  }
}

async function syncGmchainStatus() {
  const gmchain_endpoint = document.getElementById('merit-rpc-ep').value.trim();
  const fayId = document.getElementById('merit-fay-select').value;
  try {
    const cfg = await api('/api/v1/merit/config', 'POST', { gmchain_endpoint });
    if (fayId) {
      await api(`/api/v1/fays/${encodeURIComponent(fayId)}/merit`, 'GET');
    }
    toast('已同步公誉链配置 (' + cfg.sync_mode + ')');
    await refreshAll();
  } catch (e) {
    toast(e.message);
  }
}

async function presentMeritCredential(privacy_tier) {
  const fayId = document.getElementById('merit-fay-select').value;
  if (!fayId) return toast('请先选择目标 Fay 实例');
  try {
    const cred = await api(`/api/v1/fays/${encodeURIComponent(fayId)}/merit/credential`, 'POST', { privacy_tier });
    const box = document.getElementById('merit-cred-preview');
    box.style.display = 'block';
    box.textContent = JSON.stringify(cred, null, 2);
    toast('已生成 ' + privacy_tier + ' 美誉凭证');
    await refreshAll();
  } catch (e) {
    toast(e.message);
  }
}

async function submitMeritAdd() {
  const fayId = document.getElementById('merit-fay-select').value;
  const fb = document.getElementById('merit-add-feedback');
  if (!fayId) return toast('请先创建并选择 Fay 实例');
  const title = document.getElementById('merit-add-title').value;
  const domain_chain_id = document.getElementById('merit-add-domain').value;
  const amount_mu = Number(document.getElementById('merit-add-amount').value || 0);
  const horizon_days = Number(document.getElementById('merit-add-horizon').value || 90);
  const summary = document.getElementById('merit-add-summary').value;
  try {
    const res = await api(`/api/v1/fays/${encodeURIComponent(fayId)}/merit/add`, 'POST', {
      title, domain_chain_id, amount_mu, horizon_days, summary
    });
    fb.innerHTML = `<div class="ctx-approval" style="border-color:var(--ok)"><b>贡献入账成功 (${esc(res.batch_id)})</b>：当前 <code>curMerit = ${esc(res.cur_merit_formatted)} μ</code>，可消费池 <code>${esc(res.consumable_merit_formatted)} μ</code>，保底 <code>minMerit = ${esc(res.min_merit_floor_formatted)} μ</code>。</div>`;
    toast('贡献已入账 +' + amount_mu + ' μ');
    await refreshAll();
  } catch (e) {
    fb.innerHTML = `<div class="ctx-alert"><b>入账被拦截</b>：${esc(e.message)}</div>`;
    toast(e.message);
  }
}

async function submitMeritConsume() {
  const fayId = document.getElementById('merit-fay-select').value;
  const fb = document.getElementById('merit-consume-feedback');
  if (!fayId) return toast('请先创建并选择 Fay 实例');
  const priority_kind = document.getElementById('merit-consume-kind').value;
  const proposal_id = document.getElementById('merit-consume-proposal').value.trim() || null;
  const amount_mu = Number(document.getElementById('merit-consume-amount').value || 0);
  const reason = document.getElementById('merit-consume-reason').value;
  try {
    const res = await api(`/api/v1/fays/${encodeURIComponent(fayId)}/merit/consume`, 'POST', {
      priority_kind, amount_mu, reason, proposal_id
    });
    const beforeMu = (Number(res.before_cur_merit_raw) / 1000000).toFixed(2);
    const afterMu = (Number(res.after_cur_merit_raw) / 1000000).toFixed(2);
    fb.innerHTML = `<div class="ctx-approval" style="border-color:var(--ok)"><b>优先权已生效 (${esc(res.consumption_id)})</b>：<code>curMerit</code> 由 <b>${beforeMu} μ → ${afterMu} μ</b>，历史底值 <code>minMerit = ${esc(res.min_merit_floor_formatted)} μ</code> <b>严格保持不变</b>！</div>`;
    toast('优先权已行使，消减 -' + amount_mu + ' μ');
    await refreshAll();
  } catch (e) {
    fb.innerHTML = `<div class="ctx-alert"><b>宪章底值保护 / 状态守卫拦截</b>：${esc(e.message)}</div>`;
    toast(e.message);
  }
}

function getHardwarePresetButtons(d) {
  if (d.device_id === 'drone_px4_01') {
    return {
      safeLabel: '安全起飞 takeoff 35m',
      safeAction: 'takeoff',
      safeParams: { altitude_m: 35, speed_mps: 6 },
      overLabel: '越界测试: 超限高 300m (>120m)',
      overAction: 'takeoff',
      overParams: { altitude_m: 300 }
    };
  }
  if (d.device_id === 'rccar_ackermann_01') {
    return {
      safeLabel: '安全巡航 drive +15° / 40%',
      safeAction: 'drive_control',
      safeParams: { steering_deg: 15, throttle_pct: 40 },
      overLabel: '越界测试: 超速 95% (>60%)',
      overAction: 'drive_control',
      overParams: { steering_deg: 0, throttle_pct: 95 }
    };
  }
  if (d.device_id === 'robot_arm_6dof_01') {
    return {
      safeLabel: '安全位姿 move_pose 500mm',
      safeAction: 'move_pose',
      safeParams: { reach_mm: 500, joint_speed_dps: 30 },
      overLabel: '越界测试: 超臂展 1200mm (>850mm)',
      overAction: 'move_pose',
      overParams: { reach_mm: 1200, joint_speed_dps: 30 }
    };
  }
  if (d.device_id === 'cnc_mill_5axis_01') {
    return {
      safeLabel: '启动主轴 8000 RPM [需CAP审批]',
      safeAction: 'start_spindle',
      safeParams: { spindle_rpm: 8000, feed_mm_min: 1200 },
      overLabel: '越界测试: 超速 25000 RPM (>12000)',
      overAction: 'start_spindle',
      overParams: { spindle_rpm: 25000 }
    };
  }
  if (d.device_id === 'camera_jib_pro_01') {
    return {
      safeLabel: '平滑运镜 sweep pan=45° tilt=20°',
      safeAction: 'sweep_keyframe',
      safeParams: { pan_deg: 45, tilt_deg: 20, angular_speed_dps: 18 },
      overLabel: '越界测试: 超俯仰 85° (>60°)',
      overAction: 'sweep_keyframe',
      overParams: { pan_deg: 0, tilt_deg: 85, angular_speed_dps: 20 }
    };
  }
  const act = (d.supported_actions && d.supported_actions[0]) || 'actuate';
  const envKeys = Object.keys(d.envelope || {});
  const safeParams = {};
  const overParams = {};
  if (envKeys.length > 0) {
    const k = envKeys[0];
    const b = d.envelope[k];
    safeParams[k] = Number(((b.min + b.max) / 2).toFixed(1));
    overParams[k] = b.max + 100;
  }
  return {
    safeLabel: `执行 ${act} (包络内)`,
    safeAction: act,
    safeParams,
    overLabel: `越界测试 (${act})`,
    overAction: act,
    overParams
  };
}

function renderHardwareTab() {
  const fays = STATE.fays || [];
  const sel = document.getElementById('hw-fay-select');
  const prev = sel.value;
  sel.innerHTML = fays.map(f => `<option value="${esc(f.fay_id)}">${esc(f.display_name)} (${esc(f.faying_state)}) — ${esc(f.instance_id)}</option>`).join('');
  if (prev && fays.some(f => f.fay_id === prev)) sel.value = prev;

  const devs = STATE.hardware_devices || [];
  const grid = document.getElementById('hw-devices-grid');
  grid.innerHTML = devs.map(d => {
    const isTierA = (d.integration_tier || '').startsWith('tier_a');
    const tierBadge = isTierA ? 'Tier A Linux/ROS2' : 'Tier B C99 Bridge';
    const stClass = d.current_state === 'ACTUATING' ? 'ok' : (d.current_state === 'READY' ? '' : 'danger');
    const envSummary = Object.entries(d.envelope || {}).map(([k, b]) => `${k}: [${b.min}, ${b.max}]${b.unit ? ' ' + b.unit : ''}`).join(' · ');
    const preset = getHardwarePresetButtons(d);
    const safeJson = encodeURIComponent(JSON.stringify(preset.safeParams));
    const overJson = encodeURIComponent(JSON.stringify(preset.overParams));

    return `
      <div class="card" style="margin-bottom:0">
        <div class="row" style="justify-content:space-between;margin-top:0">
          <div>
            <strong>${esc(d.display_name)}</strong>
            <code class="muted">${esc(d.device_id)}</code>
          </div>
          <div class="row" style="margin:0;gap:6px">
            <span class="pill ${isTierA ? 'merit-pill' : 'warn'}">${tierBadge}</span>
            <span class="pill ${stClass}">${esc(d.current_state)}</span>
          </div>
        </div>
        <div class="muted" style="margin-top:6px">
          附身分身: <b>${esc(d.embodied_fay_id || '未附身 (Detached)')}</b>
          · 安全停机态: <code>${esc(d.safe_state_name)}</code>
          ${d.bridge_endpoint ? ` · 实时固件桥: <code>${esc(d.bridge_endpoint)}</code>` : ' · 内置 FHBP/1.0 数字孪生'}
        </div>
        <div class="muted" style="margin-top:4px">
          物理安全包络 (CAP Envelope): <code>${esc(envSummary)}</code>
        </div>
        <pre class="code">${esc(JSON.stringify(d.telemetry, null, 2))}</pre>
        <div class="row" style="margin-top:10px;gap:6px">
          <button class="btn secondary" onclick="embodyHardware('${esc(d.device_id)}', false)">附身此硬件 (Embody)</button>
          ${d.embodied_fay_id ? `<button class="btn secondary" onclick="embodyHardware('${esc(d.device_id)}', true)">解除附身</button>` : ''}
          <button class="btn ok" onclick="actuateHardware('${esc(d.device_id)}', '${esc(preset.safeAction)}', '${safeJson}')">${esc(preset.safeLabel)}</button>
          <button class="btn secondary" style="border-color:var(--danger);color:#fca5a5" onclick="actuateHardware('${esc(d.device_id)}', '${esc(preset.overAction)}', '${overJson}')">${esc(preset.overLabel)}</button>
        </div>
      </div>
    `;
  }).join('');
}

async function connectExternalHardware() {
  const endpoint = document.getElementById('hw-connect-ep').value.trim();
  const fb = document.getElementById('hw-feedback');
  try {
    const dev = await api('/api/v1/hardware/connect', 'POST', { endpoint });
    fb.innerHTML = `<div class="ctx-approval" style="border-color:var(--ok)"><b>已接入外部 FHBP/1.0 固件节点</b>：<code>${esc(dev.device_id)}</code> (${esc(dev.display_name)}) @ <code>${esc(dev.bridge_endpoint || endpoint)}</code></div>`;
    toast('已连接外部硬件节点: ' + dev.device_id);
    await refreshAll();
  } catch (e) {
    fb.innerHTML = `<div class="ctx-alert"><b>外部硬件节点连接失败</b>：${esc(e.message)}</div>`;
    toast(e.message);
  }
}

async function embodyHardware(deviceId, detach) {
  const fb = document.getElementById('hw-feedback');
  const fay_id = detach ? null : document.getElementById('hw-fay-select').value;
  if (!detach && !fay_id) return toast('请先在 Tab 3 创建分身实例');
  try {
    const dev = await api(`/api/v1/hardware/devices/${encodeURIComponent(deviceId)}/embody`, 'POST', { fay_id });
    fb.innerHTML = `<div class="ctx-approval" style="border-color:var(--ok)"><b>硬件附身关系已更新</b>：<code>${esc(dev.device_id)}</code> → <code>${esc(dev.embodied_fay_id || '已解除附身')}</code></div>`;
    toast(detach ? '已解除硬件附身' : ('已附身 ' + deviceId));
    await refreshAll();
  } catch (e) {
    fb.innerHTML = `<div class="ctx-alert">${esc(e.message)}</div>`;
    toast(e.message);
  }
}

async function actuateHardware(deviceId, action, encodedParams) {
  const fb = document.getElementById('hw-feedback');
  const params = JSON.parse(decodeURIComponent(encodedParams));
  try {
    const res = await api(`/api/v1/hardware/devices/${encodeURIComponent(deviceId)}/actuate`, 'POST', { action, params });
    if (res.status === 'pending_cap_approval') {
      fb.innerHTML = `<div class="ctx-approval"><b>高危硬件工序已挂起等待 CAP 人类原型确认 (${esc(res.approval_id)})</b>：请在 Tab 4 或 <a href="/mobile" target="_blank" style="color:var(--accent)">📱 手机监护端 (/mobile)</a> 点击批准执行！</div>`;
      toast('已转入 CAP 待审批: ' + res.approval_id);
    } else {
      fb.innerHTML = `<div class="ctx-approval" style="border-color:var(--ok)"><b>硬件驱动执行成功</b>：${esc(res.message)}</div>`;
      toast('已执行硬件动作: ' + action);
    }
    await refreshAll();
  } catch (e) {
    fb.innerHTML = `<div class="ctx-alert"><b>CAP 物理安全包络 / Faying 门禁拦截</b>：${esc(e.message)}</div>`;
    toast('已拦截: ' + e.message);
    await refreshAll();
  }
}

async function haltAllHardwareG0() {
  const fb = document.getElementById('hw-feedback');
  try {
    const res = await api('/api/v1/hardware/g0_all', 'POST', {
      freeze_all_fays: false,
      reason: 'Human Prime clicked Hardware Hub G0 Emergency Stop'
    });
    fb.innerHTML = `<div class="ctx-alert"><b>G0 全硬件紧急安全停机已生效</b>：${res.halted_devices.length} 台硬件已强制切入物理安全态 (${esc(res.halted_devices.join(', '))})</div>`;
    toast('全硬件已切入 G0 物理安全态');
    await refreshAll();
  } catch (e) {
    toast(e.message);
  }
}

function esc(str) {
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

refreshAll();
</script>
</body>
</html>
"#;
