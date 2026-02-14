# Prompt Optimization Proxy Operations Guide

This guide covers the optional OpenAI-compatible prompt optimization attribution proxy mode.

## Run Prompt Optimization Proxy

From repository root:

```bash
cargo run -p tau-coding-agent -- \
  --prompt-optimization-proxy-server \
  --prompt-optimization-proxy-bind 127.0.0.1:8788 \
  --prompt-optimization-proxy-upstream-url http://127.0.0.1:4000 \
  --prompt-optimization-proxy-state-dir .tau \
  --prompt-optimization-proxy-timeout-ms 30000
```

Legacy `--training-proxy-*` flags remain accepted as aliases.

## Required Attribution Headers

Proxy requests to `/v1/chat/completions` must include:

- `x-rollout-id`
- `x-attempt-id`

Optional headers:

- `x-sequence-id`
- `x-trace-id`

When required attribution headers are missing or invalid, the proxy returns `400`.

## Health Endpoint

Proxy health endpoint:

- `GET /training/proxy/health`

It returns upstream target and attribution log location.

## Attribution Log

The proxy appends JSONL records to:

- `.tau/training/proxy-attribution.jsonl`

Each record includes rollout/attempt IDs, optional sequence/trace IDs, request and response byte
counts, latency, and upstream status/error outcome.

## Migration Notes

- `--training-proxy-server` -> `--prompt-optimization-proxy-server`
- `--training-proxy-bind` -> `--prompt-optimization-proxy-bind`
- `--training-proxy-upstream-url` -> `--prompt-optimization-proxy-upstream-url`
- `--training-proxy-state-dir` -> `--prompt-optimization-proxy-state-dir`
- `--training-proxy-timeout-ms` -> `--prompt-optimization-proxy-timeout-ms`
