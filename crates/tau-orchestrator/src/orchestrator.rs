use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;

use crate::multi_agent_router::{
    build_multi_agent_role_prompt, resolve_multi_agent_role_profile, select_multi_agent_route,
    MultiAgentRoutePhase, MultiAgentRouteTable,
};
use tau_core::time_utils::current_unix_timestamp_ms;

const ORCHESTRATOR_ROUTE_TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorPromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrchestratorRenderOptions {
    pub stream_output: bool,
    pub stream_delay_ms: u64,
}

#[async_trait(?Send)]
pub trait OrchestratorRuntime {
    async fn run_prompt_with_cancellation(
        &mut self,
        prompt: &str,
        turn_timeout_ms: u64,
        render_options: OrchestratorRenderOptions,
    ) -> Result<OrchestratorPromptRunStatus>;

    fn latest_assistant_text(&self) -> Option<String>;

    fn report_prompt_status(&self, status: OrchestratorPromptRunStatus);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutedPromptRunState {
    Completed,
    Interrupted,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_plan_first_prompt<R: OrchestratorRuntime>(
    runtime: &mut R,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: OrchestratorRenderOptions,
    max_plan_steps: usize,
    max_delegated_steps: usize,
    max_executor_response_chars: usize,
    max_delegated_step_response_chars: usize,
    max_delegated_total_response_chars: usize,
    delegate_steps: bool,
) -> Result<()> {
    let fallback_policy_context = delegate_steps.then_some("legacy_policy_context=implicit");
    run_plan_first_prompt_with_policy_context(
        runtime,
        user_prompt,
        turn_timeout_ms,
        render_options,
        max_plan_steps,
        max_delegated_steps,
        max_executor_response_chars,
        max_delegated_step_response_chars,
        max_delegated_total_response_chars,
        delegate_steps,
        fallback_policy_context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_plan_first_prompt_with_policy_context<R: OrchestratorRuntime>(
    runtime: &mut R,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: OrchestratorRenderOptions,
    max_plan_steps: usize,
    max_delegated_steps: usize,
    max_executor_response_chars: usize,
    max_delegated_step_response_chars: usize,
    max_delegated_total_response_chars: usize,
    delegate_steps: bool,
    delegated_policy_context: Option<&str>,
) -> Result<()> {
    let default_route_table = MultiAgentRouteTable::default();
    run_plan_first_prompt_with_policy_context_and_routing(
        runtime,
        user_prompt,
        turn_timeout_ms,
        render_options,
        max_plan_steps,
        max_delegated_steps,
        max_executor_response_chars,
        max_delegated_step_response_chars,
        max_delegated_total_response_chars,
        delegate_steps,
        delegated_policy_context,
        &default_route_table,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_plan_first_prompt_with_policy_context_and_routing<R: OrchestratorRuntime>(
    runtime: &mut R,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: OrchestratorRenderOptions,
    max_plan_steps: usize,
    max_delegated_steps: usize,
    max_executor_response_chars: usize,
    max_delegated_step_response_chars: usize,
    max_delegated_total_response_chars: usize,
    delegate_steps: bool,
    delegated_policy_context: Option<&str>,
    route_table: &MultiAgentRouteTable,
    route_trace_log_path: Option<&Path>,
) -> Result<()> {
    let planner_prompt = build_plan_first_planner_prompt(user_prompt, max_plan_steps);
    let planner_render_options = OrchestratorRenderOptions {
        stream_output: false,
        stream_delay_ms: 0,
    };
    let planner_state = run_routed_prompt_with_fallback(
        runtime,
        route_table,
        MultiAgentRoutePhase::Planner,
        None,
        None,
        &planner_prompt,
        "planner produced no text output",
        turn_timeout_ms,
        planner_render_options,
        route_trace_log_path,
    )
    .await?;
    if planner_state == RoutedPromptRunState::Interrupted {
        return Ok(());
    }
    let plan_text = runtime.latest_assistant_text().ok_or_else(|| {
        anyhow!("plan-first orchestrator failed: planner produced no text output")
    })?;
    let plan_steps = parse_numbered_plan_steps(&plan_text);
    if plan_steps.is_empty() {
        bail!("plan-first orchestrator failed: planner response did not include numbered steps");
    }
    if plan_steps.len() > max_plan_steps {
        bail!(
            "plan-first orchestrator failed: planner produced {} steps (max allowed {})",
            plan_steps.len(),
            max_plan_steps
        );
    }

    println!(
        "orchestrator trace: mode=plan-first phase=planner approved_steps={} max_steps={}",
        plan_steps.len(),
        max_plan_steps
    );
    for (index, step) in plan_steps.iter().enumerate() {
        println!(
            "orchestrator trace: phase=planner step={} text={}",
            index + 1,
            flatten_whitespace(step)
        );
    }
    let execution_prompt = if delegate_steps {
        let Some(policy_context) = delegated_policy_context
            .map(str::trim)
            .filter(|context| !context.is_empty())
        else {
            println!(
                "orchestrator trace: mode=plan-first phase=executor strategy=delegated-steps decision=reject reason=policy_inheritance_context_missing"
            );
            bail!(
                "plan-first orchestrator failed: delegated policy inheritance context is unavailable"
            );
        };
        println!(
            "orchestrator trace: mode=plan-first phase=executor strategy=delegated-steps total_steps={} policy_inheritance=verified policy_context_chars={}",
            plan_steps.len(),
            policy_context.chars().count()
        );
        if plan_steps.len() > max_delegated_steps {
            println!(
                "orchestrator trace: mode=plan-first phase=delegated-step decision=reject reason=delegated_step_count_budget_exceeded approved_steps={} max_delegated_steps={}",
                plan_steps.len(),
                max_delegated_steps
            );
            bail!(
                "plan-first orchestrator failed: delegated step budget exceeded (steps {} > max {})",
                plan_steps.len(),
                max_delegated_steps
            );
        }
        let mut delegated_outputs = Vec::new();
        let mut delegated_total_response_chars = 0usize;
        for (index, step) in plan_steps.iter().enumerate() {
            println!(
                "orchestrator trace: mode=plan-first phase=delegated-step step={} action=start text={}",
                index + 1,
                flatten_whitespace(step)
            );
            let delegated_prompt = build_plan_first_delegated_step_prompt(
                user_prompt,
                &plan_steps,
                index,
                step,
                policy_context,
            );
            let delegated_state = run_routed_prompt_with_fallback(
                runtime,
                route_table,
                MultiAgentRoutePhase::DelegatedStep,
                Some(step.as_str()),
                Some(index + 1),
                &delegated_prompt,
                &format!("delegated step {} produced no text output", index + 1),
                turn_timeout_ms,
                planner_render_options,
                route_trace_log_path,
            )
            .await?;
            if delegated_state == RoutedPromptRunState::Interrupted {
                return Ok(());
            }
            let delegated_text = runtime.latest_assistant_text().ok_or_else(|| {
                anyhow!(
                    "plan-first orchestrator failed: delegated step {} produced no text output",
                    index + 1
                )
            })?;
            if delegated_text.trim().is_empty() {
                bail!(
                    "plan-first orchestrator failed: delegated step {} produced no text output",
                    index + 1
                );
            }
            let delegated_response_chars = delegated_text.chars().count();
            if !executor_response_within_budget(
                delegated_response_chars,
                max_delegated_step_response_chars,
            ) {
                println!(
                    "orchestrator trace: mode=plan-first phase=delegated-step step={} decision=reject reason=delegated_step_response_budget_exceeded response_chars={} max_response_chars={}",
                    index + 1,
                    delegated_response_chars,
                    max_delegated_step_response_chars
                );
                bail!(
                    "plan-first orchestrator failed: delegated step {} response exceeded budget (chars {} > max {})",
                    index + 1,
                    delegated_response_chars,
                    max_delegated_step_response_chars
                );
            }
            delegated_total_response_chars =
                delegated_total_response_chars.saturating_add(delegated_response_chars);
            if !executor_response_within_budget(
                delegated_total_response_chars,
                max_delegated_total_response_chars,
            ) {
                println!(
                    "orchestrator trace: mode=plan-first phase=delegated-step step={} decision=reject reason=delegated_total_response_budget_exceeded total_response_chars={} max_total_response_chars={}",
                    index + 1,
                    delegated_total_response_chars,
                    max_delegated_total_response_chars
                );
                bail!(
                    "plan-first orchestrator failed: delegated responses exceeded cumulative budget (chars {} > max {})",
                    delegated_total_response_chars,
                    max_delegated_total_response_chars
                );
            }
            println!(
                "orchestrator trace: mode=plan-first phase=delegated-step step={} action=complete response_chars={} total_response_chars={} max_step_response_chars={} max_total_response_chars={}",
                index + 1,
                delegated_response_chars,
                delegated_total_response_chars,
                max_delegated_step_response_chars,
                max_delegated_total_response_chars
            );
            delegated_outputs.push(delegated_text);
        }
        println!(
            "orchestrator trace: mode=plan-first phase=consolidation delegated_steps={}",
            delegated_outputs.len()
        );
        build_plan_first_consolidation_prompt(user_prompt, &plan_steps, &delegated_outputs)
    } else {
        println!("orchestrator trace: mode=plan-first phase=executor");
        build_plan_first_execution_prompt(user_prompt, &plan_steps)
    };

    let execution_state = run_routed_prompt_with_fallback(
        runtime,
        route_table,
        MultiAgentRoutePhase::Review,
        None,
        None,
        &execution_prompt,
        if delegate_steps {
            "consolidation produced no text output"
        } else {
            "executor produced no text output"
        },
        turn_timeout_ms,
        render_options,
        route_trace_log_path,
    )
    .await?;
    if execution_state == RoutedPromptRunState::Interrupted {
        return Ok(());
    }

    let execution_phase_label = if delegate_steps {
        "consolidation"
    } else {
        "executor"
    };
    let execution_text = runtime.latest_assistant_text().ok_or_else(|| {
        anyhow!(
            "plan-first orchestrator failed: {} produced no text output",
            execution_phase_label
        )
    })?;
    if execution_text.trim().is_empty() {
        bail!(
            "plan-first orchestrator failed: {} produced no text output",
            execution_phase_label
        );
    }
    let response_chars = execution_text.chars().count();
    let covered_steps = count_reviewed_plan_steps(&plan_steps, &execution_text);
    let within_budget =
        executor_response_within_budget(response_chars, max_executor_response_chars);
    println!(
        "orchestrator trace: mode=plan-first phase=review covered_steps={} total_steps={} response_chars={} max_response_chars={} within_budget={}",
        covered_steps,
        plan_steps.len(),
        response_chars,
        max_executor_response_chars,
        within_budget
    );
    if !within_budget {
        println!(
            "orchestrator trace: mode=plan-first phase=consolidation decision=reject reason=executor_response_budget_exceeded"
        );
        bail!(
            "plan-first orchestrator failed: {} response exceeded budget (chars {} > max {})",
            execution_phase_label,
            response_chars,
            max_executor_response_chars
        );
    }
    println!("orchestrator trace: mode=plan-first phase=consolidation decision=accept");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_routed_prompt_with_fallback<R: OrchestratorRuntime>(
    runtime: &mut R,
    route_table: &MultiAgentRouteTable,
    phase: MultiAgentRoutePhase,
    step_text: Option<&str>,
    step_index: Option<usize>,
    base_prompt: &str,
    empty_output_reason: &str,
    turn_timeout_ms: u64,
    render_options: OrchestratorRenderOptions,
    route_trace_log_path: Option<&Path>,
) -> Result<RoutedPromptRunState> {
    let selection = select_multi_agent_route(route_table, phase, step_text);
    emit_route_trace(
        route_trace_log_path,
        phase,
        selection.category.as_deref(),
        step_index,
        "route-selected",
        Some(&selection.primary_role),
        None,
        Some("accept"),
        None,
        Some(&selection.fallback_roles.join(",")),
        None,
    );

    for (attempt_index, role) in selection.attempt_roles.iter().enumerate() {
        let profile = resolve_multi_agent_role_profile(route_table, role);
        let model_hint = profile.model.as_deref().unwrap_or("inherit");
        let tool_policy_hint = profile.tool_policy_preset.as_deref().unwrap_or("inherit");
        emit_route_trace(
            route_trace_log_path,
            phase,
            selection.category.as_deref(),
            step_index,
            "attempt-start",
            Some(role),
            Some((attempt_index + 1, selection.attempt_roles.len())),
            None,
            None,
            Some(&format!(
                "model_hint={};tool_policy_preset={}",
                model_hint, tool_policy_hint
            )),
            None,
        );

        let attempt_prompt = build_multi_agent_role_prompt(base_prompt, phase, role, &profile);
        let attempt_status = match runtime
            .run_prompt_with_cancellation(&attempt_prompt, turn_timeout_ms, render_options)
            .await
        {
            Ok(status) => status,
            Err(error) => {
                let has_fallback = attempt_index + 1 < selection.attempt_roles.len();
                if has_fallback {
                    let next_role = selection.attempt_roles[attempt_index + 1].as_str();
                    emit_route_trace(
                        route_trace_log_path,
                        phase,
                        selection.category.as_deref(),
                        step_index,
                        "fallback",
                        Some(role),
                        Some((attempt_index + 1, selection.attempt_roles.len())),
                        Some("retry"),
                        Some("prompt_execution_error"),
                        Some(&format!("next_role={next_role} error={error}")),
                        None,
                    );
                    continue;
                }
                emit_route_trace(
                    route_trace_log_path,
                    phase,
                    selection.category.as_deref(),
                    step_index,
                    "fallback",
                    Some(role),
                    Some((attempt_index + 1, selection.attempt_roles.len())),
                    Some("reject"),
                    Some("prompt_execution_error_exhausted"),
                    Some(&format!("error={error}")),
                    None,
                );
                return Err(error).context(format!(
                    "plan-first orchestrator failed: {} route exhausted after role '{}'",
                    phase.as_str(),
                    role
                ));
            }
        };
        runtime.report_prompt_status(attempt_status);
        if attempt_status != OrchestratorPromptRunStatus::Completed {
            return Ok(RoutedPromptRunState::Interrupted);
        }
        let Some(assistant_text) = runtime.latest_assistant_text() else {
            emit_route_trace(
                route_trace_log_path,
                phase,
                selection.category.as_deref(),
                step_index,
                "attempt-complete",
                Some(role),
                Some((attempt_index + 1, selection.attempt_roles.len())),
                Some("reject"),
                Some("empty_output"),
                None,
                Some(0),
            );
            bail!("plan-first orchestrator failed: {empty_output_reason}");
        };
        if assistant_text.trim().is_empty() {
            emit_route_trace(
                route_trace_log_path,
                phase,
                selection.category.as_deref(),
                step_index,
                "attempt-complete",
                Some(role),
                Some((attempt_index + 1, selection.attempt_roles.len())),
                Some("reject"),
                Some("empty_output"),
                None,
                Some(assistant_text.chars().count()),
            );
            bail!("plan-first orchestrator failed: {empty_output_reason}");
        }
        emit_route_trace(
            route_trace_log_path,
            phase,
            selection.category.as_deref(),
            step_index,
            "attempt-complete",
            Some(role),
            Some((attempt_index + 1, selection.attempt_roles.len())),
            Some("accept"),
            None,
            None,
            Some(assistant_text.chars().count()),
        );
        return Ok(RoutedPromptRunState::Completed);
    }

    bail!(
        "plan-first orchestrator failed: {} route did not yield any attempts",
        phase.as_str()
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_route_trace(
    route_trace_log_path: Option<&Path>,
    phase: MultiAgentRoutePhase,
    category: Option<&str>,
    step_index: Option<usize>,
    event: &str,
    role: Option<&str>,
    attempt: Option<(usize, usize)>,
    decision: Option<&str>,
    reason: Option<&str>,
    detail: Option<&str>,
    response_chars: Option<usize>,
) {
    let mut parts = vec![
        "orchestrator trace: mode=plan-first".to_string(),
        format!("phase={}", phase.as_str()),
        format!("event={event}"),
    ];
    if let Some(category) = category {
        parts.push(format!("category={}", flatten_whitespace(category)));
    }
    if let Some(step_index) = step_index {
        parts.push(format!("step={step_index}"));
    }
    if let Some(role) = role {
        parts.push(format!("role={role}"));
    }
    if let Some((index, total)) = attempt {
        parts.push(format!("attempt={index}/{total}"));
    }
    if let Some(decision) = decision {
        parts.push(format!("decision={decision}"));
    }
    if let Some(reason) = reason {
        parts.push(format!("reason={reason}"));
    }
    if let Some(detail) = detail {
        parts.push(format!("detail={}", flatten_whitespace(detail)));
    }
    if let Some(response_chars) = response_chars {
        parts.push(format!("response_chars={response_chars}"));
    }
    println!("{}", parts.join(" "));

    let Some(path) = route_trace_log_path else {
        return;
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "orchestrator trace logger warning: failed to create {}: {error}",
                    parent.display()
                );
                return;
            }
        }
    }
    let record = serde_json::json!({
        "record_type": "orchestrator_route_trace_v1",
        "schema_version": ORCHESTRATOR_ROUTE_TRACE_SCHEMA_VERSION,
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "mode": "plan-first",
        "phase": phase.as_str(),
        "category": category,
        "step_index": step_index,
        "event": event,
        "role": role,
        "attempt_index": attempt.map(|value| value.0),
        "attempt_total": attempt.map(|value| value.1),
        "decision": decision,
        "reason": reason,
        "detail": detail,
        "response_chars": response_chars,
    });
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        eprintln!(
            "orchestrator trace logger warning: failed to open {}",
            path.display()
        );
        return;
    };
    let Ok(line) = serde_json::to_string(&record) else {
        eprintln!("orchestrator trace logger warning: failed to serialize route trace");
        return;
    };
    if let Err(error) = writeln!(file, "{line}") {
        eprintln!(
            "orchestrator trace logger warning: failed to write {}: {error}",
            path.display()
        );
    }
}

pub fn parse_numbered_plan_steps(plan: &str) -> Vec<String> {
    let mut steps = Vec::new();
    for line in plan.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let digit_prefix_len = trimmed
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count();
        if digit_prefix_len == 0 {
            continue;
        }
        let remainder = trimmed[digit_prefix_len..].trim_start();
        let Some(remainder) = remainder
            .strip_prefix('.')
            .or_else(|| remainder.strip_prefix(')'))
        else {
            continue;
        };
        let step = remainder.trim();
        if step.is_empty() {
            continue;
        }
        steps.push(step.to_string());
    }
    steps
}

fn build_plan_first_planner_prompt(user_prompt: &str, max_plan_steps: usize) -> String {
    format!(
        "ORCHESTRATOR_PLANNER_PHASE\nYou are operating in plan-first orchestration mode.\nCreate a numbered implementation plan with at most {} steps.\nUse exactly one line per step in the format '1. <step>'.\nDo not execute anything.\n\nUser request:\n{}",
        max_plan_steps, user_prompt
    )
}

fn build_plan_first_execution_prompt(user_prompt: &str, plan_steps: &[String]) -> String {
    let numbered_steps = render_numbered_plan_steps(plan_steps);
    format!(
        "ORCHESTRATOR_EXECUTION_PHASE\nExecute the user request using the approved plan.\n\nApproved plan:\n{}\n\nUser request:\n{}\n\nProvide the final response.",
        numbered_steps, user_prompt
    )
}

fn build_plan_first_delegated_step_prompt(
    user_prompt: &str,
    plan_steps: &[String],
    step_index: usize,
    step: &str,
    policy_context: &str,
) -> String {
    let numbered_steps = render_numbered_plan_steps(plan_steps);
    format!(
        "ORCHESTRATOR_DELEGATED_STEP_PHASE\nYou are executing one delegated plan step in plan-first mode.\nFocus only on the assigned step and produce useful progress for that step.\n\nApproved plan:\n{}\n\nAssigned step ({} of {}):\n{}. {}\n\nUser request:\n{}\n\nInherited execution policy (must be preserved):\n{}\n\nReturn concise output for this delegated step.",
        numbered_steps,
        step_index + 1,
        plan_steps.len(),
        step_index + 1,
        step,
        user_prompt,
        policy_context
    )
}

fn build_plan_first_consolidation_prompt(
    user_prompt: &str,
    plan_steps: &[String],
    delegated_outputs: &[String],
) -> String {
    let numbered_steps = render_numbered_plan_steps(plan_steps);
    let delegated_section = delegated_outputs
        .iter()
        .enumerate()
        .map(|(index, output)| format!("Step {} output:\n{}", index + 1, output.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "ORCHESTRATOR_CONSOLIDATION_PHASE\nSynthesize a final response from delegated step outputs.\n\nApproved plan:\n{}\n\nDelegated outputs:\n{}\n\nUser request:\n{}\n\nProvide the final response.",
        numbered_steps, delegated_section, user_prompt
    )
}

fn render_numbered_plan_steps(plan_steps: &[String]) -> String {
    plan_steps
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {}", index + 1, step))
        .collect::<Vec<_>>()
        .join("\n")
}

fn count_reviewed_plan_steps(plan_steps: &[String], execution_text: &str) -> usize {
    let normalized_execution = execution_text.to_ascii_lowercase();
    plan_steps
        .iter()
        .filter(|step| {
            let tokens = step_review_tokens(step);
            if tokens.is_empty() {
                return normalized_execution.contains(step.trim().to_ascii_lowercase().as_str());
            }
            tokens
                .iter()
                .any(|token| normalized_execution.contains(token.as_str()))
        })
        .count()
}

fn step_review_tokens(step: &str) -> Vec<String> {
    step.split(|character: char| !character.is_ascii_alphanumeric())
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| token.len() >= 4)
        .collect()
}

fn executor_response_within_budget(response_chars: usize, max_response_chars: usize) -> bool {
    response_chars <= max_response_chars
}

fn flatten_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        build_plan_first_consolidation_prompt, build_plan_first_delegated_step_prompt,
        count_reviewed_plan_steps, executor_response_within_budget, parse_numbered_plan_steps,
    };

    #[test]
    fn unit_parse_numbered_plan_steps_extracts_dot_and_paren_prefixes() {
        let steps = parse_numbered_plan_steps(
            "1. Inspect current behavior\n2) Design fix\n3. Add tests\nDone",
        );
        assert_eq!(
            steps,
            vec![
                "Inspect current behavior".to_string(),
                "Design fix".to_string(),
                "Add tests".to_string(),
            ]
        );
    }

    #[test]
    fn regression_parse_numbered_plan_steps_ignores_unstructured_lines() {
        let steps = parse_numbered_plan_steps("- inspect\n* patch\nstep three");
        assert!(steps.is_empty());
    }

    #[test]
    fn unit_build_plan_first_delegated_step_prompt_contains_step_metadata() {
        let prompt = build_plan_first_delegated_step_prompt(
            "ship feature",
            &["Inspect constraints".to_string(), "Apply fix".to_string()],
            1,
            "Apply fix",
            "preset=balanced;max_command_length=4096",
        );
        assert!(prompt.contains("ORCHESTRATOR_DELEGATED_STEP_PHASE"));
        assert!(prompt.contains("Assigned step (2 of 2)"));
        assert!(prompt.contains("2. Apply fix"));
        assert!(prompt.contains("Inherited execution policy"));
        assert!(prompt.contains("preset=balanced;max_command_length=4096"));
    }

    #[test]
    fn unit_build_plan_first_consolidation_prompt_includes_delegated_outputs() {
        let prompt = build_plan_first_consolidation_prompt(
            "ship feature",
            &["Inspect constraints".to_string(), "Apply fix".to_string()],
            &[
                "constraints reviewed".to_string(),
                "patch applied".to_string(),
            ],
        );
        assert!(prompt.contains("ORCHESTRATOR_CONSOLIDATION_PHASE"));
        assert!(prompt.contains("Step 1 output"));
        assert!(prompt.contains("Step 2 output"));
        assert!(prompt.contains("patch applied"));
    }

    #[test]
    fn unit_count_reviewed_plan_steps_matches_token_overlap_deterministically() {
        let plan_steps = vec![
            "Inspect constraints".to_string(),
            "Apply change".to_string(),
            "Run verification tests".to_string(),
        ];
        let execution_text =
            "Applied change after inspecting constraints, then verification tests passed.";
        assert_eq!(count_reviewed_plan_steps(&plan_steps, execution_text), 3);
        assert_eq!(
            count_reviewed_plan_steps(&plan_steps, "no related content"),
            0
        );
    }

    #[test]
    fn unit_executor_response_within_budget_respects_boundary() {
        assert!(executor_response_within_budget(24, 24));
        assert!(executor_response_within_budget(12, 24));
        assert!(!executor_response_within_budget(25, 24));
    }
}
