//! Operator-control summary and drift helpers for channel-store admin.

use super::*;

pub(super) fn collect_operator_control_summary_report(
    cli: &Cli,
) -> Result<OperatorControlSummaryReport> {
    let components = vec![
        collect_operator_events_component(cli),
        collect_operator_dashboard_component(cli),
        collect_operator_multi_channel_component(cli),
        collect_operator_multi_agent_component(cli),
        collect_operator_gateway_component(cli),
        collect_operator_deployment_component(cli),
        collect_operator_custom_command_component(cli),
        collect_operator_voice_component(cli),
    ];
    let daemon = collect_operator_daemon_summary(cli);
    let release_channel = collect_operator_release_channel_summary();
    let policy_posture = collect_operator_policy_posture(cli);

    let mut rollout_gate = "pass".to_string();
    let mut health_rank = 0_u8;
    let mut reason_codes = Vec::new();
    let mut recommendations = Vec::new();

    for component in &components {
        health_rank = health_rank.max(operator_health_state_rank(&component.health_state));
        if component.rollout_gate == "hold" {
            rollout_gate = "hold".to_string();
            push_unique_string(
                &mut reason_codes,
                format!("{}:{}", component.component, component.reason_code),
            );
            push_unique_string(&mut recommendations, component.recommendation.clone());
        }
    }

    health_rank = health_rank.max(operator_health_state_rank(&daemon.health_state));
    if daemon.rollout_gate == "hold" {
        rollout_gate = "hold".to_string();
        push_unique_string(&mut reason_codes, format!("daemon:{}", daemon.reason_code));
        push_unique_string(&mut recommendations, daemon.recommendation.clone());
    }

    health_rank = health_rank.max(operator_health_state_rank(&release_channel.health_state));
    if release_channel.rollout_gate == "hold" {
        rollout_gate = "hold".to_string();
        push_unique_string(
            &mut reason_codes,
            format!("release-channel:{}", release_channel.reason_code),
        );
        push_unique_string(&mut recommendations, release_channel.recommendation.clone());
    }

    if policy_posture.gateway_remote_gate == "hold" {
        rollout_gate = "hold".to_string();
        health_rank = health_rank.max(1);
        let posture_reason = policy_posture
            .gateway_remote_reason_codes
            .first()
            .cloned()
            .unwrap_or_else(|| "remote_profile_hold".to_string());
        push_unique_string(
            &mut reason_codes,
            format!("gateway-remote-profile:{posture_reason}"),
        );
        for recommendation in &policy_posture.gateway_remote_recommendations {
            push_unique_string(&mut recommendations, recommendation.clone());
        }
    }

    if reason_codes.is_empty() {
        reason_codes.push("all_checks_passing".to_string());
    }
    if recommendations.is_empty() {
        recommendations.push("no_immediate_action_required".to_string());
    }

    Ok(OperatorControlSummaryReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        health_state: operator_health_state_label(health_rank).to_string(),
        rollout_gate,
        reason_codes,
        recommendations,
        policy_posture,
        daemon,
        release_channel,
        components,
    })
}

pub(super) fn save_operator_control_summary_snapshot(
    path: &Path,
    report: &OperatorControlSummaryReport,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create snapshot directory {}", parent.display())
            })?;
        }
    }
    let payload = serde_json::to_string_pretty(report)
        .context("failed to serialize operator control summary snapshot")?;
    std::fs::write(path, payload).with_context(|| {
        format!(
            "failed to write operator control summary snapshot {}",
            path.display()
        )
    })
}

pub(super) fn load_operator_control_summary_snapshot(
    path: &Path,
) -> Result<OperatorControlSummaryReport> {
    let payload = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read operator control summary snapshot {}",
            path.display()
        )
    })?;
    serde_json::from_str::<OperatorControlSummaryReport>(&payload).with_context(|| {
        format!(
            "failed to parse operator control summary snapshot {}",
            path.display()
        )
    })
}

