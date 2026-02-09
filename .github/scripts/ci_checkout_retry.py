#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path


ALLOWED_OUTCOMES = {"success", "failure", "skipped", "cancelled", "unknown"}


@dataclass(frozen=True)
class CheckoutRetryPolicy:
    max_attempts: int
    base_delay_seconds: int
    cap_delay_seconds: int
    max_total_delay_seconds: int


@dataclass(frozen=True)
class CheckoutRetryReport:
    policy: CheckoutRetryPolicy
    outcomes: list[str]
    retry_delays_seconds: list[int]
    planned_total_delay_seconds: int
    actual_total_delay_seconds: int
    success_attempt: int | None
    status: str
    mode: str


def normalize_outcomes(raw: str, max_attempts: int) -> list[str]:
    outcomes = [entry.strip().lower() for entry in raw.split(",") if entry.strip()]
    if len(outcomes) > max_attempts:
        raise ValueError(
            f"checkout outcomes length {len(outcomes)} exceeds --max-attempts {max_attempts}"
        )
    for outcome in outcomes:
        if outcome not in ALLOWED_OUTCOMES:
            raise ValueError(
                f"unsupported checkout outcome '{outcome}'; expected one of: {sorted(ALLOWED_OUTCOMES)}"
            )
    while len(outcomes) < max_attempts:
        outcomes.append("skipped")
    return outcomes


def compute_retry_delays(policy: CheckoutRetryPolicy) -> list[int]:
    delays: list[int] = []
    for retry_index in range(1, policy.max_attempts):
        delay = policy.base_delay_seconds * (2 ** (retry_index - 1))
        delays.append(min(delay, policy.cap_delay_seconds))
    return delays


def build_report(policy: CheckoutRetryPolicy, outcomes: list[str]) -> CheckoutRetryReport:
    retry_delays = compute_retry_delays(policy)
    planned_total_delay = sum(retry_delays)
    success_attempt = next(
        (index + 1 for index, outcome in enumerate(outcomes) if outcome == "success"),
        None,
    )
    if success_attempt is None:
        actual_delay = planned_total_delay
        status = "failure"
        mode = "checkout_retries_exhausted"
    else:
        actual_delay = sum(retry_delays[: max(0, success_attempt - 1)])
        status = "success"
        mode = (
            "first_attempt_success"
            if success_attempt == 1
            else "resolved_after_retry"
        )

    return CheckoutRetryReport(
        policy=policy,
        outcomes=outcomes,
        retry_delays_seconds=retry_delays,
        planned_total_delay_seconds=planned_total_delay,
        actual_total_delay_seconds=actual_delay,
        success_attempt=success_attempt,
        status=status,
        mode=mode,
    )


def render_summary(report: CheckoutRetryReport) -> str:
    success_attempt = (
        str(report.success_attempt) if report.success_attempt is not None else "none"
    )
    return "\n".join(
        [
            "### Checkout Retry",
            f"- Status: {report.status}",
            f"- Mode: {report.mode}",
            f"- Outcomes: {','.join(report.outcomes)}",
            f"- Max attempts: {report.policy.max_attempts}",
            f"- Retry delays (s): {report.retry_delays_seconds}",
            f"- Planned total delay (s): {report.planned_total_delay_seconds}",
            f"- Actual total delay (s): {report.actual_total_delay_seconds}",
            f"- Success attempt: {success_attempt}",
        ]
    )


def write_summary(path: Path | None, summary: str) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(summary)
        handle.write("\n")


def write_output(path: Path | None, report: CheckoutRetryReport) -> None:
    if path is None:
        return
    success_attempt = (
        str(report.success_attempt) if report.success_attempt is not None else ""
    )
    rows = [
        f"checkout_retry_status={report.status}",
        f"checkout_retry_mode={report.mode}",
        f"checkout_retry_success_attempt={success_attempt}",
        f"checkout_retry_outcomes={','.join(report.outcomes)}",
        f"checkout_retry_planned_total_delay_seconds={report.planned_total_delay_seconds}",
        f"checkout_retry_actual_total_delay_seconds={report.actual_total_delay_seconds}",
    ]
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n".join(rows))
        handle.write("\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Deterministic checkout retry diagnostics helper for CI."
    )
    parser.add_argument(
        "--outcomes",
        default="",
        help="Comma-separated attempt outcomes (success|failure|skipped|cancelled|unknown)",
    )
    parser.add_argument(
        "--max-attempts",
        type=int,
        default=3,
        help="Maximum checkout attempts",
    )
    parser.add_argument(
        "--base-delay-seconds",
        type=int,
        default=3,
        help="Base backoff delay in seconds",
    )
    parser.add_argument(
        "--cap-delay-seconds",
        type=int,
        default=6,
        help="Maximum backoff delay in seconds",
    )
    parser.add_argument(
        "--max-total-delay-seconds",
        type=int,
        default=12,
        help="Maximum allowed planned retry delay budget",
    )
    parser.add_argument(
        "--summary",
        default="",
        help="Optional markdown summary output path",
    )
    parser.add_argument(
        "--output",
        default="",
        help="Optional key=value output path (GitHub output format)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit report as JSON to stdout",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Emit policy report without requiring checkout outcomes",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.max_attempts < 1:
        raise ValueError("--max-attempts must be >= 1")
    if args.base_delay_seconds < 0:
        raise ValueError("--base-delay-seconds must be >= 0")
    if args.cap_delay_seconds < 0:
        raise ValueError("--cap-delay-seconds must be >= 0")
    if args.cap_delay_seconds < args.base_delay_seconds:
        raise ValueError("--cap-delay-seconds must be >= --base-delay-seconds")
    if args.max_total_delay_seconds < 0:
        raise ValueError("--max-total-delay-seconds must be >= 0")

    policy = CheckoutRetryPolicy(
        max_attempts=args.max_attempts,
        base_delay_seconds=args.base_delay_seconds,
        cap_delay_seconds=args.cap_delay_seconds,
        max_total_delay_seconds=args.max_total_delay_seconds,
    )
    outcomes = (
        ["skipped"] * policy.max_attempts
        if args.dry_run
        else normalize_outcomes(args.outcomes, policy.max_attempts)
    )
    report = build_report(policy, outcomes)

    if report.planned_total_delay_seconds > policy.max_total_delay_seconds:
        raise ValueError(
            "planned total retry delay exceeds budget: "
            f"{report.planned_total_delay_seconds}s > {policy.max_total_delay_seconds}s"
        )

    summary_text = render_summary(report)
    if args.json:
        print(
            json.dumps(
                {
                    "status": report.status,
                    "mode": report.mode,
                    "outcomes": report.outcomes,
                    "max_attempts": report.policy.max_attempts,
                    "retry_delays_seconds": report.retry_delays_seconds,
                    "planned_total_delay_seconds": report.planned_total_delay_seconds,
                    "actual_total_delay_seconds": report.actual_total_delay_seconds,
                    "success_attempt": report.success_attempt,
                },
                indent=2,
            )
        )
    else:
        print(summary_text)

    summary_path = Path(args.summary).resolve() if args.summary.strip() else None
    output_path = Path(args.output).resolve() if args.output.strip() else None
    write_summary(summary_path, summary_text)
    write_output(output_path, report)

    if args.dry_run:
        return 0
    return 0 if report.status == "success" else 1


if __name__ == "__main__":
    raise SystemExit(main())
