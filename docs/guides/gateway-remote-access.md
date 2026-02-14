# Gateway Remote Access Runbook

Run all commands from repository root.

## Scope

This runbook covers safe remote-access posture checks for the OpenResponses gateway,
including wave-9 profiles and deterministic guardrail validation.

Remote profiles:
- `local-only`
- `password-remote`
- `proxy-remote`
- `tailscale-serve`
- `tailscale-funnel`

Remote plan workflows exported by `--gateway-remote-plan`:
- `tailscale-serve`
- `tailscale-funnel`
- `ssh-tunnel-fallback`

## Inspect posture without starting the gateway

Default local-only posture:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-profile-inspect
```

JSON posture report:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-profile-inspect \
  --gateway-remote-profile-json
```

Inspect a tailscale-serve rollout posture:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-profile-inspect \
  --gateway-openresponses-server \
  --gateway-remote-profile tailscale-serve \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token edge-token \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-remote-profile-json
```

Export the remote workflow plan (JSON):

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-plan \
  --gateway-remote-plan-json \
  --gateway-openresponses-server \
  --gateway-remote-profile tailscale-serve \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token edge-token \
  --gateway-openresponses-bind 127.0.0.1:8787
```

## Profile selection matrix

| Profile | Intended exposure | Required auth mode | Required secret | Bind requirement | Common hold reason codes |
| --- | --- | --- | --- | --- | --- |
| `local-only` | workstation/local service | any | none | loopback recommended | `local_only_non_loopback_bind` |
| `password-remote` | controlled remote operators | `password-session` | `--gateway-openresponses-auth-password` | loopback recommended | `password_remote_auth_mode_mismatch`, `password_remote_missing_password` |
| `proxy-remote` | reverse proxy or private tunnel | `token` | `--gateway-openresponses-auth-token` | loopback recommended | `proxy_remote_auth_mode_mismatch`, `proxy_remote_missing_token` |
| `tailscale-serve` | private tailnet access | `token` or `password-session` | token or password | loopback required | `tailscale_serve_non_loopback_bind`, `tailscale_serve_localhost_dev_auth_unsupported` |
| `tailscale-funnel` | public funnel exposure | `password-session` | `--gateway-openresponses-auth-password` | loopback required | `tailscale_funnel_auth_mode_mismatch`, `tailscale_funnel_missing_password`, `tailscale_funnel_non_loopback_bind` |

## Fail-closed troubleshooting

- Symptom: `--gateway-remote-plan` exits non-zero.
  Action: read `reason_codes` in stderr, then run `--gateway-remote-profile-inspect --gateway-remote-profile-json` with the same flags.
- Symptom: selected `tailscale-funnel` profile fails with `tailscale_funnel_missing_password`.
  Action: set non-empty `--gateway-openresponses-auth-password` and keep `--gateway-openresponses-auth-mode password-session`.
- Symptom: selected profile fails with `*_non_loopback_bind`.
  Action: force loopback bind (`127.0.0.1:8787` or `::1:8787`) and expose via tunnel/proxy.
- Symptom: selected `tailscale-serve` profile fails with `tailscale_serve_localhost_dev_auth_unsupported`.
  Action: switch to `token` or `password-session` auth mode and set the corresponding secret.
- Symptom: profile fails with `*_server_disabled`.
  Action: add `--gateway-openresponses-server` before remote profile/plan evaluation.

## Deterministic demo path

```bash
./scripts/demo/gateway-remote-access.sh
```

Artifacts:
- `.tau/demo-gateway-remote-access/trace.log` (deterministic command trace)

## Security recommendations

- Prefer loopback bind with external tunnel/proxy termination over direct public binds.
- Keep bearer/password secrets out of shell history; use environment variables or secret stores.
- Rotate gateway auth credentials on operator changes.
- Keep transport health and gateway status checks in rollout gates:
  - `--transport-health-inspect gateway --transport-health-json`
  - `--gateway-status-inspect --gateway-status-json`

## Rollback steps

1. Stop gateway service mode:
   `--gateway-service-stop --gateway-service-stop-reason remote_access_rollback`
2. Revert to local-only profile and loopback bind in launch configs:
   - `--gateway-remote-profile local-only`
   - `--gateway-openresponses-bind 127.0.0.1:8787`
3. Re-run remote profile inspect and verify `gate=pass`.
4. Rotate affected auth token/password values before re-enabling remote exposure.
