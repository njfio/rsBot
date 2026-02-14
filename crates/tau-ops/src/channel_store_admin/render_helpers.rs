//! Output rendering helpers for channel-store admin status surfaces.

use std::collections::BTreeMap;

use tau_core::current_unix_timestamp_ms;

use super::{
    CustomCommandStatusInspectReport, DashboardStatusInspectReport, DeploymentStatusInspectReport,
    GatewayStatusInspectReport, GithubStatusInspectReport, MultiAgentStatusInspectReport,
    MultiChannelStatusInspectReport, OperatorControlSummaryDiffReport,
    OperatorControlSummaryReport, TransportHealthInspectRow, VoiceStatusInspectReport,
};

/// Renders the top-level operator control summary block used by CLI status output.
pub(super) fn render_operator_control_summary_report(
    report: &OperatorControlSummaryReport,
) -> String {
    let mut lines = vec![
        format!(
            "operator control summary: generated_unix_ms={} health_state={} rollout_gate={} reason_codes={} recommendations={}",
            report.generated_unix_ms,
            report.health_state,
            report.rollout_gate,
            render_string_vec(&report.reason_codes),
            render_string_vec(&report.recommendations),
        ),
        format!(
            "operator control policy posture: pairing_strict_effective={} pairing_config_strict_mode={} pairing_allowlist_strict={} pairing_rules_configured={} pairing_registry_entries={} pairing_allowlist_channel_rules={} provider_subscription_strict={} gateway_auth_mode={} gateway_remote_profile={} gateway_remote_posture={} gateway_remote_gate={} gateway_remote_risk_level={} gateway_remote_reason_codes={} gateway_remote_recommendations={}",
            report.policy_posture.pairing_strict_effective,
            report.policy_posture.pairing_config_strict_mode,
            report.policy_posture.pairing_allowlist_strict,
            report.policy_posture.pairing_rules_configured,
            report.policy_posture.pairing_registry_entries,
            report.policy_posture.pairing_allowlist_channel_rules,
            report.policy_posture.provider_subscription_strict,
            report.policy_posture.gateway_auth_mode,
            report.policy_posture.gateway_remote_profile,
            report.policy_posture.gateway_remote_posture,
            report.policy_posture.gateway_remote_gate,
            report.policy_posture.gateway_remote_risk_level,
            render_string_vec(&report.policy_posture.gateway_remote_reason_codes),
            render_string_vec(&report.policy_posture.gateway_remote_recommendations),
        ),
        format!(
            "operator control daemon: health_state={} rollout_gate={} reason_code={} recommendation={} profile={} installed={} running={} start_attempts={} stop_attempts={} diagnostics={} state_path={}",
            report.daemon.health_state,
            report.daemon.rollout_gate,
            report.daemon.reason_code,
            report.daemon.recommendation,
            report.daemon.profile,
            report.daemon.installed,
            report.daemon.running,
            report.daemon.start_attempts,
            report.daemon.stop_attempts,
            report.daemon.diagnostics,
            report.daemon.state_path,
        ),
        format!(
            "operator control release channel: health_state={} rollout_gate={} reason_code={} recommendation={} configured={} channel={} path={}",
            report.release_channel.health_state,
            report.release_channel.rollout_gate,
            report.release_channel.reason_code,
            report.release_channel.recommendation,
            report.release_channel.configured,
            report.release_channel.channel,
            report.release_channel.path,
        ),
    ];

    for component in &report.components {
        lines.push(format!(
            "operator control component: component={} health_state={} rollout_gate={} reason_code={} health_reason={} recommendation={} queue_depth={} failure_streak={} state_path={}",
            component.component,
            component.health_state,
            component.rollout_gate,
            component.reason_code,
            component.health_reason,
            component.recommendation,
            component.queue_depth,
            component.failure_streak,
            component.state_path,
        ));
    }

    lines.join("\n")
}

