#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIGNIFICANCE_SCRIPT="${SCRIPT_DIR}/m24-rl-benchmark-significance-report.sh"
VALIDATE_REPORT_SCRIPT="${SCRIPT_DIR}/validate-m24-rl-benchmark-report.sh"
VALIDATE_PROOF_SCRIPT="${SCRIPT_DIR}/validate-m24-rl-benchmark-proof-template.sh"

GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
BASELINE_SAMPLES=""
TRAINED_SAMPLES=""
RUN_ID=""
OUTPUT_DIR="tasks/reports"
SUITE_NAME="m24-rl-suite"
SUITE_VERSION="v1"
MIN_REWARD_DELTA="0.05"
MAX_SAFETY_REGRESSION="0.0"
MAX_P_VALUE="0.05"
MIN_CONFIDENCE_LEVEL="0.95"
BASELINE_SAFETY_PENALTY="0.0"
TRAINED_SAFETY_PENALTY="0.0"

usage() {
  cat <<EOF
Usage: m24-rl-live-benchmark-proof.sh [options]

Generate baseline/trained/significance/proof benchmark artifacts for M24 live-run proof.

Required:
  --baseline-samples <path>    JSON array file with baseline reward samples.
  --trained-samples <path>     JSON array file with trained reward samples.
  --run-id <id>                Run id matching ^m24-[a-z0-9-]+$.

Optional:
  --output-dir <path>                  Output directory for generated artifacts (default: ${OUTPUT_DIR}).
  --generated-at <iso>                 UTC generated-at timestamp.
  --suite-name <name>                  Benchmark suite name (default: ${SUITE_NAME}).
  --suite-version <version>            Benchmark suite version (default: ${SUITE_VERSION}).
  --min-reward-delta <value>           Required minimum reward gain (default: ${MIN_REWARD_DELTA}).
  --max-safety-regression <value>      Maximum allowed safety regression (default: ${MAX_SAFETY_REGRESSION}).
  --max-p-value <value>                Maximum allowed p-value (default: ${MAX_P_VALUE}).
  --min-confidence-level <value>       Minimum required confidence level (default: ${MIN_CONFIDENCE_LEVEL}).
  --baseline-safety-penalty <value>    Baseline mean safety penalty (default: ${BASELINE_SAFETY_PENALTY}).
  --trained-safety-penalty <value>     Trained mean safety penalty (default: ${TRAINED_SAFETY_PENALTY}).
  --help                               Show this usage message.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline-samples)
      BASELINE_SAMPLES="$2"
      shift 2
      ;;
    --trained-samples)
      TRAINED_SAMPLES="$2"
      shift 2
      ;;
    --run-id)
      RUN_ID="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --generated-at)
      GENERATED_AT="$2"
      shift 2
      ;;
    --suite-name)
      SUITE_NAME="$2"
      shift 2
      ;;
    --suite-version)
      SUITE_VERSION="$2"
      shift 2
      ;;
    --min-reward-delta)
      MIN_REWARD_DELTA="$2"
      shift 2
      ;;
    --max-safety-regression)
      MAX_SAFETY_REGRESSION="$2"
      shift 2
      ;;
    --max-p-value)
      MAX_P_VALUE="$2"
      shift 2
      ;;
    --min-confidence-level)
      MIN_CONFIDENCE_LEVEL="$2"
      shift 2
      ;;
    --baseline-safety-penalty)
      BASELINE_SAFETY_PENALTY="$2"
      shift 2
      ;;
    --trained-safety-penalty)
      TRAINED_SAFETY_PENALTY="$2"
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${BASELINE_SAMPLES}" || -z "${TRAINED_SAMPLES}" || -z "${RUN_ID}" ]]; then
  echo "error: --baseline-samples, --trained-samples, and --run-id are required" >&2
  usage >&2
  exit 2
fi
if [[ ! -f "${BASELINE_SAMPLES}" ]]; then
  echo "error: baseline samples file not found: ${BASELINE_SAMPLES}" >&2
  exit 2
fi
if [[ ! -f "${TRAINED_SAMPLES}" ]]; then
  echo "error: trained samples file not found: ${TRAINED_SAMPLES}" >&2
  exit 2
fi

mkdir -p "${OUTPUT_DIR}"
baseline_report="${OUTPUT_DIR}/m24-benchmark-baseline.json"
trained_report="${OUTPUT_DIR}/m24-benchmark-trained.json"
significance_report="${OUTPUT_DIR}/m24-benchmark-significance.json"
proof_report="${OUTPUT_DIR}/m24-benchmark-proof-${RUN_ID}.json"

