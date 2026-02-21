# M221 - tau-gaps Report Accuracy Resync After M218-M220

Status: In Progress

## Context
`tasks/tau-gaps-issues-improvements.md` still contains stale claims (e.g., missing contributor/security docs and outdated under-tested counts) that no longer match current repository state.

## Scope
- Refresh stale evidence rows in `tasks/tau-gaps-issues-improvements.md`.
- Update `scripts/dev/test-tau-gaps-issues-improvements.sh` to enforce refreshed markers and reject stale claims.
- Keep scope constrained to report + conformance script synchronization.

## Linked Issues
- Epic: #3174
- Story: #3175
- Task: #3176

## Success Signals
- `scripts/dev/test-tau-gaps-issues-improvements.sh`
- report assertions confirm updated closure state markers