fn component_drift_rank(
    before: &OperatorControlComponentSummaryRow,
    after: &OperatorControlComponentSummaryRow,
) -> i8 {
    let before_health = operator_health_state_rank(&before.health_state) as i8;
    let after_health = operator_health_state_rank(&after.health_state) as i8;
    let health_delta = after_health - before_health;
    let before_gate = if before.rollout_gate == "hold" { 1 } else { 0 };
    let after_gate = if after.rollout_gate == "hold" { 1 } else { 0 };
    let gate_delta = after_gate - before_gate;

    if health_delta > 0 || gate_delta > 0 {
        1
    } else if health_delta < 0 || gate_delta < 0 {
        -1
    } else {
        0
    }
}

fn classify_component_drift_state(
    before: &OperatorControlComponentSummaryRow,
    after: &OperatorControlComponentSummaryRow,
) -> (&'static str, &'static str) {
    let changed = before.health_state != after.health_state
        || before.rollout_gate != after.rollout_gate
        || before.reason_code != after.reason_code
        || before.recommendation != after.recommendation
        || before.queue_depth != after.queue_depth
        || before.failure_streak != after.failure_streak;

    if !changed {
        return ("stable", "none");
    }

    match component_drift_rank(before, after) {
        1 => ("regressed", "high"),
        -1 => ("improved", "low"),
        _ => ("changed", "medium"),
    }
}

fn component_snapshot_placeholder(name: &str) -> OperatorControlComponentSummaryRow {
    OperatorControlComponentSummaryRow {
        component: name.to_string(),
        state_path: "snapshot_missing".to_string(),
        health_state: "failing".to_string(),
        health_reason: "component snapshot missing".to_string(),
        rollout_gate: "hold".to_string(),
        reason_code: "snapshot_missing".to_string(),
        recommendation: "capture a complete baseline snapshot and rerun compare".to_string(),
        queue_depth: 0,
        failure_streak: 0,
    }
}

fn vec_delta(before: &[String], after: &[String]) -> (Vec<String>, Vec<String>) {
    let before_set: BTreeSet<String> = before.iter().cloned().collect();
    let after_set: BTreeSet<String> = after.iter().cloned().collect();
    let added = after_set
        .difference(&before_set)
        .cloned()
        .collect::<Vec<String>>();
    let removed = before_set
        .difference(&after_set)
        .cloned()
        .collect::<Vec<String>>();
    (added, removed)
}

