# Issue 1723 Plan

Status: Reviewed

## Approach

1. Implement `.github/scripts/doc_density_annotations.py` that:
   - reads `ci-artifacts/rust-doc-density.json`
   - discovers changed files (from explicit file list or git diff)
   - scans changed Rust files for undocumented public items
   - emits GitHub warning annotations with file/line hints
2. Add unit tests for failed crate parsing, changed-file mapping, and annotation rendering.
3. Wire script into CI with `if: always()` after rust density check.

## Affected Areas

- `.github/scripts/doc_density_annotations.py`
- `.github/scripts/test_doc_density_annotations.py`
- `.github/workflows/ci.yml`

## Risks And Mitigations

- Risk: noisy annotations.
  - Mitigation: limit to changed files and cap emitted hints.
- Risk: diff detection failures on shallow checkouts.
  - Mitigation: support explicit changed-file list input and robust base-ref fallback.

## ADR

No architecture/dependency/protocol change. ADR not required.
