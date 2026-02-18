//! Tests for onboarding transport-mode routing and guardrails.

use super::{
    build_browser_automation_live_runner_config, build_deployment_contract_runner_config,
    build_events_runner_cli_config, build_gateway_contract_runner_config,
    build_gateway_openresponses_server_config, build_github_issues_bridge_cli_config,
    build_multi_agent_contract_runner_config, build_multi_channel_contract_runner_config,
    build_multi_channel_live_connectors_config, build_multi_channel_live_runner_config,
    build_multi_channel_media_config, build_multi_channel_outbound_config,
    build_multi_channel_runtime_dependencies, build_multi_channel_telemetry_config,
    build_runtime_heartbeat_scheduler_config, build_slack_bridge_cli_config,
    build_transport_doctor_config, build_transport_runtime_defaults,
    build_voice_contract_runner_config, build_voice_live_runner_config,
    execute_transport_runtime_mode, map_gateway_openresponses_auth_mode,
    resolve_bridge_transport_mode, resolve_contract_transport_mode,
    resolve_gateway_openresponses_auth, resolve_github_issues_bridge_repo_and_token_from_cli,
    resolve_multi_channel_outbound_secret, resolve_multi_channel_transport_mode,
    resolve_slack_bridge_tokens_from_cli, resolve_transport_runtime_mode,
    run_events_runner_if_requested, run_events_runner_with_runtime_defaults_if_requested,
    run_github_issues_bridge_if_requested,
    run_github_issues_bridge_with_runtime_defaults_if_requested,
    run_multi_channel_contract_runner_with_runtime_dependencies_if_requested,
    run_multi_channel_live_runner_with_runtime_dependencies_if_requested,
    run_slack_bridge_if_requested, run_slack_bridge_with_runtime_defaults_if_requested,
    run_transport_mode_if_requested, validate_transport_mode_cli, BridgeTransportMode,
    ContractTransportMode, EventsRunnerCliConfig, MultiChannelTransportMode, SlackBridgeCliConfig,
    TransportRuntimeDefaults, TransportRuntimeExecutor, TransportRuntimeMode,
};
use async_trait::async_trait;
use clap::Parser;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, ModelRef, TauAiError};
use tau_cli::{Cli, CliGatewayOpenResponsesAuthMode};
use tau_gateway::GatewayOpenResponsesAuthMode;
use tau_multi_channel::{
    MultiChannelLiveConnectorMode, MultiChannelPairingDecision, MultiChannelPairingEvaluator,
};
use tau_provider::{
    load_credential_store, save_credential_store, CredentialStoreData,
    CredentialStoreEncryptionMode, IntegrationCredentialStoreRecord,
};
use tau_skills::default_skills_lock_path;
use tau_tools::tools::ToolPolicy;
use tempfile::tempdir;

fn parse_cli_with_stack() -> Cli {
    std::thread::Builder::new()
        .name("tau-cli-parse".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| Cli::parse_from(["tau-rs"]))
        .expect("spawn cli parse thread")
        .join()
        .expect("join cli parse thread")
}

fn make_script_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("set executable permissions");
    }
}

struct NoopClient;

#[async_trait]
impl LlmClient for NoopClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        Ok(ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })
    }
}

struct AllowAllPairingEvaluator;

impl MultiChannelPairingEvaluator for AllowAllPairingEvaluator {
    fn evaluate_pairing(
        &self,
        _state_dir: &Path,
        _policy_channel: &str,
        _actor_id: &str,
        _now_unix_ms: u64,
    ) -> anyhow::Result<MultiChannelPairingDecision> {
        Ok(MultiChannelPairingDecision::Allow {
            reason_code: "allowed_for_test".to_string(),
        })
    }
}

struct RecordingTransportRuntimeExecutor {
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail_mode: Option<TransportRuntimeMode>,
}

impl RecordingTransportRuntimeExecutor {
    fn new(fail_mode: Option<TransportRuntimeMode>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_mode,
        }
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn record(&self, mode: TransportRuntimeMode, marker: &'static str) -> anyhow::Result<()> {
        self.calls.lock().expect("calls lock").push(marker);
        if self.fail_mode == Some(mode) {
            return Err(anyhow::anyhow!("forced failure for {marker}"));
        }
        Ok(())
    }
}

#[async_trait]
impl TransportRuntimeExecutor for RecordingTransportRuntimeExecutor {
    async fn run_gateway_openresponses_server(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::GatewayOpenResponsesServer,
            "gateway-openresponses",
        )
    }

    async fn run_github_issues_bridge(&self) -> anyhow::Result<()> {
        self.record(TransportRuntimeMode::GithubIssuesBridge, "github-issues")
    }

    async fn run_slack_bridge(&self) -> anyhow::Result<()> {
        self.record(TransportRuntimeMode::SlackBridge, "slack")
    }

    async fn run_events_runner(&self) -> anyhow::Result<()> {
        self.record(TransportRuntimeMode::EventsRunner, "events")
    }

    async fn run_multi_channel_contract_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::MultiChannelContractRunner,
            "multi-channel-contract",
        )
    }

    async fn run_multi_channel_live_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::MultiChannelLiveRunner,
            "multi-channel-live",
        )
    }

    async fn run_multi_channel_live_connectors_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::MultiChannelLiveConnectorsRunner,
            "multi-channel-live-connectors",
        )
    }

    async fn run_multi_agent_contract_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::MultiAgentContractRunner,
            "multi-agent-contract",
        )
    }

    async fn run_browser_automation_live_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::BrowserAutomationLiveRunner,
            "browser-automation-live",
        )
    }

    async fn run_gateway_contract_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::GatewayContractRunner,
            "gateway-contract",
        )
    }

    async fn run_deployment_contract_runner(&self) -> anyhow::Result<()> {
        self.record(
            TransportRuntimeMode::DeploymentContractRunner,
            "deployment-contract",
        )
    }

    async fn run_voice_contract_runner(&self) -> anyhow::Result<()> {
        self.record(TransportRuntimeMode::VoiceContractRunner, "voice-contract")
    }

    async fn run_voice_live_runner(&self) -> anyhow::Result<()> {
        self.record(TransportRuntimeMode::VoiceLiveRunner, "voice-live")
    }
}

fn write_integration_secret(
    path: &Path,
    integration_id: &str,
    secret: Option<&str>,
    revoked: bool,
) {
    let mut store = load_credential_store(path, CredentialStoreEncryptionMode::None, None)
        .unwrap_or(CredentialStoreData {
            encryption: CredentialStoreEncryptionMode::None,
            providers: BTreeMap::new(),
            integrations: BTreeMap::new(),
        });
    store.integrations.insert(
        integration_id.to_string(),
        IntegrationCredentialStoreRecord {
            secret: secret.map(str::to_string),
            revoked,
            updated_unix: Some(100),
        },
    );
    save_credential_store(path, &store, None).expect("save credential store");
}

#[test]
fn unit_resolve_multi_channel_outbound_secret_prefers_direct_secret() {
    let cli = parse_cli_with_stack();
    let resolved = resolve_multi_channel_outbound_secret(&cli, Some("  direct-secret  "), "unused");
    assert_eq!(resolved.as_deref(), Some("direct-secret"));
}

#[test]
fn unit_map_gateway_openresponses_auth_mode_matches_cli_variants() {
    assert_eq!(
        map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::Token),
        GatewayOpenResponsesAuthMode::Token
    );
    assert_eq!(
        map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::PasswordSession),
        GatewayOpenResponsesAuthMode::PasswordSession
    );
    assert_eq!(
        map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::LocalhostDev),
        GatewayOpenResponsesAuthMode::LocalhostDev
    );
}

#[test]
fn unit_validate_transport_mode_cli_accepts_default_cli() {
    let cli = parse_cli_with_stack();
    validate_transport_mode_cli(&cli).expect("default transport validation should succeed");
}

#[test]
fn unit_build_transport_doctor_config_uses_default_skills_lock_path() {
    let cli = parse_cli_with_stack();
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let config = build_transport_doctor_config(&cli, &model_ref);
    assert_eq!(config.skills_dir, cli.skills_dir);
    assert_eq!(
        config.skills_lock_path,
        default_skills_lock_path(&cli.skills_dir)
    );
}

#[test]
fn unit_build_transport_runtime_defaults_preserves_model_and_prompt() {
    let cli = parse_cli_with_stack();
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let defaults = build_transport_runtime_defaults(&cli, &model_ref, "system prompt");

    assert_eq!(defaults.model, "gpt-4o-mini");
    assert_eq!(defaults.system_prompt, "system prompt");
}

#[test]
fn functional_build_transport_runtime_defaults_carries_timeout_defaults() {
    let mut cli = parse_cli_with_stack();
    cli.turn_timeout_ms = 42_000;
    cli.request_timeout_ms = 7_000;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let defaults = build_transport_runtime_defaults(&cli, &model_ref, "system prompt");

    assert_eq!(defaults.turn_timeout_ms, 42_000);
    assert_eq!(defaults.request_timeout_ms, 7_000);
}

#[test]
fn integration_build_transport_runtime_defaults_preserves_session_lock_values() {
    let mut cli = parse_cli_with_stack();
    cli.session_lock_wait_ms = 12_345;
    cli.session_lock_stale_ms = 98_765;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let defaults = build_transport_runtime_defaults(&cli, &model_ref, "system prompt");

    assert_eq!(defaults.session_lock_wait_ms, 12_345);
    assert_eq!(defaults.session_lock_stale_ms, 98_765);
}

#[test]
fn regression_build_transport_runtime_defaults_preserves_max_turns() {
    let mut cli = parse_cli_with_stack();
    cli.max_turns = 77;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let defaults = build_transport_runtime_defaults(&cli, &model_ref, "system prompt");

    assert_eq!(
        defaults,
        TransportRuntimeDefaults {
            model: "gpt-4o-mini".to_string(),
            system_prompt: "system prompt".to_string(),
            max_turns: 77,
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
        }
    );
}

