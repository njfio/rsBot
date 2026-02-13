use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use tau_agent_core::Agent;
use tau_ai::MessageRole;
use tau_orchestrator::{
    OrchestratorPromptRunStatus, OrchestratorRenderOptions, OrchestratorRuntime,
};
use tau_session::SessionRuntime;

use crate::multi_agent_router::MultiAgentRouteTable;
use crate::runtime_loop::run_prompt_with_cancellation;
use crate::runtime_types::RenderOptions;

struct OrchestratorRuntimeAdapter<'a> {
    agent: &'a mut Agent,
    session_runtime: &'a mut Option<SessionRuntime>,
}

#[async_trait(?Send)]
impl OrchestratorRuntime for OrchestratorRuntimeAdapter<'_> {
    async fn run_prompt_with_cancellation(
        &mut self,
        prompt: &str,
        turn_timeout_ms: u64,
        render_options: OrchestratorRenderOptions,
    ) -> Result<OrchestratorPromptRunStatus> {
        let status = run_prompt_with_cancellation(
            self.agent,
            self.session_runtime,
            prompt,
            turn_timeout_ms,
            tokio::signal::ctrl_c(),
            RenderOptions {
                stream_output: render_options.stream_output,
                stream_delay_ms: render_options.stream_delay_ms,
            },
        )
        .await?;
        Ok(match status {
            crate::runtime_loop::PromptRunStatus::Completed => {
                OrchestratorPromptRunStatus::Completed
            }
            crate::runtime_loop::PromptRunStatus::Cancelled => {
                OrchestratorPromptRunStatus::Cancelled
            }
            crate::runtime_loop::PromptRunStatus::TimedOut => OrchestratorPromptRunStatus::TimedOut,
        })
    }

    fn latest_assistant_text(&self) -> Option<String> {
        self.agent
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::Assistant)
            .map(|message| message.text_content())
    }

    fn report_prompt_status(&self, status: OrchestratorPromptRunStatus) {
        if status == OrchestratorPromptRunStatus::Cancelled {
            println!("\nrequest cancelled\n");
        } else if status == OrchestratorPromptRunStatus::TimedOut {
            println!("\nrequest timed out\n");
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_plan_first_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    max_plan_steps: usize,
    max_delegated_steps: usize,
    max_executor_response_chars: usize,
    max_delegated_step_response_chars: usize,
    max_delegated_total_response_chars: usize,
    delegate_steps: bool,
) -> Result<()> {
    let mut adapter = OrchestratorRuntimeAdapter {
        agent,
        session_runtime,
    };
    tau_orchestrator::run_plan_first_prompt(
        &mut adapter,
        user_prompt,
        turn_timeout_ms,
        OrchestratorRenderOptions {
            stream_output: render_options.stream_output,
            stream_delay_ms: render_options.stream_delay_ms,
        },
        max_plan_steps,
        max_delegated_steps,
        max_executor_response_chars,
        max_delegated_step_response_chars,
        max_delegated_total_response_chars,
        delegate_steps,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_plan_first_prompt_with_policy_context(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    max_plan_steps: usize,
    max_delegated_steps: usize,
    max_executor_response_chars: usize,
    max_delegated_step_response_chars: usize,
    max_delegated_total_response_chars: usize,
    delegate_steps: bool,
    delegated_policy_context: Option<&str>,
) -> Result<()> {
    let mut adapter = OrchestratorRuntimeAdapter {
        agent,
        session_runtime,
    };
    tau_orchestrator::run_plan_first_prompt_with_policy_context(
        &mut adapter,
        user_prompt,
        turn_timeout_ms,
        OrchestratorRenderOptions {
            stream_output: render_options.stream_output,
            stream_delay_ms: render_options.stream_delay_ms,
        },
        max_plan_steps,
        max_delegated_steps,
        max_executor_response_chars,
        max_delegated_step_response_chars,
        max_delegated_total_response_chars,
        delegate_steps,
        delegated_policy_context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_plan_first_prompt_with_policy_context_and_routing(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    user_prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
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
    let mut adapter = OrchestratorRuntimeAdapter {
        agent,
        session_runtime,
    };
    tau_orchestrator::run_plan_first_prompt_with_policy_context_and_routing(
        &mut adapter,
        user_prompt,
        turn_timeout_ms,
        OrchestratorRenderOptions {
            stream_output: render_options.stream_output,
            stream_delay_ms: render_options.stream_delay_ms,
        },
        max_plan_steps,
        max_delegated_steps,
        max_executor_response_chars,
        max_delegated_step_response_chars,
        max_delegated_total_response_chars,
        delegate_steps,
        delegated_policy_context,
        route_table,
        route_trace_log_path,
    )
    .await
}
