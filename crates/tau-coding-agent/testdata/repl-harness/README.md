# REPL Harness Fixtures

Schema version: `1`

Each fixture defines:

- `args`: CLI args passed to `tau-coding-agent`
- `stdin_script`: scripted REPL input written to stdin
- `timeout_ms`: per-fixture timeout floor (overridden upward by `TAU_REPL_HARNESS_TIMEOUT_MS`)
- `expect`: stdout/stderr fragments and prompt-count assertions

Supported template variables use `{{NAME}}` syntax, for example `{{API_BASE}}`.
