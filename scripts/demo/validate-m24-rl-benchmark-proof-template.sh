#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: validate-m24-rl-benchmark-proof-template.sh <proof-json>" >&2
  exit 2
fi

proof_json="$1"

if [[ ! -f "${proof_json}" ]]; then
  echo "error: proof JSON not found: ${proof_json}" >&2
  exit 2
fi

python3 - "${proof_json}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text(encoding="utf-8"))

def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)

def require_object(root: dict, key: str) -> dict:
    value = root.get(key)
    if not isinstance(value, dict):
        fail(f"missing object field '{key}'")
    return value

def require_number(root: dict, key: str) -> float:
    value = root.get(key)
    if not isinstance(value, (int, float)):
        fail(f"missing numeric field '{key}'")
    return float(value)

def require_string(root: dict, key: str) -> str:
    value = root.get(key)
    if not isinstance(value, str) or not value.strip():
        fail(f"missing string field '{key}'")
    return value

if payload.get("schema_version") != 1:
    fail("schema_version must be 1")

require_string(payload, "run_id")
require_string(payload, "generated_at")

suite = require_object(payload, "benchmark_suite")
require_string(suite, "name")
require_string(suite, "version")

baseline = require_object(payload, "baseline")
require_string(baseline, "checkpoint_id")
if int(require_number(baseline, "episodes")) <= 0:
    fail("baseline.episodes must be > 0")
require_number(baseline, "mean_reward")

trained = require_object(payload, "trained")
require_string(trained, "checkpoint_id")
if int(require_number(trained, "episodes")) <= 0:
    fail("trained.episodes must be > 0")
require_number(trained, "mean_reward")

significance = require_object(payload, "significance")
p_value = require_number(significance, "p_value")
confidence_level = require_number(significance, "confidence_level")
if not isinstance(significance.get("pass"), bool):
    fail("significance.pass must be boolean")
if not (0.0 <= p_value <= 1.0):
    fail("significance.p_value must be between 0 and 1")
if not (0.0 < confidence_level <= 1.0):
    fail("significance.confidence_level must be between 0 and 1")

criteria = require_object(payload, "criteria")
min_reward_delta = require_number(criteria, "min_reward_delta")
max_safety_regression = require_number(criteria, "max_safety_regression")
max_p_value = require_number(criteria, "max_p_value")
if min_reward_delta < 0.0:
    fail("criteria.min_reward_delta must be >= 0")
if max_safety_regression < 0.0:
    fail("criteria.max_safety_regression must be >= 0")
if not (0.0 <= max_p_value <= 1.0):
    fail("criteria.max_p_value must be between 0 and 1")

artifacts = require_object(payload, "artifacts")
require_string(artifacts, "baseline_report")
require_string(artifacts, "trained_report")
require_string(artifacts, "significance_report")

print("ok - m24 benchmark proof artifact valid")
PY