pub(super) fn build_operator_control_summary_diff_report(
    baseline: &OperatorControlSummaryReport,
    current: &OperatorControlSummaryReport,
) -> OperatorControlSummaryDiffReport {
    let mut baseline_components = BTreeMap::new();
    for component in &baseline.components {
        baseline_components.insert(component.component.clone(), component.clone());
    }

    let mut current_components = BTreeMap::new();
    for component in &current.components {
        current_components.insert(component.component.clone(), component.clone());
    }

    let mut component_names: BTreeSet<String> = BTreeSet::new();
    component_names.extend(baseline_components.keys().cloned());
    component_names.extend(current_components.keys().cloned());

    let mut changed_components = Vec::new();
    let mut unchanged_component_count = 0usize;
    for name in component_names {
        let before = baseline_components
            .get(&name)
            .cloned()
            .unwrap_or_else(|| component_snapshot_placeholder(&name));
        let after = current_components
            .get(&name)
            .cloned()
            .unwrap_or_else(|| component_snapshot_placeholder(&name));
        let (drift_state, severity) = classify_component_drift_state(&before, &after);
        if drift_state == "stable" {
            unchanged_component_count = unchanged_component_count.saturating_add(1);
            continue;
        }
        changed_components.push(OperatorControlSummaryDiffComponentRow {
            component: name,
            drift_state: drift_state.to_string(),
            severity: severity.to_string(),
            health_state_before: before.health_state,
            health_state_after: after.health_state,
            rollout_gate_before: before.rollout_gate,
            rollout_gate_after: after.rollout_gate,
            reason_code_before: before.reason_code,
            reason_code_after: after.reason_code,
            recommendation_before: before.recommendation,
            recommendation_after: after.recommendation,
            queue_depth_before: before.queue_depth,
            queue_depth_after: after.queue_depth,
            failure_streak_before: before.failure_streak,
            failure_streak_after: after.failure_streak,
        });
    }

    let (reason_codes_added, reason_codes_removed) =
        vec_delta(&baseline.reason_codes, &current.reason_codes);
    let (recommendations_added, recommendations_removed) =
        vec_delta(&baseline.recommendations, &current.recommendations);

    let health_drift = operator_health_state_rank(&current.health_state) as i8
        - operator_health_state_rank(&baseline.health_state) as i8;
    let gate_before = if baseline.rollout_gate == "hold" {
        1
    } else {
        0
    };
    let gate_after = if current.rollout_gate == "hold" { 1 } else { 0 };
    let gate_drift = gate_after - gate_before;
    let drift_state = if health_drift > 0 || gate_drift > 0 {
        "regressed"
    } else if health_drift < 0 || gate_drift < 0 {
        "improved"
    } else if changed_components.is_empty()
        && reason_codes_added.is_empty()
        && reason_codes_removed.is_empty()
        && recommendations_added.is_empty()
        && recommendations_removed.is_empty()
    {
        "stable"
    } else {
        "changed"
    };

    let risk_level = if drift_state == "regressed" && current.rollout_gate == "hold" {
        "high"
    } else if drift_state == "regressed" || current.health_state == "degraded" {
        "moderate"
    } else {
        "low"
    };

    OperatorControlSummaryDiffReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        baseline_generated_unix_ms: baseline.generated_unix_ms,
        current_generated_unix_ms: current.generated_unix_ms,
        drift_state: drift_state.to_string(),
        risk_level: risk_level.to_string(),
        health_state_before: baseline.health_state.clone(),
        health_state_after: current.health_state.clone(),
        rollout_gate_before: baseline.rollout_gate.clone(),
        rollout_gate_after: current.rollout_gate.clone(),
        reason_codes_added,
        reason_codes_removed,
        recommendations_added,
        recommendations_removed,
        changed_components,
        unchanged_component_count,
    }
}

fn collect_operator_dashboard_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.dashboard_state_dir.join("state.json");
    match collect_dashboard_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "dashboard",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "dashboard_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("dashboard", &state_path, &error),
    }
}

fn collect_operator_events_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.events_state_path.clone();
    let events_dir_exists = cli.events_dir.is_dir();
    let state_exists = state_path.is_file();
    if !events_dir_exists && !state_exists {
        return build_operator_component_row(
            "events",
            OperatorControlComponentInputs {
                state_path: state_path.display().to_string(),
                health_state: "healthy".to_string(),
                health_reason: "events scheduler is not configured".to_string(),
                rollout_gate: "pass".to_string(),
                reason_code: "events_not_configured".to_string(),
                recommendation:
                    "create event definition files under --events-dir to enable routine scheduling"
                        .to_string(),
                queue_depth: 0,
                failure_streak: 0,
            },
        );
    }

    match inspect_events(
        &EventsInspectConfig {
            events_dir: cli.events_dir.clone(),
            state_path: state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    ) {
        Ok(report) => {
            let mut health_state = "healthy".to_string();
            let mut rollout_gate = "pass".to_string();
            let mut reason_code = "events_ready".to_string();
            let mut recommendation = "no immediate action required".to_string();
            let mut health_reason = "events scheduler diagnostics are healthy".to_string();

            if report.discovered_events == 0 {
                reason_code = "events_none_discovered".to_string();
                recommendation =
                    "add event definition files under --events-dir to enable routines".to_string();
                health_reason =
                    "events scheduler is configured but no definitions were discovered".to_string();
            }
            if report.malformed_events > 0 || report.due_eval_failed_events > 0 {
                health_state = "degraded".to_string();
                rollout_gate = "hold".to_string();
                reason_code = "events_definition_invalid".to_string();
                recommendation =
                    "run --events-validate and repair malformed/invalid event definitions"
                        .to_string();
                health_reason = format!(
                    "events inspect found malformed={} due_eval_failed={}",
                    report.malformed_events, report.due_eval_failed_events
                );
            }
            if report.failed_history_entries > 0 {
                health_state = "degraded".to_string();
                rollout_gate = "hold".to_string();
                reason_code = "events_recent_failures".to_string();
                recommendation =
                    "inspect channel-store logs and execution history for failing routines"
                        .to_string();
                health_reason = format!(
                    "events execution history includes {} failed runs",
                    report.failed_history_entries
                );
            }

            build_operator_component_row(
                "events",
                OperatorControlComponentInputs {
                    state_path: report.state_path,
                    health_state,
                    health_reason,
                    rollout_gate,
                    reason_code,
                    recommendation,
                    queue_depth: report.queued_now_events,
                    failure_streak: report.failed_history_entries,
                },
            )
        }
        Err(error) => operator_component_unavailable("events", &state_path, &error),
    }
}

