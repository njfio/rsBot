use std::{
    future::Future,
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use pi_agent_core::Agent;
use pi_ai::StreamDeltaHandler;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{
    ensure_non_empty_text, handle_command_with_session_import_mode, persist_messages,
    print_assistant_messages, Cli, CommandAction, CommandExecutionContext, RenderOptions,
    SessionRuntime,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Copy)]
pub(crate) struct InteractiveRuntimeConfig<'a> {
    pub(crate) turn_timeout_ms: u64,
    pub(crate) render_options: RenderOptions,
    pub(crate) command_context: CommandExecutionContext<'a>,
}

pub(crate) async fn run_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
) -> Result<()> {
    let status = run_prompt_with_cancellation(
        agent,
        session_runtime,
        prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        render_options,
    )
    .await?;
    report_prompt_status(status);
    Ok(())
}

pub(crate) async fn run_interactive(
    mut agent: Agent,
    mut session_runtime: Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        print!("pi> ");
        std::io::stdout()
            .flush()
            .context("failed to flush stdout")?;

        let Some(line) = lines.next_line().await? else {
            break;
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('/') {
            if handle_command_with_session_import_mode(
                trimmed,
                &mut agent,
                &mut session_runtime,
                config.command_context.tool_policy_json,
                config.command_context.session_import_mode,
                config.command_context.profile_defaults,
                config.command_context.skills_command_config,
                config.command_context.auth_command_config,
            )? == CommandAction::Exit
            {
                break;
            }
            continue;
        }

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut session_runtime,
            trimmed,
            config.turn_timeout_ms,
            tokio::signal::ctrl_c(),
            config.render_options,
        )
        .await?;
        report_prompt_status(status);
    }

    Ok(())
}

pub(crate) async fn run_prompt_with_cancellation<F>(
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
    let streamed_output = Arc::new(AtomicBool::new(false));
    let stream_delta_handler = if render_options.stream_output {
        let streamed_output = streamed_output.clone();
        let stream_delay_ms = render_options.stream_delay_ms;
        Some(Arc::new(move |delta: String| {
            if delta.is_empty() {
                return;
            }
            streamed_output.store(true, Ordering::Relaxed);
            print!("{delta}");
            let _ = std::io::stdout().flush();
            if stream_delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(stream_delay_ms));
            }
        }) as StreamDeltaHandler)
    } else {
        None
    };
    tokio::pin!(cancellation_signal);

    enum PromptOutcome<T> {
        Result(T),
        Cancelled,
        TimedOut,
    }

    let prompt_result = if turn_timeout_ms == 0 {
        tokio::select! {
            result = agent.prompt_with_stream(prompt, stream_delta_handler.clone()) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
        }
    } else {
        let timeout = tokio::time::sleep(Duration::from_millis(turn_timeout_ms));
        tokio::pin!(timeout);
        tokio::select! {
            result = agent.prompt_with_stream(prompt, stream_delta_handler.clone()) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
            _ = &mut timeout => PromptOutcome::TimedOut,
        }
    };

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

    let new_messages = prompt_result?;
    persist_messages(session_runtime, &new_messages)?;
    print_assistant_messages(
        &new_messages,
        render_options,
        streamed_output.load(Ordering::Relaxed),
    );
    Ok(PromptRunStatus::Completed)
}

pub(crate) fn resolve_prompt_input(cli: &Cli) -> Result<Option<String>> {
    if let Some(prompt) = &cli.prompt {
        return Ok(Some(prompt.clone()));
    }

    let Some(path) = cli.prompt_file.as_ref() else {
        return Ok(None);
    };

    if path == std::path::Path::new("-") {
        let mut prompt = String::new();
        std::io::stdin()
            .read_to_string(&mut prompt)
            .context("failed to read prompt from stdin")?;
        return Ok(Some(ensure_non_empty_text(
            prompt,
            "stdin prompt".to_string(),
        )?));
    }

    let prompt = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read prompt file {}", path.display()))?;

    Ok(Some(ensure_non_empty_text(
        prompt,
        format!("prompt file {}", path.display()),
    )?))
}

fn report_prompt_status(status: PromptRunStatus) {
    if status == PromptRunStatus::Cancelled {
        println!("\nrequest cancelled\n");
    } else if status == PromptRunStatus::TimedOut {
        println!("\nrequest timed out\n");
    }
}
