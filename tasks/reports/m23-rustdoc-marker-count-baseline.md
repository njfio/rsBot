# M23 Rustdoc Marker Count

Generated at: 2026-02-15T17:37:34Z

## Summary

- Scan root: `crates`
- Total markers: `1486`
- Crates scanned: `42`

## Per-Crate Breakdown

| Crate | Markers | Files Scanned |
| --- | ---: | ---: |
| kamn-core | 4 | 1 |
| kamn-sdk | 3 | 1 |
| tau-access | 21 | 7 |
| tau-agent-core | 283 | 6 |
| tau-ai | 26 | 7 |
| tau-algorithm | 17 | 3 |
| tau-browser-automation | 20 | 3 |
| tau-cli | 36 | 9 |
| tau-coding-agent | 21 | 57 |
| tau-contract | 40 | 1 |
| tau-core | 10 | 3 |
| tau-custom-command | 17 | 4 |
| tau-dashboard | 22 | 3 |
| tau-deployment | 22 | 6 |
| tau-diagnostics | 22 | 1 |
| tau-events | 27 | 3 |
| tau-extensions | 20 | 2 |
| tau-gateway | 37 | 16 |
| tau-github-issues | 29 | 23 |
| tau-github-issues-runtime | 39 | 17 |
| tau-memory | 48 | 3 |
| tau-multi-channel | 105 | 15 |
| tau-onboarding | 66 | 23 |
| tau-ops | 20 | 13 |
| tau-orchestrator | 23 | 5 |
| tau-provider | 39 | 14 |
| tau-release-channel | 15 | 4 |
| tau-runtime | 111 | 14 |
| tau-safety | 13 | 1 |
| tau-session | 41 | 11 |
| tau-skills | 43 | 4 |
| tau-slack-runtime | 15 | 7 |
| tau-startup | 22 | 8 |
| tau-tools | 45 | 10 |
| tau-trainer | 8 | 1 |
| tau-training-proxy | 5 | 1 |
| tau-training-runner | 10 | 1 |
| tau-training-store | 9 | 2 |
| tau-training-tracer | 9 | 1 |
| tau-training-types | 77 | 1 |
| tau-tui | 17 | 2 |
| tau-voice | 29 | 4 |

## Reproduction Command

```bash
scripts/dev/rustdoc-marker-count.sh \
  --repo-root . \
  --scan-root crates \
  --output-json tasks/reports/m23-rustdoc-marker-count.json \
  --output-md tasks/reports/m23-rustdoc-marker-count.md
```
