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
