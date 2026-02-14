## `expect()/unwrap()` Audit - Wave 1

Date: 2026-02-14  
Issue: #1514

### Scope

- Inventory `expect()/unwrap()` usage by crate/path.
- Classify each hit as `production`, `debug-only`, or `test-only`.
- Remove production panic paths in Cleanup 3 scope crates.

### Inventory Method

Commands used:

```bash
rg -n "\b(expect|unwrap)\(" crates --glob '*.rs'
```

```bash
python3 - <<'PY'
import pathlib,re,collections
root=pathlib.Path('crates')
pat=re.compile(r'\b(expect|unwrap)\s*\(')
results=collections.defaultdict(lambda: {'production':0,'test_only':0})
for path in root.rglob('*.rs'):
    text=path.read_text(encoding='utf-8')
    lines=text.splitlines()
    cfg_test_line=next((i for i,l in enumerate(lines,1) if '#[cfg(test)]' in l),None)
    path_test=('/tests/' in str(path) or str(path).endswith('/tests.rs'))
    for i,l in enumerate(lines,1):
        if not pat.search(l):
            continue
        if path_test or (cfg_test_line is not None and i>cfg_test_line):
            results[str(path)]['test_only'] += 1
        else:
            results[str(path)]['production'] += 1
for p,v in sorted(results.items()):
    if v['production'] or v['test_only']:
        print(p, v)
PY
```

### Classification Summary

- Production runtime panic paths in this wave: `8` (fixed)
  - `crates/tau-training-tracer/src/lib.rs`: `7`
  - `crates/tau-training-store/src/sqlite.rs`: `1`
- Production/doc-example-only (non-runtime) hits: `2`
  - `crates/tau-agent-core/src/lib.rs` Rustdoc examples
- Debug-only panic paths (`#[cfg(debug_assertions)]` scoped): `0`
- Test-only hits in modified files after remediation: `16`
  - `crates/tau-training-store/src/sqlite.rs`: `14`
  - `crates/tau-training-tracer/src/lib.rs`: `2`

### Remediation Completed

1. `crates/tau-training-tracer/src/lib.rs`
   - Replaced mutex lock panics with poison recovery via `lock_inner()`.
   - Removed all production `expect()` usage from tracer runtime paths.
   - Added regression test `regression_poisoned_mutex_does_not_panic_or_drop_spans`.

2. `crates/tau-training-store/src/sqlite.rs`
   - Removed `attempt_id.expect("checked above")` in `query_spans(...)`.
   - Switched to explicit `if let Some(attempt_id)` binding for query params.
   - Preserved behavior while removing panic-only control flow.

### Validation

- `cargo fmt --all --check`
- `cargo test -p tau-training-tracer -p tau-training-store --all-targets`
- `cargo clippy -p tau-training-tracer -p tau-training-store --all-targets -- -D warnings`

### Follow-up

- Keep Rustdoc `expect()` usage in `crates/tau-agent-core/src/lib.rs` under review; these are illustrative examples and not runtime panic paths.