fn collect_operator_multi_channel_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.multi_channel_state_dir.join("state.json");
    match collect_multi_channel_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "multi-channel",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "multi_channel_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("multi-channel", &state_path, &error),
    }
}

fn collect_operator_multi_agent_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.multi_agent_state_dir.join("state.json");
    match collect_multi_agent_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "multi-agent",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "multi_agent_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("multi-agent", &state_path, &error),
    }
}

fn collect_operator_gateway_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.gateway_state_dir.join("state.json");
    match collect_gateway_status_report(cli) {
        Ok(report) => {
            let recommendation = if report.rollout_reason_code == "service_stopped" {
                "start gateway service mode or clear stop reason before resuming traffic"
            } else {
                report.health.classify().recommendation
            };
            build_operator_component_row(
                "gateway",
                OperatorControlComponentInputs {
                    state_path: report.state_path,
                    health_state: report.health_state,
                    health_reason: report.health_reason,
                    rollout_gate: report.rollout_gate,
                    reason_code: report.rollout_reason_code,
                    recommendation: recommendation.to_string(),
                    queue_depth: report.health.queue_depth,
                    failure_streak: report.health.failure_streak,
                },
            )
        }
        Err(error) => operator_component_unavailable("gateway", &state_path, &error),
    }
}

fn collect_operator_deployment_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.deployment_state_dir.join("state.json");
    match collect_deployment_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "deployment",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "deployment_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("deployment", &state_path, &error),
    }
}

fn collect_operator_custom_command_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.custom_command_state_dir.join("state.json");
    match collect_custom_command_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "custom-command",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "custom_command_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("custom-command", &state_path, &error),
    }
}

fn collect_operator_voice_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.voice_state_dir.join("state.json");
    match collect_voice_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "voice",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "voice_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("voice", &state_path, &error),
    }
}

fn collect_operator_daemon_summary(cli: &Cli) -> OperatorControlDaemonSummary {
    let config = TauDaemonConfig {
        state_dir: cli.daemon_state_dir.clone(),
        profile: cli.daemon_profile,
    };
    match inspect_tau_daemon(&config) {
        Ok(report) => {
            let (health_state, rollout_gate, reason_code, recommendation) = if report.running {
                (
                    "healthy".to_string(),
                    "pass".to_string(),
                    "daemon_running".to_string(),
                    "no immediate action required".to_string(),
                )
            } else if report.installed {
                (
                    "degraded".to_string(),
                    "hold".to_string(),
                    "daemon_not_running".to_string(),
                    "start daemon with --daemon-start to restore background processing".to_string(),
                )
            } else {
                (
                    "degraded".to_string(),
                    "hold".to_string(),
                    "daemon_not_installed".to_string(),
                    "install daemon with --daemon-install if background lifecycle management is required".to_string(),
                )
            };
            OperatorControlDaemonSummary {
                health_state,
                rollout_gate,
                reason_code,
                recommendation,
                profile: report.profile,
                installed: report.installed,
                running: report.running,
                start_attempts: report.start_attempts,
                stop_attempts: report.stop_attempts,
                diagnostics: report.diagnostics.len(),
                state_path: report.state_path,
            }
        }
        Err(error) => OperatorControlDaemonSummary {
            health_state: "failing".to_string(),
            rollout_gate: "hold".to_string(),
            reason_code: "daemon_status_unavailable".to_string(),
            recommendation: "inspect --daemon-state-dir permissions and rerun --daemon-status"
                .to_string(),
            profile: cli.daemon_profile.as_str().to_string(),
            installed: false,
            running: false,
            start_attempts: 0,
            stop_attempts: 0,
            diagnostics: 1,
            state_path: format!("{} ({error})", cli.daemon_state_dir.display()),
        },
    }
}

