# Project Index Guide

Tau provides a local, deterministic project index workflow for fast path/symbol/token lookup.

## Commands

Build or refresh index artifacts:

```bash
cargo run -p tau-coding-agent -- --project-index-build
```

Build from a specific root/state path:

```bash
cargo run -p tau-coding-agent -- \
  --project-index-build \
  --project-index-root /path/to/workspace \
  --project-index-state-dir /path/to/workspace/.tau/index
```

Query the index:

```bash
cargo run -p tau-coding-agent -- \
  --project-index-query "gateway status" \
  --project-index-limit 20
```

Inspect metadata and inventory counters:

```bash
cargo run -p tau-coding-agent -- --project-index-inspect
```

Use JSON output for automation:

```bash
cargo run -p tau-coding-agent -- \
  --project-index-query "route table" \
  --project-index-json
```

## Behavior

- Index state is stored at `--project-index-state-dir/project-index.json`.
- Builds are deterministic and reuse unchanged file entries by content hash.
- Query ranking favors path and symbol hits, then token matches.
- On corrupt index state, query/inspect fail closed with guidance to rebuild.

## Demo Path

```bash
cargo run -p tau-coding-agent -- --project-index-build
cargo run -p tau-coding-agent -- --project-index-query "main loop"
cargo run -p tau-coding-agent -- --project-index-inspect --project-index-json
```
