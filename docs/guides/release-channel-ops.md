# Release Channel Ops

Run commands from repository root.

## Inspect channel and rollback metadata

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel show
/quit
EOF
```

The output includes persisted rollback metadata fields:
- `rollback_channel`
- `rollback_version`
- `rollback_reason`
- `rollback_reference_unix_ms`

## Switch channel policy

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel set stable
/release-channel set beta
/release-channel set dev
/quit
EOF
```

Channel changes persist rollback hints automatically.

## Plan an update

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel plan
/release-channel plan --target v0.1.5 --dry-run
/quit
EOF
```

Planning writes `.tau/release-update-state.json` with:
- target version
- guard decision and reason code
- action (`upgrade|noop|blocked`)
- dry-run mode and lookup source

## Apply an update plan

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel apply
/release-channel apply --target v0.1.5 --dry-run
/quit
EOF
```

`apply` enforces fail-closed guards:
- malformed version fields
- prerelease target blocked on `stable`
- unsafe major-version jumps

Current implementation records deterministic apply metadata and rollback hints.
Binary replacement remains an operator-managed step.

## Cache maintenance

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel cache show
/release-channel cache refresh
/release-channel cache prune
/release-channel cache clear
/quit
EOF
```