fn collect_operator_release_channel_summary() -> OperatorControlReleaseChannelSummary {
    match default_release_channel_path() {
        Ok(path) => match load_release_channel_store(&path) {
            Ok(Some(channel)) => OperatorControlReleaseChannelSummary {
                health_state: "healthy".to_string(),
                rollout_gate: "pass".to_string(),
                reason_code: "release_channel_loaded".to_string(),
                recommendation: "no immediate action required".to_string(),
                configured: true,
                channel: channel.as_str().to_string(),
                path: path.display().to_string(),
            },
            Ok(None) => OperatorControlReleaseChannelSummary {
                health_state: "degraded".to_string(),
                rollout_gate: "hold".to_string(),
                reason_code: "release_channel_missing".to_string(),
                recommendation:
                    "set a release channel with '/release-channel set <stable|beta|dev>'"
                        .to_string(),
                configured: false,
                channel: "unknown".to_string(),
                path: path.display().to_string(),
            },
            Err(error) => OperatorControlReleaseChannelSummary {
                health_state: "failing".to_string(),
                rollout_gate: "hold".to_string(),
                reason_code: "release_channel_load_failed".to_string(),
                recommendation:
                    "repair .tau/release-channel.json or rerun '/release-channel set ...'"
                        .to_string(),
                configured: false,
                channel: "unknown".to_string(),
                path: format!("{} ({error})", path.display()),
            },
        },
        Err(error) => OperatorControlReleaseChannelSummary {
            health_state: "failing".to_string(),
            rollout_gate: "hold".to_string(),
            reason_code: "release_channel_path_unavailable".to_string(),
            recommendation: "run from a writable workspace root to resolve .tau paths".to_string(),
            configured: false,
            channel: "unknown".to_string(),
            path: format!("unknown ({error})"),
        },
    }
}

