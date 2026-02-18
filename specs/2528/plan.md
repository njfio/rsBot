# Plan #2528

## Approach
1. Complete story/task/subtask artifacts and status transitions.
2. Implement missing adapter file-delivery paths in the scoped crates.
3. Validate with conformance + regression + mutation + live checks.

## Risks
- Adapter API differences (Discord multipart, Slack v2 upload flow).
- Regressions in existing outbound/reaction paths.

## Mitigations
- RED-first tests for each adapter path.
- Keep changes localized and run scoped + full verification gates.
