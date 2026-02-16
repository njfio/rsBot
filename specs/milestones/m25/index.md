# Milestone M25: Governance Alignment and Runtime Decomposition

Status: Draft

## Objective

Establish a new execution wave that keeps post-backlog velocity by:

- aligning repository governance with `AGENTS.md` (issue templates, labels, status flow),
- reconciling roadmap status artifacts with live GitHub state,
- and decomposing remaining oversized runtime modules into maintainable domains.

## Scope

In scope:

- GitHub issue/milestone governance hardening and intake contract compliance
- roadmap status synchronization and drift guardrails
- targeted runtime file decomposition for the highest remaining module concentrations
- build/test execution-latency improvement work packaged as measurable stories/tasks

Out of scope:

- net-new product features unrelated to governance/decomposition/velocity
- protocol or wire-format changes

## Success Signals

- M25 hierarchy exists and is active: 1 epic + stories + tasks + subtasks with contract labels.
- Roadmap documents report live issue closure status without drift.
- Top oversized production modules are reduced below agreed per-file thresholds.
- Build/test turnaround metrics improve with reproducible before/after evidence.

## Issue Hierarchy

Milestone: GitHub milestone `M25 Governance + Decomposition + Velocity`

Epic:

- `#2029` Epic: M25 Governance Alignment + Runtime Decomposition + Velocity

Stories:

- `#2030` Story: M25.1 Governance Contract Compliance
- `#2031` Story: M25.2 Roadmap Source-of-Truth Reconciliation
- `#2032` Story: M25.3 Runtime Decomposition Wave 3
- `#2033` Story: M25.4 Build/Test Velocity Acceleration Wave 2

Tasks:

- `#2034` Task: M25.1.1 Add Contract Issue Templates (Epic/Story/Task/Subtask)
- `#2035` Task: M25.1.2 Enforce Label Namespace + Hierarchy Validation
- `#2036` Task: M25.1.3 Backfill Milestone Spec Containers and Governance Audit
- `#2037` Task: M25.2.1 Sync Roadmap Status Blocks with GitHub Truth
- `#2038` Task: M25.2.2 Add Roadmap Drift Regression Guardrails
- `#2039` Task: M25.2.3 Publish Deterministic Roadmap Status Artifacts
- `#2040` Task: M25.3.1 Split cli_args.rs Below 3000 LOC
- `#2041` Task: M25.3.2 Split benchmark_artifact.rs Below 3000 LOC
- `#2042` Task: M25.3.3 Split tools.rs Below 3000 LOC
- `#2043` Task: M25.3.4 Split github_issues_runtime.rs Below 3000 LOC
- `#2044` Task: M25.3.5 Split channel_store_admin.rs Below 2200 LOC
- `#2045` Task: M25.4.1 Build/Test Latency Baseline and Hotspot Attribution
- `#2046` Task: M25.4.2 Implement Fast-Lane Dev/Test Command Paths
- `#2047` Task: M25.4.3 Optimize CI Cache + Parallel Execution
- `#2048` Task: M25.4.4 Enforce Build/Test Latency Regression Budgets

Subtasks:

- `#2049` through `#2071` (23 subtasks spanning governance, roadmap sync, decomposition, and velocity execution details)
