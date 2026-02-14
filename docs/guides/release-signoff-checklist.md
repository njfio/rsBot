# Release Sign-Off Checklist

Run commands from repository root.

Use this checklist for every release touching live surfaces:

- voice
- browser automation
- dashboard
- custom command
- memory

Related runbooks:

- [Release Channel Ops](release-channel-ops.md)
- [Unified Live-Run Harness Guide](live-run-unified-ops.md)
- [Voice Operations Runbook](voice-ops.md)
- [Browser Automation Live Ops Guide](browser-automation-live-ops.md)
- [Dashboard Operations Runbook](dashboard-ops.md)
- [Custom Command Operations Runbook](custom-command-ops.md)
- [Memory Operations Runbook](memory-ops.md)

## Mandatory Evidence Contract

Every checklist item must include at least one concrete evidence link:

- CI URL (workflow run, job, artifact URL), or
- repository artifact path (for example `.tau/live-run-unified/report.json`)

`TBD`, blank links, or "see logs" are not acceptable for sign-off.

## Checklist Template

Copy and fill this template in your release issue or PR comment:

```markdown
## Release Sign-Off: <release-id>

- Release date (UTC): <YYYY-MM-DD>
- Release owner: <name>
- Reviewer(s): <name(s)>
- Target channel: <stable|beta|dev>

### 1. Preflight (0%)
- [ ] Unified harness passed
  - Evidence: <link/path to .tau/live-run-unified/report.json>
- [ ] CI live-smoke matrix passed (voice/browser/dashboard/custom-command/memory)
  - Evidence: <workflow URL>
- [ ] Rollback target captured from `/release-channel show`
  - Evidence: <link/path/screenshot/log snippet>

### 2. Canary Phases
- [ ] 5% canary complete (30m) with healthy thresholds
  - Evidence: <status snapshots + logs>
- [ ] 25% canary complete (60m) with healthy thresholds
  - Evidence: <status snapshots + logs>
- [ ] 50% canary complete (120m) with healthy thresholds
  - Evidence: <status snapshots + logs>

### 3. Rollback Readiness
- [ ] Rollback trigger matrix reviewed
  - Evidence: <link to runbook section + ack>
- [ ] Rollback command rehearsal executed
  - Evidence: <command transcript path or CI URL>

### 4. Surface Verification
- [ ] Voice health/status checks
  - Evidence: <link/path>
- [ ] Browser live summary/timeline checks
  - Evidence: <link/path>
- [ ] Dashboard health/status checks
  - Evidence: <link/path>
- [ ] Custom command health/status checks
  - Evidence: <link/path>
- [ ] Memory health checks
  - Evidence: <link/path>

### 5. General Availability (100%)
- [ ] 24h post-promotion monitor completed
  - Evidence: <link/path>
- [ ] No unresolved rollback triggers
  - Evidence: <link/path>
- [ ] Final approver sign-off
  - Evidence: <review URL or comment permalink>
```

## Rehearsal Procedure (Dry Run)

Run one rehearsal before first production use of a new release train:

1. Execute unified harness:
   `./scripts/demo/live-run-unified.sh --skip-build --timeout-seconds 180`
2. Capture preflight evidence links from:
   - `.tau/live-run-unified/report.json`
   - `.tau/live-run-unified/manifest.json`
3. Collect per-surface status evidence from each runbook's inspect commands.
4. Fill the full checklist template with rehearsal evidence.
5. Attach checklist to the release issue/PR and request reviewer acknowledgment.