#[test]
fn functional_build_multi_channel_runtime_dependencies_uses_auth_and_doctor_config_defaults() {
    let mut cli = parse_cli_with_stack();
    cli.openai_api_key = Some("test-openai-key".to_string());
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let ((auth_key, doctor_lock_path), _pairing_marker) = build_multi_channel_runtime_dependencies(
        &cli,
        &model_ref,
        |auth_config, doctor_config| {
            (
                auth_config.openai_api_key.unwrap_or_default(),
                doctor_config.skills_lock_path,
            )
        },
        || (),
    );

    assert_eq!(auth_key, "test-openai-key");
    assert_eq!(doctor_lock_path, default_skills_lock_path(&cli.skills_dir));
}

#[test]
fn integration_build_multi_channel_runtime_dependencies_propagates_model_identity_to_doctor_config()
{
    let cli = parse_cli_with_stack();
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let (doctor_model, _pairing_marker) = build_multi_channel_runtime_dependencies(
        &cli,
        &model_ref,
        |_auth_config, doctor_config| doctor_config.model,
        || "pairing",
    );

    assert_eq!(doctor_model, "openai/gpt-4o-mini");
}

#[test]
fn regression_build_multi_channel_runtime_dependencies_preserves_no_session_setting() {
    let mut cli = parse_cli_with_stack();
    cli.no_session = true;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let (session_enabled, _pairing_marker) = build_multi_channel_runtime_dependencies(
        &cli,
        &model_ref,
        |_auth_config, doctor_config| doctor_config.session_enabled,
        || "pairing",
    );

    assert!(!session_enabled);
}

#[test]
fn functional_validate_transport_mode_cli_accepts_minimum_github_bridge_configuration() {
    let mut cli = parse_cli_with_stack();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token-value".to_string());

    validate_transport_mode_cli(&cli).expect("github bridge minimum configuration should validate");
}

#[test]
fn unit_resolve_multi_channel_transport_mode_defaults_to_none() {
    let cli = parse_cli_with_stack();
    assert_eq!(
        resolve_multi_channel_transport_mode(&cli),
        MultiChannelTransportMode::None
    );
}

#[test]
fn functional_resolve_multi_channel_transport_mode_prefers_contract_runner() {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_connectors_runner = true;
    assert_eq!(
        resolve_multi_channel_transport_mode(&cli),
        MultiChannelTransportMode::ContractRunner
    );
}

#[test]
fn integration_resolve_multi_channel_transport_mode_selects_live_runner() {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_live_runner = true;
    assert_eq!(
        resolve_multi_channel_transport_mode(&cli),
        MultiChannelTransportMode::LiveRunner
    );
}

#[test]
fn regression_resolve_multi_channel_transport_mode_selects_connectors_runner() {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_live_connectors_runner = true;
    assert_eq!(
        resolve_multi_channel_transport_mode(&cli),
        MultiChannelTransportMode::LiveConnectorsRunner
    );
}

#[test]
fn unit_resolve_bridge_transport_mode_defaults_to_none() {
    let cli = parse_cli_with_stack();
    assert_eq!(
        resolve_bridge_transport_mode(&cli),
        BridgeTransportMode::None
    );
}

#[test]
fn functional_resolve_bridge_transport_mode_prefers_github_issues_bridge() {
    let mut cli = parse_cli_with_stack();
    cli.github_issues_bridge = true;
    cli.slack_bridge = true;
    cli.events_runner = true;
    assert_eq!(
        resolve_bridge_transport_mode(&cli),
        BridgeTransportMode::GithubIssuesBridge
    );
}

#[test]
fn integration_resolve_bridge_transport_mode_selects_slack_bridge() {
    let mut cli = parse_cli_with_stack();
    cli.slack_bridge = true;
    assert_eq!(
        resolve_bridge_transport_mode(&cli),
        BridgeTransportMode::SlackBridge
    );
}

#[test]
fn regression_resolve_bridge_transport_mode_selects_events_runner() {
    let mut cli = parse_cli_with_stack();
    cli.events_runner = true;
    assert_eq!(
        resolve_bridge_transport_mode(&cli),
        BridgeTransportMode::EventsRunner
    );
}

#[test]
fn unit_resolve_transport_runtime_mode_defaults_to_none() {
    let cli = parse_cli_with_stack();
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::None
    );
}

#[test]
fn functional_resolve_transport_runtime_mode_prefers_gateway_openresponses_server() {
    let mut cli = parse_cli_with_stack();
    cli.gateway_openresponses_server = true;
    cli.github_issues_bridge = true;
    cli.slack_bridge = true;
    cli.events_runner = true;
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_agent_contract_runner = true;
    cli.browser_automation_contract_runner = true;
    cli.browser_automation_live_runner = true;
    cli.memory_contract_runner = true;
    cli.dashboard_contract_runner = true;
    cli.gateway_contract_runner = true;
    cli.deployment_contract_runner = true;
    cli.custom_command_contract_runner = true;
    cli.voice_contract_runner = true;
    cli.voice_live_runner = true;
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::GatewayOpenResponsesServer
    );
}

#[test]
fn integration_resolve_transport_runtime_mode_prefers_bridge_before_multi_channel_and_contract() {
    let mut cli = parse_cli_with_stack();
    cli.github_issues_bridge = true;
    cli.multi_channel_contract_runner = true;
    cli.multi_agent_contract_runner = true;
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::GithubIssuesBridge
    );
}

#[test]
fn integration_resolve_transport_runtime_mode_prefers_multi_channel_before_contract() {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_live_runner = true;
    cli.voice_contract_runner = true;
    cli.voice_live_runner = true;
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::MultiChannelLiveRunner
    );
}

#[test]
fn regression_resolve_transport_runtime_mode_selects_voice_contract_runner() {
    let mut cli = parse_cli_with_stack();
    cli.voice_contract_runner = true;
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::VoiceContractRunner
    );
}

#[test]
fn regression_resolve_transport_runtime_mode_selects_voice_live_runner() {
    let mut cli = parse_cli_with_stack();
    cli.voice_live_runner = true;
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::VoiceLiveRunner
    );
}

#[test]
fn regression_resolve_transport_runtime_mode_selects_browser_automation_live_runner() {
    let mut cli = parse_cli_with_stack();
    cli.browser_automation_live_runner = true;
    assert_eq!(
        resolve_transport_runtime_mode(&cli),
        TransportRuntimeMode::BrowserAutomationLiveRunner
    );
}

#[tokio::test]
async fn unit_execute_transport_runtime_mode_returns_false_for_none() {
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled = execute_transport_runtime_mode(TransportRuntimeMode::None, &executor)
        .await
        .expect("dispatch succeeds");
    assert!(!handled);
    assert!(executor.calls().is_empty());
}

#[tokio::test]
async fn functional_execute_transport_runtime_mode_dispatches_gateway_openresponses() {
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled =
        execute_transport_runtime_mode(TransportRuntimeMode::GatewayOpenResponsesServer, &executor)
            .await
            .expect("dispatch succeeds");
    assert!(handled);
    assert_eq!(executor.calls(), vec!["gateway-openresponses"]);
}

#[tokio::test]
async fn integration_execute_transport_runtime_mode_dispatches_multi_channel_live_runner() {
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled =
        execute_transport_runtime_mode(TransportRuntimeMode::MultiChannelLiveRunner, &executor)
            .await
            .expect("dispatch succeeds");
    assert!(handled);
    assert_eq!(executor.calls(), vec!["multi-channel-live"]);
}

#[tokio::test]
async fn integration_execute_transport_runtime_mode_dispatches_voice_live_runner() {
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled = execute_transport_runtime_mode(TransportRuntimeMode::VoiceLiveRunner, &executor)
        .await
        .expect("dispatch succeeds");
    assert!(handled);
    assert_eq!(executor.calls(), vec!["voice-live"]);
}

#[tokio::test]
async fn integration_execute_transport_runtime_mode_dispatches_browser_automation_live_runner() {
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled = execute_transport_runtime_mode(
        TransportRuntimeMode::BrowserAutomationLiveRunner,
        &executor,
    )
    .await
    .expect("dispatch succeeds");
    assert!(handled);
    assert_eq!(executor.calls(), vec!["browser-automation-live"]);
}

#[tokio::test]
async fn regression_execute_transport_runtime_mode_propagates_executor_errors() {
    let executor =
        RecordingTransportRuntimeExecutor::new(Some(TransportRuntimeMode::VoiceContractRunner));
    let error =
        execute_transport_runtime_mode(TransportRuntimeMode::VoiceContractRunner, &executor)
            .await
            .expect_err("errors should propagate");
    assert!(
        error
            .to_string()
            .contains("forced failure for voice-contract"),
        "unexpected error: {error}"
    );
    assert_eq!(executor.calls(), vec!["voice-contract"]);
}

#[tokio::test]
async fn unit_run_transport_mode_if_requested_returns_false_for_none() {
    let cli = parse_cli_with_stack();
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled = run_transport_mode_if_requested(&cli, &executor)
        .await
        .expect("dispatch succeeds");
    assert!(!handled);
    assert!(executor.calls().is_empty());
}

#[tokio::test]
async fn functional_run_transport_mode_if_requested_dispatches_selected_mode() {
    let mut cli = parse_cli_with_stack();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_token = Some("test-token".to_string());
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled = run_transport_mode_if_requested(&cli, &executor)
        .await
        .expect("dispatch succeeds");
    assert!(handled);
    assert_eq!(executor.calls(), vec!["gateway-openresponses"]);
}

#[tokio::test]
async fn integration_run_transport_mode_if_requested_dispatches_bridge_mode() {
    let mut cli = parse_cli_with_stack();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("ghp_test".to_string());
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let handled = run_transport_mode_if_requested(&cli, &executor)
        .await
        .expect("dispatch succeeds");
    assert!(handled);
    assert_eq!(executor.calls(), vec!["github-issues"]);
}

