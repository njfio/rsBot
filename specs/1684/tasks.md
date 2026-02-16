# Issue 1684 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): baseline targeted `tau-provider` auth runtime compile/test signal.

T2: extract shared runtime helper core.

T3: extract per-provider login backend-ready modules (Google/OpenAI/Anthropic).

T4: refactor runtime entrypoint file to delegate to extracted modules.

T5: add split harness and run scoped checks (`cargo test -p tau-provider`, strict clippy, fmt, roadmap sync).

## Tier Mapping

- Unit: existing `tau-provider` unit tests
- Functional: split harness structure assertions
- Integration: auth runtime command behavior in crate tests
- Regression: strict lint/format + crate tests
