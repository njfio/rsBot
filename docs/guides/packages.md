# Packages Guide

Run all commands from repository root.

## Extension manifests

Validate an extension manifest:

```bash
cargo run -p tau-coding-agent -- \
  --extension-validate ./examples/extensions/issue-assistant/extension.json
```

Show extension metadata:

```bash
cargo run -p tau-coding-agent -- \
  --extension-show ./examples/extensions/issue-assistant/extension.json
```

List discovered extensions:

```bash
cargo run -p tau-coding-agent -- \
  --extension-list \
  --extension-list-root ./examples/extensions
```

Execute one extension hook:

```bash
cargo run -p tau-coding-agent -- \
  --extension-exec-manifest ./examples/extensions/issue-assistant/extension.json \
  --extension-exec-hook run-start \
  --extension-exec-payload-file ./examples/extensions/issue-assistant/payload.json
```

### WASM extension runtime

Extension manifests support `runtime: "wasm"` in addition to `runtime: "process"`.
WASM modules are executed in a sandbox with:
- Fuel metering
- Memory ceiling
- Timeout budget
- Max response size
- Deny-by-default filesystem/network/env capabilities

WASM entrypoints must export:
- `memory`
- `tau_extension_alloc(i32) -> i32`
- `tau_extension_invoke(i32, i32) -> i64` (packed pointer/length response)

Optional manifest-level WASM controls:

```json
{
  "runtime": "wasm",
  "entrypoint": "hook.wasm",
  "timeout_ms": 5000,
  "wasm": {
    "fuel_limit": 2000000,
    "memory_limit_bytes": 33554432,
    "max_response_bytes": 256000,
    "filesystem_mode": "deny",
    "network_mode": "deny",
    "env_allowlist": []
  }
}
```

## Package lifecycle

Validate package manifest:

```bash
cargo run -p tau-coding-agent -- \
  --package-validate ./examples/starter/package.json
```

Install package:

```bash
cargo run -p tau-coding-agent -- \
  --package-install ./examples/starter/package.json \
  --package-install-root .tau/packages
```

Show/list package inventory:

```bash
cargo run -p tau-coding-agent -- --package-show ./examples/starter/package.json
cargo run -p tau-coding-agent -- --package-list --package-list-root .tau/packages
```

Update/remove/rollback package versions:

```bash
cargo run -p tau-coding-agent -- --package-update ./examples/starter/package.json --package-update-root .tau/packages
cargo run -p tau-coding-agent -- --package-remove tau-starter-bundle@1.0.0 --package-remove-root .tau/packages
cargo run -p tau-coding-agent -- --package-rollback tau-starter-bundle@1.0.0 --package-rollback-root .tau/packages
```

Audit conflicts and activate package components:

```bash
cargo run -p tau-coding-agent -- --package-conflicts --package-conflicts-root .tau/packages
cargo run -p tau-coding-agent -- \
  --package-activate \
  --package-activate-root .tau/packages \
  --package-activate-destination .tau/packages-active \
  --package-activate-conflict-policy keep-first
```

## Signed package installs

Require trusted signatures:

```bash
cargo run -p tau-coding-agent -- \
  --package-install ./examples/starter/package.json \
  --package-install-root .tau/packages \
  --require-signed-packages \
  --skill-trust-root publisher=BASE64_PUBLIC_KEY
```
