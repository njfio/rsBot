# Tasks #2258

Status: Completed
Spec: specs/2258/spec.md
Plan: specs/2258/plan.md

- T1 (tests first): add failing conformance tests C-01..C-07 for provider parsing, auth parsing/matrix, OpenRouter routing, and headers.
- T2: add `Provider::OpenRouter` and update parser/error/help text paths.
- T3: wire OpenRouter auth parsing/capability/key candidate paths in `tau-provider`.
- T4: implement OpenRouter client routing and header behavior in provider client + OpenAI transport.
- T5: run scoped fmt/clippy/tests; verify AC-to-test mapping and update spec status to Implemented.
