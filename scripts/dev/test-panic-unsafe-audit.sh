#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUDIT_SCRIPT="${SCRIPT_DIR}/panic-unsafe-audit.sh"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

mkdir -p "${tmp_dir}/crates/example/src"
mkdir -p "${tmp_dir}/crates/example/tests"

cat > "${tmp_dir}/crates/example/src/lib.rs" <<'RS'
pub fn f() {
    panic!("boom");
}

#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        panic!("test panic");
    }
}
RS

cat > "${tmp_dir}/crates/example/tests/integration.rs" <<'RS'
#[test]
fn integration() {
    unsafe {
        std::env::set_var("K", "V");
    }
}
RS

output_json="${tmp_dir}/audit.json"
"${AUDIT_SCRIPT}" --repo-root "${tmp_dir}" --output-json "${output_json}" --quiet

panic_total="$(jq -r '.counters.panic_total' "${output_json}")"
panic_review_required="$(jq -r '.counters.panic_review_required' "${output_json}")"
panic_cfg_test_module="$(jq -r '.counters.panic_cfg_test_module' "${output_json}")"
unsafe_total="$(jq -r '.counters.unsafe_total' "${output_json}")"
unsafe_path_test="$(jq -r '.counters.unsafe_path_test' "${output_json}")"

assert_equals "2" "${panic_total}" "panic total"
assert_equals "1" "${panic_review_required}" "panic review required"
assert_equals "1" "${panic_cfg_test_module}" "panic cfg-test bucket"
assert_equals "1" "${unsafe_total}" "unsafe total"
assert_equals "1" "${unsafe_path_test}" "unsafe path-test bucket"

echo "panic-unsafe-audit tests passed"
