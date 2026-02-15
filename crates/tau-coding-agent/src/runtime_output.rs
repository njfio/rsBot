//! Runtime output rendering and event stream helpers.
//!
//! Converts agent/runtime events into user-facing output while preserving
//! deterministic formatting and error context for diagnostics.

use anyhow::Result;
use tau_agent_core::AgentEvent;
use tau_ai::Message;
use tau_session::SessionRuntime;

use crate::runtime_types::RenderOptions;

pub(crate) fn persist_messages(
    session_runtime: &mut Option<SessionRuntime>,
    new_messages: &[Message],
) -> Result<()> {
    tau_runtime::runtime_output_runtime::persist_messages(session_runtime, new_messages)
}

pub(crate) fn print_assistant_messages(
    messages: &[Message],
    render_options: RenderOptions,
    suppress_first_streamed_text: bool,
) {
    tau_runtime::runtime_output_runtime::print_assistant_messages(
        messages,
        render_options.stream_output,
        render_options.stream_delay_ms,
        suppress_first_streamed_text,
    );
}

#[cfg(test)]
pub(crate) fn stream_text_chunks(text: &str) -> Vec<&str> {
    tau_runtime::runtime_output_runtime::stream_text_chunks(text)
}

pub(crate) fn event_to_json(event: &AgentEvent) -> serde_json::Value {
    tau_runtime::runtime_output_runtime::event_to_json(event)
}
