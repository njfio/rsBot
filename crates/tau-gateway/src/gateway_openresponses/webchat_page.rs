//! Webchat HTML renderer for the gateway operator shell.
use super::*;

const DASHBOARD_PAGE_TEMPLATE: &str = r###"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Tau Operator Dashboard</title>
  <style>
    :root {
      color-scheme: light;
      --ink: #0e1a21;
      --ink-muted: #314b5f;
      --line: #c6d4df;
      --surface: #f5f7fa;
      --panel: #ffffff;
      --accent: #0d7f67;
      --accent-soft: #d5f4ec;
      --warn: #c06116;
      --bad: #b42318;
      --good: #127a3f;
      --shadow: 0 16px 36px rgba(18, 30, 39, 0.08);
      font-family: "Space Grotesk", "IBM Plex Sans", "Segoe UI", sans-serif;
    }
    * {
      box-sizing: border-box;
    }
    body {
      margin: 0;
      color: var(--ink);
      background:
        radial-gradient(circle at 10% 5%, #e8f6f2 0, rgba(232, 246, 242, 0) 42%),
        radial-gradient(circle at 95% 0%, #fbead8 0, rgba(251, 234, 216, 0) 35%),
        linear-gradient(165deg, #eff3f7 0%, #f9fbfd 100%);
      min-height: 100vh;
      animation: page-intro 420ms ease-out;
    }
    .shell {
      max-width: 1240px;
      margin: 0 auto;
      padding: 1.2rem 1rem 1.8rem;
    }
    .hero {
      border: 1px solid var(--line);
      border-radius: 16px;
      background: linear-gradient(130deg, rgba(13, 127, 103, 0.12) 0%, rgba(13, 127, 103, 0) 45%), var(--panel);
      box-shadow: var(--shadow);
      padding: 1rem 1.1rem;
      display: grid;
      grid-template-columns: 1fr auto;
      gap: 1rem;
      align-items: center;
    }
    .hero h1 {
      margin: 0;
      letter-spacing: 0.01em;
      font-size: clamp(1.15rem, 2.3vw, 1.7rem);
    }
    .hero p {
      margin: 0.35rem 0 0;
      color: var(--ink-muted);
      font-size: 0.95rem;
    }
    .hero-links {
      display: flex;
      flex-wrap: wrap;
      gap: 0.45rem;
    }
    .hero-links a {
      text-decoration: none;
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 0.32rem 0.72rem;
      color: #184158;
      background: #fdfefe;
      font-size: 0.84rem;
      font-weight: 600;
    }
    .cards {
      margin-top: 0.95rem;
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 0.65rem;
    }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 0.75rem 0.8rem;
      box-shadow: var(--shadow);
      min-height: 92px;
    }
    .card .k {
      color: var(--ink-muted);
      font-size: 0.78rem;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      display: block;
      margin-bottom: 0.3rem;
    }
    .card .v {
      font-size: 1.25rem;
      font-weight: 700;
      line-height: 1.15;
      display: block;
    }
    .card .meta {
      margin-top: 0.25rem;
      font-size: 0.82rem;
      color: #3f5d6f;
    }
    .layout {
      margin-top: 0.85rem;
      display: grid;
      grid-template-columns: minmax(290px, 380px) 1fr;
      gap: 0.75rem;
      align-items: start;
    }
    .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 0.85rem;
      box-shadow: var(--shadow);
    }
    .panel h2 {
      margin: 0;
      font-size: 0.98rem;
      letter-spacing: 0.01em;
    }
    .muted {
      color: var(--ink-muted);
    }
    .status-pill {
      display: inline-flex;
      align-items: center;
      gap: 0.3rem;
      padding: 0.18rem 0.52rem;
      border-radius: 999px;
      font-size: 0.78rem;
      font-weight: 700;
      letter-spacing: 0.02em;
      border: 1px solid var(--line);
      background: #f5faf8;
      color: #115e50;
    }
    .status-pill.degraded {
      background: #fff6eb;
      color: #8f4708;
      border-color: #f3d7b4;
    }
    .status-pill.failing {
      background: #fff1f1;
      color: #8f1a1a;
      border-color: #e9c2c2;
    }
    .control-grid {
      margin-top: 0.65rem;
      display: grid;
      gap: 0.58rem;
    }
    .control-row {
      display: grid;
      gap: 0.45rem;
    }
    .control-row.two {
      grid-template-columns: 1fr 1fr;
    }
    label {
      font-size: 0.77rem;
      color: #385668;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      font-weight: 600;
    }
    input[type="text"],
    textarea {
      width: 100%;
      border: 1px solid #b4c7d6;
      background: #fbfdff;
      color: var(--ink);
      border-radius: 10px;
      padding: 0.5rem 0.6rem;
      font-size: 0.93rem;
      font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    }
    textarea {
      min-height: 90px;
      resize: vertical;
    }
    .inline-check {
      display: inline-flex;
      gap: 0.35rem;
      align-items: center;
      font-size: 0.86rem;
      color: #274759;
    }
    .actions {
      margin-top: 0.2rem;
      display: flex;
      flex-wrap: wrap;
      gap: 0.42rem;
    }
    button {
      border: 0;
      border-radius: 9px;
      background: linear-gradient(130deg, #0d7f67 0%, #086753 100%);
      color: #fff;
      font-weight: 650;
      padding: 0.47rem 0.78rem;
      cursor: pointer;
      box-shadow: 0 8px 18px rgba(13, 127, 103, 0.2);
      font-size: 0.86rem;
    }
    button.subtle {
      background: linear-gradient(130deg, #3d5c72 0%, #2e475a 100%);
      box-shadow: 0 8px 18px rgba(35, 54, 69, 0.18);
    }
    button.warn {
      background: linear-gradient(130deg, #b15618 0%, #8f430f 100%);
      box-shadow: 0 8px 18px rgba(159, 74, 18, 0.2);
    }
    button:disabled {
      opacity: 0.6;
      cursor: wait;
    }
    .audit-log {
      margin-top: 0.65rem;
      border: 1px solid #d4dee5;
      border-radius: 10px;
      max-height: 200px;
      overflow: auto;
      background: #f8fbfd;
      padding: 0.4rem 0.52rem;
    }
    .audit-item {
      border-bottom: 1px solid #e1e9ef;
      padding: 0.38rem 0;
      font-size: 0.84rem;
    }
    .audit-item:last-child {
      border-bottom: 0;
    }
    .audit-item .time {
      color: #4a697c;
      font-size: 0.75rem;
    }
    .audit-item .tag {
      font-weight: 700;
    }
    .audit-item.ok .tag {
      color: var(--good);
    }
    .audit-item.fail .tag {
      color: var(--bad);
    }
    .stack {
      display: grid;
      gap: 0.62rem;
    }
    .list-feed {
      margin: 0;
      padding: 0;
      list-style: none;
      display: grid;
      gap: 0.3rem;
    }
    .list-feed li {
      border: 1px solid #d8e3eb;
      border-radius: 9px;
      background: #fbfdff;
      padding: 0.4rem 0.5rem;
      font-size: 0.83rem;
      color: #19384d;
    }
    .transport-table {
      width: 100%;
      border-collapse: collapse;
      margin-top: 0.2rem;
      font-size: 0.8rem;
    }
    .transport-table th,
    .transport-table td {
      text-align: left;
      padding: 0.38rem 0.35rem;
      border-bottom: 1px solid #e2ebf1;
      vertical-align: top;
    }
    .transport-table th {
      color: #436376;
      font-size: 0.74rem;
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }
    pre {
      margin: 0;
      border-radius: 10px;
      border: 1px solid #d0dde6;
      background: #0f202b;
      color: #dceaf4;
      padding: 0.68rem;
      max-height: 220px;
      overflow: auto;
      white-space: pre-wrap;
      word-break: break-word;
      font-size: 0.8rem;
      line-height: 1.4;
      font-family: "IBM Plex Mono", "SFMono-Regular", Consolas, monospace;
    }
    .hint {
      font-size: 0.78rem;
      color: #456476;
    }
    .panel-top {
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 0.5rem;
      flex-wrap: wrap;
    }
    .last-refresh {
      font-size: 0.76rem;
      color: #4f6a7d;
    }
    @media (max-width: 1080px) {
      .cards {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
      .layout {
        grid-template-columns: 1fr;
      }
    }
    @media (max-width: 640px) {
      .hero {
        grid-template-columns: 1fr;
      }
      .cards {
        grid-template-columns: 1fr;
      }
      .control-row.two {
        grid-template-columns: 1fr;
      }
    }
    @keyframes page-intro {
      from {
        opacity: 0;
        transform: translateY(8px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <div>
        <h1>Tau Operator Dashboard</h1>
        <p>Live control plane for gateway health, transport signals, queue pressure, and operator actions.</p>
      </div>
      <nav class="hero-links">
        <a href="__DASHBOARD_ENDPOINT__">Dashboard Home</a>
        <a href="__WEBCHAT_ENDPOINT__">Fallback Webchat</a>
        <a href="__STATUS_ENDPOINT__">Gateway Status JSON</a>
        <a href="__WEBSOCKET_ENDPOINT__">WebSocket Control Plane</a>
      </nav>
    </section>

    <section class="cards">
      <article class="card">
        <span class="k">Gateway Service</span>
        <span class="v" id="serviceStatus">unknown</span>
        <div class="meta">Rollout gate: <strong id="rolloutGate">hold</strong></div>
      </article>
      <article class="card">
        <span class="k">Multi-Channel Health</span>
        <span class="v" id="healthState">unknown</span>
        <div class="meta">Reason: <span id="healthReason">unavailable</span></div>
      </article>
      <article class="card">
        <span class="k">Queue + Failures</span>
        <span class="v"><span id="queueDepth">0</span> / <span id="failureStreak">0</span></span>
        <div class="meta">queue_depth / failure_streak</div>
      </article>
      <article class="card">
        <span class="k">Auth + Sessions</span>
        <span class="v"><span id="authMode">unknown</span></span>
        <div class="meta">active=<span id="activeSessions">0</span> failures=<span id="authFailures">0</span></div>
      </article>
    </section>

    <section class="layout">
      <section class="panel">
        <div class="panel-top">
          <h2>Operator Controls</h2>
          <span id="connectionPill" class="status-pill">disconnected</span>
        </div>
        <div class="control-grid">
          <div class="control-row two">
            <div>
              <label for="authToken">Bearer token</label>
              <input id="authToken" type="text" autocomplete="off" placeholder="gateway auth token" />
            </div>
            <div>
              <label for="sessionKey">Session key</label>
              <input id="sessionKey" type="text" autocomplete="off" value="__DEFAULT_SESSION_KEY__" />
            </div>
          </div>
          <div class="control-row">
            <label for="prompt">Prompt</label>
            <textarea id="prompt" placeholder="Send a new operator prompt through /v1/responses"></textarea>
          </div>
          <div class="control-row">
            <label class="inline-check">
              <input id="stream" type="checkbox" checked />
              Stream response with SSE
            </label>
            <label class="inline-check">
              <input id="autoRefresh" type="checkbox" checked />
              Auto-refresh status every 8s
            </label>
          </div>
          <div class="actions">
            <button id="send">Send Prompt</button>
            <button id="refreshStatus" class="subtle">Refresh Status</button>
            <button id="resetSession" class="warn" disabled>Reset Session</button>
            <button id="clearOutput" class="subtle">Clear Output</button>
          </div>
          <p class="hint">Actions write a local audit feed and attempt live status refresh after completion.</p>
        </div>

        <div class="audit-log" id="auditFeed">
          <div class="audit-item">
            <div class="tag">no-audit-events</div>
            <div class="time">Run a control action to populate operator feedback.</div>
          </div>
        </div>
      </section>

      <section class="stack">
        <section class="panel">
          <div class="panel-top">
            <h2>Transport Table</h2>
            <span class="last-refresh">last_update=<span id="lastUpdate">never</span></span>
          </div>
          <table class="transport-table">
            <thead>
              <tr>
                <th>transport</th>
                <th>liveness</th>
                <th>breaker</th>
                <th>events</th>
                <th>errors</th>
              </tr>
            </thead>
            <tbody id="transportRows">
              <tr><td colspan="5" class="muted">No connector data yet.</td></tr>
            </tbody>
          </table>
        </section>

        <section class="panel">
          <h2>Reason Codes</h2>
          <ul class="list-feed" id="reasonFeed">
            <li class="muted">No reason codes recorded yet.</li>
          </ul>
        </section>

        <section class="panel">
          <h2>Diagnostics</h2>
          <ul class="list-feed" id="diagnosticFeed">
            <li class="muted">No diagnostics recorded.</li>
          </ul>
        </section>

        <section class="panel">
          <h2>Response Output</h2>
          <pre id="output">No response yet.</pre>
        </section>

        <section class="panel">
          <h2>Gateway Status Snapshot</h2>
          <pre id="statusRaw">Waiting for first status update...</pre>
        </section>
      </section>
    </section>
  </main>

  <script>
    const RESPONSES_ENDPOINT = "__RESPONSES_ENDPOINT__";
    const STATUS_ENDPOINT = "__STATUS_ENDPOINT__";
    const WEBSOCKET_ENDPOINT = "__WEBSOCKET_ENDPOINT__";
    const DEFAULT_SESSION_KEY = "__DEFAULT_SESSION_KEY__";
    const STORAGE_TOKEN = "tau.gateway.dashboard.token";
    const STORAGE_SESSION = "tau.gateway.dashboard.session";

    const tokenInput = document.getElementById("authToken");
    const sessionInput = document.getElementById("sessionKey");
    const promptInput = document.getElementById("prompt");
    const streamInput = document.getElementById("stream");
    const autoRefreshInput = document.getElementById("autoRefresh");
    const sendButton = document.getElementById("send");
    const refreshButton = document.getElementById("refreshStatus");
    const resetSessionButton = document.getElementById("resetSession");
    const clearOutputButton = document.getElementById("clearOutput");
    const outputPre = document.getElementById("output");
    const statusRawPre = document.getElementById("statusRaw");
    const auditFeed = document.getElementById("auditFeed");
    const connectionPill = document.getElementById("connectionPill");

    const wsState = {
      socket: null,
      isOpen: false,
      reconnectAttempt: 0,
      pending: new Map(),
      closeRequested: false,
      enabled: false
    };
    let autoRefreshTimer = null;
    let requestSequence = 0;

    function nextRequestId(prefix) {
      requestSequence += 1;
      return prefix + "-" + String(requestSequence);
    }

    function loadLocalValues() {
      const token = window.localStorage.getItem(STORAGE_TOKEN);
      const sessionKey = window.localStorage.getItem(STORAGE_SESSION);
      if (token) {
        tokenInput.value = token;
      }
      if (sessionKey) {
        sessionInput.value = sessionKey;
      }
    }

    function saveLocalValues() {
      window.localStorage.setItem(STORAGE_TOKEN, tokenInput.value.trim());
      window.localStorage.setItem(STORAGE_SESSION, sessionInput.value.trim());
    }

    function authHeaders() {
      const token = tokenInput.value.trim();
      if (token.length === 0) {
        return {};
      }
      return {
        "Authorization": "Bearer " + token
      };
    }

    function setOutput(text) {
      outputPre.textContent = text;
    }

    function appendOutput(text) {
      if (outputPre.textContent === "No response yet.") {
        outputPre.textContent = "";
      }
      outputPre.textContent += text;
    }

    function setConnectionPill(state, detail) {
      connectionPill.textContent = state + (detail ? " · " + detail : "");
      connectionPill.classList.remove("degraded", "failing");
      if (state === "degraded") {
        connectionPill.classList.add("degraded");
      } else if (state === "failing") {
        connectionPill.classList.add("failing");
      }
    }

    function toRecordTimestamp() {
      return new Date().toISOString();
    }

    function recordAudit(action, status, detail) {
      const row = document.createElement("div");
      row.className = "audit-item " + (status === "ok" ? "ok" : "fail");
      row.innerHTML =
        "<div class='tag'>" + action + " · " + status + "</div>" +
        "<div>" + String(detail || "") + "</div>" +
        "<div class='time'>" + toRecordTimestamp() + "</div>";
      const existingPlaceholder = auditFeed.querySelector(".audit-item .tag");
      if (existingPlaceholder && existingPlaceholder.textContent === "no-audit-events") {
        auditFeed.textContent = "";
      }
      auditFeed.prepend(row);
    }

    function renderHealthCards(payload) {
      const service = payload && payload.service ? payload.service : {};
      const auth = payload && payload.auth ? payload.auth : {};
      const multi = payload && payload.multi_channel ? payload.multi_channel : {};
      document.getElementById("serviceStatus").textContent = String(service.service_status || "unknown");
      document.getElementById("rolloutGate").textContent = String(service.rollout_gate || "hold");
      document.getElementById("healthState").textContent = String(multi.health_state || "unknown");
      document.getElementById("healthReason").textContent = String(multi.health_reason || "unavailable");
      document.getElementById("queueDepth").textContent = String(multi.queue_depth || 0);
      document.getElementById("failureStreak").textContent = String(multi.failure_streak || 0);
      document.getElementById("authMode").textContent = String(auth.mode || "unknown");
      document.getElementById("activeSessions").textContent = String(auth.active_sessions || 0);
      document.getElementById("authFailures").textContent = String(auth.auth_failures || 0);
      document.getElementById("lastUpdate").textContent = toRecordTimestamp();
    }

    function renderTransportRows(payload) {
      const body = document.getElementById("transportRows");
      body.textContent = "";
      const channels = payload && payload.multi_channel && payload.multi_channel.connectors
        ? payload.multi_channel.connectors.channels || {}
        : {};
      const names = Object.keys(channels).sort();
      if (names.length === 0) {
        body.innerHTML = "<tr><td colspan='5' class='muted'>No connector data yet.</td></tr>";
        return;
      }
      names.forEach((name) => {
        const entry = channels[name] || {};
        const row = document.createElement("tr");
        row.innerHTML =
          "<td>" + name + "</td>" +
          "<td>" + String(entry.liveness || "unknown") + "</td>" +
          "<td>" + String(entry.breaker_state || "unknown") + "</td>" +
          "<td>" + String(entry.events_ingested || 0) + "</td>" +
          "<td>auth=" + String(entry.auth_failures || 0) +
            " parse=" + String(entry.parse_failures || 0) +
            " provider=" + String(entry.provider_failures || 0) + "</td>";
        body.appendChild(row);
      });
    }

    function renderListFeed(elementId, entries, emptyMessage) {
      const root = document.getElementById(elementId);
      root.textContent = "";
      if (!Array.isArray(entries) || entries.length === 0) {
        const item = document.createElement("li");
        item.className = "muted";
        item.textContent = emptyMessage;
        root.appendChild(item);
        return;
      }
      entries.forEach((entry) => {
        const item = document.createElement("li");
        item.textContent = String(entry);
        root.appendChild(item);
      });
    }

    function applyGatewayStatus(payload, source) {
      renderHealthCards(payload);
      renderTransportRows(payload);
      const multi = payload && payload.multi_channel ? payload.multi_channel : {};
      const auth = payload && payload.auth ? payload.auth : {};
      const authMode = String(auth.mode || "unknown");
      wsState.enabled = authMode === "localhost-dev";
      renderListFeed("reasonFeed", multi.last_reason_codes || [], "No reason codes recorded yet.");
      renderListFeed("diagnosticFeed", multi.diagnostics || [], "No diagnostics recorded.");
      statusRawPre.textContent = JSON.stringify(payload, null, 2);
      if (!wsState.enabled) {
        resetSessionButton.disabled = true;
        if (wsState.socket) {
          wsState.closeRequested = true;
          wsState.socket.close();
        }
        setConnectionPill("connected", source + " (http-only)");
      } else {
        resetSessionButton.disabled = false;
        if (!wsState.socket) {
          connectGatewayWs();
        }
        if (wsState.isOpen) {
          setConnectionPill("connected", source + " + ws");
        } else {
          setConnectionPill("degraded", source + " (ws pending)");
        }
      }
    }

    function processSseFrame(frame) {
      if (!frame || frame.trim().length === 0) {
        return;
      }
      let eventName = "";
      let data = "";
      const lines = frame.split(/\r?\n/);
      for (const line of lines) {
        if (line.startsWith("event:")) {
          eventName = line.slice("event:".length).trim();
        } else if (line.startsWith("data:")) {
          data += line.slice("data:".length).trim();
        }
      }
      if (data.length === 0 || data === "[DONE]") {
        return;
      }
      let payload = null;
      try {
        payload = JSON.parse(data);
      } catch (error) {
        appendOutput("\n[invalid sse payload] " + data + "\n");
        return;
      }
      if (eventName === "response.output_text.delta") {
        appendOutput(payload.delta || "");
        return;
      }
      if (eventName === "response.output_text.done") {
        appendOutput("\n");
        return;
      }
      if (eventName === "response.failed") {
        const message = payload && payload.error ? payload.error.message : "unknown";
        appendOutput("\n[gateway error] " + message + "\n");
      }
    }

    async function readSseBody(response) {
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {
        const result = await reader.read();
        if (result.done) {
          break;
        }
        buffer += decoder.decode(result.value, { stream: true });
        while (true) {
          const splitIndex = buffer.indexOf("\n\n");
          if (splitIndex < 0) {
            break;
          }
          const frame = buffer.slice(0, splitIndex);
          buffer = buffer.slice(splitIndex + 2);
          processSseFrame(frame);
        }
      }
      if (buffer.trim().length > 0) {
        processSseFrame(buffer);
      }
    }

    async function fetchGatewayStatusFromHttp() {
      const response = await fetch(STATUS_ENDPOINT, {
        headers: authHeaders()
      });
      const raw = await response.text();
      if (!response.ok) {
        throw new Error("status " + response.status + ": " + raw);
      }
      return JSON.parse(raw);
    }

    function wsRequest(kind, payload, timeoutMs) {
      if (!wsState.isOpen || !wsState.socket) {
        return Promise.reject(new Error("websocket not connected"));
      }
      const requestId = nextRequestId("dash");
      return new Promise((resolve, reject) => {
        const timer = window.setTimeout(() => {
          wsState.pending.delete(requestId);
          reject(new Error("websocket request timeout"));
        }, timeoutMs || 2500);
        wsState.pending.set(requestId, { resolve, reject, timer });
        wsState.socket.send(JSON.stringify({
          schema_version: 1,
          request_id: requestId,
          kind: kind,
          payload: payload || {}
        }));
      });
    }

    function resolvePendingWs(frame) {
      if (!frame || typeof frame.request_id !== "string") {
        return false;
      }
      const pending = wsState.pending.get(frame.request_id);
      if (!pending) {
        return false;
      }
      wsState.pending.delete(frame.request_id);
      window.clearTimeout(pending.timer);
      if (frame.kind === "error") {
        const payload = frame.payload || {};
        pending.reject(new Error(String(payload.message || payload.code || "gateway websocket error")));
      } else {
        pending.resolve(frame);
      }
      return true;
    }

    function flushPendingWsOnDisconnect(reason) {
      wsState.pending.forEach((pending, requestId) => {
        window.clearTimeout(pending.timer);
        pending.reject(new Error("websocket disconnected before response: " + requestId + " (" + reason + ")"));
      });
      wsState.pending.clear();
    }

    function buildWsUrl() {
      const protocol = window.location.protocol === "https:" ? "wss" : "ws";
      return protocol + "://" + window.location.host + WEBSOCKET_ENDPOINT;
    }

    function connectGatewayWs() {
      if (!wsState.enabled) {
        return;
      }
      wsState.closeRequested = false;
      const url = buildWsUrl();
      let socket = null;
      try {
        socket = new WebSocket(url);
      } catch (error) {
        setConnectionPill("degraded", "ws init failed");
        scheduleWsReconnect();
        return;
      }
      wsState.socket = socket;
      setConnectionPill("degraded", "connecting");

      socket.addEventListener("open", () => {
        wsState.isOpen = true;
        wsState.reconnectAttempt = 0;
        setConnectionPill("connected", "ws");
        requestStatus();
      });

      socket.addEventListener("message", (event) => {
        let frame = null;
        try {
          frame = JSON.parse(event.data);
        } catch (_) {
          recordAudit("ws.frame", "fail", "invalid JSON frame");
          return;
        }
        if (resolvePendingWs(frame)) {
          return;
        }
        if (frame.kind === "gateway.status.response" && frame.payload) {
          applyGatewayStatus(frame.payload, "ws");
          return;
        }
      });

      socket.addEventListener("close", () => {
        wsState.isOpen = false;
        wsState.socket = null;
        flushPendingWsOnDisconnect("close");
        if (!wsState.closeRequested) {
          setConnectionPill("degraded", "ws reconnect");
          scheduleWsReconnect();
        }
      });

      socket.addEventListener("error", () => {
        setConnectionPill("degraded", "ws error");
      });
    }

    function scheduleWsReconnect() {
      if (wsState.closeRequested || !wsState.enabled) {
        return;
      }
      wsState.reconnectAttempt += 1;
      const delayMs = Math.min(7000, 600 * wsState.reconnectAttempt);
      window.setTimeout(() => {
        connectGatewayWs();
      }, delayMs);
    }

    async function requestStatus() {
      try {
        if (wsState.isOpen) {
          const frame = await wsRequest("gateway.status.request", {}, 2200);
          applyGatewayStatus(frame.payload || {}, "ws");
          return;
        }
      } catch (error) {
        recordAudit("status.refresh", "fail", String(error));
      }
      try {
        const payload = await fetchGatewayStatusFromHttp();
        applyGatewayStatus(payload, "http");
        recordAudit("status.refresh", "ok", "fetched via HTTP");
      } catch (error) {
        statusRawPre.textContent = "status request failed: " + String(error);
        setConnectionPill("failing", "status fetch failed");
        recordAudit("status.refresh", "fail", String(error));
      }
    }

    async function resetSession() {
      const sessionKey = sessionInput.value.trim() || DEFAULT_SESSION_KEY;
      if (!window.confirm("Reset session '" + sessionKey + "'?")) {
        return;
      }
      saveLocalValues();
      try {
        if (!wsState.enabled) {
          throw new Error("session reset requires localhost-dev websocket mode");
        }
        if (!wsState.isOpen) {
          throw new Error("websocket not connected");
        }
        const frame = await wsRequest("session.reset.request", { session_key: sessionKey }, 3000);
        const payload = frame.payload || {};
        const reset = Boolean(payload.reset);
        recordAudit("session.reset", reset ? "ok" : "fail", JSON.stringify(payload));
      } catch (error) {
        recordAudit("session.reset", "fail", String(error));
      } finally {
        requestStatus();
      }
    }

    async function sendPrompt() {
      const prompt = promptInput.value.trim();
      const sessionKey = sessionInput.value.trim() || DEFAULT_SESSION_KEY;
      if (prompt.length === 0) {
        setOutput("Prompt is required.");
        return;
      }
      saveLocalValues();
      sendButton.disabled = true;
      try {
        setOutput("");
        const payload = {
          input: prompt,
          stream: streamInput.checked,
          metadata: {
            session_id: sessionKey
          }
        };
        const response = await fetch(RESPONSES_ENDPOINT, {
          method: "POST",
          headers: Object.assign({
            "Content-Type": "application/json"
          }, authHeaders()),
          body: JSON.stringify(payload)
        });
        if (!response.ok) {
          const failureBody = await response.text();
          setOutput("request failed: status=" + response.status + "\n" + failureBody);
          recordAudit("prompt.send", "fail", "status=" + response.status);
          return;
        }
        if (streamInput.checked) {
          await readSseBody(response);
        } else {
          const body = await response.json();
          const outputText = typeof body.output_text === "string"
            ? body.output_text
            : JSON.stringify(body, null, 2);
          setOutput(outputText);
        }
        recordAudit("prompt.send", "ok", "session=" + sessionKey);
      } catch (error) {
        setOutput("request failed: " + String(error));
        recordAudit("prompt.send", "fail", String(error));
      } finally {
        sendButton.disabled = false;
        requestStatus();
      }
    }

    function startAutoRefresh() {
      if (autoRefreshTimer) {
        window.clearInterval(autoRefreshTimer);
        autoRefreshTimer = null;
      }
      if (!autoRefreshInput.checked) {
        return;
      }
      autoRefreshTimer = window.setInterval(() => {
        requestStatus();
      }, 8000);
    }

    sendButton.addEventListener("click", sendPrompt);
    refreshButton.addEventListener("click", () => {
      requestStatus();
    });
    resetSessionButton.addEventListener("click", resetSession);
    clearOutputButton.addEventListener("click", () => setOutput("No response yet."));
    tokenInput.addEventListener("change", () => {
      saveLocalValues();
      if (wsState.socket) {
        wsState.closeRequested = true;
        wsState.socket.close();
      }
      requestStatus();
    });
    sessionInput.addEventListener("change", saveLocalValues);
    autoRefreshInput.addEventListener("change", startAutoRefresh);

    loadLocalValues();
    startAutoRefresh();
    requestStatus();
  </script>
</body>
</html>
"###;

fn inject_gateway_page_template(template: &str) -> String {
    template
        .replace("__RESPONSES_ENDPOINT__", OPENRESPONSES_ENDPOINT)
        .replace("__STATUS_ENDPOINT__", GATEWAY_STATUS_ENDPOINT)
        .replace("__WEBSOCKET_ENDPOINT__", GATEWAY_WS_ENDPOINT)
        .replace("__DASHBOARD_ENDPOINT__", DASHBOARD_ENDPOINT)
        .replace("__WEBCHAT_ENDPOINT__", WEBCHAT_ENDPOINT)
        .replace("__DEFAULT_SESSION_KEY__", DEFAULT_SESSION_KEY)
}

pub(super) fn render_gateway_dashboard_page() -> String {
    inject_gateway_page_template(DASHBOARD_PAGE_TEMPLATE)
}

pub(super) fn render_gateway_webchat_page() -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Tau Gateway Webchat</title>
  <style>
    :root {{
      color-scheme: light;
      font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    }}
    body {{
      margin: 0;
      background: linear-gradient(160deg, #f4f6f8 0%, #eef2f7 100%);
      color: #13232f;
    }}
    .container {{
      max-width: 980px;
      margin: 0 auto;
      padding: 1.5rem;
    }}
    h1 {{
      margin: 0 0 0.5rem 0;
      font-size: 1.5rem;
    }}
    p {{
      margin: 0.25rem 0 1rem 0;
      color: #3a4f5f;
    }}
    .grid {{
      display: grid;
      gap: 1rem;
      grid-template-columns: 1fr;
    }}
    .panel {{
      background: #ffffff;
      border: 1px solid #d2dde6;
      border-radius: 12px;
      padding: 1rem;
      box-shadow: 0 8px 20px rgba(12, 25, 38, 0.06);
    }}
    label {{
      display: block;
      font-size: 0.85rem;
      margin-bottom: 0.25rem;
      color: #375062;
    }}
    input[type="text"], textarea {{
      width: 100%;
      box-sizing: border-box;
      border: 1px solid #b8c9d6;
      border-radius: 8px;
      padding: 0.55rem 0.7rem;
      font-size: 0.95rem;
      background: #fbfdff;
      color: #13232f;
    }}
    textarea {{
      min-height: 150px;
      resize: vertical;
    }}
    .row {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 0.8rem;
      margin-bottom: 0.8rem;
    }}
    .actions {{
      display: flex;
      gap: 0.5rem;
      flex-wrap: wrap;
      margin-top: 0.8rem;
    }}
    button {{
      border: 0;
      border-radius: 8px;
      background: #0f7d5f;
      color: #ffffff;
      padding: 0.55rem 0.9rem;
      font-weight: 600;
      cursor: pointer;
    }}
    button.secondary {{
      background: #3f5f74;
    }}
    button:disabled {{
      cursor: wait;
      opacity: 0.6;
    }}
    .checkbox {{
      display: inline-flex;
      align-items: center;
      gap: 0.4rem;
      margin-top: 0.2rem;
      color: #274355;
      font-size: 0.9rem;
    }}
    pre {{
      margin: 0;
      background: #0f1f2b;
      color: #d9ecf7;
      border-radius: 10px;
      padding: 0.8rem;
      overflow: auto;
      max-height: 300px;
      white-space: pre-wrap;
      word-break: break-word;
      font-size: 0.85rem;
    }}
    @media (min-width: 900px) {{
      .grid {{
        grid-template-columns: 1.4fr 1fr;
      }}
    }}
  </style>
</head>
<body>
  <main class="container">
    <h1>Tau Gateway Webchat</h1>
    <p>Operator webchat for the OpenResponses gateway runtime.</p>
    <div class="grid">
      <section class="panel">
        <div class="row">
          <div>
            <label for="authToken">Bearer token</label>
            <input id="authToken" type="text" autocomplete="off" placeholder="gateway auth token" />
          </div>
          <div>
            <label for="sessionKey">Session key</label>
            <input id="sessionKey" type="text" autocomplete="off" value="{default_session_key}" />
          </div>
        </div>
        <label class="checkbox">
          <input id="stream" type="checkbox" checked />
          Stream response (SSE)
        </label>
        <div style="margin-top: 0.8rem;">
          <label for="prompt">Prompt</label>
          <textarea id="prompt" placeholder="Ask Tau through the gateway..."></textarea>
        </div>
        <div class="actions">
          <button id="send">Send</button>
          <button id="refreshStatus" class="secondary">Refresh status</button>
          <button id="clearOutput" class="secondary">Clear output</button>
        </div>
      </section>
      <section class="panel">
        <h2 style="margin: 0 0 0.5rem 0; font-size: 1rem;">Gateway status</h2>
        <pre id="status">Press "Refresh status" to inspect gateway service state, multi-channel lifecycle summary, connector counters, and recent reason codes.</pre>
      </section>
    </div>
    <section class="panel" style="margin-top: 1rem;">
      <h2 style="margin: 0 0 0.5rem 0; font-size: 1rem;">Response output</h2>
      <pre id="output">No response yet.</pre>
    </section>
  </main>
  <script>
    const RESPONSES_ENDPOINT = "{responses_endpoint}";
    const STATUS_ENDPOINT = "{status_endpoint}";
    const WEBSOCKET_ENDPOINT = "{websocket_endpoint}";
    const STORAGE_TOKEN = "tau.gateway.webchat.token";
    const STORAGE_SESSION = "tau.gateway.webchat.session";
    const tokenInput = document.getElementById("authToken");
    const sessionInput = document.getElementById("sessionKey");
    const streamInput = document.getElementById("stream");
    const promptInput = document.getElementById("prompt");
    const outputPre = document.getElementById("output");
    const statusPre = document.getElementById("status");
    const sendButton = document.getElementById("send");
    const refreshButton = document.getElementById("refreshStatus");
    const clearButton = document.getElementById("clearOutput");

    function loadLocalValues() {{
      const storedToken = window.localStorage.getItem(STORAGE_TOKEN);
      const storedSession = window.localStorage.getItem(STORAGE_SESSION);
      if (storedToken) {{
        tokenInput.value = storedToken;
      }}
      if (storedSession) {{
        sessionInput.value = storedSession;
      }}
    }}

    function saveLocalValues() {{
      window.localStorage.setItem(STORAGE_TOKEN, tokenInput.value.trim());
      window.localStorage.setItem(STORAGE_SESSION, sessionInput.value.trim());
    }}

    function authHeaders() {{
      const token = tokenInput.value.trim();
      if (token.length === 0) {{
        return {{}};
      }}
      return {{
        "Authorization": "Bearer " + token
      }};
    }}

    function setOutput(text) {{
      outputPre.textContent = text;
    }}

    function appendOutput(text) {{
      if (outputPre.textContent === "No response yet.") {{
        outputPre.textContent = "";
      }}
      outputPre.textContent += text;
    }}

    function processSseFrame(frame) {{
      if (!frame || frame.trim().length === 0) {{
        return;
      }}
      let eventName = "";
      let data = "";
      const lines = frame.split(/\r?\n/);
      for (const line of lines) {{
        if (line.startsWith("event:")) {{
          eventName = line.slice("event:".length).trim();
        }} else if (line.startsWith("data:")) {{
          data += line.slice("data:".length).trim();
        }}
      }}
      if (data.length === 0 || data === "[DONE]") {{
        return;
      }}
      let payload = null;
      try {{
        payload = JSON.parse(data);
      }} catch (error) {{
        appendOutput("\n[invalid sse payload] " + data + "\n");
        return;
      }}
      if (eventName === "response.output_text.delta") {{
        appendOutput(payload.delta || "");
        return;
      }}
      if (eventName === "response.output_text.done") {{
        appendOutput("\n");
        return;
      }}
      if (eventName === "response.failed") {{
        const message = payload && payload.error ? payload.error.message : "unknown";
        appendOutput("\n[gateway error] " + message + "\n");
      }}
    }}

    async function readSseBody(response) {{
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {{
        const result = await reader.read();
        if (result.done) {{
          break;
        }}
        buffer += decoder.decode(result.value, {{ stream: true }});
        while (true) {{
          const splitIndex = buffer.indexOf("\n\n");
          if (splitIndex < 0) {{
            break;
          }}
          const frame = buffer.slice(0, splitIndex);
          buffer = buffer.slice(splitIndex + 2);
          processSseFrame(frame);
        }}
      }}
      if (buffer.trim().length > 0) {{
        processSseFrame(buffer);
      }}
    }}

    function renderMultiChannelChannelRows(connectors) {{
      if (!connectors || !connectors.channels) {{
        return "none";
      }}
      const entries = Object.entries(connectors.channels);
      if (entries.length === 0) {{
        return "none";
      }}
      entries.sort((left, right) => left[0].localeCompare(right[0]));
      return entries.map(([channel, status]) => {{
        return channel +
          ":liveness=" + (status.liveness || "unknown") +
          " breaker=" + (status.breaker_state || "unknown") +
          " ingested=" + String(status.events_ingested || 0) +
          " dup=" + String(status.duplicates_skipped || 0) +
          " retry=" + String(status.retry_attempts || 0) +
          " auth_fail=" + String(status.auth_failures || 0) +
          " parse_fail=" + String(status.parse_failures || 0) +
          " provider_fail=" + String(status.provider_failures || 0);
      }}).join("\n");
    }}

    function formatGatewayStatusSummary(payload) {{
      const service = payload && payload.service ? payload.service : {{}};
      const auth = payload && payload.auth ? payload.auth : {{}};
      const mc = payload && payload.multi_channel ? payload.multi_channel : {{}};
      const connectors = mc.connectors || {{}};
      const reasonCodes = Array.isArray(mc.last_reason_codes) && mc.last_reason_codes.length > 0
        ? mc.last_reason_codes.join(",")
        : "none";
      const diagnostics = Array.isArray(mc.diagnostics) && mc.diagnostics.length > 0
        ? mc.diagnostics.join(",")
        : "none";
      const transportCounts = mc.transport_counts ? JSON.stringify(mc.transport_counts) : "{{}}";
      return [
        "gateway_service: status=" + String(service.service_status || "unknown") +
          " rollout_gate=" + String(service.rollout_gate || "unknown") +
          " reason_code=" + String(service.rollout_reason_code || "unknown"),
        "gateway_auth: mode=" + String(auth.mode || "unknown") +
          " active_sessions=" + String(auth.active_sessions || 0) +
          " auth_failures=" + String(auth.auth_failures || 0) +
          " rate_limited=" + String(auth.rate_limited_requests || 0),
        "multi_channel_lifecycle: state_present=" + String(Boolean(mc.state_present)) +
          " health=" + String(mc.health_state || "unknown") +
          " rollout_gate=" + String(mc.rollout_gate || "hold") +
          " processed=" + String(mc.processed_event_count || 0) +
          " queue_depth=" + String(mc.queue_depth || 0) +
          " failure_streak=" + String(mc.failure_streak || 0) +
          " last_cycle_failed=" + String(mc.last_cycle_failed || 0) +
          " last_cycle_completed=" + String(mc.last_cycle_completed || 0),
        "multi_channel_reason_codes_recent: " + reasonCodes,
        "multi_channel_reason_code_counts: " + JSON.stringify(mc.reason_code_counts || {{}}),
        "multi_channel_transport_counts: " + transportCounts,
        "connectors: state_present=" + String(Boolean(connectors.state_present)) +
          " processed=" + String(connectors.processed_event_count || 0),
        "connector_channels:\n" + renderMultiChannelChannelRows(connectors),
        "multi_channel_diagnostics: " + diagnostics,
      ].join("\n");
    }}

    async function refreshStatus() {{
      statusPre.textContent = "Loading gateway status...";
      try {{
        const response = await fetch(STATUS_ENDPOINT, {{
          headers: authHeaders()
        }});
        const raw = await response.text();
        if (!response.ok) {{
          statusPre.textContent = "status " + response.status + "\n" + raw;
          return;
        }}
        const payload = JSON.parse(raw);
        const summary = formatGatewayStatusSummary(payload);
        statusPre.textContent = summary + "\n\nraw_payload:\n" + JSON.stringify(payload, null, 2);
      }} catch (error) {{
        statusPre.textContent = "status request failed: " + String(error);
      }}
    }}

    async function sendPrompt() {{
      const prompt = promptInput.value.trim();
      const sessionKey = sessionInput.value.trim() || "{default_session_key}";
      if (prompt.length === 0) {{
        setOutput("Prompt is required.");
        return;
      }}
      saveLocalValues();
      sendButton.disabled = true;
      try {{
        setOutput("");
        const payload = {{
          input: prompt,
          stream: streamInput.checked,
          metadata: {{
            session_id: sessionKey
          }}
        }};
        const response = await fetch(RESPONSES_ENDPOINT, {{
          method: "POST",
          headers: Object.assign({{
            "Content-Type": "application/json"
          }}, authHeaders()),
          body: JSON.stringify(payload)
        }});
        if (!response.ok) {{
          setOutput("request failed: status=" + response.status + "\n" + await response.text());
          return;
        }}
        if (streamInput.checked) {{
          await readSseBody(response);
        }} else {{
          const body = await response.json();
          const outputText = typeof body.output_text === "string"
            ? body.output_text
            : JSON.stringify(body, null, 2);
          setOutput(outputText);
        }}
        await refreshStatus();
      }} catch (error) {{
        setOutput("request failed: " + String(error));
      }} finally {{
        sendButton.disabled = false;
      }}
    }}

    sendButton.addEventListener("click", sendPrompt);
    refreshButton.addEventListener("click", refreshStatus);
    clearButton.addEventListener("click", () => setOutput("No response yet."));
    tokenInput.addEventListener("change", saveLocalValues);
    sessionInput.addEventListener("change", saveLocalValues);

    loadLocalValues();
  </script>
</body>
</html>
"#,
        responses_endpoint = OPENRESPONSES_ENDPOINT,
        status_endpoint = GATEWAY_STATUS_ENDPOINT,
        websocket_endpoint = GATEWAY_WS_ENDPOINT,
        default_session_key = DEFAULT_SESSION_KEY,
    )
}
