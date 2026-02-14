## Tool Policy Sandbox Mode

Date: 2026-02-14  
Story: #1436  
Task: #1437

### Scope

Tool policy now supports explicit sandbox fallback posture for bash execution:

- `best-effort`: allow unsandboxed fallback when no launcher/template is available.
- `required`: fail closed if execution would run unsandboxed.

This mode is independent from sandbox launcher selection (`off`, `auto`, `force`).

### Preset Defaults

| Preset      | `os_sandbox_mode` | `os_sandbox_policy_mode` |
|-------------|-------------------|--------------------------|
| permissive  | off               | best-effort              |
| balanced    | off               | best-effort              |
| strict      | auto              | required                 |
| hardened    | force             | required                 |

### CLI and JSON Surfaces

Set sandbox posture explicitly:

```bash
--os-sandbox-policy-mode best-effort|required
```

Environment override:

```bash
TAU_OS_SANDBOX_POLICY_MODE=best-effort|required
```

`--print-tool-policy` output now includes:

- `os_sandbox_policy_mode`

Runtime/orchestrator policy context now includes:

- `os_sandbox_policy_mode=<value>`

### Fail-Closed Error Contract

When sandbox execution is denied, bash tool payloads include deterministic diagnostics:

- `policy_rule: "os_sandbox_mode"`
- `reason_code`
  - `sandbox_policy_required` when `required` mode would fall back unsandboxed
  - `sandbox_launcher_unavailable` when `force` mode cannot resolve launcher
- `sandbox_mode`
- `sandbox_policy_mode`
- `error`

### Runbook

1. Verify effective posture:

```bash
cargo run -p tau-coding-agent -- --print-tool-policy --tool-policy-preset strict
```

2. Force fail-closed behavior on hosts without an OS sandbox launcher/template:

```bash
cargo run -p tau-coding-agent -- --print-tool-policy --os-sandbox-mode off --os-sandbox-policy-mode required
```

3. If rollout safety requires strict isolation guarantees, use `strict` or `hardened` preset and keep `os_sandbox_policy_mode=required`.
