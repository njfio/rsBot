//! Bridge adapters between coding-agent runtime and orchestrator services.
//!
//! This module wires orchestrator-facing contracts into local runtime flows and
//! preserves explicit error propagation at the bridge boundary.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use tau_agent_core::Agent;
use tau_ai::MessageRole;
use tau_orchestrator::{
    OrchestratorPromptRunStatus, OrchestratorRenderOptions, OrchestratorRuntime,
    PlanFirstPromptPolicyRequest as OrchestratorPlanFirstPromptPolicyRequest,
    PlanFirstPromptRequest as OrchestratorPlanFirstPromptRequest,
    PlanFirstPromptRoutingRequest as OrchestratorPlanFirstPromptRoutingRequest,
};
use tau_session::SessionRuntime;

use crate::multi_agent_router::MultiAgentRouteTable;
use crate::runtime_loop::run_prompt_with_cancellation;
use crate::runtime_types::RenderOptions;

struct OrchestratorRuntimeAdapter<'a> {
    agent: &'a mut Agent,
    session_runtime: &'a mut Option<SessionRuntime>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PlanFirstPromptRequest<'a> {
    pub user_prompt: &'a str,
    pub turn_timeout_ms: u64,
    pub render_options: RenderOptions,
    pub max_plan_steps: usize,
    pub max_delegated_steps: usize,
    pub max_executor_response_chars: usize,
    pub max_delegated_step_response_chars: usize,
    pub max_delegated_total_response_chars: usize,
    pub delegate_steps: bool,
    pub delegated_skill_context: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PlanFirstPromptPolicyRequest<'a> {
    pub user_prompt: &'a str,
    pub turn_timeout_ms: u64,
    pub render_options: RenderOptions,
    pub max_plan_steps: usize,
    pub max_delegated_steps: usize,
    pub max_executor_response_chars: usize,
    pub max_delegated_step_response_chars: usize,
    pub max_delegated_total_response_chars: usize,
    pub delegate_steps: bool,
    pub delegated_policy_context: Option<&'a str>,
    pub delegated_skill_context: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PlanFirstPromptRoutingRequest<'a> {
    pub user_prompt: &'a str,
    pub turn_timeout_ms: u64,
    pub render_options: RenderOptions,
    pub max_plan_steps: usize,
    pub max_delegated_steps: usize,
    pub max_executor_response_chars: usize,
    pub max_delegated_step_response_chars: usize,
    pub max_delegated_total_response_chars: usize,
    pub delegate_steps: bool,
    pub delegated_policy_context: Option<&'a str>,
    pub delegated_skill_context: Option<&'a str>,
    pub route_table: &'a MultiAgentRouteTable,
    pub route_trace_log_path: Option<&'a Path>,
}

fn map_render_options(render_options: RenderOptions) -> OrchestratorRenderOptions {
    OrchestratorRenderOptions {
        stream_output: render_options.stream_output,
        stream_delay_ms: render_options.stream_delay_ms,
    }
}

#[async_trait(?Send)]
impl OrchestratorRuntime for OrchestratorRuntimeAdapter<'_> {
    async fn run_prompt_with_cancellation(
        &mut self,
        prompt: &str,
        model_override: Option<&str>,
        turn_timeout_ms: u64,
        render_options: OrchestratorRenderOptions,
    ) -> Result<OrchestratorPromptRunStatus> {
        let agent = &mut *self.agent;
        let session_runtime = &mut *self.session_runtime;
        let mapped_render_options = RenderOptions {
            stream_output: render_options.stream_output,
            stream_delay_ms: render_options.stream_delay_ms,
        };
        let normalized_model_override = model_override
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let previous_model = normalized_model_override
            .map(|model_override| agent.swap_dispatch_model(model_override.to_string()));
        let prompt_status = run_prompt_with_cancellation(
            agent,
            session_runtime,
            prompt,
            turn_timeout_ms,
            tokio::signal::ctrl_c(),
            mapped_render_options,
        )
        .await;
        if let Some(previous_model) = previous_model {
            agent.restore_dispatch_model(previous_model);
        }
        let status = prompt_status?;
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

pub(crate) async fn run_plan_first_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    request: PlanFirstPromptRequest<'_>,
) -> Result<()> {
    let mut adapter = OrchestratorRuntimeAdapter {
        agent,
        session_runtime,
    };
    tau_orchestrator::run_plan_first_prompt(
        &mut adapter,
        OrchestratorPlanFirstPromptRequest {
            user_prompt: request.user_prompt,
            turn_timeout_ms: request.turn_timeout_ms,
            render_options: map_render_options(request.render_options),
            max_plan_steps: request.max_plan_steps,
            max_delegated_steps: request.max_delegated_steps,
            max_executor_response_chars: request.max_executor_response_chars,
            max_delegated_step_response_chars: request.max_delegated_step_response_chars,
            max_delegated_total_response_chars: request.max_delegated_total_response_chars,
            delegate_steps: request.delegate_steps,
            delegated_skill_context: request.delegated_skill_context,
        },
    )
    .await
}

pub(crate) async fn run_plan_first_prompt_with_policy_context(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    request: PlanFirstPromptPolicyRequest<'_>,
) -> Result<()> {
    let mut adapter = OrchestratorRuntimeAdapter {
        agent,
        session_runtime,
    };
    tau_orchestrator::run_plan_first_prompt_with_policy_context(
        &mut adapter,
        OrchestratorPlanFirstPromptPolicyRequest {
            user_prompt: request.user_prompt,
            turn_timeout_ms: request.turn_timeout_ms,
            render_options: map_render_options(request.render_options),
            max_plan_steps: request.max_plan_steps,
            max_delegated_steps: request.max_delegated_steps,
            max_executor_response_chars: request.max_executor_response_chars,
            max_delegated_step_response_chars: request.max_delegated_step_response_chars,
            max_delegated_total_response_chars: request.max_delegated_total_response_chars,
            delegate_steps: request.delegate_steps,
            delegated_policy_context: request.delegated_policy_context,
            delegated_skill_context: request.delegated_skill_context,
        },
    )
    .await
}

pub(crate) async fn run_plan_first_prompt_with_policy_context_and_routing(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    request: PlanFirstPromptRoutingRequest<'_>,
) -> Result<()> {
    let mut adapter = OrchestratorRuntimeAdapter {
        agent,
        session_runtime,
    };
    tau_orchestrator::run_plan_first_prompt_with_policy_context_and_routing(
        &mut adapter,
        OrchestratorPlanFirstPromptRoutingRequest {
            user_prompt: request.user_prompt,
            turn_timeout_ms: request.turn_timeout_ms,
            render_options: map_render_options(request.render_options),
            max_plan_steps: request.max_plan_steps,
            max_delegated_steps: request.max_delegated_steps,
            max_executor_response_chars: request.max_executor_response_chars,
            max_delegated_step_response_chars: request.max_delegated_step_response_chars,
            max_delegated_total_response_chars: request.max_delegated_total_response_chars,
            delegate_steps: request.delegate_steps,
            delegated_policy_context: request.delegated_policy_context,
            delegated_skill_context: request.delegated_skill_context,
            route_table: request.route_table,
            route_trace_log_path: request.route_trace_log_path,
        },
    )
    .await
}
