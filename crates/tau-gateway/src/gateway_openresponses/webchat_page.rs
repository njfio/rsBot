//! Webchat HTML renderer for the gateway operator shell.
use super::*;

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
      --bg: #f4f6f8;
      --panel: #ffffff;
      --ink: #102736;
      --ink-muted: #415d70;
      --line: #d4dee6;
      --ok: #0f7d5f;
      --warn: #a55e00;
      --bad: #b42318;
      --primary: #0b6f56;
      --secondary: #36566b;
    }}
    body {{
      margin: 0;
      background: radial-gradient(circle at top right, #eaf1f8 0%, var(--bg) 45%, #eef3f7 100%);
      color: var(--ink);
    }}
    .container {{
      max-width: 1200px;
      margin: 0 auto;
      padding: 1.25rem;
    }}
    .header {{
      display: flex;
      flex-wrap: wrap;
      gap: 1rem;
      align-items: flex-end;
      justify-content: space-between;
      margin-bottom: 0.9rem;
    }}
    h1 {{
      margin: 0;
      font-size: 1.55rem;
      letter-spacing: 0.01em;
    }}
    .subtitle {{
      margin: 0.15rem 0 0 0;
      color: var(--ink-muted);
      font-size: 0.95rem;
    }}
    .shell {{
      display: grid;
      gap: 0.9rem;
    }}
    .panel {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 12px;
      box-shadow: 0 8px 24px rgba(17, 31, 44, 0.08);
      padding: 0.9rem;
    }}
    .row {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
      gap: 0.7rem;
      margin-bottom: 0.8rem;
    }}
    label {{
      display: block;
      margin-bottom: 0.2rem;
      font-size: 0.8rem;
      color: var(--ink-muted);
      letter-spacing: 0.02em;
    }}
    input[type="text"],
    input[type="password"],
    select,
    textarea {{
      width: 100%;
      box-sizing: border-box;
      border: 1px solid #becdd9;
      border-radius: 8px;
      padding: 0.5rem 0.65rem;
      font-size: 0.93rem;
      color: var(--ink);
      background: #fcfeff;
    }}
    textarea {{
      min-height: 130px;
      resize: vertical;
    }}
    .tabs {{
      display: flex;
      flex-wrap: wrap;
      gap: 0.45rem;
      margin-bottom: 0.8rem;
    }}
    .tab {{
      border: 1px solid #c2d2de;
      background: #eaf2f8;
      color: #173548;
      border-radius: 999px;
      padding: 0.4rem 0.75rem;
      font-size: 0.82rem;
      font-weight: 600;
      cursor: pointer;
    }}
    .tab.active {{
      background: #11405a;
      border-color: #11405a;
      color: #ffffff;
    }}
    .view {{
      display: none;
    }}
    .view.active {{
      display: block;
    }}
    .actions {{
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem;
      margin-top: 0.65rem;
    }}
    button {{
      border: 0;
      border-radius: 8px;
      background: var(--primary);
      color: #ffffff;
      padding: 0.52rem 0.86rem;
      font-size: 0.9rem;
      font-weight: 600;
      cursor: pointer;
    }}
    button.secondary {{
      background: var(--secondary);
    }}
    button.warn {{
      background: #9b2c21;
    }}
    button:disabled {{
      cursor: wait;
      opacity: 0.65;
    }}
    .checkbox {{
      display: inline-flex;
      align-items: center;
      gap: 0.38rem;
      margin-top: 0.1rem;
      font-size: 0.9rem;
      color: #203f52;
    }}
    pre {{
      margin: 0;
      background: #0f1f2b;
      color: #d9ecf7;
      border-radius: 10px;
      padding: 0.75rem;
      overflow: auto;
      max-height: 340px;
      white-space: pre-wrap;
      word-break: break-word;
      font-size: 0.82rem;
      line-height: 1.4;
    }}
    .status-dashboard {{
      display: grid;
      gap: 0.8rem;
      margin-bottom: 0.8rem;
    }}
    .status-cards {{
      display: grid;
      gap: 0.6rem;
      grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    }}
    .metric-card {{
      border: 1px solid #cfdae4;
      border-radius: 10px;
      padding: 0.55rem 0.65rem;
      background: linear-gradient(180deg, #fdfefe 0%, #f4f8fb 100%);
    }}
    .metric-label {{
      font-size: 0.75rem;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      color: #4a667a;
      margin-bottom: 0.2rem;
    }}
    .metric-value {{
      display: inline-block;
      font-size: 1.05rem;
      font-weight: 700;
      color: #173142;
    }}
    .metric-value.ok {{ color: var(--ok); }}
    .metric-value.warn {{ color: var(--warn); }}
    .metric-value.bad {{ color: var(--bad); }}
    .table-scroll {{
      overflow: auto;
      border: 1px solid #d2dde6;
      border-radius: 10px;
      background: #f9fbfd;
    }}
    table.status-table {{
      width: 100%;
      border-collapse: collapse;
      min-width: 460px;
    }}
    table.status-table th {{
      text-align: left;
      font-size: 0.75rem;
      letter-spacing: 0.04em;
      text-transform: uppercase;
      color: #486275;
      background: #edf3f8;
      border-bottom: 1px solid #cfdae4;
      padding: 0.45rem 0.5rem;
      white-space: nowrap;
    }}
    table.status-table td {{
      border-bottom: 1px solid #dde6ed;
      padding: 0.4rem 0.5rem;
      font-size: 0.82rem;
      color: #1f3a4b;
      white-space: nowrap;
    }}
    table.status-table tbody tr:last-child td {{
      border-bottom: none;
    }}
    .split {{
      display: grid;
      gap: 0.8rem;
      grid-template-columns: 1fr;
    }}
    .list {{
      max-height: 240px;
      overflow: auto;
      border: 1px solid #d0dae2;
      border-radius: 10px;
      background: #f9fcff;
      padding: 0.45rem;
    }}
    .list button {{
      display: block;
      width: 100%;
      text-align: left;
      margin-bottom: 0.35rem;
      background: #1f5574;
      font-size: 0.82rem;
    }}
    .mono {{
      font-family: "IBM Plex Mono", "SFMono-Regular", Menlo, monospace;
      font-size: 0.8rem;
      color: #213f52;
      background: #edf3f8;
      padding: 0.35rem 0.45rem;
      border-radius: 6px;
      border: 1px solid #d1dce5;
      display: inline-block;
    }}
    #memoryGraphCanvas {{
      display: block;
      border: 1px solid #d0dae2;
      border-radius: 10px;
      background: linear-gradient(180deg, #f9fcff 0%, #eef4f8 100%);
      margin-top: 0.6rem;
    }}
    .memory-graph-node-label {{
      font-size: 11px;
      fill: #173142;
      font-family: "IBM Plex Mono", "SFMono-Regular", Menlo, monospace;
    }}
    @media (min-width: 960px) {{
      .split {{
        grid-template-columns: 1fr 1.3fr;
      }}
    }}
  </style>