pub(super) fn render_operator_control_summary_diff_report(
    report: &OperatorControlSummaryDiffReport,
) -> String {
    let mut lines = vec![format!(
        "operator control summary diff: generated_unix_ms={} baseline_generated_unix_ms={} current_generated_unix_ms={} drift_state={} risk_level={} health_state_before={} health_state_after={} rollout_gate_before={} rollout_gate_after={} reason_codes_added={} reason_codes_removed={} recommendations_added={} recommendations_removed={} changed_components={} unchanged_components={}",
        report.generated_unix_ms,
        report.baseline_generated_unix_ms,
        report.current_generated_unix_ms,
        report.drift_state,
        report.risk_level,
        report.health_state_before,
        report.health_state_after,
        report.rollout_gate_before,
        report.rollout_gate_after,
        render_string_vec(&report.reason_codes_added),
        render_string_vec(&report.reason_codes_removed),
        render_string_vec(&report.recommendations_added),
        render_string_vec(&report.recommendations_removed),
        report.changed_components.len(),
        report.unchanged_component_count,
    )];

    for component in &report.changed_components {
        lines.push(format!(
            "operator control summary diff component: component={} drift_state={} severity={} health_state_before={} health_state_after={} rollout_gate_before={} rollout_gate_after={} reason_code_before={} reason_code_after={} recommendation_before={} recommendation_after={} queue_depth_before={} queue_depth_after={} failure_streak_before={} failure_streak_after={}",
            component.component,
            component.drift_state,
            component.severity,
            component.health_state_before,
            component.health_state_after,
            component.rollout_gate_before,
            component.rollout_gate_after,
            component.reason_code_before,
            component.reason_code_after,
            component.recommendation_before,
            component.recommendation_after,
            component.queue_depth_before,
            component.queue_depth_after,
            component.failure_streak_before,
            component.failure_streak_after,
        ));
    }

    lines.join("\n")
}