#[tokio::test]
async fn regression_run_transport_mode_if_requested_propagates_validation_errors() {
    let mut cli = parse_cli_with_stack();
    cli.slack_bridge = true;
    let executor = RecordingTransportRuntimeExecutor::new(None);
    let error = run_transport_mode_if_requested(&cli, &executor)
        .await
        .expect_err("validation should fail");
    assert!(
        error.to_string().contains("slack-app-token"),
        "unexpected error: {error}"
    );
    assert!(executor.calls().is_empty());
}

#[test]
fn unit_resolve_contract_transport_mode_defaults_to_none() {
    let cli = parse_cli_with_stack();
    assert_eq!(
        resolve_contract_transport_mode(&cli),
        ContractTransportMode::None
    );
}

#[test]
fn functional_resolve_contract_transport_mode_prefers_multi_agent() {
    let mut cli = parse_cli_with_stack();
    cli.multi_agent_contract_runner = true;
    cli.browser_automation_contract_runner = true;
    cli.browser_automation_live_runner = true;
    cli.dashboard_contract_runner = true;
    cli.gateway_contract_runner = true;
    cli.deployment_contract_runner = true;
    cli.custom_command_contract_runner = true;
    cli.voice_contract_runner = true;
    cli.voice_live_runner = true;
    assert_eq!(
        resolve_contract_transport_mode(&cli),
        ContractTransportMode::MultiAgent
    );
}

#[test]
fn integration_resolve_contract_transport_mode_selects_browser_automation_live() {
    let mut cli = parse_cli_with_stack();
    cli.browser_automation_live_runner = true;
    assert_eq!(
        resolve_contract_transport_mode(&cli),
        ContractTransportMode::BrowserAutomationLive
    );
}

#[test]
fn integration_resolve_contract_transport_mode_ignores_removed_memory_runner() {
    let mut cli = parse_cli_with_stack();
    cli.memory_contract_runner = true;
    assert_eq!(
        resolve_contract_transport_mode(&cli),
        ContractTransportMode::None
    );
}

#[test]
fn regression_resolve_contract_transport_mode_selects_voice() {
    let mut cli = parse_cli_with_stack();
    cli.voice_contract_runner = true;
    assert_eq!(
        resolve_contract_transport_mode(&cli),
        ContractTransportMode::Voice
    );
}

#[test]
fn regression_resolve_contract_transport_mode_selects_voice_live() {
    let mut cli = parse_cli_with_stack();
    cli.voice_live_runner = true;
    assert_eq!(
        resolve_contract_transport_mode(&cli),
        ContractTransportMode::VoiceLive
    );
}

#[test]
fn regression_validate_transport_mode_cli_rejects_events_prompt_template_conflict() {
    let mut cli = parse_cli_with_stack();
    cli.events_runner = true;
    cli.prompt_template_file = Some(PathBuf::from("template.txt"));

    let error = validate_transport_mode_cli(&cli).expect_err("conflicting flags should fail");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_transport_mode_cli_rejects_removed_browser_automation_contract_runner() {
    let mut cli = parse_cli_with_stack();
    cli.browser_automation_contract_runner = true;

    let error = validate_transport_mode_cli(&cli)
        .expect_err("removed browser automation contract runner should fail");
    assert!(error
        .to_string()
        .contains("--browser-automation-contract-runner has been removed"));
    assert!(error
        .to_string()
        .contains("--browser-automation-live-runner"));
}

#[test]
fn regression_validate_transport_mode_cli_rejects_removed_memory_contract_runner() {
    let mut cli = parse_cli_with_stack();
    cli.memory_contract_runner = true;

    let error =
        validate_transport_mode_cli(&cli).expect_err("removed memory contract runner should fail");
    assert!(error
        .to_string()
        .contains("--memory-contract-runner has been removed"));
}

#[test]
fn regression_validate_transport_mode_cli_rejects_removed_dashboard_contract_runner() {
    let mut cli = parse_cli_with_stack();
    cli.dashboard_contract_runner = true;

    let error = validate_transport_mode_cli(&cli)
        .expect_err("removed dashboard contract runner should fail");
    assert!(error
        .to_string()
        .contains("--dashboard-contract-runner has been removed"));
}

#[test]
fn regression_validate_transport_mode_cli_rejects_removed_custom_command_contract_runner() {
    let mut cli = parse_cli_with_stack();
    cli.custom_command_contract_runner = true;

    let error = validate_transport_mode_cli(&cli)
        .expect_err("removed custom-command contract runner should fail");
    assert!(error
        .to_string()
        .contains("--custom-command-contract-runner has been removed"));
}

#[test]
fn functional_resolve_gateway_openresponses_auth_trims_non_empty_values() {
    let mut cli = parse_cli_with_stack();
    cli.gateway_openresponses_auth_token = Some(" token-value ".to_string());
    cli.gateway_openresponses_auth_password = Some(" password-value ".to_string());

    let (token, password) = resolve_gateway_openresponses_auth(&cli);
    assert_eq!(token.as_deref(), Some("token-value"));
    assert_eq!(password.as_deref(), Some("password-value"));
}

#[test]
fn regression_resolve_gateway_openresponses_auth_ignores_empty_values() {
    let mut cli = parse_cli_with_stack();
    cli.gateway_openresponses_auth_token = Some("   ".to_string());
    cli.gateway_openresponses_auth_password = Some(String::new());

    let (token, password) = resolve_gateway_openresponses_auth(&cli);
    assert!(token.is_none());
    assert!(password.is_none());
}

#[test]
fn regression_build_gateway_contract_runner_config_enforces_guardrail_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.gateway_guardrail_failure_streak_threshold = 0;
    cli.gateway_guardrail_retryable_failures_threshold = 0;

    let config = build_gateway_contract_runner_config(&cli);
    assert_eq!(config.guardrail_failure_streak_threshold, 1);
    assert_eq!(config.guardrail_retryable_failures_threshold, 1);
}

#[test]
fn regression_build_multi_agent_contract_runner_config_enforces_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.multi_agent_queue_limit = 0;
    cli.multi_agent_processed_case_cap = 0;
    cli.multi_agent_retry_max_attempts = 0;

    let config = build_multi_agent_contract_runner_config(&cli);
    assert_eq!(config.queue_limit, 1);
    assert_eq!(config.processed_case_cap, 1);
    assert_eq!(config.retry_max_attempts, 1);
}

#[test]
fn regression_build_multi_channel_contract_runner_config_enforces_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_queue_limit = 0;
    cli.multi_channel_processed_event_cap = 0;
    cli.multi_channel_retry_max_attempts = 0;

    let config = build_multi_channel_contract_runner_config(
        &cli,
        tau_multi_channel::MultiChannelCommandHandlers::default(),
        Arc::new(AllowAllPairingEvaluator),
    );
    assert_eq!(config.queue_limit, 1);
    assert_eq!(config.processed_event_cap, 1);
    assert_eq!(config.retry_max_attempts, 1);
}

#[test]
fn regression_build_standard_contract_runner_builders_enforce_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.deployment_queue_limit = 0;
    cli.deployment_processed_case_cap = 0;
    cli.deployment_retry_max_attempts = 0;
    cli.voice_queue_limit = 0;
    cli.voice_processed_case_cap = 0;
    cli.voice_retry_max_attempts = 0;
    cli.voice_live_max_turns = 0;

    let deployment = build_deployment_contract_runner_config(&cli);
    let voice = build_voice_contract_runner_config(&cli);
    let voice_live = build_voice_live_runner_config(&cli);

    assert_eq!(deployment.queue_limit, 1);
    assert_eq!(deployment.processed_case_cap, 1);
    assert_eq!(deployment.retry_max_attempts, 1);
    assert_eq!(voice.queue_limit, 1);
    assert_eq!(voice.processed_case_cap, 1);
    assert_eq!(voice.retry_max_attempts, 1);
    assert_eq!(voice_live.max_turns, 1);
}

#[test]
fn regression_build_events_runner_cli_config_enforces_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.events_poll_interval_ms = 0;
    cli.events_queue_limit = 0;

    let config = build_events_runner_cli_config(&cli);
    assert_eq!(config.poll_interval_ms, 1);
    assert_eq!(config.queue_limit, 1);
}

#[test]
fn regression_build_slack_bridge_cli_config_enforces_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.slack_thread_detail_threshold_chars = 0;
    cli.slack_processed_event_cap = 0;
    cli.slack_reconnect_delay_ms = 0;
    cli.slack_retry_max_attempts = 0;
    cli.slack_retry_base_delay_ms = 0;

    let config =
        build_slack_bridge_cli_config(&cli, "app-token".to_string(), "bot-token".to_string());
    assert_eq!(config.detail_thread_threshold_chars, 1);
    assert_eq!(config.processed_event_cap, 1);
    assert_eq!(config.reconnect_delay_ms, 1);
    assert_eq!(config.retry_max_attempts, 1);
    assert_eq!(config.retry_base_delay_ms, 1);
}

#[test]
fn regression_build_github_issues_bridge_cli_config_enforces_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.github_poll_interval_seconds = 0;
    cli.github_processed_event_cap = 0;
    cli.github_retry_max_attempts = 0;
    cli.github_retry_base_delay_ms = 0;

    let config =
        build_github_issues_bridge_cli_config(&cli, "owner/repo".to_string(), "token".to_string());
    assert_eq!(config.poll_interval_seconds, 1);
    assert_eq!(config.processed_event_cap, 1);
    assert_eq!(config.retry_max_attempts, 1);
    assert_eq!(config.retry_base_delay_ms, 1);
}

#[test]
fn functional_build_github_issues_bridge_cli_config_trims_required_labels() {
    let mut cli = parse_cli_with_stack();
    cli.github_required_label = vec![" bug ".to_string(), "triage".to_string()];

    let config =
        build_github_issues_bridge_cli_config(&cli, "owner/repo".to_string(), "token".to_string());
    assert_eq!(
        config.required_labels,
        vec!["bug".to_string(), "triage".to_string()]
    );
}

