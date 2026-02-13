//! GitHub Issues bridge runtime for Tau.
//!
//! Exposes the GitHub transport bridge runtime and its configuration as a
//! standalone crate dependency used by `tau-coding-agent`.

use std::{
    future::Future,
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Result;
use tau_agent_core::{Agent, AgentError, CooperativeCancellationToken};
use tau_ai::StreamDeltaHandler;
use tau_session::SessionRuntime;
use tokio::sync::mpsc;

pub mod github_issues_runtime;

pub use github_issues_runtime::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
pub use tau_access::pairing::{
    evaluate_pairing_access, pairing_policy_for_state_dir, PairingDecision,
};
pub use tau_access::rbac::{
    authorize_action_for_principal_with_policy_path, github_principal,
    rbac_policy_path_for_state_dir, RbacDecision,
};
pub use tau_core::{current_unix_timestamp_ms, write_text_atomic};
pub use tau_ops::{
    execute_canvas_command, CanvasCommandConfig, CanvasEventOrigin, CanvasSessionLinkContext,
};
pub use tau_provider::{AuthCommandConfig, CredentialStoreEncryptionMode, ProviderAuthMethod};
pub use tau_runtime::TransportHealthSnapshot;
pub use tau_session::{session_message_preview, session_message_role};
pub use tau_startup::runtime_types::RenderOptions;
pub use tau_startup::runtime_types::{DoctorCommandConfig, DoctorMultiChannelReadinessConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

pub async fn run_prompt_with_cancellation<F>(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    cancellation_signal: F,
    render_options: RenderOptions,
) -> Result<PromptRunStatus>
where
    F: Future,
{
    let checkpoint = agent.messages().to_vec();
    let cancellation_token = CooperativeCancellationToken::new();
    agent.set_cancellation_token(Some(cancellation_token.clone()));
    let streamed_output = Arc::new(AtomicBool::new(false));
    let (stream_delta_handler, mut stream_task) = if render_options.stream_output {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let streamed_output = Arc::clone(&streamed_output);
        let stream_delay_ms = render_options.stream_delay_ms;
        let task = tokio::spawn(async move {
            while let Some(delta) = rx.recv().await {
                if delta.is_empty() {
                    continue;
                }
                streamed_output.store(true, Ordering::Relaxed);
                print!("{delta}");
                let _ = std::io::stdout().flush();
                if stream_delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(stream_delay_ms)).await;
                }
            }
        });
        (
            Some(Arc::new(move |delta: String| {
                let _ = tx.send(delta);
            }) as StreamDeltaHandler),
            Some(task),
        )
    } else {
        (None, None)
    };
    tokio::pin!(cancellation_signal);

    enum PromptOutcome<T> {
        Result(T),
        Cancelled,
        TimedOut,
    }

    let prompt_result = {
        let mut prompt_future =
            std::pin::pin!(agent.prompt_with_stream(prompt, stream_delta_handler.clone()));
        if turn_timeout_ms == 0 {
            tokio::select! {
                result = &mut prompt_future => PromptOutcome::Result(result),
                _ = &mut cancellation_signal => {
                    cancellation_token.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut prompt_future).await;
                    PromptOutcome::Cancelled
                },
            }
        } else {
            let timeout = tokio::time::sleep(Duration::from_millis(turn_timeout_ms));
            tokio::pin!(timeout);
            tokio::select! {
                result = &mut prompt_future => PromptOutcome::Result(result),
                _ = &mut cancellation_signal => {
                    cancellation_token.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut prompt_future).await;
                    PromptOutcome::Cancelled
                },
                _ = &mut timeout => {
                    cancellation_token.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut prompt_future).await;
                    PromptOutcome::TimedOut
                },
            }
        }
    };
    agent.set_cancellation_token(None);

    drop(stream_delta_handler);
    if let Some(task) = stream_task.take() {
        let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
    }

    let prompt_result = match prompt_result {
        PromptOutcome::Result(result) => result,
        PromptOutcome::Cancelled => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::Cancelled);
        }
        PromptOutcome::TimedOut => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::TimedOut);
        }
    };

    let new_messages = match prompt_result {
        Ok(messages) => messages,
        Err(AgentError::Cancelled) => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::Cancelled);
        }
        Err(error) => return Err(error.into()),
    };
    tau_runtime::persist_messages(session_runtime, &new_messages)?;
    tau_runtime::print_assistant_messages(
        &new_messages,
        render_options.stream_output,
        render_options.stream_delay_ms,
        streamed_output.load(Ordering::Relaxed),
    );
    Ok(PromptRunStatus::Completed)
}

mod auth_commands {
    pub use tau_provider::{
        execute_auth_command, parse_auth_command, AuthCommand, AUTH_MATRIX_USAGE, AUTH_STATUS_USAGE,
    };
}

mod channel_store {
    pub use tau_runtime::{
        ChannelArtifactRecord, ChannelAttachmentRecord, ChannelLogEntry, ChannelStore,
    };
}

mod runtime_types {
    pub use tau_startup::runtime_types::{AuthCommandConfig, DoctorCommandConfig};
}

mod tools {
    pub use tau_tools::tools::{register_builtin_tools, ToolPolicy};
}
