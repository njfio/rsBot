#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${REPO_ROOT}/scripts/dev/rustdoc-marker-ratchet-check.sh"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

cat > "${tmp_dir}/baseline.json" <<'JSON'
{
  "schema_version": 1,
  "total_markers": 100,
  "crates": [
    { "crate": "crate-a", "markers": 60, "files_scanned": 2 },
    { "crate": "crate-b", "markers": 40, "files_scanned": 1 }
  ]
}
JSON

cat > "${tmp_dir}/current-pass.json" <<'JSON'
{
  "schema_version": 1,
  "total_markers": 120,
  "crates": [
    { "crate": "crate-a", "markers": 70, "files_scanned": 2 },
    { "crate": "crate-b", "markers": 50, "files_scanned": 1 }
  ]
}
JSON

cat > "${tmp_dir}/current-fail.json" <<'JSON'
{
  "schema_version": 1,
  "total_markers": 95,
  "crates": [
    { "crate": "crate-a", "markers": 55, "files_scanned": 2 },
    { "crate": "crate-b", "markers": 40, "files_scanned": 1 }
  ]
}
JSON

cat > "${tmp_dir}/policy.json" <<JSON
{
  "schema_version": 1,
  "floor_markers": 110,
  "baseline_artifact": "${tmp_dir}/baseline.json",
  "fail_on_regression": true
}
JSON

pass_json="${tmp_dir}/pass-report.json"
pass_md="${tmp_dir}/pass-report.md"

"${SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --policy-file "${tmp_dir}/policy.json" \
  --current-json "${tmp_dir}/current-pass.json" \
  --generated-at "2026-02-15T00:00:00Z" \
  --output-json "${pass_json}" \
  --output-md "${pass_md}" \
  --quiet

python3 - "${pass_json}" "${pass_md}" <<'PY'
import json
import pathlib
import sys

json_path = pathlib.Path(sys.argv[1])
md_path = pathlib.Path(sys.argv[2])
payload = json.loads(json_path.read_text(encoding="utf-8"))

assert payload["schema_version"] == 1
assert payload["floor_markers"] == 110
assert payload["current_total_markers"] == 120
assert payload["meets_floor"] is True
assert payload["below_floor_by"] == 0

md = md_path.read_text(encoding="utf-8")
assert "Ratchet status: `PASS`" in md
assert "- None" in md
PY

fail_json="${tmp_dir}/fail-report.json"
fail_md="${tmp_dir}/fail-report.md"
set +e
"${SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --policy-file "${tmp_dir}/policy.json" \
  --current-json "${tmp_dir}/current-fail.json" \
  --generated-at "2026-02-15T00:00:00Z" \
  --output-json "${fail_json}" \
  --output-md "${fail_md}" \
  --quiet
status=$?
set -e

if [[ "${status}" -eq 0 ]]; then
  echo "expected failing ratchet run to exit non-zero" >&2
  exit 1
fi

python3 - "${fail_json}" "${fail_md}" <<'PY'
import json
import pathlib
import sys

json_path = pathlib.Path(sys.argv[1])
md_path = pathlib.Path(sys.argv[2])
payload = json.loads(json_path.read_text(encoding="utf-8"))

assert payload["current_total_markers"] == 95
assert payload["meets_floor"] is False
assert payload["below_floor_by"] == 15
assert payload["negative_crate_deltas"], "expected at least one negative crate delta"

md = md_path.read_text(encoding="utf-8")
assert "Ratchet status: `FAIL`" in md
assert "| crate-a | 60 | 55 | -5 |" in md
PY

echo "ok - rustdoc marker ratchet check contract"
