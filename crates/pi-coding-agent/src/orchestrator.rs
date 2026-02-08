use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_plan_first_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    max_plan_steps: usize,
    max_executor_response_chars: usize,
    max_delegated_step_response_chars: usize,
    max_delegated_total_response_chars: usize,
    delegate_steps: bool,
) -> Result<()> {
    let planner_prompt = build_plan_first_planner_prompt(user_prompt, max_plan_steps);
    let planner_render_options = RenderOptions {
        stream_output: false,
        stream_delay_ms: 0,
    };
    let planner_status = run_prompt_with_cancellation(
        agent,
        session_runtime,
        &planner_prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        planner_render_options,
    )
    .await?;
    report_prompt_status_internal(planner_status);
    if planner_status != PromptRunStatus::Completed {
        return Ok(());
    }

    let plan_text = latest_assistant_text(agent).ok_or_else(|| {
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
        println!(
            "orchestrator trace: mode=plan-first phase=executor strategy=delegated-steps total_steps={}",
            plan_steps.len()
        );
        let mut delegated_outputs = Vec::new();
        let mut delegated_total_response_chars = 0usize;
        for (index, step) in plan_steps.iter().enumerate() {
            println!(
                "orchestrator trace: mode=plan-first phase=delegated-step step={} action=start text={}",
                index + 1,
                flatten_whitespace(step)
            );
            let delegated_prompt =
                build_plan_first_delegated_step_prompt(user_prompt, &plan_steps, index, step);
            let delegated_status = run_prompt_with_cancellation(
                agent,
                session_runtime,
                &delegated_prompt,
                turn_timeout_ms,
                tokio::signal::ctrl_c(),
                planner_render_options,
            )
            .await?;
            report_prompt_status_internal(delegated_status);
            if delegated_status != PromptRunStatus::Completed {
                return Ok(());
            }
            let delegated_text = latest_assistant_text(agent).ok_or_else(|| {
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

    let execution_status = run_prompt_with_cancellation(
        agent,
        session_runtime,
        &execution_prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        render_options,
    )
    .await?;
    report_prompt_status_internal(execution_status);
    if execution_status != PromptRunStatus::Completed {
        return Ok(());
    }

    let execution_phase_label = if delegate_steps {
        "consolidation"
    } else {
        "executor"
    };
    let execution_text = latest_assistant_text(agent).ok_or_else(|| {
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

pub(crate) fn parse_numbered_plan_steps(plan: &str) -> Vec<String> {
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

fn latest_assistant_text(agent: &Agent) -> Option<String> {
    agent
        .messages()
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(|message| message.text_content())
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
) -> String {
    let numbered_steps = render_numbered_plan_steps(plan_steps);
    format!(
        "ORCHESTRATOR_DELEGATED_STEP_PHASE\nYou are executing one delegated plan step in plan-first mode.\nFocus only on the assigned step and produce useful progress for that step.\n\nApproved plan:\n{}\n\nAssigned step ({} of {}):\n{}. {}\n\nUser request:\n{}\n\nReturn concise output for this delegated step.",
        numbered_steps,
        step_index + 1,
        plan_steps.len(),
        step_index + 1,
        step,
        user_prompt
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

fn report_prompt_status_internal(status: PromptRunStatus) {
    if status == PromptRunStatus::Cancelled {
        println!("\nrequest cancelled\n");
    } else if status == PromptRunStatus::TimedOut {
        println!("\nrequest timed out\n");
    }
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
        );
        assert!(prompt.contains("ORCHESTRATOR_DELEGATED_STEP_PHASE"));
        assert!(prompt.contains("Assigned step (2 of 2)"));
        assert!(prompt.contains("2. Apply fix"));
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
