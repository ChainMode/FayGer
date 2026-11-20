//! 内嵌离线可用的 `FayGer Dashboard` 单页可视化控制台（零外部 CDN 依赖）。
//!
//! 提供五大核心工作台标签页：
//! 1. 设备环境与资源池（`Overview & Device Profile`）
//! 2. 身份与凭据保险箱（`FayID Wallet & Auth Exchange`）
//! 3. 分身实例与上岗监护（`Running Fays & Faying Monitor` + 红色 `G0` 一键急刹）
//! 4. `ICP` 富交互对话与 `CAP` 审批（`ICP Interactive Console`）
//! 5. 双 `Fay` 心灵感应协作间（`Telepathy (TP) Shared Workspace`）

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
      <small>Multi-Tenant Digital Soul Engine · CAP Host Gatekeeper · ICP (.ctx) · Telepathy (TP)</small>
    </div>
  </div>
  <div class="status-pills">
    <span class="pill" id="hdr-version">fayger ...</span>
    <span class="pill ok" id="hdr-audit">Audit Chain: verifying...</span>
    <span class="pill" id="hdr-ego">Ego Slot: ...</span>
    <button class="btn secondary" onclick="refreshAll()">Refresh</button>
  </div>
</header>

<nav class="tabs">
  <button class="active" data-tab="tab-overview" onclick="switchTab('tab-overview', this)">1. 设备环境与资源池 (Overview)</button>
  <button data-tab="tab-wallet" onclick="switchTab('tab-wallet', this)">2. 身份与凭据保险箱 (FayID Wallet)</button>
  <button data-tab="tab-fays" onclick="switchTab('tab-fays', this)">3. 分身实例与上岗监护 (Running Fays &amp; G0 Brake)</button>
  <button data-tab="tab-icp" onclick="switchTab('tab-icp', this)">4. ICP 富交互对话与 CAP 审批</button>
  <button data-tab="tab-tp" onclick="switchTab('tab-tp', this)">5. 双 Fay 心灵感应协作间 (Telepathy TP)</button>
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
        <div class="row">
          <label class="muted">Active Fay:</label>
          <select id="chat-fay-select" onchange="renderSelectedChat()"></select>
        </div>
        <div class="row">
          <span class="muted">Quick Sandbox Triggers:</span>
          <button class="btn secondary" onclick="fillPrompt('/write notes/todo.md Ship FayGer MVP')">Routine Write</button>
          <button class="btn secondary" onclick="fillPrompt('/read notes/todo.md')">Routine Read</button>
          <button class="btn secondary" onclick="fillPrompt('/exec workspace.ls')">High-Risk Exec (CAP Hold)</button>
          <button class="btn secondary" onclick="fillPrompt('/camera desk_view')">High-Risk Camera (CAP Hold)</button>
        </div>
        <textarea id="chat-input" placeholder="Send message or /write, /read, /exec, /camera..."></textarea>
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
</main>

<div id="toast"></div>

<script>
const KNOWN_CAPS = ["fs.read", "fs.write", "process.exec", "net.http", "media.camera", "media.mic"];
let STATE = null;

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
      return `
        <div class="fay-card ${isFaying ? 'faying' : 'rogue'}">
          <div class="row" style="justify-content:space-between">
            <div>
              <strong>${esc(f.display_name)}</strong>
              <span class="pill">${esc(f.kind.toUpperCase())}</span>
              <code class="muted">${esc(f.fay_id)}</code>
              <span class="pill ${isFaying ? 'ok' : 'danger'}">${esc(f.faying_state)}</span>
            </div>
            <div class="row">
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
  const fay = (STATE.fays || []).find(f => f.fay_id === fayId);
  if (!fay) {
    stream.innerHTML = '<p class="muted">Create or mount a Fay first.</p>';
    return;
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
          ICP <code>${esc(c.schema_version)}</code> · Ego: <code>${esc(c.ego_trace.mode)}</code> (intent=${esc(c.ego_trace.intuition_intent)})
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
  return '';
}

function fillPrompt(txt) {
  document.getElementById('chat-input').value = txt;
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