python3 - "${BASELINE_SAMPLES}" "${TRAINED_SAMPLES}" "${RUN_ID}" "${GENERATED_AT}" "${SUITE_NAME}" "${SUITE_VERSION}" "${BASELINE_SAFETY_PENALTY}" "${TRAINED_SAFETY_PENALTY}" "${baseline_report}" "${trained_report}" <<'PY'
import json
import math
import re
import statistics
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

(
    baseline_samples_path,
    trained_samples_path,
    run_id,
    generated_at,
    suite_name,
    suite_version,
    baseline_safety_raw,
    trained_safety_raw,
    baseline_report_path,
    trained_report_path,
) = sys.argv[1:]

if not re.match(r"^m24-[a-z0-9-]+$", run_id):
    raise SystemExit("run_id must match ^m24-[a-z0-9-]+$")

def parse_samples(path: str, label: str) -> list[float]:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise SystemExit(f"{label} samples payload must be a JSON array")
    if len(payload) < 2:
        raise SystemExit(f"{label} samples must contain at least two values")
    values: list[float] = []
    for index, item in enumerate(payload):
        if not isinstance(item, (int, float)):
            raise SystemExit(f"{label} sample at index {index} must be a finite number")
        value = float(item)
        if not math.isfinite(value):
            raise SystemExit(f"{label} sample at index {index} must be a finite number")
        values.append(value)
    return values

baseline = parse_samples(baseline_samples_path, "baseline")
trained = parse_samples(trained_samples_path, "trained")
if len(baseline) != len(trained):
    raise SystemExit("baseline and trained samples must contain the same number of samples")

try:
    baseline_safety = float(baseline_safety_raw)
    trained_safety = float(trained_safety_raw)
except ValueError as error:
    raise SystemExit(f"safety penalty must be numeric: {error}")
if not math.isfinite(baseline_safety) or not math.isfinite(trained_safety):
    raise SystemExit("safety penalty values must be finite")

baseline_mean = statistics.fmean(baseline)
trained_mean = statistics.fmean(trained)
episodes = len(baseline)

