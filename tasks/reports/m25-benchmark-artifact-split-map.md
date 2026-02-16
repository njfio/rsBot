# Benchmark Artifact Split Map (M25)

- Generated at (UTC): `2026-02-16T09:21:38Z`
- Source file: `crates/tau-trainer/src/benchmark_artifact.rs`
- Target line budget: `3000`
- Current line count: `3868`
- Current gap to target: `868`
- Estimated lines to extract: `950`
- Estimated post-split line count: `2918`

## Extraction Phases

| Phase | Owner | Est. Reduction | Depends On | Modules | Notes |
| --- | --- | ---: | --- | --- | --- |
| phase-1-schema-types (Benchmark schema/types and serde payload structures) | trainer-core | 220 | - | benchmark_artifact/schema.rs, benchmark_artifact/types.rs | Preserve externally consumed artifact JSON schema and field names. |
| phase-2-io-persistence (Artifact IO, path handling, and persistence utilities) | trainer-runtime | 260 | phase-1-schema-types | benchmark_artifact/io.rs, benchmark_artifact/persistence.rs | Keep filesystem contracts and error surface stable for callers. |
| phase-3-report-rendering (Markdown/JSON reporting and presentation helpers) | trainer-observability | 230 | phase-2-io-persistence | benchmark_artifact/reporting.rs, benchmark_artifact/render.rs | Separate formatting logic from core benchmark artifact state. |
| phase-4-validation-compare (Validation, comparison, and regression guard logic) | trainer-quality | 240 | phase-3-report-rendering | benchmark_artifact/validation.rs, benchmark_artifact/compare.rs | Retain benchmark conformance semantics and regression reason-code behavior. |

## Public API Impact

- Retain current benchmark artifact struct names and serialized field contracts.
- Keep public constructor/loader entrypoints stable for trainer and CLI callers.
- Confine extraction changes behind module boundaries without changing call signatures.

## Import Impact

- Introduce benchmark_artifact module tree under crates/tau-trainer/src/benchmark_artifact/.
- Move domain-specific helpers into phased modules while preserving root re-exports.
- Minimize cross-module imports by grouping schema/IO/report/validation concerns.

## Test Migration Plan

| Order | Step | Command | Expected Signal |
| ---: | --- | --- | --- |
| 1 | benchmark-conformance-suite: Run benchmark artifact conformance tests after each extraction phase. | cargo test -p tau-trainer benchmark_artifact | benchmark artifact conformance tests remain green |
| 2 | trainer-integration-suite: Run trainer integration tests that persist/load benchmark artifacts. | cargo test -p tau-trainer | trainer integration flows preserve artifact behavior |
| 3 | workspace-regression-suite: Run workspace-level governance/contract checks for generated artifacts. | python3 -m unittest discover -s .github/scripts -p test_*.py | contract suite remains green after module extraction |
