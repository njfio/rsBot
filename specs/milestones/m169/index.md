# M169 - Operator Deployment Guide and Runbook

Status: In Progress

## Objective
Ship an operator-ready deployment guide that documents provider/auth setup, gateway launch,
dashboard access, readiness validation, and rollback procedures.

## Scope
- Author canonical operator deployment instructions for local + production-style gateway startup.
- Document required environment variables and credential-store workflow.
- Document live readiness validation and troubleshooting procedures.

## Issues
- Epic: #2956
- Story: #2957
- Task: #2958

## Exit Criteria
- `docs/guides/operator-deployment-guide.md` exists and is linked from `docs/README.md`.
- Guide commands are validated live in local environment with recorded pass/fail posture.
- Issue #2958 is closed with conformance evidence.