generated_dt = datetime.strptime(generated_at, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
purge_after = (generated_dt + timedelta(days=365)).strftime("%Y-%m-%dT%H:%M:%SZ")
archive_prefix = generated_dt.strftime("%Y/%m")

def benchmark_report(kind: str, mean_reward: float, mean_safety_penalty: float) -> dict:
    return {
        "schema_version": 1,
        "report_kind": kind,
        "run_id": run_id,
        "generated_at": generated_at,
        "benchmark_suite": {
            "name": suite_name,
            "version": suite_version,
        },
        "metrics": {
            "episodes": episodes,
            "mean_reward": mean_reward,
            "mean_safety_penalty": mean_safety_penalty,
        },
        "publication": {
            "report_path": f".tau/reports/m24/{run_id}/m24-benchmark-report-{kind}.json",
            "archive_path": f".tau/reports/archive/m24/{archive_prefix}/m24-benchmark-report-{run_id}-{kind}.json",
        },
        "retention": {
            "policy": "archive-then-purge",
            "retain_days": 365,
            "archive_after_days": 30,
            "purge_after": purge_after,
        },
    }

Path(baseline_report_path).write_text(
    json.dumps(benchmark_report("baseline", baseline_mean, baseline_safety), indent=2) + "\n",
    encoding="utf-8",
)
Path(trained_report_path).write_text(
    json.dumps(benchmark_report("trained", trained_mean, trained_safety), indent=2) + "\n",
    encoding="utf-8",
)
PY

"${SIGNIFICANCE_SCRIPT}" \
  --baseline-samples "${BASELINE_SAMPLES}" \
  --trained-samples "${TRAINED_SAMPLES}" \
  --run-id "${RUN_ID}" \
  --generated-at "${GENERATED_AT}" \
  --suite-name "${SUITE_NAME}" \
  --suite-version "${SUITE_VERSION}" \
  --baseline-safety-penalty "${BASELINE_SAFETY_PENALTY}" \
  --trained-safety-penalty "${TRAINED_SAFETY_PENALTY}" \
  --output-report "${significance_report}"

"${VALIDATE_REPORT_SCRIPT}" "${baseline_report}"
"${VALIDATE_REPORT_SCRIPT}" "${trained_report}"
"${VALIDATE_REPORT_SCRIPT}" "${significance_report}"

python3 - "${baseline_report}" "${trained_report}" "${significance_report}" "${proof_report}" "${RUN_ID}" "${GENERATED_AT}" "${SUITE_NAME}" "${SUITE_VERSION}" "${MIN_REWARD_DELTA}" "${MAX_SAFETY_REGRESSION}" "${MAX_P_VALUE}" "${MIN_CONFIDENCE_LEVEL}" <<'PY'
import json
import math
import sys
from pathlib import Path

(
    baseline_report_path,
    trained_report_path,
    significance_report_path,
    proof_report_path,
    run_id,
    generated_at,
    suite_name,
    suite_version,
    min_reward_delta_raw,
    max_safety_regression_raw,
    max_p_value_raw,
    min_confidence_level_raw,
) = sys.argv[1:]

baseline = json.loads(Path(baseline_report_path).read_text(encoding="utf-8"))
trained = json.loads(Path(trained_report_path).read_text(encoding="utf-8"))
significance_report = json.loads(Path(significance_report_path).read_text(encoding="utf-8"))
significance_metrics = significance_report.get("significance", {})

min_reward_delta = float(min_reward_delta_raw)
max_safety_regression = float(max_safety_regression_raw)
max_p_value = float(max_p_value_raw)
min_confidence_level = float(min_confidence_level_raw)
if any(
    not math.isfinite(value)
    for value in (min_reward_delta, max_safety_regression, max_p_value, min_confidence_level)
):
    raise SystemExit("criteria values must be finite numbers")

baseline_mean_reward = float(baseline["metrics"]["mean_reward"])
trained_mean_reward = float(trained["metrics"]["mean_reward"])
baseline_safety = float(baseline["metrics"]["mean_safety_penalty"])
trained_safety = float(trained["metrics"]["mean_safety_penalty"])
p_value = float(significance_metrics.get("p_value", 1.0))
confidence_level = float(significance_metrics.get("confidence_level", 0.0))

reward_delta = trained_mean_reward - baseline_mean_reward
safety_regression = trained_safety - baseline_safety

reasons = []
if reward_delta < min_reward_delta:
    reasons.append("reward_gain_below_threshold")
if safety_regression > max_safety_regression:
    reasons.append("safety_regression_above_threshold")
if p_value > max_p_value:
    reasons.append("p_value_above_threshold")
if confidence_level < min_confidence_level:
    reasons.append("confidence_below_threshold")

proof_pass = len(reasons) == 0

proof = {
    "schema_version": 1,
    "run_id": run_id,
    "generated_at": generated_at,
    "benchmark_suite": {
        "name": suite_name,
        "version": suite_version,
        "fixture_path": "tasks/fixtures/m24/benchmark-suite.json",
    },
    "baseline": {
        "checkpoint_id": "baseline",
        "episodes": baseline["metrics"]["episodes"],
        "mean_reward": baseline_mean_reward,
        "mean_safety_penalty": baseline_safety,
    },
    "trained": {
        "checkpoint_id": "trained",
        "episodes": trained["metrics"]["episodes"],
        "mean_reward": trained_mean_reward,
        "mean_safety_penalty": trained_safety,
    },
    "significance": {
        "p_value": p_value,
        "confidence_level": confidence_level,
        "pass": proof_pass,
    },
    "criteria": {
        "min_reward_delta": min_reward_delta,
        "max_safety_regression": max_safety_regression,
        "max_p_value": max_p_value,
    },
    "artifacts": {
        "baseline_report": baseline_report_path,
        "trained_report": trained_report_path,
        "significance_report": significance_report_path,
    },
}

if proof_pass:
    failure_analysis = {
        "summary": "benchmark proof criteria satisfied",
        "reasons": [],
        "reward_delta": reward_delta,
        "safety_regression": safety_regression,
    }
else:
    failure_analysis = {
        "summary": "benchmark proof did not meet criteria",
        "reasons": reasons,
        "reward_delta": reward_delta,
        "safety_regression": safety_regression,
    }
proof["failure_analysis"] = failure_analysis

Path(proof_report_path).write_text(json.dumps(proof, indent=2) + "\n", encoding="utf-8")
print(f"proof_status={'pass' if proof_pass else 'fail'} output={proof_report_path}")
if not proof_pass:
    raise SystemExit(1)
PY

"${VALIDATE_PROOF_SCRIPT}" "${proof_report}"