pub(super) fn render_transport_health_rows(rows: &[TransportHealthInspectRow]) -> String {
    rows.iter()
        .map(render_transport_health_row)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn render_transport_health_row(row: &TransportHealthInspectRow) -> String {
    format!(
        "transport health inspect: transport={} target={} state_path={} updated_unix_ms={} cycle_duration_ms={} queue_depth={} active_runs={} failure_streak={} last_cycle_discovered={} last_cycle_processed={} last_cycle_completed={} last_cycle_failed={} last_cycle_duplicates={}",
        row.transport,
        row.target,
        row.state_path,
        row.health.updated_unix_ms,
        row.health.cycle_duration_ms,
        row.health.queue_depth,
        row.health.active_runs,
        row.health.failure_streak,
        row.health.last_cycle_discovered,
        row.health.last_cycle_processed,
        row.health.last_cycle_completed,
        row.health.last_cycle_failed,
        row.health.last_cycle_duplicates,
    )
}

pub(super) fn render_github_status_report(report: &GithubStatusInspectReport) -> String {
    let reason_codes = if report.outbound_last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.outbound_last_reason_codes.join(",")
    };
    let outbound_reasons = if report.outbound_reason_code_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .outbound_reason_code_counts
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let outbound_commands = if report.outbound_command_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .outbound_command_counts
            .iter()
            .map(|(command, count)| format!("{command}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let inbound_kinds = if report.inbound_kind_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .inbound_kind_counts
            .iter()
            .map(|(kind, count)| format!("{kind}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "github status inspect: repo={} state_path={} inbound_log_path={} outbound_log_path={} health_state={} health_reason={} rollout_gate={} processed_event_count={} issue_session_count={} inbound_records={} inbound_invalid_records={} inbound_kind_counts={} outbound_records={} outbound_invalid_records={} outbound_command_counts={} outbound_reason_code_counts={} outbound_last_reason_codes={} outbound_last_event_key={}",
        report.repo,
        report.state_path,
        report.inbound_log_path,
        report.outbound_log_path,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_event_count,
        report.issue_session_count,
        report.inbound_records,
        report.inbound_invalid_records,
        inbound_kinds,
        report.outbound_records,
        report.outbound_invalid_records,
        outbound_commands,
        outbound_reasons,
        reason_codes,
        report.outbound_last_event_key.as_deref().unwrap_or("none"),
    )
}

pub(super) fn render_dashboard_status_report(report: &DashboardStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "dashboard status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} widget_count={} control_audit_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.widget_count,
        report.control_audit_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

pub(super) fn render_multi_channel_status_report(
    report: &MultiChannelStatusInspectReport,
) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    let telemetry_summary = format!(
        "typing_events={} presence_events={} usage_records={} usage_chars={} usage_chunks={} usage_cost_micros={} typing_by_transport={} presence_by_transport={} usage_records_by_transport={} usage_chars_by_transport={} usage_chunks_by_transport={} usage_cost_micros_by_transport={} policy=typing_presence:{}|usage_summary:{}|include_identifiers:{}|min_response_chars:{}",
        report.telemetry.typing_events_emitted,
        report.telemetry.presence_events_emitted,
        report.telemetry.usage_summary_records,
        report.telemetry.usage_response_chars,
        report.telemetry.usage_chunks,
        report.telemetry.usage_estimated_cost_micros,
        render_counter_map(&report.telemetry.typing_events_by_transport),
        render_counter_map(&report.telemetry.presence_events_by_transport),
        render_counter_map(&report.telemetry.usage_summary_records_by_transport),
        render_counter_map(&report.telemetry.usage_response_chars_by_transport),
        render_counter_map(&report.telemetry.usage_chunks_by_transport),
        render_u64_counter_map(&report.telemetry.usage_estimated_cost_micros_by_transport),
        report.telemetry.policy.typing_presence_enabled,
        report.telemetry.policy.usage_summary_enabled,
        report.telemetry.policy.include_identifiers,
        report.telemetry.policy.typing_presence_min_response_chars,
    );
    let connector_summary = report
        .connectors
        .as_ref()
        .map(|connectors| {
            let now_unix_ms = current_unix_timestamp_ms();
            let mut channel_rows = Vec::new();
            for (channel, state) in &connectors.channels {
                let operator_guidance = if state.breaker_state == "open"
                    && state.breaker_open_until_unix_ms > now_unix_ms
                {
                    format!(
                        "wait_for_breaker_recovery_until:{}",
                        state.breaker_open_until_unix_ms
                    )
                } else if state.liveness == "degraded" {
                    "inspect_provider_errors_and_credentials".to_string()
                } else {
                    "none".to_string()
                };
                channel_rows.push(format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    channel,
                    if state.mode.trim().is_empty() {
                        "unknown"
                    } else {
                        state.mode.as_str()
                    },
                    if state.liveness.trim().is_empty() {
                        "unknown"
                    } else {
                        state.liveness.as_str()
                    },
                    if state.breaker_state.trim().is_empty() {
                        "unknown"
                    } else {
                        state.breaker_state.as_str()
                    },
                    state.events_ingested,
                    state.duplicates_skipped,
                    state.retry_budget_remaining,
                    state.breaker_open_until_unix_ms,
                    if state.breaker_last_open_reason.trim().is_empty() {
                        "none"
                    } else {
                        state.breaker_last_open_reason.as_str()
                    },
                    state.breaker_open_count,
                    operator_guidance
                ));
            }
            channel_rows.sort();
            format!(
                "state_present={} processed_event_count={} channels={}",
                connectors.state_present,
                connectors.processed_event_count,
                if channel_rows.is_empty() {
                    "none".to_string()
                } else {
                    channel_rows.join(",")
                }
            )
        })
        .unwrap_or_else(|| "none".to_string());
    format!(
        "multi-channel status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_event_count={} transport_counts={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={} telemetry={} connectors={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_event_count,
        render_counter_map(&report.transport_counts),
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
        telemetry_summary,
        connector_summary,
    )
}

pub(super) fn render_multi_agent_status_report(report: &MultiAgentStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "multi-agent status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} routed_case_count={} phase_counts={} selected_role_counts={} category_counts={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.routed_case_count,
        render_counter_map(&report.phase_counts),
        render_counter_map(&report.selected_role_counts),
        render_counter_map(&report.category_counts),
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

pub(super) fn render_gateway_status_report(report: &GatewayStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    let service_last_startup_error = if report.service_last_startup_error.trim().is_empty() {
        "none".to_string()
    } else {
        report.service_last_startup_error.clone()
    };
    let service_last_stop_reason = if report.service_last_stop_reason.trim().is_empty() {
        "none".to_string()
    } else {
        report.service_last_stop_reason.clone()
    };
    format!(
        "gateway status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} rollout_reason_code={} service_status={} service_last_transition_unix_ms={} service_last_started_unix_ms={} service_last_stopped_unix_ms={} service_last_stop_reason={} service_startup_attempts={} service_startup_failure_streak={} service_last_startup_error={} guardrail_gate={} guardrail_reason_code={} processed_case_count={} request_count={} method_counts={} status_code_counts={} error_code_counts={} guardrail_failure_streak_threshold={} guardrail_retryable_failures_threshold={} guardrail_failure_streak={} guardrail_last_failed_cases={} guardrail_last_retryable_failures={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.rollout_reason_code,
        report.service_status,
        report.service_last_transition_unix_ms,
        report.service_last_started_unix_ms,
        report.service_last_stopped_unix_ms,
        service_last_stop_reason,
        report.service_startup_attempts,
        report.service_startup_failure_streak,
        service_last_startup_error,
        report.guardrail_gate,
        report.guardrail_reason_code,
        report.processed_case_count,
        report.request_count,
        render_counter_map(&report.method_counts),
        render_counter_map(&report.status_code_counts),
        render_counter_map(&report.error_code_counts),
        report.guardrail_failure_streak_threshold,
        report.guardrail_retryable_failures_threshold,
        report.guardrail_failure_streak,
        report.guardrail_last_failed_cases,
        report.guardrail_last_retryable_failures,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

pub(super) fn render_custom_command_status_report(
    report: &CustomCommandStatusInspectReport,
) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "custom-command status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} command_count={} command_name_counts={} operation_counts={} outcome_counts={} status_code_counts={} total_run_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.command_count,
        render_counter_map(&report.command_name_counts),
        render_counter_map(&report.operation_counts),
        render_counter_map(&report.outcome_counts),
        render_counter_map(&report.status_code_counts),
        report.total_run_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

pub(super) fn render_voice_status_report(report: &VoiceStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "voice status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} interaction_count={} mode_counts={} speaker_counts={} outcome_counts={} status_code_counts={} utterance_count={} total_run_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.interaction_count,
        render_counter_map(&report.mode_counts),
        render_counter_map(&report.speaker_counts),
        render_counter_map(&report.outcome_counts),
        render_counter_map(&report.status_code_counts),
        report.utterance_count,
        report.total_run_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

pub(super) fn render_deployment_status_report(report: &DeploymentStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "deployment status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} rollout_count={} target_counts={} runtime_profile_counts={} environment_counts={} outcome_counts={} status_code_counts={} error_code_counts={} total_replicas={} wasm_rollout_count={} cloud_rollout_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.rollout_count,
        render_counter_map(&report.target_counts),
        render_counter_map(&report.runtime_profile_counts),
        render_counter_map(&report.environment_counts),
        render_counter_map(&report.outcome_counts),
        render_counter_map(&report.status_code_counts),
        render_counter_map(&report.error_code_counts),
        report.total_replicas,
        report.wasm_rollout_count,
        report.cloud_rollout_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_counter_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_u64_counter_map(counts: &BTreeMap<String, u64>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_string_vec(values: &[String]) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    values.join(",")
}
