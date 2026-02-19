//! Safety endpoint handlers and persistence helpers.

use super::*;

pub(super) async fn handle_gateway_safety_policy_get(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let path = gateway_safety_policy_path(&state.config.state_dir);
    let persisted = match read_gateway_safety_policy(&path) {
        Ok(policy) => policy,
        Err(error) => return error.into_response(),
    };
    let (policy, source) = match persisted {
        Some(policy) => (policy, "persisted"),
        None => (SafetyPolicy::default(), "default"),
    };

    state.record_ui_telemetry_event(
        "configuration",
        "safety_policy_get",
        "safety_policy_get_requested",
    );
    (
        StatusCode::OK,
        Json(json!({
            "policy": policy,
            "source": source,
            "path": path.display().to_string(),
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_safety_policy_put(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewaySafetyPolicyUpdateRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };

    let mut policy = request.policy;
    policy.redaction_token = policy.redaction_token.trim().to_string();
    if policy.redaction_token.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_redaction_token",
            "policy.redaction_token must be non-empty",
        )
        .into_response();
    }

    policy.secret_leak_redaction_token = policy.secret_leak_redaction_token.trim().to_string();
    if policy.secret_leak_redaction_token.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_secret_leak_redaction_token",
            "policy.secret_leak_redaction_token must be non-empty",
        )
        .into_response();
    }

    let path = gateway_safety_policy_path(&state.config.state_dir);
    let payload = match serde_json::to_string_pretty(&policy) {
        Ok(payload) => payload,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to encode safety policy payload: {error}"
            ))
            .into_response();
        }
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return OpenResponsesApiError::internal(format!(
                    "failed to create safety policy directory '{}': {error}",
                    parent.display()
                ))
                .into_response();
            }
        }
    }
    if let Err(error) = write_text_atomic(&path, format!("{payload}\n").as_str()) {
        return OpenResponsesApiError::internal(format!(
            "failed to write safety policy '{}': {error}",
            path.display()
        ))
        .into_response();
    }

    let updated_unix_ms = current_unix_timestamp_ms();
    state.record_ui_telemetry_event(
        "configuration",
        "safety_policy_put",
        "safety_policy_put_applied",
    );
    (
        StatusCode::OK,
        Json(json!({
            "updated": true,
            "policy": policy,
            "source": "persisted",
            "path": path.display().to_string(),
            "updated_unix_ms": updated_unix_ms,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_safety_rules_get(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let path = gateway_safety_rules_path(&state.config.state_dir);
    let persisted = match read_gateway_safety_rules(&path) {
        Ok(rules) => rules,
        Err(error) => return error.into_response(),
    };
    let (rules, source) = match persisted {
        Some(rules) => (rules, "persisted"),
        None => (default_safety_rule_set(), "default"),
    };

    state.record_ui_telemetry_event(
        "configuration",
        "safety_rules_get",
        "safety_rules_get_requested",
    );
    (
        StatusCode::OK,
        Json(json!({
            "rules": rules,
            "source": source,
            "path": path.display().to_string(),
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_safety_rules_put(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewaySafetyRulesUpdateRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };

    if let Err(error) = validate_safety_rule_set(&request.rules) {
        return OpenResponsesApiError::bad_request("invalid_safety_rules", error).into_response();
    }

    let path = gateway_safety_rules_path(&state.config.state_dir);
    let payload = match serde_json::to_string_pretty(&request.rules) {
        Ok(payload) => payload,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to encode safety rules payload: {error}"
            ))
            .into_response();
        }
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return OpenResponsesApiError::internal(format!(
                    "failed to create safety rules directory '{}': {error}",
                    parent.display()
                ))
                .into_response();
            }
        }
    }
    if let Err(error) = write_text_atomic(&path, format!("{payload}\n").as_str()) {
        return OpenResponsesApiError::internal(format!(
            "failed to write safety rules '{}': {error}",
            path.display()
        ))
        .into_response();
    }

    let updated_unix_ms = current_unix_timestamp_ms();
    state.record_ui_telemetry_event(
        "configuration",
        "safety_rules_put",
        "safety_rules_put_applied",
    );
    (
        StatusCode::OK,
        Json(json!({
            "updated": true,
            "rules": request.rules,
            "source": "persisted",
            "path": path.display().to_string(),
            "updated_unix_ms": updated_unix_ms,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_safety_test(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewaySafetyTestRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };

    let input = request.input.trim().to_string();
    if input.is_empty() {
        return OpenResponsesApiError::bad_request("invalid_test_input", "input must be non-empty")
            .into_response();
    }
    let include_secret_leaks = request.include_secret_leaks.unwrap_or(true);

    let rules_path = gateway_safety_rules_path(&state.config.state_dir);
    let (rules, rules_source) = match read_gateway_safety_rules(&rules_path) {
        Ok(Some(rules)) => (rules, "persisted"),
        Ok(None) => (default_safety_rule_set(), "default"),
        Err(error) => return error.into_response(),
    };
    let policy_path = gateway_safety_policy_path(&state.config.state_dir);
    let (policy, policy_source) = match read_gateway_safety_policy(&policy_path) {
        Ok(Some(policy)) => (policy, "persisted"),
        Ok(None) => (SafetyPolicy::default(), "default"),
        Err(error) => return error.into_response(),
    };

    let prompt_scan = scan_safety_rules(
        input.as_str(),
        policy.redaction_token.as_str(),
        &rules.prompt_injection_rules,
    );
    let secret_scan = if include_secret_leaks {
        scan_safety_rules(
            input.as_str(),
            policy.secret_leak_redaction_token.as_str(),
            &rules.secret_leak_rules,
        )
    } else {
        scan_safety_rules(
            input.as_str(),
            policy.secret_leak_redaction_token.as_str(),
            &[],
        )
    };

    let mut reason_codes = prompt_scan.reason_codes();
    for code in secret_scan.reason_codes() {
        if !reason_codes.iter().any(|existing| existing == &code) {
            reason_codes.push(code);
        }
    }
    reason_codes.sort();

    let mut matches = prompt_scan
        .matches
        .iter()
        .map(|matched| {
            json!({
                "stage": "prompt_injection",
                "rule_id": matched.rule_id,
                "reason_code": matched.reason_code,
                "start": matched.start,
                "end": matched.end,
            })
        })
        .collect::<Vec<_>>();
    matches.extend(secret_scan.matches.iter().map(|matched| {
        json!({
            "stage": "secret_leak",
            "rule_id": matched.rule_id,
            "reason_code": matched.reason_code,
            "start": matched.start,
            "end": matched.end,
        })
    }));
    matches.sort_by(|left, right| {
        (
            left["start"].as_u64(),
            left["end"].as_u64(),
            left["rule_id"].as_str(),
        )
            .cmp(&(
                right["start"].as_u64(),
                right["end"].as_u64(),
                right["rule_id"].as_str(),
            ))
    });

    let prompt_blocked = policy.enabled
        && policy.apply_to_inbound_messages
        && matches!(policy.mode, SafetyMode::Block)
        && !prompt_scan.matches.is_empty();
    let secret_blocked = include_secret_leaks
        && policy.enabled
        && policy.secret_leak_detection_enabled
        && matches!(policy.secret_leak_mode, SafetyMode::Block)
        && !secret_scan.matches.is_empty();
    let blocked = prompt_blocked || secret_blocked;

    state.record_ui_telemetry_event("safety", "test", "safety_test_evaluated");
    (
        StatusCode::OK,
        Json(json!({
            "blocked": blocked,
            "reason_codes": reason_codes,
            "matches": matches,
            "source": rules_source,
            "rules_path": rules_path.display().to_string(),
            "policy_source": policy_source,
            "policy_path": policy_path.display().to_string(),
            "include_secret_leaks": include_secret_leaks,
            "prompt_scan": {
                "match_count": prompt_scan.matches.len(),
                "redacted_text": prompt_scan.redacted_text,
            },
            "secret_leak_scan": {
                "enabled": include_secret_leaks,
                "match_count": secret_scan.matches.len(),
                "redacted_text": secret_scan.redacted_text,
            },
        })),
    )
        .into_response()
}

pub(super) fn gateway_safety_policy_path(state_dir: &Path) -> PathBuf {
    state_dir.join("openresponses").join("safety-policy.json")
}

pub(super) fn gateway_safety_rules_path(state_dir: &Path) -> PathBuf {
    state_dir.join("openresponses").join("safety-rules.json")
}

pub(super) fn read_gateway_safety_policy(
    path: &Path,
) -> Result<Option<SafetyPolicy>, OpenResponsesApiError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read safety policy '{}': {error}",
            path.display()
        ))
    })?;
    let policy = serde_json::from_str::<SafetyPolicy>(raw.as_str()).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to parse safety policy '{}': {error}",
            path.display()
        ))
    })?;
    Ok(Some(policy))
}

pub(super) fn read_gateway_safety_rules(
    path: &Path,
) -> Result<Option<SafetyRuleSet>, OpenResponsesApiError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read safety rules '{}': {error}",
            path.display()
        ))
    })?;
    let rules = serde_json::from_str::<SafetyRuleSet>(raw.as_str()).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to parse safety rules '{}': {error}",
            path.display()
        ))
    })?;
    Ok(Some(rules))
}
