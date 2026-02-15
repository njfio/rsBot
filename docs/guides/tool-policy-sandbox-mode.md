## Tool Policy Sandbox Mode

Date: 2026-02-15  
Story: #1438  
Task: #1439

### Scope

Tool policy now supports explicit sandbox fallback posture for bash execution:

- `best-effort`: allow unsandboxed fallback when no launcher/template is available.
- `required`: fail closed if execution would run unsandboxed.

This mode is independent from sandbox launcher selection (`off`, `auto`, `force`), and can be paired with
an optional Docker launcher fallback.

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

Enable Docker fallback launcher with explicit isolation controls:

```bash
--os-sandbox-docker-enabled=true \
--os-sandbox-docker-image debian:stable-slim \
--os-sandbox-docker-network none|bridge|host \
--os-sandbox-docker-memory-mb 256 \
--os-sandbox-docker-cpus 1.0 \
--os-sandbox-docker-pids-limit 256 \
--os-sandbox-docker-read-only-rootfs=true|false \
--os-sandbox-docker-env OPENAI_API_KEY,TAU_TOKEN
```

`--print-tool-policy` output now includes:

- `os_sandbox_policy_mode`
- `os_sandbox_docker_enabled`
- `os_sandbox_docker_image`
- `os_sandbox_docker_network`
- `os_sandbox_docker_memory_mb`
- `os_sandbox_docker_cpu_limit`
- `os_sandbox_docker_pids_limit`
- `os_sandbox_docker_read_only_rootfs`
- `os_sandbox_docker_env_allowlist`

Runtime/orchestrator policy context now includes:

- `os_sandbox_policy_mode=<value>`
- `os_sandbox_docker_enabled=<value>`

### Fail-Closed Error Contract

When sandbox execution is denied, bash tool payloads include deterministic diagnostics:

- `policy_rule: "os_sandbox_mode"`
- `reason_code`
  - `sandbox_policy_required` when `required` mode would fall back unsandboxed
  - `sandbox_launcher_unavailable` when `force` mode cannot resolve launcher
  - `sandbox_docker_unavailable` when Docker fallback is enabled but Docker CLI is unavailable
- `sandbox_mode`
- `sandbox_policy_mode`
- `sandbox_launcher_bwrap_available`
- `sandbox_launcher_docker_enabled`
- `sandbox_launcher_docker_available`
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

4. On non-Linux hosts (or Linux hosts without `bwrap`), enable Docker fallback:

```bash
cargo run -p tau-coding-agent -- \
  --print-tool-policy \
  --os-sandbox-mode auto \
  --os-sandbox-policy-mode required \
  --os-sandbox-docker-enabled=true \
  --os-sandbox-docker-image debian:stable-slim \
  --os-sandbox-docker-network none
```
