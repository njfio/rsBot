#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
BASELINE_SCRIPT="${SCRIPT_DIR}/pr-throughput-baseline.sh"

BASELINE_JSON="${REPO_ROOT}/tasks/reports/pr-throughput-baseline.json"
CURRENT_JSON=""
OUTPUT_MD="${REPO_ROOT}/tasks/reports/pr-throughput-delta.md"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/pr-throughput-delta.json"
REPORTING_INTERVAL="daily"
REPO_SLUG=""
LIMIT=60
SINCE_DAYS=30
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: pr-throughput-delta-report.sh [options]

Generate a current-vs-baseline throughput delta report.

Options:
  --baseline-json <path>       Baseline report JSON path.
  --current-json <path>        Current report JSON path (skip live generation when provided).
  --repo <owner/name>          Repository slug for live current metrics generation.
  --limit <n>                  Max merged PRs for live current metrics generation.
  --since-days <n>             Merge window in days for live current metrics generation.
  --reporting-interval <name>  Reporting interval label (default: daily).
  --output-md <path>           Markdown report output path.
  --output-json <path>         JSON report output path.
  --generated-at <iso>         Override generated timestamp.
  --quiet                      Suppress informational output.
  --help                       Show this help text.
EOF
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "error: required command '${name}' not found" >&2
    exit 1
  fi
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

format_duration() {
  local value="${1:-null}"
  if [[ -z "${value}" || "${value}" == "null" ]]; then
    printf 'n/a'
    return 0
  fi
  awk -v seconds="${value}" '
    BEGIN {
      if (seconds >= 3600 || seconds <= -3600) {
        printf "%.2fh", seconds / 3600.0;
      } else if (seconds >= 60 || seconds <= -60) {
        printf "%.2fm", seconds / 60.0;
      } else {
        printf "%.0fs", seconds;
      }
    }
  '
}

format_percent() {
  local value="${1:-null}"
  if [[ -z "${value}" || "${value}" == "null" ]]; then
    printf 'n/a'
    return 0
  fi
  awk -v pct="${value}" 'BEGIN { printf "%+.2f%%", pct }'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline-json)
      BASELINE_JSON="$2"
      shift 2
      ;;
    --current-json)
      CURRENT_JSON="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --limit)
      LIMIT="$2"
      shift 2
      ;;
    --since-days)
      SINCE_DAYS="$2"
      shift 2
      ;;
    --reporting-interval)
      REPORTING_INTERVAL="$2"
      shift 2
      ;;
    --output-md)
      OUTPUT_MD="$2"
      shift 2
      ;;
    --output-json)
      OUTPUT_JSON="$2"
      shift 2
      ;;
    --generated-at)
      GENERATED_AT="$2"
      shift 2
      ;;
    --quiet)
      QUIET_MODE="true"
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd jq

if ! [[ "${LIMIT}" =~ ^[0-9]+$ ]]; then
  echo "error: --limit must be a non-negative integer" >&2
  exit 1
fi
if ! [[ "${SINCE_DAYS}" =~ ^[0-9]+$ ]]; then
  echo "error: --since-days must be a non-negative integer" >&2
  exit 1
fi

if [[ ! -f "${BASELINE_JSON}" ]]; then
  echo "error: baseline JSON not found: ${BASELINE_JSON}" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
current_json_path="${tmp_dir}/current.json"
current_source="live"

if [[ -n "${CURRENT_JSON}" ]]; then
  if [[ ! -f "${CURRENT_JSON}" ]]; then
    echo "error: current JSON not found: ${CURRENT_JSON}" >&2
    exit 1
  fi
  cp "${CURRENT_JSON}" "${current_json_path}"
  current_source="file"
else
  current_source="live"
  "${BASELINE_SCRIPT}" \
    --quiet \
    ${REPO_SLUG:+--repo "${REPO_SLUG}"} \
    --since-days "${SINCE_DAYS}" \
    --limit "${LIMIT}" \
    --generated-at "${GENERATED_AT}" \
    --output-md "${tmp_dir}/current.md" \
    --output-json "${current_json_path}"
fi

