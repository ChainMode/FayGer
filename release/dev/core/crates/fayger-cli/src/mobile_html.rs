//! 手机伴随监护端 PWA（`Mobile Companion Guardian` · `/mobile` 与 `/manifest.webmanifest`）。
//!
//! 专为 iPhone / Android 触控单手监护设计（零外部 CDN 依赖，桌面端居中预览 `max-width: 480px`，手机端全屏沉浸）：
//! - **超大拇指热区红色 `G0` 一键全端急刹按钮**：瞬间将分身退回 `ROGUE` 并级联触发全部具身硬件进入物理安全态（`G0_SAFE_HALT`）；
//! - **分身上岗监护拨杆**：手机远程切换 `ROGUE <-> FAYING` 与心跳续期；
//! - **远程 `CAP` 实时审批收件箱**：一键批准或拒绝桌面高危命令与五轴机床 `start_spindle` 等高危硬件动作；
//! - **具身硬件遥测与安全态监控**：实时查看无人机、遥控赛车、协作机器人、五轴机床、摄影机摇臂及外部 C99 节点状态；
//! - **轻量移动 `ICP` 对话、`FayID dync_` 动态码与 `MeriToken` (`μ`) 美誉值胶囊**。

/// 返回 `/manifest.webmanifest` Web App Manifest JSON 文本。
#[must_use]
pub const fn mobile_webmanifest_json() -> &'static str {
    MOBILE_WEBMANIFEST_JSON
}

/// 返回 `/mobile` 手机伴随监护端完整单页 HTML 文本。
#[must_use]
pub const fn mobile_guardian_html() -> &'static str {
    MOBILE_GUARDIAN_HTML
}

const MOBILE_WEBMANIFEST_JSON: &str = r##"{
  "name": "FayGer Mobile Guardian",
  "short_name": "FayGer",
  "description": "Human Prime Mobile Companion Guardian for iFay, CAP Approvals, G0 Emergency Brake & Embodied Hardware",
  "start_url": "/mobile",
  "display": "standalone",
  "background_color": "#090d16",
  "theme_color": "#0f172a"
}
"##;