#[test]
fn unit_build_runtime_heartbeat_scheduler_config_uses_expected_defaults() {
    let cli = parse_cli_with_stack();
    let config = build_runtime_heartbeat_scheduler_config(&cli);
    assert!(config.enabled);
    assert_eq!(config.interval, std::time::Duration::from_millis(5_000));
    assert_eq!(
        config.state_path,
        PathBuf::from(".tau/runtime-heartbeat/state.json")
    );
    assert_eq!(config.events_dir, Some(PathBuf::from(".tau/events")));
    assert_eq!(
        config.jobs_dir,
        Some(PathBuf::from(".tau/custom-command/jobs"))
    );
    assert!(config.self_repair_enabled);
    assert_eq!(
        config.self_repair_timeout,
        std::time::Duration::from_millis(300_000)
    );
    assert_eq!(config.self_repair_max_retries, 2);
    assert_eq!(
        config.self_repair_tool_builds_dir,
        Some(PathBuf::from(".tau/tool-builds"))
    );
    assert_eq!(
        config.self_repair_orphan_artifact_max_age,
        std::time::Duration::from_secs(3_600)
    );
    assert_eq!(
        config.lifecycle_memory_store_roots,
        vec![PathBuf::from(".tau/memory")]
    );
    assert!(
        config.lifecycle_policy.is_some(),
        "lifecycle policy should be configured by default"
    );
}

#[test]
fn functional_build_runtime_heartbeat_scheduler_config_accepts_overrides() {
    let mut cli = parse_cli_with_stack();
    cli.runtime_heartbeat_enabled = false;
    cli.runtime_heartbeat_interval_ms = 900;
    cli.runtime_heartbeat_state_path = PathBuf::from(".tau/ops/heartbeat-state.json");
    cli.events_dir = PathBuf::from(".tau/events-alt");
    cli.custom_command_state_dir = PathBuf::from(".tau/custom-commands-alt");
    cli.runtime_self_repair_enabled = false;
    cli.runtime_self_repair_timeout_ms = 9_000;
    cli.runtime_self_repair_max_retries = 3;
    cli.runtime_self_repair_tool_builds_dir = PathBuf::from(".tau/tool-builds-alt");
    cli.runtime_self_repair_orphan_max_age_seconds = 180;
    cli.memory_state_dir = PathBuf::from(".tau/memory-alt");

    let config = build_runtime_heartbeat_scheduler_config(&cli);
    assert!(!config.enabled);
    assert_eq!(config.interval, std::time::Duration::from_millis(900));
    assert_eq!(
        config.state_path,
        PathBuf::from(".tau/ops/heartbeat-state.json")
    );
    assert_eq!(config.events_dir, Some(PathBuf::from(".tau/events-alt")));
    assert_eq!(
        config.jobs_dir,
        Some(PathBuf::from(".tau/custom-commands-alt/jobs"))
    );
    assert!(!config.self_repair_enabled);
    assert_eq!(
        config.self_repair_timeout,
        std::time::Duration::from_millis(9_000)
    );
    assert_eq!(config.self_repair_max_retries, 3);
    assert_eq!(
        config.self_repair_tool_builds_dir,
        Some(PathBuf::from(".tau/tool-builds-alt"))
    );
    assert_eq!(
        config.self_repair_orphan_artifact_max_age,
        std::time::Duration::from_secs(180)
    );
    assert_eq!(
        config.lifecycle_memory_store_roots,
        vec![PathBuf::from(".tau/memory-alt")]
    );
    assert!(
        config.lifecycle_policy.is_some(),
        "lifecycle policy should remain configured when overrides are applied"
    );
}

#[test]
fn integration_build_gateway_openresponses_server_config_preserves_runtime_fields() {
    let mut cli = parse_cli_with_stack();
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = Some("  secret-pass  ".to_string());
    cli.gateway_openresponses_auth_token = Some("  secret-token  ".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:9090".to_string();
    cli.max_turns = 7;
    cli.turn_timeout_ms = 20_000;
    cli.gateway_openresponses_session_ttl_seconds = 1_800;
    cli.gateway_openresponses_rate_limit_window_seconds = 120;
    cli.gateway_openresponses_rate_limit_max_requests = 40;
    cli.gateway_openresponses_max_input_chars = 24_000;
    cli.runtime_heartbeat_enabled = false;
    cli.runtime_heartbeat_interval_ms = 2_500;
    cli.runtime_heartbeat_state_path = PathBuf::from(".tau/gateway/custom-heartbeat/state.json");
    cli.runtime_self_repair_enabled = false;
    cli.runtime_self_repair_timeout_ms = 5_500;
    cli.runtime_self_repair_max_retries = 4;
    cli.runtime_self_repair_tool_builds_dir = PathBuf::from(".tau/gateway/tool-builds");
    cli.runtime_self_repair_orphan_max_age_seconds = 90;

    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
    let tool_policy = ToolPolicy::new(vec![]);
    let config = build_gateway_openresponses_server_config(
        &cli,
        client.clone(),
        &model_ref,
        "system prompt",
        &tool_policy,
    );

    assert_eq!(config.model, "gpt-4o-mini");
    assert!(config.model_input_cost_per_million.is_some());
    assert!(config.model_output_cost_per_million.is_some());
    assert_eq!(config.system_prompt, "system prompt");
    assert_eq!(config.max_turns, 7);
    assert_eq!(config.turn_timeout_ms, 20_000);
    assert_eq!(config.bind, "127.0.0.1:9090");
    assert_eq!(
        config.auth_mode,
        GatewayOpenResponsesAuthMode::PasswordSession
    );
    assert_eq!(config.auth_token.as_deref(), Some("secret-token"));
    assert_eq!(config.auth_password.as_deref(), Some("secret-pass"));
    assert_eq!(config.session_ttl_seconds, 1_800);
    assert_eq!(config.rate_limit_window_seconds, 120);
    assert_eq!(config.rate_limit_max_requests, 40);
    assert_eq!(config.max_input_chars, 24_000);
    assert!(!config.runtime_heartbeat.enabled);
    assert_eq!(
        config.runtime_heartbeat.interval,
        std::time::Duration::from_millis(2_500)
    );
    assert_eq!(
        config.runtime_heartbeat.state_path,
        PathBuf::from(".tau/gateway/custom-heartbeat/state.json")
    );
    assert!(!config.runtime_heartbeat.self_repair_enabled);
    assert_eq!(
        config.runtime_heartbeat.self_repair_timeout,
        std::time::Duration::from_millis(5_500)
    );
    assert_eq!(config.runtime_heartbeat.self_repair_max_retries, 4);
    assert_eq!(
        config.runtime_heartbeat.self_repair_tool_builds_dir,
        Some(PathBuf::from(".tau/gateway/tool-builds"))
    );
    assert_eq!(
        config.runtime_heartbeat.self_repair_orphan_artifact_max_age,
        std::time::Duration::from_secs(90)
    );
}

#[test]
fn regression_build_gateway_openresponses_server_config_routes_default_heartbeat_path_to_gateway_state_dir(
) {
    let mut cli = parse_cli_with_stack();
    cli.gateway_state_dir = PathBuf::from(".tau/gateway-ops");
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("token".to_string());

    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
    let tool_policy = ToolPolicy::new(vec![]);
    let config = build_gateway_openresponses_server_config(
        &cli,
        client,
        &model_ref,
        "system prompt",
        &tool_policy,
    );

    assert_eq!(
        config.runtime_heartbeat.state_path,
        PathBuf::from(".tau/gateway-ops/runtime-heartbeat/state.json")
    );
}

#[tokio::test]
async fn unit_run_gateway_openresponses_server_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
    let tool_policy = ToolPolicy::new(vec![]);

    let handled = super::run_gateway_openresponses_server_if_requested(
        &cli,
        client,
        &model_ref,
        "system prompt",
        &tool_policy,
    )
    .await
    .expect("gateway helper");

    assert!(!handled);
}

#[tokio::test]
async fn unit_run_gateway_contract_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_gateway_contract_runner_if_requested(&cli)
        .await
        .expect("gateway contract helper");

    assert!(!handled);
}

#[tokio::test]
async fn unit_run_multi_agent_contract_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_multi_agent_contract_runner_if_requested(&cli)
        .await
        .expect("multi-agent contract helper");

    assert!(!handled);
}

#[tokio::test]
async fn unit_run_multi_channel_contract_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_multi_channel_contract_runner_if_requested(
        &cli,
        tau_multi_channel::MultiChannelCommandHandlers::default(),
        Arc::new(AllowAllPairingEvaluator),
    )
    .await
    .expect("multi-channel contract helper");

    assert!(!handled);
}

#[tokio::test]
async fn unit_run_multi_channel_contract_runner_with_runtime_dependencies_composes_inputs() {
    let cli = parse_cli_with_stack();
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let handler_called = Arc::new(Mutex::new(false));
    let handler_called_sink = Arc::clone(&handler_called);
    let pairing_called = Arc::new(Mutex::new(false));
    let pairing_called_sink = Arc::clone(&pairing_called);

    let handled = run_multi_channel_contract_runner_with_runtime_dependencies_if_requested(
        &cli,
        &model_ref,
        move |_auth_config, _doctor_config| {
            *handler_called_sink.lock().expect("handler lock") = true;
            tau_multi_channel::MultiChannelCommandHandlers::default()
        },
        move || {
            *pairing_called_sink.lock().expect("pairing lock") = true;
            Arc::new(AllowAllPairingEvaluator)
        },
    )
    .await
    .expect("composed helper should succeed");

    assert!(!handled);
    assert!(*handler_called.lock().expect("handler lock"));
    assert!(*pairing_called.lock().expect("pairing lock"));
}