delta_json="$(
  jq -n \
    --arg generated_at "${GENERATED_AT}" \
    --arg interval "${REPORTING_INTERVAL}" \
    --arg baseline_path "${BASELINE_JSON}" \
    --arg current_path "${CURRENT_JSON:-live-generated}" \
    --arg current_source "${current_source}" \
    --slurpfile baseline "${BASELINE_JSON}" \
    --slurpfile current "${current_json_path}" '
      def avg_delta($baseline_avg; $current_avg):
        if ($baseline_avg == null or $current_avg == null) then
          {
            baseline_avg_seconds: $baseline_avg,
            current_avg_seconds: $current_avg,
            delta_seconds: null,
            delta_percent: null,
            status: "unknown"
          }
        else
          ($current_avg - $baseline_avg) as $delta |
          {
            baseline_avg_seconds: $baseline_avg,
            current_avg_seconds: $current_avg,
            delta_seconds: $delta,
            delta_percent: (if $baseline_avg == 0 then null else (($delta / $baseline_avg) * 100) end),
            status: (if $delta < 0 then "improved" elif $delta > 0 then "regressed" else "flat" end)
          }
        end;

      ($baseline[0]) as $b |
      ($current[0]) as $c |
      {
        schema_version: 1,
        generated_at: $generated_at,
        reporting_interval: $interval,
        baseline: {
          path: $baseline_path,
          generated_at: $b.generated_at,
          window: $b.window,
          metrics: $b.metrics
        },
        current: {
          source: $current_source,
          path: $current_path,
          generated_at: $c.generated_at,
          window: $c.window,
          metrics: $c.metrics
        },
        delta: {
          pr_age: avg_delta($b.metrics.pr_age.avg_seconds; $c.metrics.pr_age.avg_seconds),
          review_latency: avg_delta($b.metrics.review_latency.avg_seconds; $c.metrics.review_latency.avg_seconds),
          merge_interval: avg_delta($b.metrics.merge_interval.avg_seconds; $c.metrics.merge_interval.avg_seconds)
        }
      } |
      .summary = (
        (.delta | [.pr_age.status, .review_latency.status, .merge_interval.status]) as $statuses |
        {
          improved_metrics: ($statuses | map(select(. == "improved")) | length),
          regressed_metrics: ($statuses | map(select(. == "regressed")) | length),
          flat_metrics: ($statuses | map(select(. == "flat")) | length),
          unknown_metrics: ($statuses | map(select(. == "unknown")) | length)
        }
      )
    '
)"

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"
printf '%s\n' "${delta_json}" | jq '.' >"${OUTPUT_JSON}"

baseline_generated_at="$(jq -r '.baseline.generated_at // "unknown"' <<<"${delta_json}")"
baseline_count="$(jq -r '.baseline.window.merged_pr_count // 0' <<<"${delta_json}")"
current_generated_at="$(jq -r '.current.generated_at // "unknown"' <<<"${delta_json}")"
current_count="$(jq -r '.current.window.merged_pr_count // 0' <<<"${delta_json}")"

improved_count="$(jq -r '.summary.improved_metrics' <<<"${delta_json}")"
regressed_count="$(jq -r '.summary.regressed_metrics' <<<"${delta_json}")"
flat_count="$(jq -r '.summary.flat_metrics' <<<"${delta_json}")"
unknown_count="$(jq -r '.summary.unknown_metrics' <<<"${delta_json}")"

render_delta_row() {
  local metric_key="$1"
  local label="$2"
  local baseline_avg
  local current_avg
  local delta_seconds
  local delta_percent
  local status
  baseline_avg="$(jq -r ".delta.${metric_key}.baseline_avg_seconds" <<<"${delta_json}")"
  current_avg="$(jq -r ".delta.${metric_key}.current_avg_seconds" <<<"${delta_json}")"
  delta_seconds="$(jq -r ".delta.${metric_key}.delta_seconds" <<<"${delta_json}")"
  delta_percent="$(jq -r ".delta.${metric_key}.delta_percent" <<<"${delta_json}")"
  status="$(jq -r ".delta.${metric_key}.status" <<<"${delta_json}")"

  printf '| %s | %s | %s | %s | %s | %s |\n' \
    "${label}" \
    "$(format_duration "${baseline_avg}")" \
    "$(format_duration "${current_avg}")" \
    "$(format_duration "${delta_seconds}")" \
    "$(format_percent "${delta_percent}")" \
    "${status}"
}

{
  cat <<EOF
# PR Throughput Delta Report

- Generated at: ${GENERATED_AT}
- Reporting interval: ${REPORTING_INTERVAL}
- Baseline generated at: ${baseline_generated_at}
- Current generated at: ${current_generated_at}
- Baseline sample count: ${baseline_count}
- Current sample count: ${current_count}
- Reproduce:
  - \`scripts/dev/pr-throughput-delta-report.sh --baseline-json ${BASELINE_JSON} --reporting-interval ${REPORTING_INTERVAL} --repo ${REPO_SLUG:-<repo>} --since-days ${SINCE_DAYS} --limit ${LIMIT} --output-md ${OUTPUT_MD} --output-json ${OUTPUT_JSON}\`

## Delta Summary

- Improved metrics: ${improved_count}
- Regressed metrics: ${regressed_count}
- Flat metrics: ${flat_count}
- Unknown metrics: ${unknown_count}

## Average Delta (lower is better)

| Metric | Baseline Avg | Current Avg | Delta | Delta % | Status |
| --- | ---: | ---: | ---: | ---: | --- |
EOF
  render_delta_row "pr_age" "PR age (created -> merged)"
  render_delta_row "review_latency" "Review latency (created -> first review)"
  render_delta_row "merge_interval" "Merge interval (between merged PRs)"
  cat <<'EOF'

## Notes Template

- Wins observed:
  - <capture the highest-confidence contributors to improvements>
- Regressions observed:
  - <capture the highest-impact regressions and likely causes>
- Next actions:
  1. <action 1>
  2. <action 2>
  3. <action 3>
EOF
} >"${OUTPUT_MD}"

log_info "wrote throughput delta report:"
log_info "  - ${OUTPUT_MD}"
log_info "  - ${OUTPUT_JSON}"