const MOBILE_GUARDIAN_HTML: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, viewport-fit=cover" />
<meta name="apple-mobile-web-app-capable" content="yes" />
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
<meta name="theme-color" content="#0f172a" />
<link rel="manifest" href="/manifest.webmanifest" />
<title>FayGer Mobile Guardian (v0.3.1)</title>
<style>
  :root {
    --bg: #090d16;
    --card: #111827;
    --card-alt: #1e293b;
    --border: #334155;
    --text: #f8fafc;
    --muted: #94a3b8;
    --accent: #38bdf8;
    --ok: #10b981;
    --warn: #f59e0b;
    --danger: #ef4444;
    --mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  * { box-sizing: border-box; -webkit-tap-highlight-color: transparent; }
  body {
    margin: 0;
    background: #05080f;
    color: var(--text);
    font-family: var(--sans);
    line-height: 1.45;
  }
  .shell {
    max-width: 480px;
    min-height: 100vh;
    margin: 0 auto;
    background: var(--bg);
    border-left: 1px solid #1e293b;
    border-right: 1px solid #1e293b;
    padding-bottom: 40px;
  }
  header {
    position: sticky;
    top: 0;
    z-index: 20;
    background: rgba(15, 23, 42, 0.94);
    backdrop-filter: blur(10px);
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .hdr-title { font-size: 15px; font-weight: 800; }
  .hdr-sub { font-size: 11px; color: var(--muted); font-family: var(--mono); }
  .pill {
    display: inline-block;
    font-size: 11px;
    font-family: var(--mono);
    padding: 3px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--card);
  }
  .pill.ok { border-color: var(--ok); color: #6ee7b7; }
  .pill.warn { border-color: var(--warn); color: #fcd34d; }
  .pill.danger { border-color: var(--danger); color: #fca5a5; }
  .sec { padding: 12px 14px 0; }
  .card {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 14px;
    margin-bottom: 12px;
  }
  .card h2 {
    margin: 0 0 8px;
    font-size: 14px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .g0-btn {
    width: 100%;
    padding: 18px 16px;
    border-radius: 16px;
    border: 2px solid #fca5a5;
    background: linear-gradient(180deg, #ef4444, #b91c1c);
    color: #fff;
    font-size: 17px;
    font-weight: 900;
    letter-spacing: 0.3px;
    cursor: pointer;
    box-shadow: 0 6px 24px rgba(239, 68, 68, 0.45);
  }
  .g0-btn:active { transform: scale(0.985); }
  .btn {
    border: none;
    border-radius: 10px;
    padding: 10px 13px;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    background: var(--accent);
    color: #090d16;
  }
  .btn.ok { background: var(--ok); color: #052e16; }
  .btn.danger { background: var(--danger); color: #fff; }
  .btn.secondary { background: var(--card-alt); color: var(--text); border: 1px solid var(--border); }
  .row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  select, input[type="text"] {
    width: 100%;
    background: #090d16;
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    font-size: 14px;
  }
  .hw-item {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px;
    margin-top: 8px;
    background: #0b1120;
  }
  .apr-item {
    border: 1px solid var(--warn);
    background: rgba(245, 158, 11, 0.1);
    border-radius: 10px;
    padding: 10px;
    margin-top: 8px;
  }
  .muted { color: var(--muted); font-size: 12px; }
  #toast {
    position: fixed;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    background: #1e293b;
    border: 1px solid var(--accent);
    color: var(--text);
    padding: 9px 15px;
    border-radius: 999px;
    font-size: 12px;
    display: none;
    z-index: 99;
  }
</style>
</head>
<body>
<div class="shell">
  <header>
    <div>
      <div class="hdr-title">fayger v0.3.1 · Mobile Guardian</div>
      <div class="hdr-sub">人类原型随身监护端 · <a href="/" style="color:var(--accent)">返回桌面端</a></div>
    </div>
    <span class="pill ok" id="m-audit">Chain OK</span>
  </header>

  <div class="sec">
    <!-- Giant Thumb-Friendly Red G0 Button -->
    <div class="card" style="padding:10px;border-color:#7f1d1d">
      <button class="g0-btn" onclick="triggerGlobalG0()">
        🛑 G0 一键全端急刹 (FREEZE ALL + SAFE HALT)
      </button>
      <div class="muted" style="text-align:center;margin-top:6px">
        立即冻结活跃分身退回 <code>ROGUE</code>，并强制全部具身硬件切入物理安全态
      </div>
    </div>

    <!-- Instance Faying Switcher & Merit Standing -->
    <div class="card">
      <h2>
        <span>分身上岗监护拨杆 (Faying Switch)</span>
        <span class="pill" id="m-merit-pill">μ --</span>
      </h2>
      <select id="m-fay-select" onchange="renderMobile()"></select>
      <div id="m-fay-controls" style="margin-top:10px"></div>
      <div class="row" style="margin-top:10px;justify-content:space-between">
        <button class="btn secondary" style="flex:1" onclick="issueMobileDync()">生成一次性 FayID 动态码 (dync_)</button>
        <button class="btn secondary" onclick="refreshMobile()">刷新</button>
      </div>
      <div id="m-dync-out" class="muted" style="margin-top:6px;font-family:var(--mono)"></div>
    </div>

    <!-- Remote CAP Approval Inbox -->
    <div class="card">
      <h2>
        <span>远程 CAP 高危审批收件箱</span>
        <span class="pill warn" id="m-apr-count">0 待批</span>
      </h2>
      <div id="m-approvals-box"></div>
    </div>

    <!-- Embodied Hardware Quick Telemetry & Safe-Halt Monitor -->
    <div class="card">
      <h2>
        <span>具身硬件遥测与安全态监控 (FHBP/1.0)</span>
        <span class="pill" id="m-hw-count">0 台</span>
      </h2>
      <div id="m-hardware-box"></div>
    </div>

    <!-- Mobile ICP Compact Chat -->
    <div class="card">
      <h2><span>移动 ICP 轻量对话 (.ctx)</span></h2>
      <div class="row">
        <input type="text" id="m-chat-input" placeholder="发送随身指令（如 /hardware 或 查询美誉值 μ）" />
      </div>
      <div class="row" style="margin-top:8px">
        <button class="btn" style="width:100%" onclick="sendMobileChat()">发送 ICP 指令</button>
      </div>
      <div id="m-chat-box" style="margin-top:10px"></div>
    </div>
  </div>
</div>

<div id="toast"></div>

<script>
let STATE = null;

function toast(msg) {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.style.display = 'block';
  setTimeout(() => { el.style.display = 'none'; }, 2600);
}

async function api(path, method = 'GET', body = null) {
  const opts = { method, headers: { 'Content-Type': 'application/json' } };
  if (body !== null) opts.body = JSON.stringify(body);
  const res = await fetch(path, opts);
  const data = await res.json();
  if (!res.ok) throw new Error(data.error || ('HTTP ' + res.status));
  return data;
}

async function refreshMobile() {
  try {
    STATE = await api('/api/v1/state');
    const sel = document.getElementById('m-fay-select');
    const prev = sel.value;
    const fays = STATE.fays || [];
    sel.innerHTML = fays.map(f => `<option value="${esc(f.fay_id)}">${esc(f.display_name)} [${esc(f.faying_state)}]</option>`).join('');
    if (prev && fays.some(f => f.fay_id === prev)) sel.value = prev;
    renderMobile();
  } catch (e) {
    toast(e.message);
  }
}

function renderMobile() {
  if (!STATE) return;
  const auditEl = document.getElementById('m-audit');
  auditEl.textContent = STATE.audit_chain_verified ? 'SHA-256 OK' : 'TAMPERED';
  auditEl.className = 'pill ' + (STATE.audit_chain_verified ? 'ok' : 'danger');

  const fays = STATE.fays || [];
  const fay = fays.find(f => f.fay_id === document.getElementById('m-fay-select').value) || fays[0];
  const ctl = document.getElementById('m-fay-controls');
  const meritPill = document.getElementById('m-merit-pill');

  if (!fay) {
    meritPill.textContent = '未挂载分身';
    ctl.innerHTML = '<button class="btn" style="width:100%" onclick="quickCreateMobileIfay()">一键创建个人 iFay</button>';
  } else {
    const m = fay.merit || {};
    meritPill.textContent = `${m.cur_merit_formatted || '5.00'} μ (可消减 ${m.consumable_merit_formatted || '2.28'} μ)`;
    const isFaying = fay.faying_state === 'FAYING';
    ctl.innerHTML = `
      <div class="row" style="justify-content:space-between">
        <span class="pill ${isFaying ? 'ok' : 'danger'}">状态: ${esc(fay.faying_state)}</span>
        ${isFaying
          ? `<button class="btn danger" onclick="brakeSingleFay('${esc(fay.fay_id)}')">切回 ROGUE 停机</button>`
          : `<button class="btn ok" onclick="activateMobileFaying('${esc(fay.fay_id)}')">开启 Faying 监护 (G2)</button>`
        }
      </div>
    `;
    const turns = (fay.chat_history || []).slice(-3).reverse();
    document.getElementById('m-chat-box').innerHTML = turns.map(t => `
      <div class="hw-item">
        <div class="muted">You: ${esc(t.user_input)}</div>
        <div style="font-size:13px;margin-top:4px">${esc(t.response_ctx.summary_text)}</div>
      </div>
    `).join('');
  }

  const pending = (STATE.cap_approvals || []).filter(a => a.status === 'pending');
  document.getElementById('m-apr-count').textContent = pending.length + ' 待批';
  const aprBox = document.getElementById('m-approvals-box');
  if (!pending.length) {
    aprBox.innerHTML = '<div class="muted">当前无待审批高危操作。</div>';
  } else {
    aprBox.innerHTML = pending.map(a => `
      <div class="apr-item">
        <div style="font-size:13px"><b>${esc(a.approval_id)}</b> · <code>${esc(a.required_capability)}</code></div>
        <div class="muted" style="margin:4px 0">${esc(JSON.stringify(a.action))}</div>
        <div class="row">
          <button class="btn ok" style="flex:1" onclick="decideMobileCap('${esc(a.approval_id)}', true)">批准执行</button>
          <button class="btn danger" style="flex:1" onclick="decideMobileCap('${esc(a.approval_id)}', false)">拒绝</button>
        </div>
      </div>
    `).join('');
  }

  const devs = STATE.hardware_devices || [];
  document.getElementById('m-hw-count').textContent = devs.length + ' 台';
  document.getElementById('m-hardware-box').innerHTML = devs.map(d => {
    const stTone = d.current_state === 'ACTUATING' ? 'ok' : (d.current_state === 'READY' ? '' : 'danger');
    return `
      <div class="hw-item">
        <div class="row" style="justify-content:space-between">
          <strong style="font-size:13px">${esc(d.display_name)}</strong>
          <span class="pill ${stTone}">${esc(d.current_state)}</span>
        </div>
        <div class="muted" style="margin-top:4px;font-family:var(--mono)">
          附身: ${esc(d.embodied_fay_id || '未附身')} · 遥测: ${esc(JSON.stringify(d.telemetry))}
        </div>
      </div>
    `;
  }).join('');
}

async function triggerGlobalG0() {
  try {
    const res = await api('/api/v1/hardware/g0_all', 'POST', {
      freeze_all_fays: true,
      reason: 'Human Prime tapped Red G0 Global Emergency Brake on Mobile Guardian'
    });
    toast(`G0 急刹已生效：${res.halted_devices.length} 台硬件进入安全态`);
    await refreshMobile();
  } catch (e) { toast(e.message); }
}

async function quickCreateMobileIfay() {
  try {
    await api('/api/v1/fays/create', 'POST', { kind: 'ifay', name: 'MobilePrimeSoul' });
    await refreshMobile();
  } catch (e) { toast(e.message); }
}

async function activateMobileFaying(fayId) {
  try {
    await api(`/api/v1/fays/${encodeURIComponent(fayId)}/faying`, 'POST', { grade: 'G2' });
    toast('Faying 上岗监护已开启');
    await refreshMobile();
  } catch (e) { toast(e.message); }
}

async function brakeSingleFay(fayId) {
  try {
    await api(`/api/v1/fays/${encodeURIComponent(fayId)}/faying/g0`, 'POST', {
      reason: 'Human Prime switched Fay to ROGUE on Mobile Guardian'
    });
    toast('已退回 ROGUE 并级联停机附身硬件');
    await refreshMobile();
  } catch (e) { toast(e.message); }
}

async function decideMobileCap(aprId, approve) {
  try {
    await api(`/api/v1/cap/approvals/${encodeURIComponent(aprId)}/decide`, 'POST', { approve });
    toast(approve ? '已远程批准执行' : '已远程拒绝');
    await refreshMobile();
  } catch (e) { toast(e.message); }
}

async function issueMobileDync() {
  try {
    const t = await api('/api/v1/wallet/dynamic-code', 'POST', { purpose: 'mobile_guardian_verify', ttl_secs: 300 });
    document.getElementById('m-dync-out').textContent = 'Dynamic Code: ' + t.dynamic_code;
    toast('已签发不可关联动态码');
  } catch (e) { toast(e.message); }
}

async function sendMobileChat() {
  const fayId = document.getElementById('m-fay-select').value;
  if (!fayId) return toast('请先创建分身');
  const message = document.getElementById('m-chat-input').value.trim();
  if (!message) return;
  try {
    await api(`/api/v1/fays/${encodeURIComponent(fayId)}/chat`, 'POST', { message });
    document.getElementById('m-chat-input').value = '';
    await refreshMobile();
  } catch (e) { toast(e.message); }
}

function esc(s) {
  return String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

refreshMobile();
</script>
</body>
</html>
"##;