#[tokio::test]
async fn functional_run_multi_channel_contract_runner_with_runtime_dependencies_preserves_auth_and_doctor_config(
) {
    let mut cli = parse_cli_with_stack();
    cli.openai_api_key = Some("test-openai-key".to_string());
    cli.no_session = true;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let captured = Arc::new(Mutex::new(None::<(String, bool, PathBuf)>));
    let captured_sink = Arc::clone(&captured);

    let handled = run_multi_channel_contract_runner_with_runtime_dependencies_if_requested(
        &cli,
        &model_ref,
        move |auth_config, doctor_config| {
            *captured_sink.lock().expect("capture lock") = Some((
                auth_config.openai_api_key.unwrap_or_default(),
                doctor_config.session_enabled,
                doctor_config.skills_lock_path,
            ));
            tau_multi_channel::MultiChannelCommandHandlers::default()
        },
        || Arc::new(AllowAllPairingEvaluator),
    )
    .await
    .expect("composed helper should succeed");

    assert!(!handled);
    let (openai_key, session_enabled, skills_lock_path) = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("captured payload");
    assert_eq!(openai_key, "test-openai-key");
    assert!(!session_enabled);
    assert_eq!(skills_lock_path, default_skills_lock_path(&cli.skills_dir));
}

#[tokio::test]
async fn unit_run_multi_channel_live_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_multi_channel_live_runner_if_requested(
        &cli,
        tau_multi_channel::MultiChannelCommandHandlers::default(),
        Arc::new(AllowAllPairingEvaluator),
    )
    .await
    .expect("multi-channel live helper");

    assert!(!handled);
}

#[tokio::test]
async fn integration_run_multi_channel_live_runner_with_runtime_dependencies_preserves_model_identity(
) {
    let cli = parse_cli_with_stack();
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let captured_model = Arc::new(Mutex::new(None::<String>));
    let captured_model_sink = Arc::clone(&captured_model);
    let pairing_called = Arc::new(Mutex::new(false));
    let pairing_called_sink = Arc::clone(&pairing_called);

    let handled = run_multi_channel_live_runner_with_runtime_dependencies_if_requested(
        &cli,
        &model_ref,
        move |_auth_config, doctor_config| {
            *captured_model_sink.lock().expect("capture lock") = Some(doctor_config.model);
            tau_multi_channel::MultiChannelCommandHandlers::default()
        },
        move || {
            *pairing_called_sink.lock().expect("pairing lock") = true;
            Arc::new(AllowAllPairingEvaluator)
        },
    )
    .await
    .expect("composed helper should succeed");

    assert!(!handled);
    assert_eq!(
        captured_model.lock().expect("capture lock").clone(),
        Some("openai/gpt-4o-mini".to_string())
    );
    assert!(*pairing_called.lock().expect("pairing lock"));
}

#[tokio::test]
async fn regression_run_multi_channel_contract_runner_with_runtime_dependencies_propagates_runner_errors(
) {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = PathBuf::from("/definitely/missing-contract-fixture.json");
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let error = run_multi_channel_contract_runner_with_runtime_dependencies_if_requested(
        &cli,
        &model_ref,
        |_auth_config, _doctor_config| tau_multi_channel::MultiChannelCommandHandlers::default(),
        || Arc::new(AllowAllPairingEvaluator),
    )
    .await
    .expect_err("runner error should propagate");

    assert!(
        error.to_string().contains("missing-contract-fixture"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn unit_run_browser_automation_live_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_browser_automation_live_runner_if_requested(&cli)
        .await
        .expect("browser automation live helper");

    assert!(!handled);
}

#[tokio::test]
async fn integration_run_browser_automation_live_runner_if_requested_executes_runtime() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("browser-live-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "browser-live",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "snapshot-live",
      "operation": "snapshot",
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "snapshot",
          "snapshot_id": "snapshot-live",
          "elements": [{ "id": "e1", "role": "button", "name": "Run" }]
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let script_path = temp.path().join("mock-playwright-cli.py");
    std::fs::write(
        &script_path,
        r#"#!/usr/bin/env python3
import json
import sys

command = sys.argv[1] if len(sys.argv) > 1 else ""
if command in ("start-session", "shutdown-session"):
    print(json.dumps({"status": "ok"}))
    raise SystemExit(0)
if command != "execute-action":
    raise SystemExit(2)
payload = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
print(json.dumps({
  "status_code": 200,
  "response_body": {
    "status": "ok",
    "operation": payload.get("operation", ""),
    "snapshot_id": "snapshot-live",
    "elements": [{"id": "e1", "role": "button", "name": "Run"}]
  }
}))
"#,
    )
    .expect("write script");
    make_script_executable(&script_path);

    let mut cli = parse_cli_with_stack();
    cli.browser_automation_live_runner = true;
    cli.browser_automation_live_fixture = fixture_path;
    cli.browser_automation_playwright_cli = script_path.display().to_string();
    cli.browser_automation_state_dir = temp.path().join("browser-automation-state");

    let handled = super::run_browser_automation_live_runner_if_requested(&cli)
        .await
        .expect("live runner should execute");
    assert!(handled);

    let state_raw = std::fs::read_to_string(cli.browser_automation_state_dir.join("state.json"))
        .expect("read live browser automation state");
    let state_json: serde_json::Value = serde_json::from_str(&state_raw).expect("parse state");
    assert_eq!(
        state_json["discovered_cases"].as_u64(),
        Some(1),
        "expected persisted discovered case count"
    );
    assert_eq!(
        state_json["success_cases"].as_u64(),
        Some(1),
        "expected persisted success case count"
    );
    assert_eq!(
        state_json["health"]["last_cycle_discovered"].as_u64(),
        Some(1),
        "expected persisted health discovered count"
    );
    assert_eq!(
        state_json["health"]["last_cycle_failed"].as_u64(),
        Some(0),
        "expected no failed live cases"
    );

    let events_raw = std::fs::read_to_string(
        cli.browser_automation_state_dir
            .join("runtime-events.jsonl"),
    )
    .expect("read live browser automation events");
    assert!(
        events_raw.contains("live_actions_succeeded"),
        "expected success reason code in events log"
    );
}

#[tokio::test]
async fn unit_run_deployment_contract_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_deployment_contract_runner_if_requested(&cli)
        .await
        .expect("deployment helper");

    assert!(!handled);
}

#[tokio::test]
async fn unit_run_voice_contract_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_voice_contract_runner_if_requested(&cli)
        .await
        .expect("voice helper");

    assert!(!handled);
}

#[tokio::test]
async fn unit_run_voice_live_runner_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_voice_live_runner_if_requested(&cli)
        .await
        .expect("voice live helper");

    assert!(!handled);
}

#[tokio::test]
async fn integration_run_voice_contract_runner_if_requested_executes_runtime() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-contract.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "voice-contract-test",
  "cases": [
{
  "schema_version": 1,
  "case_id": "voice-turn-contract",
  "mode": "turn",
  "wake_word": "tau",
  "transcript": "tau open dashboard",
  "locale": "en-US",
  "speaker_id": "ops-voice",
  "expected": {
    "outcome": "success",
    "status_code": 202,
    "response_body": {
      "status": "accepted",
      "mode": "turn",
      "wake_word": "tau",
      "utterance": "open dashboard",
      "locale": "en-US",
      "speaker_id": "ops-voice"
    }
  }
}
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = parse_cli_with_stack();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;
    cli.voice_state_dir = temp.path().join("voice-state");

    let handled = super::run_voice_contract_runner_if_requested(&cli)
        .await
        .expect("voice contract runner");

    assert!(handled);
    assert!(cli.voice_state_dir.join("state.json").exists());
    assert!(cli.voice_state_dir.join("runtime-events.jsonl").exists());
}

#[tokio::test]
async fn integration_run_voice_live_runner_if_requested_executes_runtime() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-live.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "session_id": "voice-live-test",
  "frames": [
{
  "frame_id": "frame-1",
  "transcript": "tau open dashboard",
  "speaker_id": "ops-live",
  "locale": "en-US"
}
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = parse_cli_with_stack();
    cli.voice_live_runner = true;
    cli.voice_live_input = fixture_path;
    cli.voice_state_dir = temp.path().join("voice-state");

    let handled = super::run_voice_live_runner_if_requested(&cli)
        .await
        .expect("voice live runner");

    assert!(handled);
    assert!(cli.voice_state_dir.join("state.json").exists());
}

#[tokio::test]
async fn regression_run_voice_contract_runner_if_requested_ignores_live_fixture_when_disabled() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-contract.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "voice-contract-only",
  "cases": [
{
  "schema_version": 1,
  "case_id": "voice-wake-word-only",
  "mode": "wake_word",
  "wake_word": "tau",
  "transcript": "tau are you online",
  "locale": "en-US",
  "speaker_id": "ops-voice",
  "expected": {
    "outcome": "success",
    "status_code": 202,
    "response_body": {
      "status": "accepted",
      "mode": "wake_word",
      "wake_word": "tau",
      "wake_detected": true
    }
  }
}
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = parse_cli_with_stack();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;
    cli.voice_state_dir = temp.path().join("voice-state");
    cli.voice_live_input = temp.path().join("missing-live-input.json");

    let handled = super::run_voice_contract_runner_if_requested(&cli)
        .await
        .expect("voice contract runner should ignore live fixture when live mode is disabled");

    assert!(handled);
    assert!(cli.voice_state_dir.join("state.json").exists());
    assert!(cli.voice_state_dir.join("runtime-events.jsonl").exists());
}

#[test]
fn integration_build_gateway_contract_runner_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.gateway_fixture = temp.path().join("gateway-fixture.json");
    cli.gateway_state_dir = temp.path().join("gateway-state");
    cli.gateway_guardrail_failure_streak_threshold = 7;
    cli.gateway_guardrail_retryable_failures_threshold = 9;

    let config = build_gateway_contract_runner_config(&cli);
    assert_eq!(config.fixture_path, cli.gateway_fixture);
    assert_eq!(config.state_dir, cli.gateway_state_dir);
    assert_eq!(config.queue_limit, 64);
    assert_eq!(config.processed_case_cap, 10_000);
    assert_eq!(config.retry_max_attempts, 4);
    assert_eq!(config.retry_base_delay_ms, 0);
    assert_eq!(config.guardrail_failure_streak_threshold, 7);
    assert_eq!(config.guardrail_retryable_failures_threshold, 9);
}

