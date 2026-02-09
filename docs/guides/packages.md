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
