# M22 Compatibility Alias Validation

- Generated at: `2026-02-15T17:09:53Z`
- Repo root: `.`

## Summary

- Total checks: `4`
- Passed: `4`
- Failed: `0`

## Command Results

| Name | Status | Command |
| --- | --- | --- |
| legacy_train_alias | pass | `cargo test -p tau-coding-agent legacy_train_aliases_with_warning_snapshot` |
| legacy_proxy_alias | pass | `cargo test -p tau-coding-agent legacy_training_aliases_with_warning_snapshot` |
| unknown_flag_fail_closed | pass | `cargo test -p tau-coding-agent prompt_optimization_alias_normalization_keeps_unknown_flags_fail_closed` |
| docs_policy_discoverability | pass | `python3 -m unittest discover -s .github/scripts -p 'test_docs_link_check.py'` |

## Migration Policy

Use canonical `--prompt-optimization-*` flags for all new automation.
Legacy `--train-*` and `--training-proxy-*` aliases remain temporary compatibility paths and emit deprecation warnings.