#[test]
fn integration_build_multi_agent_contract_runner_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.multi_agent_fixture = temp.path().join("multi-agent-fixture.json");
    cli.multi_agent_state_dir = temp.path().join("multi-agent-state");
    cli.multi_agent_queue_limit = 42;
    cli.multi_agent_processed_case_cap = 3_200;
    cli.multi_agent_retry_max_attempts = 8;
    cli.multi_agent_retry_base_delay_ms = 250;

    let config = build_multi_agent_contract_runner_config(&cli);
    assert_eq!(config.fixture_path, cli.multi_agent_fixture);
    assert_eq!(config.state_dir, cli.multi_agent_state_dir);
    assert_eq!(config.queue_limit, 42);
    assert_eq!(config.processed_case_cap, 3_200);
    assert_eq!(config.retry_max_attempts, 8);
    assert_eq!(config.retry_base_delay_ms, 250);
}

#[test]
fn integration_build_browser_automation_live_runner_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.browser_automation_live_fixture = temp.path().join("browser-automation-live-fixture.json");
    cli.browser_automation_state_dir = temp.path().join("browser-automation-state");
    cli.browser_automation_playwright_cli = "  /tmp/mock-playwright-cli  ".to_string();

    let config = build_browser_automation_live_runner_config(&cli);
    assert_eq!(config.fixture_path, cli.browser_automation_live_fixture);
    assert_eq!(config.state_dir, cli.browser_automation_state_dir);
    assert_eq!(config.playwright_cli, "/tmp/mock-playwright-cli");
    assert_eq!(config.policy.action_timeout_ms, 5_000);
    assert_eq!(config.policy.max_actions_per_case, 8);
    assert!(!config.policy.allow_unsafe_actions);
}

#[test]
fn integration_build_standard_contract_runner_builders_preserve_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.deployment_fixture = temp.path().join("deployment-fixture.json");
    cli.deployment_state_dir = temp.path().join("deployment-state");
    cli.deployment_queue_limit = 19;
    cli.deployment_processed_case_cap = 2_000;
    cli.deployment_retry_max_attempts = 3;
    cli.deployment_retry_base_delay_ms = 8;

    cli.voice_fixture = temp.path().join("voice-fixture.json");
    cli.voice_state_dir = temp.path().join("voice-state");
    cli.voice_queue_limit = 27;
    cli.voice_processed_case_cap = 4_444;
    cli.voice_retry_max_attempts = 9;
    cli.voice_retry_base_delay_ms = 12;
    cli.voice_live_input = temp.path().join("voice-live.json");
    cli.voice_live_wake_word = "Tau".to_string();
    cli.voice_live_max_turns = 11;
    cli.voice_live_tts_output = false;

    let deployment = build_deployment_contract_runner_config(&cli);
    let voice = build_voice_contract_runner_config(&cli);
    let voice_live = build_voice_live_runner_config(&cli);

    assert_eq!(deployment.fixture_path, cli.deployment_fixture);
    assert_eq!(deployment.state_dir, cli.deployment_state_dir);
    assert_eq!(deployment.queue_limit, 19);
    assert_eq!(deployment.processed_case_cap, 2_000);
    assert_eq!(deployment.retry_max_attempts, 3);
    assert_eq!(deployment.retry_base_delay_ms, 8);

    assert_eq!(voice.fixture_path, cli.voice_fixture);
    assert_eq!(voice.state_dir, cli.voice_state_dir);
    assert_eq!(voice.queue_limit, 27);
    assert_eq!(voice.processed_case_cap, 4_444);
    assert_eq!(voice.retry_max_attempts, 9);
    assert_eq!(voice.retry_base_delay_ms, 12);

    assert_eq!(voice_live.input_path, cli.voice_live_input);
    assert_eq!(voice_live.state_dir, cli.voice_state_dir);
    assert_eq!(voice_live.wake_word, "tau".to_string());
    assert_eq!(voice_live.max_turns, 11);
    assert!(!voice_live.tts_output_enabled);
}

#[test]
fn integration_build_events_runner_cli_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.channel_store_root = temp.path().join("channel-store");
    cli.events_dir = temp.path().join("events");
    cli.events_state_path = temp.path().join("events-state.json");
    cli.events_poll_interval_ms = 2_500;
    cli.events_queue_limit = 77;
    cli.events_stale_immediate_max_age_seconds = 9_999;

    let config = build_events_runner_cli_config(&cli);
    assert_eq!(config.channel_store_root, cli.channel_store_root);
    assert_eq!(config.events_dir, cli.events_dir);
    assert_eq!(config.state_path, cli.events_state_path);
    assert_eq!(config.poll_interval_ms, 2_500);
    assert_eq!(config.queue_limit, 77);
    assert_eq!(config.stale_immediate_max_age_seconds, 9_999);
}

#[test]
fn integration_build_slack_bridge_cli_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.slack_state_dir = temp.path().join("slack-state");
    cli.slack_api_base = "https://slack.example/api".to_string();
    cli.slack_bot_user_id = Some("U123".to_string());
    cli.slack_thread_detail_output = true;
    cli.slack_thread_detail_threshold_chars = 500;
    cli.slack_processed_event_cap = 250;
    cli.slack_max_event_age_seconds = 3_600;
    cli.slack_coalescing_window_ms = 4_200;
    cli.slack_reconnect_delay_ms = 2_500;
    cli.slack_retry_max_attempts = 8;
    cli.slack_retry_base_delay_ms = 650;
    cli.slack_artifact_retention_days = 14;

    let config =
        build_slack_bridge_cli_config(&cli, "app-token".to_string(), "bot-token".to_string());
    assert_eq!(config.state_dir, cli.slack_state_dir);
    assert_eq!(config.api_base, "https://slack.example/api");
    assert_eq!(config.app_token, "app-token");
    assert_eq!(config.bot_token, "bot-token");
    assert_eq!(config.bot_user_id.as_deref(), Some("U123"));
    assert!(config.detail_thread_output);
    assert_eq!(config.detail_thread_threshold_chars, 500);
    assert_eq!(config.processed_event_cap, 250);
    assert_eq!(config.max_event_age_seconds, 3_600);
    assert_eq!(config.coalescing_window_ms, 4_200);
    assert_eq!(config.reconnect_delay_ms, 2_500);
    assert_eq!(config.retry_max_attempts, 8);
    assert_eq!(config.retry_base_delay_ms, 650);
    assert_eq!(config.artifact_retention_days, 14);
}

#[test]
fn spec_c04_build_slack_bridge_cli_config_wires_coalescing_window_defaults_and_overrides() {
    let cli_default = parse_cli_with_stack();
    let default_config = build_slack_bridge_cli_config(
        &cli_default,
        "app-token".to_string(),
        "bot-token".to_string(),
    );
    assert_eq!(default_config.coalescing_window_ms, 2_000);

    let mut cli_override = parse_cli_with_stack();
    cli_override.slack_coalescing_window_ms = 3_333;
    let override_config = build_slack_bridge_cli_config(
        &cli_override,
        "app-token".to_string(),
        "bot-token".to_string(),
    );
    assert_eq!(override_config.coalescing_window_ms, 3_333);
}

#[test]
fn integration_build_github_issues_bridge_cli_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.github_state_dir = temp.path().join("github-state");
    cli.github_api_base = "https://github.example/api/v3".to_string();
    cli.github_bot_login = Some("tau-bot".to_string());
    cli.github_poll_interval_seconds = 45;
    cli.github_poll_once = true;
    cli.github_required_label = vec!["ops".to_string(), "triage".to_string()];
    cli.github_issue_number = vec![7, 42];
    cli.github_include_issue_body = true;
    cli.github_include_edited_comments = true;
    cli.github_processed_event_cap = 321;
    cli.github_retry_max_attempts = 9;
    cli.github_retry_base_delay_ms = 700;
    cli.github_artifact_retention_days = 21;

    let config = build_github_issues_bridge_cli_config(
        &cli,
        "owner/repo".to_string(),
        "token-value".to_string(),
    );
    assert_eq!(config.state_dir, cli.github_state_dir);
    assert_eq!(config.repo_slug, "owner/repo");
    assert_eq!(config.api_base, "https://github.example/api/v3");
    assert_eq!(config.token, "token-value");
    assert_eq!(config.bot_login.as_deref(), Some("tau-bot"));
    assert_eq!(config.poll_interval_seconds, 45);
    assert!(config.poll_once);
    assert_eq!(
        config.required_labels,
        vec!["ops".to_string(), "triage".to_string()]
    );
    assert_eq!(config.required_issue_numbers, vec![7, 42]);
    assert!(config.include_issue_body);
    assert!(config.include_edited_comments);
    assert_eq!(config.processed_event_cap, 321);
    assert_eq!(config.retry_max_attempts, 9);
    assert_eq!(config.retry_base_delay_ms, 700);
    assert_eq!(config.artifact_retention_days, 21);
}

