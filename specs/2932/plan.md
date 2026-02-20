# Plan: Issue #2932 - Close operator documentation and deployment-readiness gaps

1. Add a canonical P0 readiness runbook under `docs/guides/`:
   - ordered checklist (startup, health probes, status probes, promotion gate)
   - explicit hold/pass semantics and rollback path
   - references to existing gateway/deployment/cortex commands
2. Add `scripts/dev/operator-readiness-live-check.sh`:
   - authenticated endpoint checks (`/gateway/status`, `/cortex/status`)
   - operator control summary check via `tau-coding-agent --operator-control-summary --operator-control-summary-json`
   - fail-closed checks for required fields and `rollout_gate` posture
3. Add `scripts/dev/test-operator-readiness-live-check.sh` with mocked `curl` and `cargo`:
   - healthy path passes
   - hold/degraded path fails with deterministic error
4. Integrate docs links in:
   - `docs/README.md`
   - `docs/guides/gateway-ops.md`
   - `docs/guides/deployment-ops.md`
   - `docs/guides/operator-control-summary.md`
   - `docs/guides/runbook-ownership-map.md`
5. Run scoped verification:
   - `scripts/dev/test-operator-readiness-live-check.sh`
   - `.github/scripts/runbook_ownership_docs_check.py`
   - optionally run live readiness script against a local gateway instance when available.

## Risks / Mitigations
- Risk: environments without running gateway produce noisy failures.
  - Mitigation: clear error messages, explicit required prerequisites in usage/help.
- Risk: docs drift between runbooks.
  - Mitigation: make canonical runbook authoritative and link from existing runbooks.
- Risk: shell script portability regressions.
  - Mitigation: keep POSIX-friendly patterns and add deterministic test harness.

## Interface / Contract Notes
- No API schema changes; validator consumes existing gateway/cortex/control summary payloads.
- Script output contract is additive and operator-facing (`status=pass` on success).
