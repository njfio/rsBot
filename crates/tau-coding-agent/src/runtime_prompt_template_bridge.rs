//! Runtime prompt-template hot-reload bridge for local runtime turns.
//!
//! This bridge monitors workspace startup template edits and refreshes the local
//! runtime agent system prompt without requiring process restart.

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use tau_agent_core::Agent;
use tau_cli::Cli;
use tau_onboarding::onboarding_paths::resolve_tau_root;
use tau_onboarding::startup_prompt_composition::{
    compose_startup_system_prompt_with_report, StartupPromptTemplateSource,
};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

const PROMPT_TEMPLATE_BRIDGE_POLL_INTERVAL_MS: u64 = 250;
const STARTUP_SYSTEM_PROMPT_TEMPLATE_RELATIVE_PATH: &str = "prompts/system.md.j2";

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptTemplateFingerprint {
    exists: bool,
    len: Option<u64>,
    modified: Option<SystemTime>,
}

impl PromptTemplateFingerprint {
    fn read(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(metadata) => Self {
                exists: true,
                len: Some(metadata.len()),
                modified: metadata.modified().ok(),
            },
            Err(_) => Self {
                exists: false,
                len: None,
                modified: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptTemplateBridgeOutcome {
    Applied {
        system_prompt: String,
        template_path: PathBuf,
    },
    NoChange {
        diagnostic: String,
    },
    Invalid {
        diagnostic: String,
    },
    MissingTemplate {
        template_path: PathBuf,
    },
}

impl PromptTemplateBridgeOutcome {
    pub(crate) fn reason_code(&self) -> &'static str {
        match self {
            Self::Applied { .. } => "prompt_template_bridge_applied",
            Self::NoChange { .. } => "prompt_template_bridge_no_change",
            Self::Invalid { .. } => "prompt_template_bridge_invalid",
            Self::MissingTemplate { .. } => "prompt_template_bridge_missing_template",
        }
    }
}

#[derive(Debug)]
struct RuntimePromptTemplateHotReloadBridge {
    template_path: PathBuf,
    last_template_fingerprint: Option<PromptTemplateFingerprint>,
    last_applied_system_prompt: String,
}

impl RuntimePromptTemplateHotReloadBridge {
    fn new(template_path: PathBuf, initial_system_prompt: String) -> Self {
        Self {
            template_path,
            last_template_fingerprint: None,
            last_applied_system_prompt: initial_system_prompt,
        }
    }

    fn evaluate_if_changed(
        &mut self,
        force: bool,
        cli: &Cli,
        skills_dir: &Path,
    ) -> PromptTemplateBridgeOutcome {
        let current_fingerprint = PromptTemplateFingerprint::read(&self.template_path);
        let changed = self
            .last_template_fingerprint
            .as_ref()
            .map(|previous| previous != &current_fingerprint)
            .unwrap_or(true);
        self.last_template_fingerprint = Some(current_fingerprint);

        if !force && !changed {
            return PromptTemplateBridgeOutcome::NoChange {
                diagnostic: "template_fingerprint_unchanged".to_string(),
            };
        }

        self.evaluate_rendered_prompt(cli, skills_dir)
    }

    fn evaluate_rendered_prompt(
        &mut self,
        cli: &Cli,
        skills_dir: &Path,
    ) -> PromptTemplateBridgeOutcome {
        let composition = match compose_startup_system_prompt_with_report(cli, skills_dir) {
            Ok(composition) => composition,
            Err(error) => {
                return PromptTemplateBridgeOutcome::Invalid {
                    diagnostic: format!("startup_prompt_composition_failed: {error:#}"),
                };
            }
        };

        let reason_code = composition.template_report.reason_code.clone();
        match composition.template_report.source {
            StartupPromptTemplateSource::Workspace => {
                if composition.system_prompt == self.last_applied_system_prompt {
                    PromptTemplateBridgeOutcome::NoChange {
                        diagnostic: "rendered_prompt_unchanged".to_string(),
                    }
                } else {
                    self.last_applied_system_prompt = composition.system_prompt.clone();
                    PromptTemplateBridgeOutcome::Applied {
                        system_prompt: composition.system_prompt,
                        template_path: self.template_path.clone(),
                    }
                }
            }
            StartupPromptTemplateSource::BuiltIn => {
                if reason_code == "workspace_template_missing_fallback_builtin" {
                    PromptTemplateBridgeOutcome::MissingTemplate {
                        template_path: self.template_path.clone(),
                    }
                } else {
                    PromptTemplateBridgeOutcome::Invalid {
                        diagnostic: format!(
                            "workspace_template_not_applied: source=built_in reason_code={reason_code}"
                        ),
                    }
                }
            }
            StartupPromptTemplateSource::DefaultFallback => PromptTemplateBridgeOutcome::Invalid {
                diagnostic: format!(
                    "workspace_template_not_applied: source=default_fallback reason_code={reason_code}"
                ),
            },
        }
    }
}

#[derive(Debug)]
pub(crate) struct RuntimePromptTemplateHotReloadBridgeHandle {
    bridge: RuntimePromptTemplateHotReloadBridge,
    poll_pending: Arc<AtomicBool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl RuntimePromptTemplateHotReloadBridgeHandle {
    pub(crate) fn evaluate_and_apply(
        &mut self,
        agent: &mut Agent,
        cli: &Cli,
        skills_dir: &Path,
        force: bool,
    ) -> Result<PromptTemplateBridgeOutcome> {
        let should_evaluate = force || self.poll_pending.swap(false, Ordering::Relaxed);
        if !should_evaluate {
            return Ok(PromptTemplateBridgeOutcome::NoChange {
                diagnostic: "poll_interval_not_elapsed".to_string(),
            });
        }

        let outcome = self.bridge.evaluate_if_changed(force, cli, skills_dir);
        if let PromptTemplateBridgeOutcome::Applied { system_prompt, .. } = &outcome {
            let _ = agent.replace_system_prompt(system_prompt.clone());
        }

        let should_emit = match &outcome {
            PromptTemplateBridgeOutcome::NoChange { diagnostic } => {
                diagnostic != "template_fingerprint_unchanged"
                    && diagnostic != "poll_interval_not_elapsed"
            }
            _ => true,
        };
        if should_emit {
            emit_bridge_outcome(&outcome);
        }
        Ok(outcome)
    }

    pub(crate) fn apply_pending_update(
        &mut self,
        agent: &mut Agent,
        cli: &Cli,
        skills_dir: &Path,
    ) -> Result<()> {
        let _ = self.evaluate_and_apply(agent, cli, skills_dir, false)?;
        Ok(())
    }

    pub(crate) async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

pub(crate) fn start_runtime_prompt_template_hot_reload_bridge(
    cli: &Cli,
    initial_system_prompt: &str,
) -> Result<RuntimePromptTemplateHotReloadBridgeHandle> {
    let template_path = resolve_tau_root(cli).join(STARTUP_SYSTEM_PROMPT_TEMPLATE_RELATIVE_PATH);
    let bridge =
        RuntimePromptTemplateHotReloadBridge::new(template_path, initial_system_prompt.to_string());
    let poll_pending = Arc::new(AtomicBool::new(true));
    let poll_pending_task = Arc::clone(&poll_pending);

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(
            PROMPT_TEMPLATE_BRIDGE_POLL_INTERVAL_MS,
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    poll_pending_task.store(true, Ordering::Relaxed);
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    Ok(RuntimePromptTemplateHotReloadBridgeHandle {
        bridge,
        poll_pending,
        shutdown_tx: Some(shutdown_tx),
        task: Some(task),
    })
}

fn emit_bridge_outcome(outcome: &PromptTemplateBridgeOutcome) {
    match outcome {
        PromptTemplateBridgeOutcome::Applied {
            template_path,
            system_prompt,
        } => info!(
            reason_code = outcome.reason_code(),
            template_path = %template_path.display(),
            system_prompt_chars = system_prompt.chars().count(),
            "runtime prompt-template bridge applied updated startup prompt"
        ),
        PromptTemplateBridgeOutcome::NoChange { diagnostic } => info!(
            reason_code = outcome.reason_code(),
            diagnostic = %diagnostic,
            "runtime prompt-template bridge observed no effective prompt change"
        ),
        PromptTemplateBridgeOutcome::Invalid { diagnostic } => warn!(
            reason_code = outcome.reason_code(),
            diagnostic = %diagnostic,
            "runtime prompt-template bridge ignored invalid template update"
        ),
        PromptTemplateBridgeOutcome::MissingTemplate { template_path } => info!(
            reason_code = outcome.reason_code(),
            template_path = %template_path.display(),
            "runtime prompt-template bridge missing workspace template"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        start_runtime_prompt_template_hot_reload_bridge, PromptTemplateBridgeOutcome,
        RuntimePromptTemplateHotReloadBridgeHandle,
    };
    use crate::compose_startup_system_prompt;
    use crate::tests::test_cli;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;
    use tau_agent_core::{Agent, AgentConfig};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, Message, TauAiError};
    use tempfile::tempdir;
    use tokio::sync::Mutex as AsyncMutex;

    struct RecordingPromptClient {
        outcomes: AsyncMutex<VecDeque<Result<ChatResponse, TauAiError>>>,
        request_messages: Arc<AsyncMutex<Vec<Vec<Message>>>>,
    }

    #[async_trait]
    impl tau_ai::LlmClient for RecordingPromptClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.request_messages.lock().await.push(request.messages);
            let mut outcomes = self.outcomes.lock().await;
            outcomes.pop_front().unwrap_or_else(|| {
                Ok(ChatResponse {
                    message: Message::assistant_text("ok"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                })
            })
        }
    }

    fn apply_workspace_paths(cli: &mut tau_cli::Cli, workspace: &Path) {
        let tau_root = workspace.join(".tau");
        cli.session = tau_root.join("sessions/default.sqlite");
        cli.credential_store = tau_root.join("credentials.json");
        cli.skills_dir = tau_root.join("skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
    }

    fn write_workspace_template(workspace: &Path, body: &str) {
        let template_path = workspace.join(".tau/prompts/system.md.j2");
        std::fs::create_dir_all(template_path.parent().expect("template parent"))
            .expect("create prompts dir");
        std::fs::write(template_path, body).expect("write workspace template");
    }

    fn start_bridge_for_prompt(
        cli: &tau_cli::Cli,
        initial_system_prompt: &str,
    ) -> RuntimePromptTemplateHotReloadBridgeHandle {
        start_runtime_prompt_template_hot_reload_bridge(cli, initial_system_prompt)
            .expect("start bridge")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn integration_spec_2548_c01_prompt_template_hot_reload_applies_updated_system_prompt_without_restart(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        write_workspace_template(
            temp.path(),
            "PROMPT_V1\nbase={{ base_system_prompt }}\nidentity={{ identity }}\n",
        );

        let initial_system_prompt =
            compose_startup_system_prompt(&cli, &cli.skills_dir).expect("compose initial prompt");
        let request_messages = Arc::new(AsyncMutex::new(Vec::new()));
        let client = Arc::new(RecordingPromptClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(ChatResponse {
                    message: Message::assistant_text("first"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
                Ok(ChatResponse {
                    message: Message::assistant_text("second"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            ])),
            request_messages: Arc::clone(&request_messages),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                system_prompt: initial_system_prompt.clone(),
                ..AgentConfig::default()
            },
        );
        let mut handle = start_bridge_for_prompt(&cli, &initial_system_prompt);

        agent
            .prompt("first turn")
            .await
            .expect("first turn should pass");
        write_workspace_template(
            temp.path(),
            "PROMPT_V2\nbase={{ base_system_prompt }}\nidentity={{ identity }}\n",
        );
        let outcome = handle
            .evaluate_and_apply(&mut agent, &cli, &cli.skills_dir, true)
            .expect("evaluate hot reload");
        assert!(
            matches!(outcome, PromptTemplateBridgeOutcome::Applied { .. }),
            "expected applied outcome after template edit, got {outcome:?}"
        );

        agent
            .prompt("second turn")
            .await
            .expect("second turn should pass");
        let captured = request_messages.lock().await.clone();
        assert_eq!(captured.len(), 2, "expected two model invocations");
        let first_system = captured[0]
            .first()
            .expect("first request should include system prompt")
            .text_content();
        let second_system = captured[1]
            .first()
            .expect("second request should include system prompt")
            .text_content();
        assert!(
            first_system.contains("PROMPT_V1"),
            "first turn should use V1 prompt"
        );
        assert!(
            second_system.contains("PROMPT_V2"),
            "second turn should use V2 prompt"
        );

        handle.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_spec_2548_c02_prompt_template_invalid_update_preserves_last_good_system_prompt(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        write_workspace_template(temp.path(), "PROMPT_STABLE {{ base_system_prompt }}");

        let initial_system_prompt =
            compose_startup_system_prompt(&cli, &cli.skills_dir).expect("compose initial prompt");
        let request_messages = Arc::new(AsyncMutex::new(Vec::new()));
        let client = Arc::new(RecordingPromptClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(ChatResponse {
                    message: Message::assistant_text("first"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
                Ok(ChatResponse {
                    message: Message::assistant_text("second"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            ])),
            request_messages: Arc::clone(&request_messages),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                system_prompt: initial_system_prompt.clone(),
                ..AgentConfig::default()
            },
        );
        let mut handle = start_bridge_for_prompt(&cli, &initial_system_prompt);

        agent.prompt("before invalid").await.expect("first turn");
        write_workspace_template(temp.path(), "{% if base_system_prompt %}BROKEN");
        let outcome = handle
            .evaluate_and_apply(&mut agent, &cli, &cli.skills_dir, true)
            .expect("evaluate hot reload");
        assert!(
            matches!(outcome, PromptTemplateBridgeOutcome::Invalid { .. }),
            "expected invalid outcome after broken template edit, got {outcome:?}"
        );

        agent.prompt("after invalid").await.expect("second turn");
        let captured = request_messages.lock().await.clone();
        assert_eq!(captured.len(), 2, "expected two model invocations");
        let first_system = captured[0]
            .first()
            .expect("first request should include system prompt")
            .text_content();
        let second_system = captured[1]
            .first()
            .expect("second request should include system prompt")
            .text_content();
        assert!(
            first_system.contains("PROMPT_STABLE"),
            "baseline turn should use stable template prompt"
        );
        assert!(
            second_system.contains("PROMPT_STABLE"),
            "invalid edit should preserve last-known-good prompt"
        );

        handle.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_spec_2548_c03_prompt_template_noop_update_emits_no_change_without_reapply()
    {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        write_workspace_template(temp.path(), "{{ base_system_prompt }}");

        let initial_system_prompt =
            compose_startup_system_prompt(&cli, &cli.skills_dir).expect("compose initial prompt");
        let client = Arc::new(RecordingPromptClient {
            outcomes: AsyncMutex::new(VecDeque::new()),
            request_messages: Arc::new(AsyncMutex::new(Vec::new())),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                system_prompt: initial_system_prompt.clone(),
                ..AgentConfig::default()
            },
        );
        let mut handle = start_bridge_for_prompt(&cli, &initial_system_prompt);

        write_workspace_template(temp.path(), "{% set _x = 1 %}{{ base_system_prompt }}");
        let outcome = handle
            .evaluate_and_apply(&mut agent, &cli, &cli.skills_dir, true)
            .expect("evaluate hot reload");
        assert!(
            matches!(outcome, PromptTemplateBridgeOutcome::NoChange { .. }),
            "expected no-change outcome for semantically equivalent edit, got {outcome:?}"
        );
        assert_eq!(
            agent
                .messages()
                .first()
                .expect("agent should keep system prompt")
                .text_content(),
            initial_system_prompt
        );

        handle.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn integration_spec_2548_c04_prompt_template_hot_reload_bridge_start_and_shutdown_are_clean(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let initial_system_prompt =
            compose_startup_system_prompt(&cli, &cli.skills_dir).expect("compose initial prompt");
        let mut handle = start_bridge_for_prompt(&cli, &initial_system_prompt);
        tokio::time::sleep(Duration::from_millis(80)).await;
        handle.shutdown().await;
        assert!(
            handle.shutdown_tx.is_none(),
            "shutdown should clear sender handle"
        );
        assert!(handle.task.is_none(), "shutdown should await task");
    }
}
