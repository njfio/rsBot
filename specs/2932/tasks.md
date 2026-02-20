# Tasks: Issue #2932 - Close operator documentation and deployment-readiness gaps

1. [x] T1 (RED): add failing coverage in `scripts/dev/test-operator-readiness-live-check.sh` for hold/degraded fail-closed behavior and required field checks.
2. [x] T2 (GREEN): implement `scripts/dev/operator-readiness-live-check.sh` with gateway/cortex/control summary checks.
3. [x] T3 (GREEN): add canonical runbook `docs/guides/ops-readiness-live-validation.md` and cross-link existing runbooks/docs index.
4. [x] T4 (REGRESSION): update runbook ownership map for new canonical readiness runbook ownership.
5. [x] T5 (VERIFY): run script tests, runbook ownership docs check, and live readiness validation command evidence.
