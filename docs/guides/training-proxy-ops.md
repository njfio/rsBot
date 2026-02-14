# Training Proxy Operations Guide

This guide covers the optional OpenAI-compatible training attribution proxy mode.

## Run Training Proxy

From repository root:

```bash
cargo run -p tau-coding-agent -- \
  --training-proxy-server \
  --training-proxy-bind 127.0.0.1:8788 \
  --training-proxy-upstream-url http://127.0.0.1:4000 \
  --training-proxy-state-dir .tau \
  --training-proxy-timeout-ms 30000
```

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
