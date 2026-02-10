# Gateway Remote Access Runbook

Run all commands from repository root.

## Scope

This runbook covers safe remote-access posture checks for the OpenResponses gateway.

Profiles:
- `local-only`
- `password-remote`
- `proxy-remote`

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

Inspect a proxy-remote plan before rollout:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-profile-inspect \
  --gateway-openresponses-server \
  --gateway-remote-profile proxy-remote \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token edge-token \
  --gateway-openresponses-bind 127.0.0.1:8787
```

## Profile guidance

`local-only`:
- intended for workstation/local service usage.
- keep loopback bind (`127.0.0.1` or `::1`).

`password-remote`:
- intended for controlled remote operator access with session-token exchange.
- requires `--gateway-openresponses-auth-mode password-session`.
- requires non-empty `--gateway-openresponses-auth-password`.

`proxy-remote`:
- intended for exposure behind a trusted reverse proxy or tunnel.
- requires `--gateway-openresponses-auth-mode token`.
- requires non-empty `--gateway-openresponses-auth-token`.

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