#[test]
fn integration_build_multi_channel_contract_runner_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_fixture = temp.path().join("multi-channel-fixture.json");
    cli.multi_channel_state_dir = temp.path().join("multi-channel-state");
    cli.orchestrator_route_table = Some(temp.path().join("route-table.json"));
    cli.multi_channel_queue_limit = 24;
    cli.multi_channel_processed_event_cap = 8_000;
    cli.multi_channel_retry_max_attempts = 5;
    cli.multi_channel_retry_base_delay_ms = 17;
    cli.multi_channel_retry_jitter_ms = 33;

    let config = build_multi_channel_contract_runner_config(
        &cli,
        tau_multi_channel::MultiChannelCommandHandlers::default(),
        Arc::new(AllowAllPairingEvaluator),
    );
    assert_eq!(config.fixture_path, cli.multi_channel_fixture);
    assert_eq!(config.state_dir, cli.multi_channel_state_dir);
    assert_eq!(
        config.orchestrator_route_table_path,
        cli.orchestrator_route_table
    );
    assert_eq!(config.queue_limit, 24);
    assert_eq!(config.processed_event_cap, 8_000);
    assert_eq!(config.retry_max_attempts, 5);
    assert_eq!(config.retry_base_delay_ms, 17);
    assert_eq!(config.retry_jitter_ms, 33);
}

#[test]
fn integration_build_multi_channel_live_runner_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_state_dir = temp.path().join("multi-channel-state");
    cli.orchestrator_route_table = Some(temp.path().join("route-table.json"));
    cli.multi_channel_queue_limit = 19;
    cli.multi_channel_processed_event_cap = 4_500;
    cli.multi_channel_retry_max_attempts = 6;
    cli.multi_channel_retry_base_delay_ms = 21;
    cli.multi_channel_retry_jitter_ms = 34;

    let config = build_multi_channel_live_runner_config(
        &cli,
        tau_multi_channel::MultiChannelCommandHandlers::default(),
        Arc::new(AllowAllPairingEvaluator),
    );
    assert_eq!(config.ingress_dir, cli.multi_channel_live_ingress_dir);
    assert_eq!(config.state_dir, cli.multi_channel_state_dir);
    assert_eq!(
        config.orchestrator_route_table_path,
        cli.orchestrator_route_table
    );
    assert_eq!(config.queue_limit, 19);
    assert_eq!(config.processed_event_cap, 4_500);
    assert_eq!(config.retry_max_attempts, 6);
    assert_eq!(config.retry_base_delay_ms, 21);
    assert_eq!(config.retry_jitter_ms, 34);
}

#[tokio::test]
async fn unit_run_multi_channel_live_connectors_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();

    let handled = super::run_multi_channel_live_connectors_if_requested(&cli)
        .await
        .expect("connectors helper");

    assert!(!handled);
}

#[test]
fn integration_build_multi_channel_live_connectors_config_preserves_runtime_fields() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.credential_store = temp.path().join("credentials.json");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_live_ingress_dir = temp.path().join("ingress");
    cli.multi_channel_processed_event_cap = 0;
    cli.multi_channel_retry_max_attempts = 0;
    cli.multi_channel_retry_base_delay_ms = 42;
    cli.multi_channel_live_connectors_poll_once = true;
    cli.multi_channel_live_webhook_bind = "127.0.0.1:9999".to_string();
    cli.multi_channel_telegram_ingress_mode = tau_cli::CliMultiChannelLiveConnectorMode::Polling;
    cli.multi_channel_discord_ingress_mode = tau_cli::CliMultiChannelLiveConnectorMode::Webhook;
    cli.multi_channel_whatsapp_ingress_mode = tau_cli::CliMultiChannelLiveConnectorMode::Webhook;
    cli.multi_channel_telegram_api_base = " https://telegram.example ".to_string();
    cli.multi_channel_discord_api_base = " https://discord.example ".to_string();
    cli.multi_channel_telegram_bot_token = Some(" telegram-direct ".to_string());
    cli.multi_channel_discord_ingress_channel_ids = vec![" 111 ".to_string(), "222".to_string()];
    cli.multi_channel_telegram_webhook_secret = Some(" tg-secret ".to_string());
    cli.multi_channel_whatsapp_webhook_verify_token = Some(" wa-verify-secret ".to_string());
    cli.multi_channel_whatsapp_webhook_app_secret = Some(" wa-app-secret ".to_string());
    write_integration_secret(
        &cli.credential_store,
        "discord-bot-token",
        Some("discord-store"),
        false,
    );

    let config = build_multi_channel_live_connectors_config(&cli);
    assert_eq!(
        config.state_path,
        cli.multi_channel_live_connectors_state_path
    );
    assert_eq!(config.ingress_dir, cli.multi_channel_live_ingress_dir);
    assert_eq!(config.processed_event_cap, 1);
    assert_eq!(config.retry_max_attempts, 1);
    assert_eq!(config.retry_base_delay_ms, 42);
    assert!(config.poll_once);
    assert_eq!(config.webhook_bind, "127.0.0.1:9999");
    assert_eq!(config.telegram_mode, MultiChannelLiveConnectorMode::Polling);
    assert_eq!(config.discord_mode, MultiChannelLiveConnectorMode::Webhook);
    assert_eq!(config.whatsapp_mode, MultiChannelLiveConnectorMode::Webhook);
    assert_eq!(config.telegram_api_base, "https://telegram.example");
    assert_eq!(config.discord_api_base, "https://discord.example");
    assert_eq!(
        config.telegram_bot_token.as_deref(),
        Some("telegram-direct")
    );
    assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
    assert_eq!(config.discord_ingress_channel_ids, vec!["111", "222"]);
    assert_eq!(config.telegram_webhook_secret.as_deref(), Some("tg-secret"));
    assert_eq!(
        config.whatsapp_webhook_verify_token.as_deref(),
        Some("wa-verify-secret")
    );
    assert_eq!(
        config.whatsapp_webhook_app_secret.as_deref(),
        Some("wa-app-secret")
    );
}

#[test]
fn functional_resolve_multi_channel_outbound_secret_reads_active_store_entry() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.credential_store = temp.path().join("credentials.json");
    write_integration_secret(
        &cli.credential_store,
        "telegram-bot-token",
        Some("telegram-secret"),
        false,
    );

    let resolved = resolve_multi_channel_outbound_secret(&cli, None, "telegram-bot-token");
    assert_eq!(resolved.as_deref(), Some("telegram-secret"));
}

#[test]
fn functional_build_multi_channel_outbound_config_resolves_direct_and_store_values() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.credential_store = temp.path().join("credentials.json");
    cli.multi_channel_telegram_bot_token = Some(" telegram-direct ".to_string());
    cli.multi_channel_telegram_api_base = " https://telegram.example ".to_string();
    cli.multi_channel_discord_api_base = " https://discord.example ".to_string();
    cli.multi_channel_whatsapp_api_base = " https://whatsapp.example ".to_string();
    cli.multi_channel_outbound_max_chars = 0;
    cli.multi_channel_outbound_http_timeout_ms = 0;
    write_integration_secret(
        &cli.credential_store,
        "discord-bot-token",
        Some("discord-store"),
        false,
    );
    write_integration_secret(
        &cli.credential_store,
        "whatsapp-access-token",
        Some("whatsapp-store"),
        false,
    );
    write_integration_secret(
        &cli.credential_store,
        "whatsapp-phone-number-id",
        Some("phone-store"),
        false,
    );

    let config = build_multi_channel_outbound_config(&cli);
    assert_eq!(
        config.telegram_bot_token.as_deref(),
        Some("telegram-direct")
    );
    assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
    assert_eq!(
        config.whatsapp_access_token.as_deref(),
        Some("whatsapp-store")
    );
    assert_eq!(
        config.whatsapp_phone_number_id.as_deref(),
        Some("phone-store")
    );
    assert_eq!(config.telegram_api_base, "https://telegram.example");
    assert_eq!(config.discord_api_base, "https://discord.example");
    assert_eq!(config.whatsapp_api_base, "https://whatsapp.example");
    assert_eq!(config.max_chars, 1);
    assert_eq!(config.http_timeout_ms, 1);
}

#[test]
fn functional_build_multi_channel_live_connectors_config_resolves_store_fallbacks() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.credential_store = temp.path().join("credentials.json");
    cli.multi_channel_telegram_bot_token = None;
    cli.multi_channel_discord_bot_token = None;
    write_integration_secret(
        &cli.credential_store,
        "telegram-bot-token",
        Some("telegram-store"),
        false,
    );
    write_integration_secret(
        &cli.credential_store,
        "discord-bot-token",
        Some("discord-store"),
        false,
    );

    let config = build_multi_channel_live_connectors_config(&cli);
    assert_eq!(config.telegram_bot_token.as_deref(), Some("telegram-store"));
    assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
}

#[test]
fn regression_resolve_multi_channel_outbound_secret_returns_none_for_revoked_entry() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.credential_store = temp.path().join("credentials.json");
    write_integration_secret(
        &cli.credential_store,
        "discord-bot-token",
        Some("discord-secret"),
        true,
    );

    let resolved = resolve_multi_channel_outbound_secret(&cli, None, "discord-bot-token");
    assert!(resolved.is_none());
}

#[test]
fn regression_build_multi_channel_telemetry_and_media_config_enforce_minimums() {
    let mut cli = parse_cli_with_stack();
    cli.multi_channel_telemetry_min_response_chars = 0;
    cli.multi_channel_media_max_attachments = 0;
    cli.multi_channel_media_max_summary_chars = 0;

    let telemetry = build_multi_channel_telemetry_config(&cli);
    let media = build_multi_channel_media_config(&cli);

    assert_eq!(telemetry.typing_presence_min_response_chars, 1);
    assert_eq!(media.max_attachments_per_event, 1);
    assert_eq!(media.max_summary_chars, 16);
}

#[test]
fn regression_build_multi_channel_live_connectors_config_ignores_revoked_store_secret() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli_with_stack();
    cli.credential_store = temp.path().join("credentials.json");
    cli.multi_channel_discord_bot_token = None;
    write_integration_secret(
        &cli.credential_store,
        "discord-bot-token",
        Some("discord-secret"),
        true,
    );

    let config = build_multi_channel_live_connectors_config(&cli);
    assert!(config.discord_bot_token.is_none());
}