</head>
<body>
  <main class="container">
    <header class="header">
      <div>
        <h1>Tau Gateway Webchat</h1>
        <p class="subtitle">Multi-view runtime operator shell for conversation, tools, sessions, memory, and configuration.</p>
      </div>
      <div class="mono">WebSocket: {websocket_endpoint}</div>
    </header>

    <section class="panel" aria-label="Gateway controls">
      <div class="row">
        <div>
          <label for="authToken">Bearer token</label>
          <input id="authToken" type="password" autocomplete="off" placeholder="gateway auth token" />
        </div>
        <div>
          <label for="sessionKey">Session key</label>
          <input id="sessionKey" type="text" autocomplete="off" value="{default_session_key}" />
        </div>
        <div>
          <label for="apiMode">API mode</label>
          <select id="apiMode">
            <option value="responses">/v1/responses</option>
            <option value="chat_completions">/v1/chat/completions</option>
            <option value="completions">/v1/completions</option>
          </select>
        </div>
      </div>
      <label class="checkbox">
        <input id="stream" type="checkbox" checked />
        Stream response (SSE)
      </label>
    </section>

    <section class="panel">
      <nav class="tabs" role="tablist" aria-label="Gateway web UI views">
        <button class="tab active" data-view="conversation" role="tab" aria-selected="true">Conversation</button>
        <button class="tab" data-view="dashboard" role="tab" aria-selected="false">Dashboard</button>
        <button class="tab" data-view="tools" role="tab" aria-selected="false">Tools</button>
        <button class="tab" data-view="sessions" role="tab" aria-selected="false">Sessions</button>
        <button class="tab" data-view="memory" role="tab" aria-selected="false">Memory</button>
        <button class="tab" data-view="configuration" role="tab" aria-selected="false">Configuration</button>
      </nav>

      <section id="view-conversation" class="view active" role="tabpanel">
        <label for="prompt">Prompt</label>
        <textarea id="prompt" placeholder="Ask Tau through the gateway..."></textarea>
        <div class="actions">
          <button id="send">Send</button>
          <button id="clearOutput" class="secondary">Clear output</button>
        </div>
        <h2 style="margin: 0.8rem 0 0.4rem 0; font-size: 1rem;">Response output</h2>
        <pre id="output">No response yet.</pre>
      </section>

      <section id="view-tools" class="view" role="tabpanel" aria-hidden="true">
        <div class="actions" style="margin-top: 0;">
          <button id="refreshStatus" class="secondary">Refresh status</button>
        </div>
        <div class="status-dashboard">
          <div class="status-cards">
            <article class="metric-card">
              <div class="metric-label">Health State</div>
              <div id="healthStateValue" class="metric-value">unknown</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Rollout Gate</div>
              <div id="rolloutGateValue" class="metric-value">unknown</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Queue Depth</div>
              <div id="queueDepthValue" class="metric-value">0</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Failure Streak</div>
              <div id="failureStreakValue" class="metric-value">0</div>
            </article>
          </div>
          <div>
            <label style="margin-bottom: 0.35rem;">Connector Channels</label>
            <div class="table-scroll">
              <table class="status-table" aria-label="Connector channels table">
                <thead>
                  <tr>
                    <th>Channel</th>
                    <th>Liveness</th>
                    <th>Breaker</th>
                    <th>Ingested</th>
                    <th>Dup</th>
                    <th>Retry</th>
                    <th>Auth Fail</th>
                    <th>Parse Fail</th>
                    <th>Provider Fail</th>
                  </tr>
                </thead>
                <tbody id="connectorTableBody">
                  <tr><td colspan="9">No connector data yet.</td></tr>
                </tbody>
              </table>
            </div>
          </div>
          <div>
            <label style="margin-bottom: 0.35rem;">Reason Code Counts</label>
            <div class="table-scroll">
              <table class="status-table" aria-label="Reason code table">
                <thead>
                  <tr>
                    <th>Reason Code</th>
                    <th>Count</th>
                  </tr>
                </thead>
                <tbody id="reasonCodeTableBody">
                  <tr><td colspan="2">No reason-code samples yet.</td></tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
        <pre id="status">Press "Refresh status" to inspect gateway service state, multi-channel lifecycle summary, connector counters, and recent reason codes.</pre>
      </section>

      <section id="view-dashboard" class="view" role="tabpanel" aria-hidden="true">
        <div class="actions" style="margin-top: 0;">
          <button id="dashboardRefresh" class="secondary">Refresh dashboard</button>
          <button id="dashboardPause" class="warn">Pause</button>
          <button id="dashboardResume">Resume</button>
          <button id="dashboardControlRefresh" class="secondary">Control refresh</button>
        </div>
        <div class="row" style="margin-top: 0.5rem;">
          <div>
            <label for="dashboardActionReason">Action reason</label>
            <input id="dashboardActionReason" type="text" value="web-operator" />
          </div>
          <div>
            <label for="dashboardPollSeconds">Live poll interval (seconds)</label>
            <input id="dashboardPollSeconds" type="text" value="5" />
          </div>
          <div>
            <label>&nbsp;</label>
            <label class="checkbox">
              <input id="dashboardLive" type="checkbox" checked />
              Live Dashboard
            </label>
          </div>
        </div>
        <div class="status-dashboard">
          <div class="status-cards">
            <article class="metric-card">
              <div class="metric-label">Health State</div>
              <div id="dashboardHealthStateValue" class="metric-value">unknown</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Rollout Gate</div>
              <div id="dashboardRolloutGateValue" class="metric-value">unknown</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Control Mode</div>
              <div id="dashboardControlModeValue" class="metric-value">unknown</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Run State</div>
              <div id="dashboardRunStateValue" class="metric-value">unknown</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Total Rollouts</div>
              <div id="dashboardTotalRolloutsValue" class="metric-value tabular-nums">0</div>
            </article>
            <article class="metric-card">
              <div class="metric-label">Failed Rollouts</div>
              <div id="dashboardFailedRolloutsValue" class="metric-value tabular-nums">0</div>
            </article>
          </div>
          <div>
            <label style="margin-bottom: 0.35rem;">Dashboard Widgets</label>
            <div class="table-scroll">
              <table class="status-table" aria-label="Dashboard widgets table">
                <thead>
                  <tr>
                    <th>Widget</th>
                    <th>Kind</th>
                    <th>Query</th>
                    <th>Refresh ms</th>
                    <th>Updated</th>
                  </tr>
                </thead>
                <tbody id="dashboardWidgetsTableBody">
                  <tr><td colspan="5">No widget rows yet.</td></tr>
                </tbody>
              </table>
            </div>
          </div>
          <div>
            <label style="margin-bottom: 0.35rem;">Dashboard Alerts</label>
            <div class="table-scroll">
              <table class="status-table" aria-label="Dashboard alerts table">
                <thead>
                  <tr>
                    <th>Code</th>
                    <th>Severity</th>
                    <th>Message</th>
                  </tr>
                </thead>
                <tbody id="dashboardAlertsTableBody">
                  <tr><td colspan="3">No dashboard alerts.</td></tr>
                </tbody>
              </table>
            </div>
          </div>
          <div>
            <label style="margin-bottom: 0.35rem;">Dashboard Queue Timeline</label>
            <div class="table-scroll">
              <table class="status-table" aria-label="Dashboard queue timeline table">
                <thead>
                  <tr>
                    <th>Timestamp</th>
                    <th>Health</th>
                    <th>Reason</th>
                    <th>Queued</th>
                    <th>Applied</th>
                    <th>Failed</th>
                  </tr>
                </thead>
                <tbody id="dashboardTimelineTableBody">
                  <tr><td colspan="6">No timeline entries yet.</td></tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
        <pre id="dashboardStatus">Dashboard status will appear here.</pre>
      </section>

      <section id="view-sessions" class="view" role="tabpanel" aria-hidden="true">
        <div class="split">
          <div>
            <div class="actions" style="margin-top: 0;">
              <button id="loadSessions" class="secondary">Load sessions</button>
              <button id="loadSessionDetail" class="secondary">Open current session</button>
            </div>
            <div id="sessionsList" class="list" aria-label="Session list">
              <div>No sessions loaded.</div>
            </div>
          </div>
          <div>
            <div class="row">
              <div>
                <label for="appendRole">Append role</label>
                <select id="appendRole">
                  <option value="user">user</option>
                  <option value="assistant">assistant</option>
                  <option value="system">system</option>
                </select>
              </div>
              <div>
                <label for="sessionPolicyGate">Session policy gate</label>
                <input id="sessionPolicyGate" type="text" value="{session_policy_gate}" />
              </div>
            </div>
            <label for="appendContent">Append content</label>
            <textarea id="appendContent" placeholder="Message text to append to the current session"></textarea>
            <div class="actions">
              <button id="appendSession">Append message</button>
              <button id="resetSession" class="warn">Reset session</button>
            </div>
            <pre id="sessionDetail">Session details will appear here.</pre>
          </div>
        </div>
      </section>

      <section id="view-memory" class="view" role="tabpanel" aria-hidden="true">
        <div class="row">
          <div>
            <label for="memoryPolicyGate">Memory policy gate</label>
            <input id="memoryPolicyGate" type="text" value="{memory_policy_gate}" />
          </div>
        </div>
        <div class="actions" style="margin-top: 0;">
          <button id="loadMemory" class="secondary">Load memory</button>
          <button id="saveMemory">Save memory</button>
        </div>
        <label for="memoryContent" style="margin-top: 0.6rem;">Memory content</label>
        <textarea id="memoryContent" placeholder="Editable memory note for the active session"></textarea>
        <pre id="memoryStatus">Memory status will appear here.</pre>
        <h2 style="margin: 0.8rem 0 0.4rem 0; font-size: 1rem;">Memory Graph</h2>
        <div class="row">
          <div>
            <label for="graphMaxNodes">Max nodes</label>
            <input id="graphMaxNodes" type="text" value="24" />
          </div>
          <div>
            <label for="graphMinEdgeWeight">Min edge weight</label>
            <input id="graphMinEdgeWeight" type="text" value="1" />
          </div>
          <div>
            <label for="graphRelationTypes">Relation types</label>
            <input id="graphRelationTypes" type="text" value="contains,keyword_overlap" />
          </div>
        </div>
        <div class="actions" style="margin-top: 0;">
          <button id="loadMemoryGraph" class="secondary">Load memory graph</button>
        </div>
        <svg id="memoryGraphCanvas" width="100%" height="340" viewBox="0 0 900 340" role="img" aria-label="Memory graph visualization"></svg>
        <pre id="memoryGraphStatus">Memory graph status will appear here.</pre>
      </section>

      <section id="view-configuration" class="view" role="tabpanel" aria-hidden="true">
        <p style="margin: 0 0 0.5rem 0; color: var(--ink-muted);">Runtime endpoints and policy gates discovered from gateway configuration.</p>
        <pre id="configView"></pre>
      </section>
    </section>
  </main>

  <script>
    const RESPONSES_ENDPOINT = "{responses_endpoint}";
    const CHAT_COMPLETIONS_ENDPOINT = "{chat_completions_endpoint}";
    const COMPLETIONS_ENDPOINT = "{completions_endpoint}";
    const MODELS_ENDPOINT = "{models_endpoint}";
    const STATUS_ENDPOINT = "{status_endpoint}";
    const DASHBOARD_HEALTH_ENDPOINT = "{dashboard_health_endpoint}";
    const DASHBOARD_WIDGETS_ENDPOINT = "{dashboard_widgets_endpoint}";
    const DASHBOARD_QUEUE_TIMELINE_ENDPOINT = "{dashboard_queue_timeline_endpoint}";
    const DASHBOARD_ALERTS_ENDPOINT = "{dashboard_alerts_endpoint}";
    const DASHBOARD_ACTIONS_ENDPOINT = "{dashboard_actions_endpoint}";
    const DASHBOARD_STREAM_ENDPOINT = "{dashboard_stream_endpoint}";
    const WEBSOCKET_ENDPOINT = "{websocket_endpoint}";
    const SESSIONS_ENDPOINT = "{sessions_endpoint}";
    const MEMORY_ENDPOINT_TEMPLATE = "{memory_endpoint_template}";
    const MEMORY_GRAPH_ENDPOINT_TEMPLATE = "{memory_graph_endpoint_template}";
    const SESSION_DETAIL_ENDPOINT_TEMPLATE = "{session_detail_endpoint_template}";
    const SESSION_APPEND_ENDPOINT_TEMPLATE = "{session_append_endpoint_template}";
    const SESSION_RESET_ENDPOINT_TEMPLATE = "{session_reset_endpoint_template}";
    const UI_TELEMETRY_ENDPOINT = "{ui_telemetry_endpoint}";
    const SESSION_WRITE_POLICY_GATE = "{session_policy_gate}";
    const MEMORY_WRITE_POLICY_GATE = "{memory_policy_gate}";

    const STORAGE_TOKEN = "tau.gateway.webchat.token";
    const STORAGE_SESSION = "tau.gateway.webchat.session";

    const tokenInput = document.getElementById("authToken");
    const sessionInput = document.getElementById("sessionKey");
    const apiModeInput = document.getElementById("apiMode");
    const streamInput = document.getElementById("stream");
    const promptInput = document.getElementById("prompt");
    const outputPre = document.getElementById("output");
    const statusPre = document.getElementById("status");
    const healthStateValue = document.getElementById("healthStateValue");
    const rolloutGateValue = document.getElementById("rolloutGateValue");
    const queueDepthValue = document.getElementById("queueDepthValue");
    const failureStreakValue = document.getElementById("failureStreakValue");
    const connectorTableBody = document.getElementById("connectorTableBody");
    const reasonCodeTableBody = document.getElementById("reasonCodeTableBody");
    const dashboardStatusPre = document.getElementById("dashboardStatus");
    const dashboardActionReasonInput = document.getElementById("dashboardActionReason");
    const dashboardLiveInput = document.getElementById("dashboardLive");
    const dashboardPollSecondsInput = document.getElementById("dashboardPollSeconds");
    const dashboardHealthStateValue = document.getElementById("dashboardHealthStateValue");
    const dashboardRolloutGateValue = document.getElementById("dashboardRolloutGateValue");
    const dashboardControlModeValue = document.getElementById("dashboardControlModeValue");
    const dashboardRunStateValue = document.getElementById("dashboardRunStateValue");
    const dashboardTotalRolloutsValue = document.getElementById("dashboardTotalRolloutsValue");
    const dashboardFailedRolloutsValue = document.getElementById("dashboardFailedRolloutsValue");
    const dashboardWidgetsTableBody = document.getElementById("dashboardWidgetsTableBody");
    const dashboardAlertsTableBody = document.getElementById("dashboardAlertsTableBody");
    const dashboardTimelineTableBody = document.getElementById("dashboardTimelineTableBody");
    const sessionsList = document.getElementById("sessionsList");
    const sessionDetailPre = document.getElementById("sessionDetail");
    const appendRoleInput = document.getElementById("appendRole");
    const appendContentInput = document.getElementById("appendContent");
    const sessionPolicyGateInput = document.getElementById("sessionPolicyGate");
    const memoryPolicyGateInput = document.getElementById("memoryPolicyGate");
    const memoryContentInput = document.getElementById("memoryContent");
    const memoryStatusPre = document.getElementById("memoryStatus");
    const graphMaxNodesInput = document.getElementById("graphMaxNodes");
    const graphMinEdgeWeightInput = document.getElementById("graphMinEdgeWeight");
    const graphRelationTypesInput = document.getElementById("graphRelationTypes");
    const memoryGraphCanvas = document.getElementById("memoryGraphCanvas");
    const memoryGraphStatusPre = document.getElementById("memoryGraphStatus");
    const configViewPre = document.getElementById("configView");

    const sendButton = document.getElementById("send");
    const clearButton = document.getElementById("clearOutput");
    const refreshButton = document.getElementById("refreshStatus");
    const dashboardRefreshButton = document.getElementById("dashboardRefresh");
    const dashboardPauseButton = document.getElementById("dashboardPause");
    const dashboardResumeButton = document.getElementById("dashboardResume");
    const dashboardControlRefreshButton = document.getElementById("dashboardControlRefresh");
    const loadSessionsButton = document.getElementById("loadSessions");
    const loadSessionDetailButton = document.getElementById("loadSessionDetail");
    const appendSessionButton = document.getElementById("appendSession");
    const resetSessionButton = document.getElementById("resetSession");
    const loadMemoryButton = document.getElementById("loadMemory");
    const saveMemoryButton = document.getElementById("saveMemory");
    const loadMemoryGraphButton = document.getElementById("loadMemoryGraph");

    let dashboardLiveTimer = null;
    let dashboardRefreshInFlight = false;

    function loadLocalValues() {{
      const token = window.localStorage.getItem(STORAGE_TOKEN);
      const sessionKey = window.localStorage.getItem(STORAGE_SESSION);
      if (token) {{
        tokenInput.value = token;
      }}
      if (sessionKey) {{
        sessionInput.value = sessionKey;
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
      return {{ "Authorization": "Bearer " + token }};
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

    function currentSessionKey() {{
      return (sessionInput.value.trim() || "{default_session_key}");
    }}

    function endpointForMode(mode) {{
      if (mode === "chat_completions") {{
        return CHAT_COMPLETIONS_ENDPOINT;
      }}
      if (mode === "completions") {{
        return COMPLETIONS_ENDPOINT;
      }}
      return RESPONSES_ENDPOINT;
    }}

    function telemetryPayload(view, action, reasonCode, metadata) {{
      return {{
        view: view,
        action: action,
        reason_code: reasonCode,
        session_key: currentSessionKey(),
        metadata: metadata || {{}}
      }};
    }}

    async function emitUiTelemetry(view, action, reasonCode, metadata) {{
      try {{
        await fetch(UI_TELEMETRY_ENDPOINT, {{
          method: "POST",
          headers: Object.assign({{ "Content-Type": "application/json" }}, authHeaders()),
          body: JSON.stringify(telemetryPayload(view, action, reasonCode, metadata))
        }});
      }} catch (_error) {{
        // best-effort telemetry path
      }}
    }}

    function encodeSessionPath(template) {{
      return template.replace("{{session_key}}", encodeURIComponent(currentSessionKey()));
    }}

    function applyMetricValue(node, value, tone) {{
      node.textContent = String(value);
      node.classList.remove("ok", "warn", "bad");
      if (tone) {{
        node.classList.add(tone);
      }}
    }}

    function toSafeInteger(value) {{
      if (typeof value === "number" && Number.isFinite(value)) {{
        return Math.max(0, Math.trunc(value));
      }}
      if (typeof value === "string" && value.trim().length > 0) {{
        const parsed = Number(value);
        if (Number.isFinite(parsed)) {{
          return Math.max(0, Math.trunc(parsed));
        }}
      }}
      return 0;
    }}

    function toSafeFloat(value, fallbackValue) {{
      if (typeof value === "number" && Number.isFinite(value)) {{
        return value;
      }}
      if (typeof value === "string" && value.trim().length > 0) {{
        const parsed = Number(value);
        if (Number.isFinite(parsed)) {{
          return parsed;
        }}
      }}
      return fallbackValue;
    }}

    function clampGraphMaxNodes(value) {{
      return Math.min(256, Math.max(1, toSafeInteger(value) || 24));
    }}

    function clampGraphMinEdgeWeight(value) {{
      return Math.max(0, toSafeFloat(value, 1));
    }}

    function relationColor(relationType) {{
      if (relationType === "contains") {{
        return '#0b6f56';
      }}
      if (relationType === "keyword_overlap") {{
        return '#9a5a00';
      }}
      return '#40627a';
    }}

    function escapeHtml(value) {{
      return String(value)
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll("\"", "&quot;")
        .replaceAll("'", "&#39;");
    }}

    function sortedCounterEntries(counter) {{
      return Object.entries(counter || {{}})
        .sort((left, right) => {{
          const countDelta = toSafeInteger(right[1]) - toSafeInteger(left[1]);
          if (countDelta !== 0) {{
            return countDelta;
          }}
          return String(left[0]).localeCompare(String(right[0]));
        }});
    }}

    function renderReasonCodeTable(reasonCodeCounts) {{
      const entries = sortedCounterEntries(reasonCodeCounts);
      if (entries.length === 0) {{
        reasonCodeTableBody.innerHTML = "<tr><td colspan=\"2\">No reason-code samples yet.</td></tr>";
        return;
      }}
      reasonCodeTableBody.innerHTML = entries.map(([code, count]) =>
        "<tr><td>" + escapeHtml(code) + "</td><td>" + String(toSafeInteger(count)) + "</td></tr>"
      ).join("");
    }}

    function renderConnectorTable(connectors) {{
      const entries = Object.entries((connectors && connectors.channels) || {{}})
        .sort((left, right) => String(left[0]).localeCompare(String(right[0])));
      if (entries.length === 0) {{
        connectorTableBody.innerHTML = "<tr><td colspan=\"9\">No connector data yet.</td></tr>";
        return;
      }}
      connectorTableBody.innerHTML = entries.map(([channel, status]) => {{
        const row = status || {{}};
        return [
          "<tr>",
          "<td>" + escapeHtml(channel) + "</td>",
          "<td>" + escapeHtml(row.liveness || "unknown") + "</td>",
          "<td>" + escapeHtml(row.breaker_state || "unknown") + "</td>",
          "<td>" + String(toSafeInteger(row.events_ingested)) + "</td>",
          "<td>" + String(toSafeInteger(row.duplicates_skipped)) + "</td>",
          "<td>" + String(toSafeInteger(row.retry_attempts)) + "</td>",
          "<td>" + String(toSafeInteger(row.auth_failures)) + "</td>",
          "<td>" + String(toSafeInteger(row.parse_failures)) + "</td>",
          "<td>" + String(toSafeInteger(row.provider_failures)) + "</td>",
          "</tr>"
        ].join("");
      }}).join("");
    }}

    function renderStatusDashboard(payload) {{
      const service = payload && payload.service ? payload.service : {{}};
      const mc = payload && payload.multi_channel ? payload.multi_channel : {{}};
      const connectors = mc.connectors || {{}};

      const healthState = String(mc.health_state || "unknown");
      const gateState = String(mc.rollout_gate || service.rollout_gate || "unknown");
      const queueDepth = toSafeInteger(mc.queue_depth);
      const failureStreak = toSafeInteger(mc.failure_streak);

      const healthTone = healthState === "healthy"
        ? "ok"
        : (healthState === "degraded" ? "warn" : "bad");
      const gateTone = gateState === "pass"
        ? "ok"
        : (gateState === "hold" ? "warn" : "bad");
      const streakTone = failureStreak === 0 ? "ok" : "warn";

      applyMetricValue(healthStateValue, healthState, healthTone);
      applyMetricValue(rolloutGateValue, gateState, gateTone);
      applyMetricValue(queueDepthValue, queueDepth, queueDepth === 0 ? "ok" : "warn");
      applyMetricValue(failureStreakValue, failureStreak, streakTone);

      renderConnectorTable(connectors);
      renderReasonCodeTable(mc.reason_code_counts || {{}});
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

    function formatDashboardTimestamp(unixMs) {{
      const parsed = Number(unixMs);
      if (!Number.isFinite(parsed) || parsed <= 0) {{
        return "n/a";
      }}
      return new Date(parsed).toISOString();
    }}

    function renderDashboardWidgetsTable(widgets) {{
      const entries = Array.isArray(widgets) ? widgets.slice() : [];
      entries.sort((left, right) => String(left.widget_id || "").localeCompare(String(right.widget_id || "")));
      if (entries.length === 0) {{
        dashboardWidgetsTableBody.innerHTML = "<tr><td colspan=\"5\">No widget rows yet.</td></tr>";
        return;
      }}
      dashboardWidgetsTableBody.innerHTML = entries.map((widget) => {{
        return [
          "<tr>",
          "<td>" + escapeHtml(widget.title || widget.widget_id || "unknown") + "</td>",
          "<td>" + escapeHtml(widget.kind || "unknown") + "</td>",
          "<td>" + escapeHtml(widget.query_key || "n/a") + "</td>",
          "<td>" + String(toSafeInteger(widget.refresh_interval_ms)) + "</td>",
          "<td>" + escapeHtml(formatDashboardTimestamp(widget.updated_unix_ms)) + "</td>",
          "</tr>"
        ].join("");
      }}).join("");
    }}

    function renderDashboardAlertsTable(alerts) {{
      const entries = Array.isArray(alerts) ? alerts.slice() : [];
      if (entries.length === 0) {{
        dashboardAlertsTableBody.innerHTML = "<tr><td colspan=\"3\">No dashboard alerts.</td></tr>";
        return;
      }}
      dashboardAlertsTableBody.innerHTML = entries.map((alert) => {{
        return [
          "<tr>",
          "<td>" + escapeHtml(alert.code || "unknown") + "</td>",
          "<td>" + escapeHtml(alert.severity || "info") + "</td>",
          "<td>" + escapeHtml(alert.message || "") + "</td>",
          "</tr>"
        ].join("");
      }}).join("");
    }}

    function renderDashboardTimelineTable(queueTimeline) {{
      const cycles = queueTimeline && Array.isArray(queueTimeline.recent_cycles)
        ? queueTimeline.recent_cycles.slice()
        : [];
      if (cycles.length === 0) {{
        dashboardTimelineTableBody.innerHTML = "<tr><td colspan=\"6\">No timeline entries yet.</td></tr>";
        return;
      }}
      dashboardTimelineTableBody.innerHTML = cycles.map((cycle) => {{
        return [
          "<tr>",
          "<td>" + escapeHtml(formatDashboardTimestamp(cycle.timestamp_unix_ms)) + "</td>",
          "<td>" + escapeHtml(cycle.health_state || "unknown") + "</td>",
          "<td>" + escapeHtml(cycle.health_reason || "n/a") + "</td>",
          "<td>" + String(toSafeInteger(cycle.queued_cases)) + "</td>",
          "<td>" + String(toSafeInteger(cycle.applied_cases)) + "</td>",
          "<td>" + String(toSafeInteger(cycle.failed_cases)) + "</td>",
          "</tr>"
        ].join("");
      }}).join("");
    }}

    function renderDashboardSnapshot(payloads) {{
      const healthPayload = payloads.health || {{}};
      const widgetsPayload = payloads.widgets || {{}};
      const timelinePayload = payloads.timeline || {{}};
      const alertsPayload = payloads.alerts || {{}};

      const health = healthPayload.health || {{}};
      const training = healthPayload.training || {{}};
      const control = healthPayload.control || {{}};
      const widgets = widgetsPayload.widgets || [];
      const alerts = alertsPayload.alerts || [];
      const queueTimeline = timelinePayload.queue_timeline || {{}};

      const healthState = String(health.health_state || "unknown");
      const rolloutGate = String(health.rollout_gate || "unknown");
      const controlMode = String(control.mode || "unknown");
      const runState = String(training.run_state || "unknown");
      const totalRollouts = toSafeInteger(training.total_rollouts);
      const failedRollouts = toSafeInteger(training.failed);

      const healthTone = healthState === "healthy"
        ? "ok"
        : (healthState === "degraded" ? "warn" : "bad");
      const gateTone = rolloutGate === "pass"
        ? "ok"
        : (rolloutGate === "hold" ? "warn" : "bad");
      const controlTone = controlMode === "running"
        ? "ok"
        : (controlMode === "paused" ? "warn" : "bad");
      const runTone = runState === "completed"
        ? "ok"
        : (runState === "running" ? "warn" : "bad");
      const failedTone = failedRollouts === 0 ? "ok" : "bad";

      applyMetricValue(dashboardHealthStateValue, healthState, healthTone);
      applyMetricValue(dashboardRolloutGateValue, rolloutGate, gateTone);
      applyMetricValue(dashboardControlModeValue, controlMode, controlTone);
      applyMetricValue(dashboardRunStateValue, runState, runTone);
      applyMetricValue(dashboardTotalRolloutsValue, totalRollouts, totalRollouts > 0 ? "ok" : "warn");
      applyMetricValue(dashboardFailedRolloutsValue, failedRollouts, failedTone);

      renderDashboardWidgetsTable(widgets);
      renderDashboardAlertsTable(alerts);
      renderDashboardTimelineTable(queueTimeline);

      dashboardStatusPre.textContent = [
        "dashboard_snapshot: health=" + healthState +
          " rollout_gate=" + rolloutGate +
          " control_mode=" + controlMode +
          " run_state=" + runState +
          " total_rollouts=" + String(totalRollouts) +
          " failed_rollouts=" + String(failedRollouts),
        "dashboard_widgets: count=" + String(Array.isArray(widgets) ? widgets.length : 0),
        "dashboard_alerts: count=" + String(Array.isArray(alerts) ? alerts.length : 0),
        "dashboard_timeline_cycles: count=" + String(Array.isArray(queueTimeline.recent_cycles) ? queueTimeline.recent_cycles.length : 0),
        "dashboard_stream_endpoint: " + DASHBOARD_STREAM_ENDPOINT,
        "",
        "raw_payloads:",
        JSON.stringify(payloads, null, 2),
      ].join("\n");
    }}

    function dashboardPollIntervalMs() {{
      const seconds = Math.min(60, Math.max(2, toSafeInteger(dashboardPollSecondsInput.value) || 5));
      dashboardPollSecondsInput.value = String(seconds);
      return seconds * 1000;
    }}

    function setDashboardControlsDisabled(disabled) {{
      dashboardRefreshButton.disabled = disabled;
      dashboardPauseButton.disabled = disabled;
      dashboardResumeButton.disabled = disabled;
      dashboardControlRefreshButton.disabled = disabled;
    }}

    async function refreshDashboard() {{
      const emitTelemetry = arguments.length === 0 ? true : Boolean(arguments[0]);
      if (dashboardRefreshInFlight) {{
        return;
      }}
      dashboardRefreshInFlight = true;
      setDashboardControlsDisabled(true);
      dashboardStatusPre.textContent = "Loading dashboard status...";
      try {{
        const responses = await Promise.all([
          fetch(DASHBOARD_HEALTH_ENDPOINT, {{ headers: authHeaders() }}),
          fetch(DASHBOARD_WIDGETS_ENDPOINT, {{ headers: authHeaders() }}),
          fetch(DASHBOARD_QUEUE_TIMELINE_ENDPOINT, {{ headers: authHeaders() }}),
          fetch(DASHBOARD_ALERTS_ENDPOINT, {{ headers: authHeaders() }}),
        ]);
        const rawBodies = await Promise.all(responses.map((response) => response.text()));
        const payloads = rawBodies.map((raw) => {{
          try {{
            return JSON.parse(raw);
          }} catch (_error) {{
            return {{ parse_error: true, raw: raw }};
          }}
        }});
        const failedIndex = responses.findIndex((response) => !response.ok);
        if (failedIndex >= 0) {{
          dashboardStatusPre.textContent =
            "dashboard request failed: status=" + String(responses[failedIndex].status) +
            "\n" + rawBodies[failedIndex];
          if (emitTelemetry) {{
            await emitUiTelemetry("dashboard", "refresh", "dashboard_refresh_failed", {{
              status: responses[failedIndex].status,
              endpoint_index: failedIndex
            }});
          }}
          return;
        }}
        const composed = {{
          health: payloads[0],
          widgets: payloads[1],
          timeline: payloads[2],
          alerts: payloads[3]
        }};
        renderDashboardSnapshot(composed);
        if (emitTelemetry) {{
          await emitUiTelemetry("dashboard", "refresh", "dashboard_refreshed", {{
            health_state: composed.health && composed.health.health
              ? composed.health.health.health_state
              : "unknown"
          }});
        }}
      }} catch (error) {{
        dashboardStatusPre.textContent = "dashboard request failed: " + String(error);
      }} finally {{
        dashboardRefreshInFlight = false;
        setDashboardControlsDisabled(false);
      }}
    }}

    async function postDashboardAction(action) {{
      setDashboardControlsDisabled(true);
      try {{
        const response = await fetch(DASHBOARD_ACTIONS_ENDPOINT, {{
          method: "POST",
          headers: Object.assign({{ "Content-Type": "application/json" }}, authHeaders()),
          body: JSON.stringify({{
            action: action,
            reason: dashboardActionReasonInput.value.trim()
          }})
        }});
        const raw = await response.text();
        let payload = null;
        try {{
          payload = JSON.parse(raw);
        }} catch (_error) {{
          payload = {{ raw: raw }};
        }}
        dashboardStatusPre.textContent = JSON.stringify(payload, null, 2);
        if (!response.ok) {{
          await emitUiTelemetry("dashboard", "action", "dashboard_action_failed", {{
            action: action,
            status: response.status
          }});
          return;
        }}
        await emitUiTelemetry("dashboard", "action", "dashboard_action_applied", {{ action: action }});
        await refreshDashboard(false);
      }} catch (error) {{
        dashboardStatusPre.textContent = "dashboard action failed: " + String(error);
      }} finally {{
        setDashboardControlsDisabled(false);
      }}
    }}

    function updateDashboardLiveMode() {{
      if (dashboardLiveTimer !== null) {{
        window.clearInterval(dashboardLiveTimer);
        dashboardLiveTimer = null;
      }}
      if (!dashboardLiveInput.checked) {{
        emitUiTelemetry("dashboard", "live_mode", "dashboard_live_disabled", {{}});
        return;
      }}
      const pollIntervalMs = dashboardPollIntervalMs();
      dashboardLiveTimer = window.setInterval(() => {{
        refreshDashboard(false);
      }}, pollIntervalMs);
      emitUiTelemetry("dashboard", "live_mode", "dashboard_live_enabled", {{
        poll_interval_ms: pollIntervalMs
      }});
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
        "multi_channel_diagnostics: " + diagnostics
      ].join("\n");
    }}

    function formatConfigView(payload) {{
      const gateway = payload && payload.gateway ? payload.gateway : {{}};
      return JSON.stringify({{
        endpoints: {{
          responses: RESPONSES_ENDPOINT,
          chat_completions: CHAT_COMPLETIONS_ENDPOINT,
          completions: COMPLETIONS_ENDPOINT,
          models: MODELS_ENDPOINT,
          status: STATUS_ENDPOINT,
          dashboard_health: DASHBOARD_HEALTH_ENDPOINT,
          dashboard_widgets: DASHBOARD_WIDGETS_ENDPOINT,
          dashboard_queue_timeline: DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
          dashboard_alerts: DASHBOARD_ALERTS_ENDPOINT,
          dashboard_actions: DASHBOARD_ACTIONS_ENDPOINT,
          dashboard_stream: DASHBOARD_STREAM_ENDPOINT,
          sessions: SESSIONS_ENDPOINT,
          session_detail: SESSION_DETAIL_ENDPOINT_TEMPLATE,
          session_append: SESSION_APPEND_ENDPOINT_TEMPLATE,
          session_reset: SESSION_RESET_ENDPOINT_TEMPLATE,
          memory: MEMORY_ENDPOINT_TEMPLATE,
          memory_graph: MEMORY_GRAPH_ENDPOINT_TEMPLATE,
          ui_telemetry: UI_TELEMETRY_ENDPOINT,
          websocket: WEBSOCKET_ENDPOINT
        }},
        policy_gates: {{
          session_write: SESSION_WRITE_POLICY_GATE,
          memory_write: MEMORY_WRITE_POLICY_GATE
        }},
        gateway_status_summary: gateway
      }}, null, 2);
    }}

    function parseSseFrame(rawFrame) {{
      const lines = rawFrame.split(/\r?\n/);
      let eventName = "";
      let data = "";
      for (const line of lines) {{
        if (line.startsWith("event:")) {{
          eventName = line.slice("event:".length).trim();
        }} else if (line.startsWith("data:")) {{
          data += line.slice("data:".length).trim();
        }}
      }}
      if (eventName.length === 0 && data.length === 0) {{
        return null;
      }}
      return {{ eventName, data }};
    }}

    function processSseFrame(frame, mode) {{
      if (!frame) {{
        return;
      }}
      if (frame.data === "[DONE]") {{
        appendOutput("\n");
        return;
      }}
      if (!frame.data) {{
        return;
      }}

      let payload = null;
      try {{
        payload = JSON.parse(frame.data);
      }} catch (_error) {{
        appendOutput("\n[invalid sse payload] " + frame.data + "\n");
        return;
      }}

      if (mode === "responses") {{
        if (frame.eventName === "response.output_text.delta") {{
          appendOutput(payload.delta || "");
          return;
        }}
        if (frame.eventName === "response.failed") {{
          const message = payload && payload.error ? payload.error.message : "unknown";
          appendOutput("\n[gateway error] " + message + "\n");
        }}
        return;
      }}

      if (mode === "chat_completions") {{
        const choice = payload && Array.isArray(payload.choices) ? payload.choices[0] : null;
        const delta = choice && choice.delta ? choice.delta : null;
        if (delta && typeof delta.content === "string") {{
          appendOutput(delta.content);
        }}
        return;
      }}

      if (mode === "completions") {{
        const choice = payload && Array.isArray(payload.choices) ? payload.choices[0] : null;
        if (choice && typeof choice.text === "string") {{
          appendOutput(choice.text);
        }}
      }}
    }}

    async function readSseBody(response, mode) {{
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
          const rawFrame = buffer.slice(0, splitIndex);
          buffer = buffer.slice(splitIndex + 2);
          processSseFrame(parseSseFrame(rawFrame), mode);
        }}
      }}
      if (buffer.trim().length > 0) {{
        processSseFrame(parseSseFrame(buffer), mode);
      }}
    }}

    function buildRequestPayload(mode, prompt, stream) {{
      if (mode === "chat_completions") {{
        return {{
          messages: [{{ role: "user", content: prompt }}],
          user: currentSessionKey(),
          stream: stream
        }};
      }}
      if (mode === "completions") {{
        return {{
          prompt: prompt,
          user: currentSessionKey(),
          stream: stream
        }};
      }}
      return {{
        input: prompt,
        stream: stream,
        metadata: {{ session_id: currentSessionKey() }}
      }};
    }}

    function extractNonStreamOutput(mode, payload) {{
      if (mode === "chat_completions") {{
        return payload && payload.choices && payload.choices[0] && payload.choices[0].message
          ? (payload.choices[0].message.content || "")
          : JSON.stringify(payload, null, 2);
      }}
      if (mode === "completions") {{
        return payload && payload.choices && payload.choices[0]
          ? (payload.choices[0].text || "")
          : JSON.stringify(payload, null, 2);
      }}
      return typeof payload.output_text === "string" ? payload.output_text : JSON.stringify(payload, null, 2);
    }}

    async function sendPrompt() {{
      const prompt = promptInput.value.trim();
      if (prompt.length === 0) {{
        setOutput("Prompt is required.");
        return;
      }}

      saveLocalValues();
      sendButton.disabled = true;
      const mode = apiModeInput.value;
      try {{
        setOutput("");
        const payload = buildRequestPayload(mode, prompt, streamInput.checked);
        const response = await fetch(endpointForMode(mode), {{
          method: "POST",
          headers: Object.assign({{ "Content-Type": "application/json" }}, authHeaders()),
          body: JSON.stringify(payload)
        }});
        if (!response.ok) {{
          setOutput("request failed: status=" + response.status + "\n" + await response.text());
          await emitUiTelemetry("conversation", "send", "prompt_failed", {{ mode: mode, status: response.status }});
          return;
        }}

        if (streamInput.checked) {{
          await readSseBody(response, mode);
        }} else {{
          const body = await response.json();
          setOutput(extractNonStreamOutput(mode, body));
        }}

        await emitUiTelemetry("conversation", "send", "prompt_completed", {{ mode: mode, stream: streamInput.checked }});
        await refreshStatus();
      }} catch (error) {{
        setOutput("request failed: " + String(error));
        await emitUiTelemetry("conversation", "send", "prompt_exception", {{ mode: mode }});
      }} finally {{
        sendButton.disabled = false;
      }}
    }}

    async function refreshStatus() {{
      statusPre.textContent = "Loading gateway status...";
      try {{
        const response = await fetch(STATUS_ENDPOINT, {{ headers: authHeaders() }});
        const raw = await response.text();
        if (!response.ok) {{
          renderStatusDashboard(null);
          statusPre.textContent = "status " + response.status + "\n" + raw;
          return;
        }}
        const payload = JSON.parse(raw);
        renderStatusDashboard(payload);
        statusPre.textContent = formatGatewayStatusSummary(payload) + "\n\nraw_payload:\n" + JSON.stringify(payload, null, 2);
        configViewPre.textContent = formatConfigView(payload);
        await emitUiTelemetry("tools", "refresh_status", "status_refreshed", {{ health_state: payload.multi_channel ? payload.multi_channel.health_state : "unknown" }});
      }} catch (error) {{
        renderStatusDashboard(null);
        statusPre.textContent = "status request failed: " + String(error);
      }}
    }}

    async function loadSessions() {{
      sessionsList.innerHTML = "<div>Loading sessions...</div>";
      try {{
        const response = await fetch(SESSIONS_ENDPOINT, {{ headers: authHeaders() }});
        const payload = await response.json();
        if (!response.ok) {{
          sessionsList.textContent = JSON.stringify(payload, null, 2);
          return;
        }}
        const sessions = Array.isArray(payload.sessions) ? payload.sessions : [];
        if (sessions.length === 0) {{
          sessionsList.innerHTML = "<div>No session files found.</div>";
          return;
        }}
        sessionsList.innerHTML = "";
        for (const session of sessions) {{
          const button = document.createElement("button");
          button.type = "button";
          button.className = "secondary";
          button.textContent = session.session_key + " (" + String(session.message_count || 0) + " entries)";
          button.addEventListener("click", () => {{
            sessionInput.value = session.session_key;
            loadSessionDetail();
          }});
          sessionsList.appendChild(button);
        }}
        await emitUiTelemetry("sessions", "list", "session_list_loaded", {{ count: sessions.length }});
      }} catch (error) {{
        sessionsList.textContent = "Failed to load sessions: " + String(error);
      }}
    }}

    async function loadSessionDetail() {{
      const endpoint = encodeSessionPath(SESSION_DETAIL_ENDPOINT_TEMPLATE);
      sessionDetailPre.textContent = "Loading session detail...";
      try {{
        const response = await fetch(endpoint, {{ headers: authHeaders() }});
        const payload = await response.json();
        sessionDetailPre.textContent = JSON.stringify(payload, null, 2);
        if (response.ok) {{
          await emitUiTelemetry("sessions", "detail", "session_detail_loaded", {{ entry_count: payload.entry_count || 0 }});
        }}
      }} catch (error) {{
        sessionDetailPre.textContent = "Failed to load session detail: " + String(error);
      }}
    }}

    async function appendSessionMessage() {{
      const endpoint = encodeSessionPath(SESSION_APPEND_ENDPOINT_TEMPLATE);
      const content = appendContentInput.value.trim();
      if (content.length === 0) {{
        sessionDetailPre.textContent = "Append content is required.";
        return;
      }}
      const body = {{
        role: appendRoleInput.value,
        content: content,
        policy_gate: sessionPolicyGateInput.value.trim()
      }};
      try {{
        const response = await fetch(endpoint, {{
          method: "POST",
          headers: Object.assign({{ "Content-Type": "application/json" }}, authHeaders()),
          body: JSON.stringify(body)
        }});
        const payload = await response.json();
        sessionDetailPre.textContent = JSON.stringify(payload, null, 2);
        if (response.ok) {{
          appendContentInput.value = "";
          await emitUiTelemetry("sessions", "append", "session_append_applied", {{ role: body.role }});
          await loadSessionDetail();
          await loadSessions();
        }} else {{
          await emitUiTelemetry("sessions", "append", "session_append_failed", {{ status: response.status }});
        }}
      }} catch (error) {{
        sessionDetailPre.textContent = "Failed to append session message: " + String(error);
      }}
    }}

    async function resetSession() {{
      const endpoint = encodeSessionPath(SESSION_RESET_ENDPOINT_TEMPLATE);
      const body = {{ policy_gate: sessionPolicyGateInput.value.trim() }};
      try {{
        const response = await fetch(endpoint, {{
          method: "POST",
          headers: Object.assign({{ "Content-Type": "application/json" }}, authHeaders()),
          body: JSON.stringify(body)
        }});
        const payload = await response.json();
        sessionDetailPre.textContent = JSON.stringify(payload, null, 2);
        if (response.ok) {{
          await emitUiTelemetry("sessions", "reset", "session_reset_applied", {{}});
          await loadSessions();
        }} else {{
          await emitUiTelemetry("sessions", "reset", "session_reset_failed", {{ status: response.status }});
        }}
      }} catch (error) {{
        sessionDetailPre.textContent = "Failed to reset session: " + String(error);
      }}
    }}

    async function loadMemory() {{
      const endpoint = encodeSessionPath(MEMORY_ENDPOINT_TEMPLATE);
      memoryStatusPre.textContent = "Loading memory...";
      try {{
        const response = await fetch(endpoint, {{ headers: authHeaders() }});
        const payload = await response.json();
        memoryStatusPre.textContent = JSON.stringify(payload, null, 2);
        if (response.ok) {{
          memoryContentInput.value = payload.content || "";
          await emitUiTelemetry("memory", "read", "memory_read_loaded", {{ exists: Boolean(payload.exists) }});
        }}
      }} catch (error) {{
        memoryStatusPre.textContent = "Failed to load memory: " + String(error);
      }}
    }}

    async function saveMemory() {{
      const endpoint = encodeSessionPath(MEMORY_ENDPOINT_TEMPLATE);
      const body = {{
        content: memoryContentInput.value,
        policy_gate: memoryPolicyGateInput.value.trim()
      }};
      try {{
        const response = await fetch(endpoint, {{
          method: "PUT",
          headers: Object.assign({{ "Content-Type": "application/json" }}, authHeaders()),
          body: JSON.stringify(body)
        }});
        const payload = await response.json();
        memoryStatusPre.textContent = JSON.stringify(payload, null, 2);
        if (response.ok) {{
          await emitUiTelemetry("memory", "write", "memory_write_applied", {{ bytes: payload.bytes || 0 }});
        }} else {{
          await emitUiTelemetry("memory", "write", "memory_write_failed", {{ status: response.status }});
        }}
      }} catch (error) {{
        memoryStatusPre.textContent = "Failed to write memory: " + String(error);
      }}
    }}

    function resetMemoryGraphCanvas() {{
      while (memoryGraphCanvas.firstChild) {{
        memoryGraphCanvas.removeChild(memoryGraphCanvas.firstChild);
      }}
      memoryGraphCanvas.setAttribute("viewBox", "0 0 900 340");
    }}

    function graphNodeSeed(nodeId) {{
      const value = String(nodeId || "");
      let hash = 2166136261;
      for (let index = 0; index < value.length; index += 1) {{
        hash ^= value.charCodeAt(index);
        hash = Math.imul(hash, 16777619);
      }}
      return (hash >>> 0) / 4294967295;
    }}

    function clampGraphPosition(value, min, max) {{
      return Math.max(min, Math.min(max, value));
    }}

    function computeMemoryGraphForceLayout(nodes, edges, width, height) {{
      const centerX = width / 2;
      const centerY = height / 2;
      const margin = 22;
      const nodeStates = nodes.map((node, index) => {{
        const seed = graphNodeSeed(node.id || String(index));
        const angle = ((2 * Math.PI * index) / Math.max(1, nodes.length)) + (seed * 0.4);
        const radial = Math.min(width, height) * (0.16 + (0.24 * seed));
        return {{
          id: node.id,
          node: node,
          x: centerX + (Math.cos(angle) * radial),
          y: centerY + (Math.sin(angle) * radial),
          vx: 0,
          vy: 0
        }};
      }});
      const positions = new Map();
      for (const state of nodeStates) {{
        positions.set(state.id, state);
      }}

      const repulsionStrength = 3600 / Math.max(1, Math.sqrt(nodes.length));
      const springStrength = 0.015;
      const centerStrength = 0.006;
      const damping = 0.86;
      const minimumDistance = 18;
      const springLength = Math.max(48, Math.min(130, (width + height) / Math.max(9, nodes.length)));
      const iterations = 84;

      for (let iteration = 0; iteration < iterations; iteration += 1) {{
        for (let left = 0; left < nodeStates.length; left += 1) {{
          for (let right = left + 1; right < nodeStates.length; right += 1) {{
            const source = nodeStates[left];
            const target = nodeStates[right];
            const dx = source.x - target.x;
            const dy = source.y - target.y;
            const distanceSquared = Math.max(minimumDistance * minimumDistance, (dx * dx) + (dy * dy));
            const distance = Math.sqrt(distanceSquared);
            const force = repulsionStrength / distanceSquared;
            const fx = (dx / distance) * force;
            const fy = (dy / distance) * force;
            source.vx += fx;
            source.vy += fy;
            target.vx -= fx;
            target.vy -= fy;
          }}
        }}

        for (const edge of edges) {{
          const source = positions.get(edge.source);
          const target = positions.get(edge.target);
          if (!source || !target) {{
            continue;
          }}
          const dx = target.x - source.x;
          const dy = target.y - source.y;
          const distanceSquared = Math.max(minimumDistance * minimumDistance, (dx * dx) + (dy * dy));
          const distance = Math.sqrt(distanceSquared);
          const weight = Math.max(0.45, Math.min(2.4, toSafeFloat(edge.weight, 1)));
          const pull = (distance - springLength) * springStrength * weight;
          const fx = (dx / distance) * pull;
          const fy = (dy / distance) * pull;
          source.vx += fx;
          source.vy += fy;
          target.vx -= fx;
          target.vy -= fy;
        }}

        for (const state of nodeStates) {{
          state.vx += (centerX - state.x) * centerStrength;
          state.vy += (centerY - state.y) * centerStrength;
          state.vx *= damping;
          state.vy *= damping;
          state.x = clampGraphPosition(state.x + state.vx, margin, width - margin);
          state.y = clampGraphPosition(state.y + state.vy, margin, height - margin);
        }}
      }}

      return positions;
    }}

    function renderMemoryGraph(payload) {{
      const svgNs = "http://www.w3.org/2000/svg";
      resetMemoryGraphCanvas();
      const nodes = Array.isArray(payload && payload.nodes) ? payload.nodes : [];
      const edges = Array.isArray(payload && payload.edges) ? payload.edges : [];
      const width = 900;
      const height = 340;
      memoryGraphCanvas.setAttribute("viewBox", "0 0 " + String(width) + " " + String(height));

      if (nodes.length === 0) {{
        const emptyLabel = document.createElementNS(svgNs, "text");
        emptyLabel.setAttribute("x", "22");
        emptyLabel.setAttribute("y", "30");
        emptyLabel.setAttribute("fill", '#34576d');
        emptyLabel.textContent = "No graph nodes available for current filters.";
        memoryGraphCanvas.appendChild(emptyLabel);
        return;
      }}

      const positions = computeMemoryGraphForceLayout(nodes, edges, width, height);

      for (const edge of edges) {{
        const source = positions.get(edge.source);
        const target = positions.get(edge.target);
        if (!source || !target) {{
          continue;
        }}
        const line = document.createElementNS(svgNs, "line");
        line.setAttribute("x1", String(source.x));
        line.setAttribute("y1", String(source.y));
        line.setAttribute("x2", String(target.x));
        line.setAttribute("y2", String(target.y));
        line.setAttribute("stroke", relationColor(edge.relation_type || ""));
        line.setAttribute("stroke-width", String(Math.max(1.3, Math.min(5.6, 1.0 + toSafeFloat(edge.weight, 0)))));
        line.setAttribute("stroke-opacity", "0.85");
        memoryGraphCanvas.appendChild(line);
      }}

      for (const node of nodes) {{
        const placed = positions.get(node.id);
        if (!placed) {{
          continue;
        }}
        const group = document.createElementNS(svgNs, "g");
        const circle = document.createElementNS(svgNs, "circle");
        circle.setAttribute("cx", String(placed.x));
        circle.setAttribute("cy", String(placed.y));
        const importanceSignal = Math.max(toSafeFloat(node.weight, 0), toSafeFloat(node.size, 0) / 4);
        circle.setAttribute("r", String(Math.max(8, Math.min(26, 8 + (importanceSignal * 2.4)))));
        circle.setAttribute("fill", '#d7e9f5');
        circle.setAttribute("stroke", '#1f5574');
        circle.setAttribute("stroke-width", "1.5");
        group.appendChild(circle);

        const label = document.createElementNS(svgNs, "text");
        label.setAttribute("x", String(placed.x + 10));
        label.setAttribute("y", String(placed.y - 10));
        label.setAttribute("class", "memory-graph-node-label");
        label.textContent = String(node.label || "").slice(0, 36);
        group.appendChild(label);
        memoryGraphCanvas.appendChild(group);
      }}
    }}

    async function loadMemoryGraph() {{
      const endpoint = encodeSessionPath(MEMORY_GRAPH_ENDPOINT_TEMPLATE);
      const maxNodes = clampGraphMaxNodes(graphMaxNodesInput.value);
      const minEdgeWeight = clampGraphMinEdgeWeight(graphMinEdgeWeightInput.value);
      const relationTypes = graphRelationTypesInput.value.trim();
      const query = new URLSearchParams();
      query.set("max_nodes", String(maxNodes));
      query.set("min_edge_weight", String(minEdgeWeight));
      if (relationTypes.length > 0) {{
        query.set("relation_types", relationTypes);
      }}
      memoryGraphStatusPre.textContent = "Loading memory graph...";
      try {{
        const response = await fetch(endpoint + "?" + query.toString(), {{ headers: authHeaders() }});
        const payload = await response.json();
        memoryGraphStatusPre.textContent = JSON.stringify(payload, null, 2);
        if (!response.ok) {{
          renderMemoryGraph({{ nodes: [], edges: [] }});
          await emitUiTelemetry("memory", "graph", "memory_graph_failed", {{ status: response.status }});
          return;
        }}
        renderMemoryGraph(payload);
        await emitUiTelemetry("memory", "graph", "memory_graph_loaded", {{
          node_count: payload.node_count || 0,
          edge_count: payload.edge_count || 0
        }});
      }} catch (error) {{
        renderMemoryGraph({{ nodes: [], edges: [] }});
        memoryGraphStatusPre.textContent = "Failed to load memory graph: " + String(error);
      }}
    }}

    function activateView(viewKey) {{
      const tabs = Array.from(document.querySelectorAll(".tab"));
      const views = Array.from(document.querySelectorAll(".view"));
      tabs.forEach((tab) => {{
        const selected = tab.getAttribute("data-view") === viewKey;
        tab.classList.toggle("active", selected);
        tab.setAttribute("aria-selected", selected ? "true" : "false");
      }});
      views.forEach((view) => {{
        const selected = view.id === "view-" + viewKey;
        view.classList.toggle("active", selected);
        view.setAttribute("aria-hidden", selected ? "false" : "true");
      }});
      emitUiTelemetry("navigation", "switch_view", "view_activated", {{ view: viewKey }});
    }}

    document.querySelectorAll(".tab").forEach((tab) => {{
      tab.addEventListener("click", () => activateView(tab.getAttribute("data-view")));
    }});

    sendButton.addEventListener("click", sendPrompt);
    clearButton.addEventListener("click", () => setOutput("No response yet."));
    refreshButton.addEventListener("click", refreshStatus);
    dashboardRefreshButton.addEventListener("click", () => refreshDashboard(true));
    dashboardPauseButton.addEventListener("click", () => postDashboardAction("pause"));
    dashboardResumeButton.addEventListener("click", () => postDashboardAction("resume"));
    dashboardControlRefreshButton.addEventListener("click", () => postDashboardAction("refresh"));
    loadSessionsButton.addEventListener("click", loadSessions);
    loadSessionDetailButton.addEventListener("click", loadSessionDetail);
    appendSessionButton.addEventListener("click", appendSessionMessage);
    resetSessionButton.addEventListener("click", resetSession);
    loadMemoryButton.addEventListener("click", loadMemory);
    saveMemoryButton.addEventListener("click", saveMemory);
    loadMemoryGraphButton.addEventListener("click", loadMemoryGraph);

    tokenInput.addEventListener("change", saveLocalValues);
    dashboardLiveInput.addEventListener("change", updateDashboardLiveMode);
    dashboardPollSecondsInput.addEventListener("change", updateDashboardLiveMode);
    sessionInput.addEventListener("change", () => {{
      saveLocalValues();
      loadSessionDetail();
      loadMemory();
      loadMemoryGraph();
    }});

    loadLocalValues();
    sessionPolicyGateInput.value = SESSION_WRITE_POLICY_GATE;
    memoryPolicyGateInput.value = MEMORY_WRITE_POLICY_GATE;
    configViewPre.textContent = formatConfigView({{ gateway: {{}} }});

    // renderStatusDashboard(payload) is intentionally invoked after status fetch.
    refreshStatus();
    refreshDashboard();
    updateDashboardLiveMode();
    loadSessions();
    loadSessionDetail();
    loadMemory();
    loadMemoryGraph();
  </script>
</body>
</html>
"#,
        responses_endpoint = OPENRESPONSES_ENDPOINT,
        chat_completions_endpoint = OPENAI_CHAT_COMPLETIONS_ENDPOINT,
        completions_endpoint = OPENAI_COMPLETIONS_ENDPOINT,
        models_endpoint = OPENAI_MODELS_ENDPOINT,
        status_endpoint = GATEWAY_STATUS_ENDPOINT,
        dashboard_health_endpoint = DASHBOARD_HEALTH_ENDPOINT,
        dashboard_widgets_endpoint = DASHBOARD_WIDGETS_ENDPOINT,
        dashboard_queue_timeline_endpoint = DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
        dashboard_alerts_endpoint = DASHBOARD_ALERTS_ENDPOINT,
        dashboard_actions_endpoint = DASHBOARD_ACTIONS_ENDPOINT,
        dashboard_stream_endpoint = DASHBOARD_STREAM_ENDPOINT,
        websocket_endpoint = GATEWAY_WS_ENDPOINT,
        sessions_endpoint = GATEWAY_SESSIONS_ENDPOINT,
        memory_endpoint_template = GATEWAY_MEMORY_ENDPOINT,
        memory_graph_endpoint_template = GATEWAY_MEMORY_GRAPH_ENDPOINT,
        session_detail_endpoint_template = GATEWAY_SESSION_DETAIL_ENDPOINT,
        session_append_endpoint_template = GATEWAY_SESSION_APPEND_ENDPOINT,
        session_reset_endpoint_template = GATEWAY_SESSION_RESET_ENDPOINT,
        ui_telemetry_endpoint = GATEWAY_UI_TELEMETRY_ENDPOINT,
        session_policy_gate = SESSION_WRITE_POLICY_GATE,
        memory_policy_gate = MEMORY_WRITE_POLICY_GATE,
        default_session_key = DEFAULT_SESSION_KEY,
    )
}
