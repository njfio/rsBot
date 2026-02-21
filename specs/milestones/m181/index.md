# M181 - Tau Gaps Accuracy Correction

## Context
Review #31 introduced stale crate references (`tau-context`, `tau-embedding-engine`) that do not exist at current HEAD. This milestone corrects those references and adds guardrails so stale names fail fast.

## Scope
- Correct invalid crate references in `tasks/tau-gaps-issues-improvements.md`.
- Extend conformance checks to assert stale crate names are absent.

## Linked Issues
- Epic: #3010
- Story: #3011
- Task: #3012
