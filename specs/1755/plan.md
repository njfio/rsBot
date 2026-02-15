# Issue 1755 Plan

Status: Reviewed

## Approach

1. Add allowlist policy JSON under `tasks/policies/` defining:
   - approved RL terms
   - path/content pattern constraints
   - rationale and examples
2. Add scanner script `scripts/dev/rl-terminology-scan.sh` that:
   - scans target file set for RL phrases
   - classifies matches as `approved` or `stale`
   - emits JSON + Markdown report outputs
3. Add tests-first shell contract test for scanner functional/regression behavior.
4. Add documentation guide with examples/non-examples and scanner invocation.

## Affected Areas

- `tasks/policies/rl-terms-allowlist.json` (new)
- `scripts/dev/rl-terminology-scan.sh` (new)
- `scripts/dev/test-rl-terminology-scan.sh` (new)
- `docs/guides/rl-terminology-allowlist.md` (new)
- `docs/README.md` (updated index row)

## Output Contracts

Allowlist policy minimum fields:

- `schema_version`
- `policy_id`
- `approved_terms[]` (`term`, `allowed_paths`, `required_context`, `rationale`)
- `disallowed_defaults[]`

Scanner JSON output minimum fields:

- `schema_version`
- `generated_at`
- `policy_path`
- `approved_matches[]`
- `stale_findings[]`
- `summary`

## Risks And Mitigations

- Risk: allowlist too broad and hides stale wording
  - Mitigation: require path/context constraints per term.
- Risk: scanner false positives from comments/examples
  - Mitigation: include required-context patterns and add fixture regression tests.

## ADR

No dependency or protocol changes. ADR not required.
