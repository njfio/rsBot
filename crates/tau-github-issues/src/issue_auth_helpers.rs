use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `IssueAuthSummaryKind` values.
pub enum IssueAuthSummaryKind {
    Status,
    Matrix,
}

pub fn ensure_auth_json_flag(args: &str) -> String {
    let tokens = args
        .split_whitespace()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    if tokens.contains(&"--json") {
        args.trim().to_string()
    } else if args.trim().is_empty() {
        "--json".to_string()
    } else {
        format!("{} --json", args.trim())
    }
}

pub fn build_issue_auth_summary_line(kind: IssueAuthSummaryKind, raw_json: &str) -> String {
    let Ok(payload) = serde_json::from_str::<Value>(raw_json) else {
        return "summary: unavailable (auth JSON payload was malformed)".to_string();
    };
    let providers = payload
        .get("providers")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let modes = payload.get("modes").and_then(Value::as_u64).unwrap_or(0);
    let rows = payload.get("rows").and_then(Value::as_u64).unwrap_or(0);
    let available = payload
        .get("available")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unavailable = payload
        .get("unavailable")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode_supported = payload
        .get("mode_supported")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode_unsupported = payload
        .get("mode_unsupported")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let provider_filter = payload
        .get("provider_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let mode_filter = payload
        .get("mode_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let mode_support_filter = payload
        .get("mode_support_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let availability_filter = payload
        .get("availability_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let state_filter = payload
        .get("state_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let source_kind_filter = payload
        .get("source_kind_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let revoked_filter = payload
        .get("revoked_filter")
        .and_then(Value::as_str)
        .unwrap_or("all");
    match kind {
        IssueAuthSummaryKind::Status => format!(
            "summary: providers={} rows={} available={} unavailable={} mode_supported={} mode_unsupported={} provider_filter={} mode_filter={} mode_support_filter={} availability_filter={} state_filter={} source_kind_filter={} revoked_filter={}",
            providers,
            rows,
            available,
            unavailable,
            mode_supported,
            mode_unsupported,
            provider_filter,
            mode_filter,
            mode_support_filter,
            availability_filter,
            state_filter,
            source_kind_filter,
            revoked_filter
        ),
        IssueAuthSummaryKind::Matrix => format!(
            "summary: providers={} modes={} rows={} available={} unavailable={} mode_supported={} mode_unsupported={} provider_filter={} mode_filter={} mode_support_filter={} availability_filter={} state_filter={} source_kind_filter={} revoked_filter={}",
            providers,
            modes,
            rows,
            available,
            unavailable,
            mode_supported,
            mode_unsupported,
            provider_filter,
            mode_filter,
            mode_support_filter,
            availability_filter,
            state_filter,
            source_kind_filter,
            revoked_filter
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_issue_auth_summary_line, ensure_auth_json_flag, IssueAuthSummaryKind};

    #[test]
    fn unit_ensure_auth_json_flag_appends_when_missing() {
        assert_eq!(ensure_auth_json_flag("status"), "status --json");
        assert_eq!(ensure_auth_json_flag(""), "--json");
    }

    #[test]
    fn functional_ensure_auth_json_flag_preserves_existing_flag() {
        assert_eq!(
            ensure_auth_json_flag("status --availability online --json"),
            "status --availability online --json"
        );
    }

    #[test]
    fn integration_build_issue_auth_summary_line_status_uses_expected_shape() {
        let summary = build_issue_auth_summary_line(
            IssueAuthSummaryKind::Status,
            r#"{"providers":3,"rows":9,"available":7,"unavailable":2,"mode_supported":6,"mode_unsupported":3,"provider_filter":"all","mode_filter":"api_key","mode_support_filter":"supported","availability_filter":"online","state_filter":"active","source_kind_filter":"stored","revoked_filter":"no"}"#,
        );
        assert_eq!(
            summary,
            "summary: providers=3 rows=9 available=7 unavailable=2 mode_supported=6 mode_unsupported=3 provider_filter=all mode_filter=api_key mode_support_filter=supported availability_filter=online state_filter=active source_kind_filter=stored revoked_filter=no"
        );
    }

    #[test]
    fn regression_build_issue_auth_summary_line_matrix_handles_malformed_payload() {
        assert_eq!(
            build_issue_auth_summary_line(IssueAuthSummaryKind::Matrix, "not-json"),
            "summary: unavailable (auth JSON payload was malformed)"
        );
    }
}