fn collect_operator_policy_posture(cli: &Cli) -> OperatorControlPolicyPosture {
    let pairing_policy = pairing_policy_for_state_dir(&cli.channel_store_root);
    let (pairing_allowlist_strict, pairing_allowlist_channel_rules) =
        load_pairing_allowlist_posture(&pairing_policy.allowlist_path);
    let pairing_registry_entries = load_pairing_registry_entry_count(&pairing_policy.registry_path);
    let pairing_rules_configured =
        pairing_allowlist_channel_rules > 0 || pairing_registry_entries > 0;
    let pairing_strict_effective =
        pairing_policy.strict_mode || pairing_allowlist_strict || pairing_rules_configured;

    let remote_profile = match tau_cli::gateway_remote_profile::evaluate_gateway_remote_profile(cli)
    {
        Ok(report) => report,
        Err(_) => tau_cli::gateway_remote_profile::GatewayRemoteProfileReport {
            profile: cli.gateway_remote_profile.as_str().to_string(),
            posture: "unknown".to_string(),
            gate: "hold".to_string(),
            risk_level: "high".to_string(),
            server_enabled: cli.gateway_openresponses_server,
            bind: cli.gateway_openresponses_bind.clone(),
            bind_ip: "unknown".to_string(),
            loopback_bind: false,
            auth_mode: cli.gateway_openresponses_auth_mode.as_str().to_string(),
            auth_token_configured: false,
            auth_password_configured: false,
            remote_enabled: !matches!(
                cli.gateway_remote_profile,
                CliGatewayRemoteProfile::LocalOnly
            ),
            reason_codes: vec!["remote_profile_evaluation_failed".to_string()],
            recommendations: vec![
                "run --gateway-remote-profile-inspect to inspect posture diagnostics".to_string(),
            ],
        },
    };

    OperatorControlPolicyPosture {
        pairing_strict_effective,
        pairing_config_strict_mode: pairing_policy.strict_mode,
        pairing_allowlist_strict,
        pairing_rules_configured,
        pairing_registry_entries,
        pairing_allowlist_channel_rules,
        provider_subscription_strict: cli.provider_subscription_strict,
        gateway_auth_mode: cli.gateway_openresponses_auth_mode.as_str().to_string(),
        gateway_remote_profile: remote_profile.profile,
        gateway_remote_posture: remote_profile.posture,
        gateway_remote_gate: remote_profile.gate,
        gateway_remote_risk_level: remote_profile.risk_level,
        gateway_remote_reason_codes: remote_profile.reason_codes,
        gateway_remote_recommendations: remote_profile.recommendations,
    }
}

fn load_pairing_allowlist_posture(path: &Path) -> (bool, usize) {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return (false, 0),
    };
    let parsed = match serde_json::from_str::<PairingAllowlistSummaryFile>(&raw) {
        Ok(parsed) => parsed,
        Err(_) => return (false, 0),
    };
    let rules = parsed
        .channels
        .values()
        .map(|actors| actors.len())
        .sum::<usize>();
    (parsed.strict, rules)
}

fn load_pairing_registry_entry_count(path: &Path) -> usize {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return 0,
    };
    match serde_json::from_str::<PairingRegistrySummaryFile>(&raw) {
        Ok(parsed) => parsed.pairings.len(),
        Err(_) => 0,
    }
}

fn operator_component_unavailable(
    component: &str,
    state_path: &Path,
    error: &anyhow::Error,
) -> OperatorControlComponentSummaryRow {
    build_operator_component_row(
        component,
        OperatorControlComponentInputs {
            state_path: state_path.display().to_string(),
            health_state: "failing".to_string(),
            health_reason: format!("status unavailable: {error}"),
            rollout_gate: "hold".to_string(),
            reason_code: "state_unavailable".to_string(),
            recommendation: "bootstrap or repair component state, then rerun operator summary"
                .to_string(),
            queue_depth: 0,
            failure_streak: 0,
        },
    )
}

fn build_operator_component_row(
    component: &str,
    inputs: OperatorControlComponentInputs,
) -> OperatorControlComponentSummaryRow {
    OperatorControlComponentSummaryRow {
        component: component.to_string(),
        health_state: inputs.health_state,
        health_reason: inputs.health_reason,
        rollout_gate: inputs.rollout_gate,
        reason_code: inputs.reason_code,
        recommendation: inputs.recommendation,
        queue_depth: inputs.queue_depth,
        failure_streak: inputs.failure_streak,
        state_path: inputs.state_path,
    }
}

fn latest_reason_code_or_fallback(reason_codes: &[String], fallback: &str) -> String {
    reason_codes
        .iter()
        .rev()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

pub(super) fn operator_health_state_rank(state: &str) -> u8 {
    if state.eq_ignore_ascii_case("healthy") {
        return 0;
    }
    if state.eq_ignore_ascii_case("degraded") {
        return 1;
    }
    2
}

fn operator_health_state_label(rank: u8) -> &'static str {
    match rank {
        0 => "healthy",
        1 => "degraded",
        _ => "failing",
    }
}

fn push_unique_string(list: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if value.trim().is_empty() {
        return;
    }
    if list.iter().any(|existing| existing == &value) {
        return;
    }
    list.push(value);
}
