use super::*;

pub(crate) async fn run_plan_first_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    max_plan_steps: usize,
    max_executor_response_chars: usize,
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
    println!("orchestrator trace: mode=plan-first phase=executor");

    let execution_prompt = build_plan_first_execution_prompt(user_prompt, &plan_steps);
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

    let execution_text = latest_assistant_text(agent).ok_or_else(|| {
        anyhow!("plan-first orchestrator failed: executor produced no text output")
    })?;
    if execution_text.trim().is_empty() {
        bail!("plan-first orchestrator failed: executor produced no text output");
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
            "plan-first orchestrator failed: executor response exceeded budget (chars {} > max {})",
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
    let numbered_steps = plan_steps
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {}", index + 1, step))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "ORCHESTRATOR_EXECUTION_PHASE\nExecute the user request using the approved plan.\n\nApproved plan:\n{}\n\nUser request:\n{}\n\nProvide the final response.",
        numbered_steps, user_prompt
    )
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