#[test]
fn unit_resolve_github_issues_bridge_repo_and_token_from_cli_resolves_direct_token() {
    let mut cli = parse_cli_with_stack();
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("ghp_direct".to_string());

    let (repo_slug, token) =
        resolve_github_issues_bridge_repo_and_token_from_cli(&cli, |direct, _secret_id, _flag| {
            Ok(direct.map(str::to_string))
        })
        .expect("github repo/token should resolve");

    assert_eq!(repo_slug, "owner/repo");
    assert_eq!(token, "ghp_direct");
}

#[test]
fn functional_resolve_github_issues_bridge_repo_and_token_from_cli_uses_secret_resolver_inputs() {
    let mut cli = parse_cli_with_stack();
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token_id = Some("github-token-secret".to_string());
    let captures = Arc::new(Mutex::new(
        Vec::<(Option<String>, Option<String>, String)>::new(),
    ));
    let captures_sink = Arc::clone(&captures);

    let (_repo_slug, token) = resolve_github_issues_bridge_repo_and_token_from_cli(
        &cli,
        move |direct, secret_id, flag| {
            captures_sink.lock().expect("captures lock").push((
                direct.map(str::to_string),
                secret_id.map(str::to_string),
                flag.to_string(),
            ));
            Ok(Some("ghp_from_store".to_string()))
        },
    )
    .expect("github repo/token should resolve");

    assert_eq!(token, "ghp_from_store");
    assert_eq!(
        captures.lock().expect("captures lock").as_slice(),
        &[(
            None,
            Some("github-token-secret".to_string()),
            "--github-token-id".to_string()
        )]
    );
}

#[test]
fn integration_resolve_slack_bridge_tokens_from_cli_uses_secret_resolver_for_both_tokens() {
    let mut cli = parse_cli_with_stack();
    cli.slack_app_token_id = Some("slack-app-secret".to_string());
    cli.slack_bot_token_id = Some("slack-bot-secret".to_string());

    let (app_token, bot_token) =
        resolve_slack_bridge_tokens_from_cli(&cli, |_direct, secret_id, _flag| match secret_id {
            Some("slack-app-secret") => Ok(Some("xapp-1".to_string())),
            Some("slack-bot-secret") => Ok(Some("xoxb-1".to_string())),
            _ => Ok(None),
        })
        .expect("slack tokens should resolve");

    assert_eq!(app_token, "xapp-1");
    assert_eq!(bot_token, "xoxb-1");
}

#[test]
fn regression_resolve_slack_bridge_tokens_from_cli_fails_closed_when_bot_token_missing() {
    let mut cli = parse_cli_with_stack();
    cli.slack_app_token = Some("xapp-direct".to_string());
    cli.slack_bot_token = None;
    cli.slack_bot_token_id = None;

    let error = resolve_slack_bridge_tokens_from_cli(&cli, |direct, _secret_id, _flag| {
        Ok(direct.map(str::to_string))
    })
    .expect_err("missing bot token should fail");

    assert!(
        error.to_string().contains("--slack-bot-token"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn unit_run_github_issues_bridge_if_requested_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();
    let resolver_called = Arc::new(Mutex::new(false));
    let resolver_called_sink = Arc::clone(&resolver_called);

    let ran = run_github_issues_bridge_if_requested(
        &cli,
        move || {
            *resolver_called_sink.lock().expect("resolver lock") = true;
            Ok(("owner/repo".to_string(), "token".to_string()))
        },
        |_config| async { Ok(()) },
    )
    .await
    .expect("github helper succeeds");

    assert!(!ran);
    assert!(!*resolver_called.lock().expect("resolver lock"));
}

#[tokio::test]
async fn unit_run_events_runner_with_runtime_defaults_returns_false_when_disabled() {
    let cli = parse_cli_with_stack();
    let called = Arc::new(Mutex::new(false));
    let called_sink = Arc::clone(&called);
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let ran = run_events_runner_with_runtime_defaults_if_requested(
        &cli,
        &model_ref,
        "system prompt",
        move |_config, _defaults| {
            let sink = Arc::clone(&called_sink);
            async move {
                *sink.lock().expect("called lock") = true;
                Ok(())
            }
        },
    )
    .await
    .expect("events helper succeeds");

    assert!(!ran);
    assert!(!*called.lock().expect("called lock"));
}

#[tokio::test]
async fn functional_run_slack_bridge_if_requested_builds_config_with_resolved_tokens() {
    let mut cli = parse_cli_with_stack();
    cli.slack_bridge = true;
    cli.slack_thread_detail_threshold_chars = 0;
    cli.slack_processed_event_cap = 0;
    cli.slack_reconnect_delay_ms = 0;
    cli.slack_retry_max_attempts = 0;
    cli.slack_retry_base_delay_ms = 0;
    let captured = Arc::new(Mutex::new(None::<SlackBridgeCliConfig>));
    let captured_sink = Arc::clone(&captured);

    let ran = run_slack_bridge_if_requested(
        &cli,
        || Ok(("app-token".to_string(), "bot-token".to_string())),
        move |config| {
            let sink = Arc::clone(&captured_sink);
            async move {
                *sink.lock().expect("capture lock") = Some(config);
                Ok(())
            }
        },
    )
    .await
    .expect("slack helper succeeds");

    assert!(ran);
    let config = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("captured config");
    assert_eq!(config.app_token, "app-token");
    assert_eq!(config.bot_token, "bot-token");
    assert_eq!(config.detail_thread_threshold_chars, 1);
    assert_eq!(config.processed_event_cap, 1);
    assert_eq!(config.reconnect_delay_ms, 1);
    assert_eq!(config.retry_max_attempts, 1);
    assert_eq!(config.retry_base_delay_ms, 1);
}

#[tokio::test]
async fn functional_run_slack_bridge_with_runtime_defaults_passes_defaults() {
    let mut cli = parse_cli_with_stack();
    cli.slack_bridge = true;
    cli.max_turns = 42;
    cli.turn_timeout_ms = 20_000;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let captured = Arc::new(Mutex::new(
        None::<(SlackBridgeCliConfig, TransportRuntimeDefaults)>,
    ));
    let captured_sink = Arc::clone(&captured);

    let ran = run_slack_bridge_with_runtime_defaults_if_requested(
        &cli,
        &model_ref,
        "bridge system prompt",
        || Ok(("app-token".to_string(), "bot-token".to_string())),
        move |config, defaults| {
            let sink = Arc::clone(&captured_sink);
            async move {
                *sink.lock().expect("capture lock") = Some((config, defaults));
                Ok(())
            }
        },
    )
    .await
    .expect("slack helper succeeds");

    assert!(ran);
    let (config, defaults) = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("captured payload");
    assert_eq!(config.app_token, "app-token");
    assert_eq!(defaults.model, "gpt-4o-mini");
    assert_eq!(defaults.system_prompt, "bridge system prompt");
    assert_eq!(defaults.max_turns, 42);
    assert_eq!(defaults.turn_timeout_ms, 20_000);
}

#[tokio::test]
async fn integration_run_events_runner_if_requested_passes_normalized_config() {
    let mut cli = parse_cli_with_stack();
    cli.events_runner = true;
    cli.events_poll_interval_ms = 0;
    cli.events_queue_limit = 0;
    let captured = Arc::new(Mutex::new(None::<EventsRunnerCliConfig>));
    let captured_sink = Arc::clone(&captured);

    let ran = run_events_runner_if_requested(&cli, move |config| {
        let sink = Arc::clone(&captured_sink);
        async move {
            *sink.lock().expect("capture lock") = Some(config);
            Ok(())
        }
    })
    .await
    .expect("events helper succeeds");

    assert!(ran);
    let config = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("captured config");
    assert_eq!(config.poll_interval_ms, 1);
    assert_eq!(config.queue_limit, 1);
}

#[tokio::test]
async fn integration_run_events_runner_with_runtime_defaults_passes_normalized_config() {
    let mut cli = parse_cli_with_stack();
    cli.events_runner = true;
    cli.events_poll_interval_ms = 0;
    cli.events_queue_limit = 0;
    cli.session_lock_wait_ms = 123;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
    let captured = Arc::new(Mutex::new(
        None::<(EventsRunnerCliConfig, TransportRuntimeDefaults)>,
    ));
    let captured_sink = Arc::clone(&captured);

    let ran = run_events_runner_with_runtime_defaults_if_requested(
        &cli,
        &model_ref,
        "events system prompt",
        move |config, defaults| {
            let sink = Arc::clone(&captured_sink);
            async move {
                *sink.lock().expect("capture lock") = Some((config, defaults));
                Ok(())
            }
        },
    )
    .await
    .expect("events helper succeeds");

    assert!(ran);
    let (config, defaults) = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("captured payload");
    assert_eq!(config.poll_interval_ms, 1);
    assert_eq!(config.queue_limit, 1);
    assert_eq!(defaults.system_prompt, "events system prompt");
    assert_eq!(defaults.session_lock_wait_ms, 123);
}

#[tokio::test]
async fn regression_run_github_issues_bridge_if_requested_propagates_resolver_errors() {
    let mut cli = parse_cli_with_stack();
    cli.github_issues_bridge = true;

    let error = run_github_issues_bridge_if_requested(
        &cli,
        || Err(anyhow::anyhow!("missing bridge credentials")),
        |_config| async { Ok(()) },
    )
    .await
    .expect_err("resolver failures should propagate");

    assert!(
        error.to_string().contains("missing bridge credentials"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn regression_run_github_issues_bridge_with_runtime_defaults_propagates_resolver_errors() {
    let mut cli = parse_cli_with_stack();
    cli.github_issues_bridge = true;
    let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

    let error = run_github_issues_bridge_with_runtime_defaults_if_requested(
        &cli,
        &model_ref,
        "github bridge prompt",
        || Err(anyhow::anyhow!("missing bridge credentials")),
        |_config, _defaults| async { Ok(()) },
    )
    .await
    .expect_err("resolver failures should propagate");

    assert!(
        error.to_string().contains("missing bridge credentials"),
        "unexpected error: {error}"
    );
}
