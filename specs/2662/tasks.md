# Tasks: Issue #2662

- [ ] T1 (tests, red): add spec-derived failing tests
  - `conformance_spec_c01_parse_discord_envelope_resolves_mention_tokens_to_display_names`
  - `regression_spec_c02_parse_discord_envelope_preserves_unmapped_mentions`
  - `functional_spec_c03_discord_delivery_caps_chunk_size_at_2000_chars`
- [ ] T2 (green): implement Discord mention normalization in live-ingress parser
- [ ] T3 (regression): verify existing Discord ingress/outbound tests remain green
- [ ] T4 (docs): update `tasks/spacebot-comparison.md` G10 checkbox evidence for completed items in this scope
- [ ] T5 (verify): run scoped fmt/clippy/tests and record evidence in PR
