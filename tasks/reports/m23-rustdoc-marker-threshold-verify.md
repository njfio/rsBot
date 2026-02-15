# M23 Rustdoc Marker Threshold Verification

Generated at: 2026-02-15T18:33:59Z

## Summary

- Threshold markers: `3000`
- Baseline total markers: `1486`
- Current total markers: `1964`
- Delta markers: `+478`
- Remaining to threshold: `1036`
- Gate status: `FAIL`

## Per-Crate Delta Breakdown

| Crate | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| kamn-core | 4 | 4 | +0 |
| kamn-sdk | 3 | 3 | +0 |
| tau-access | 21 | 21 | +0 |
| tau-agent-core | 283 | 283 | +0 |
| tau-ai | 26 | 26 | +0 |
| tau-algorithm | 17 | 17 | +0 |
| tau-browser-automation | 20 | 20 | +0 |
| tau-cli | 36 | 36 | +0 |
| tau-coding-agent | 21 | 166 | +145 |
| tau-contract | 40 | 40 | +0 |
| tau-core | 10 | 10 | +0 |
| tau-custom-command | 17 | 17 | +0 |
| tau-dashboard | 22 | 22 | +0 |
| tau-deployment | 22 | 22 | +0 |
| tau-diagnostics | 22 | 22 | +0 |
| tau-events | 27 | 27 | +0 |
| tau-extensions | 20 | 20 | +0 |
| tau-gateway | 37 | 62 | +25 |
| tau-github-issues | 29 | 29 | +0 |
| tau-github-issues-runtime | 39 | 39 | +0 |
| tau-memory | 48 | 48 | +0 |
| tau-multi-channel | 105 | 160 | +55 |
| tau-onboarding | 66 | 170 | +104 |
| tau-ops | 20 | 54 | +34 |
| tau-orchestrator | 23 | 23 | +0 |
| tau-provider | 39 | 104 | +65 |
| tau-release-channel | 15 | 15 | +0 |
| tau-runtime | 111 | 136 | +25 |
| tau-safety | 13 | 13 | +0 |
| tau-session | 41 | 41 | +0 |
| tau-skills | 43 | 43 | +0 |
| tau-slack-runtime | 15 | 15 | +0 |
| tau-startup | 22 | 22 | +0 |
| tau-tools | 45 | 70 | +25 |
| tau-trainer | 8 | 8 | +0 |
| tau-training-proxy | 5 | 5 | +0 |
| tau-training-runner | 10 | 10 | +0 |
| tau-training-store | 9 | 9 | +0 |
| tau-training-tracer | 9 | 9 | +0 |
| tau-training-types | 77 | 77 | +0 |
| tau-tui | 17 | 17 | +0 |
| tau-voice | 29 | 29 | +0 |

## Reproduction Commands

```bash
scripts/dev/rustdoc-marker-count.sh \
  --repo-root . \
  --scan-root crates \
  --output-json tasks/reports/m23-rustdoc-marker-count.json \
  --output-md tasks/reports/m23-rustdoc-marker-count.md

scripts/dev/rustdoc-marker-threshold-verify.sh \
  --repo-root . \
  --baseline-json tasks/reports/m23-rustdoc-marker-count-baseline.json \
  --current-json tasks/reports/m23-rustdoc-marker-count.json \
  --threshold 3000 \
  --output-json tasks/reports/m23-rustdoc-marker-threshold-verify.json \
  --output-md tasks/reports/m23-rustdoc-marker-threshold-verify.md
```
