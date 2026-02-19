# Tasks #2598

1. [x] T1 (verify): run #2597 conformance commands and collect pass evidence.
2. [x] T2 (verify): run scoped fmt/clippy/tests and mutation-in-diff for #2597 changes.
3. [x] T3 (verify/live): run sanitized live provider smoke and capture summary.
4. [x] T4 (docs/process): update specs/tasks with evidence and post closure comments.

Evidence:
- `cargo test -p tau-coding-agent 2597_ -- --test-threads=1` => pass (4/4).
- `cargo fmt --all --check` => pass.
- `cargo clippy -p tau-coding-agent -- -D warnings` => pass.
- `cargo mutants --in-place --in-diff /tmp/issue2597-working.diff -p tau-coding-agent --baseline skip --timeout 180 -- --test-threads=1 runtime_profile_policy_bridge::tests::` => `42 tested`, `12 caught`, `30 unviable`, `0 missed`, `0 timeout`.
- `env -u OPENAI_API_KEY -u TAU_API_KEY -u OPENROUTER_API_KEY -u TAU_OPENROUTER_API_KEY -u DEEPSEEK_API_KEY -u TAU_DEEPSEEK_API_KEY -u XAI_API_KEY -u MISTRAL_API_KEY -u GROQ_API_KEY -u ANTHROPIC_API_KEY -u GEMINI_API_KEY -u GOOGLE_API_KEY TAU_PROVIDER_KEYS_FILE=/tmp/provider-keys-empty.env ./scripts/dev/provider-live-smoke.sh` => `ok=0 skipped=8 failed=0`.
