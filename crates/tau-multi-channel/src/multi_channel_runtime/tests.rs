//! Multi-channel runtime integration-style tests for contracts, live ingress, and routing behavior.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use httpmock::Method::POST;
use httpmock::MockServer;
use serde::Deserialize;
use serde_json::{json, Value};
use tau_access::{
    save_trust_root_records, signed_envelope_message_bytes, TrustedRootRecord,
    SIGNED_ENVELOPE_METADATA_KEY,
};
use tempfile::tempdir;

use super::{
    current_unix_timestamp_ms, load_multi_channel_live_events, load_multi_channel_runtime_state,
    retry_delay_ms, run_multi_channel_live_runner, MultiChannelApprovalsCommandExecutor,
    MultiChannelAuthCommandExecutor, MultiChannelCommandHandlers,
    MultiChannelDoctorCommandExecutor, MultiChannelLiveRuntimeConfig, MultiChannelPairingDecision,
    MultiChannelPairingEvaluator, MultiChannelRuntime, MultiChannelRuntimeConfig,
    MultiChannelTelemetryConfig, PAIRING_REASON_ALLOW_PERMISSIVE_MODE,
};
use crate::multi_channel_contract::{
    load_multi_channel_contract_fixture, parse_multi_channel_contract_fixture,
    MultiChannelAttachment, MultiChannelEventKind, MultiChannelInboundEvent, MultiChannelTransport,
};
use crate::multi_channel_outbound::{MultiChannelOutboundConfig, MultiChannelOutboundMode};
use tau_runtime::{ChannelStore, TransportHealthState};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("multi-channel-contract")
        .join(name)
}

fn live_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("multi-channel-live-ingress")
        .join(name)
}

#[derive(Clone, Default)]
struct TestAuthHandler;

impl MultiChannelAuthCommandExecutor for TestAuthHandler {
    fn execute_auth_status(&self, provider: Option<&str>) -> String {
        if let Some(provider) = provider {
            format!("auth status {provider}: ok")
        } else {
            "auth status ok".to_string()
        }
    }
}

#[derive(Clone, Default)]
struct TestDoctorHandler;

impl MultiChannelDoctorCommandExecutor for TestDoctorHandler {
    fn execute_doctor(&self, online: bool) -> String {
        if online {
            "doctor online ok".to_string()
        } else {
            "doctor ok".to_string()
        }
    }
}

#[derive(Clone, Default)]
struct TestApprovalsHandler;

impl MultiChannelApprovalsCommandExecutor for TestApprovalsHandler {
    fn execute_approvals(
        &self,
        state_dir: &Path,
        args: &str,
        decision_actor: Option<&str>,
    ) -> String {
        match execute_test_approvals_command(state_dir, args, decision_actor) {
            Ok(output) => output,
            Err(error) => format!("approvals error: {error}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PairingRecord {
    channel: String,
    actor_id: String,
    #[serde(default)]
    expires_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct PairingRegistryFile {
    schema_version: u32,
    #[serde(default)]
    pairings: Vec<PairingRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct PairingAllowlistFile {
    schema_version: u32,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    channels: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Default)]
struct FilePairingEvaluator;

impl MultiChannelPairingEvaluator for FilePairingEvaluator {
    fn evaluate_pairing(
        &self,
        state_dir: &Path,
        policy_channel: &str,
        actor_id: &str,
        now_unix_ms: u64,
    ) -> Result<MultiChannelPairingDecision> {
        const ALLOW_ALLOWLIST_AND_PAIRING: &str = "allow_allowlist_and_pairing";
        const ALLOW_ALLOWLIST: &str = "allow_allowlist";
        const ALLOW_PAIRING: &str = "allow_pairing";
        const DENY_ACTOR_ID_MISSING: &str = "deny_actor_id_missing";
        const DENY_ACTOR_NOT_PAIRED_OR_ALLOWLISTED: &str = "deny_actor_not_paired_or_allowlisted";

        let actor_id = actor_id.trim();
        let (allowlist_path, registry_path, strict_mode) = pairing_paths_for_state_dir(state_dir);
        let allowlist = load_pairing_allowlist(&allowlist_path)?;
        let registry = load_pairing_registry(&registry_path)?;
        let candidates = channel_candidates(policy_channel);
        let strict_effective = strict_mode
            || allowlist.strict
            || channel_has_pairing_rules(&allowlist, &registry, &candidates);

        if !strict_effective {
            return Ok(MultiChannelPairingDecision::Allow {
                reason_code: PAIRING_REASON_ALLOW_PERMISSIVE_MODE.to_string(),
            });
        }
        if actor_id.is_empty() {
            return Ok(MultiChannelPairingDecision::Deny {
                reason_code: DENY_ACTOR_ID_MISSING.to_string(),
            });
        }

        let allowed_by_allowlist = allowlist_actor_allowed(&allowlist, &candidates, actor_id);
        let allowed_by_pairing =
            pairing_actor_allowed(&registry, &candidates, actor_id, now_unix_ms);

        if allowed_by_allowlist && allowed_by_pairing {
            return Ok(MultiChannelPairingDecision::Allow {
                reason_code: ALLOW_ALLOWLIST_AND_PAIRING.to_string(),
            });
        }
        if allowed_by_allowlist {
            return Ok(MultiChannelPairingDecision::Allow {
                reason_code: ALLOW_ALLOWLIST.to_string(),
            });
        }
        if allowed_by_pairing {
            return Ok(MultiChannelPairingDecision::Allow {
                reason_code: ALLOW_PAIRING.to_string(),
            });
        }
        Ok(MultiChannelPairingDecision::Deny {
            reason_code: DENY_ACTOR_NOT_PAIRED_OR_ALLOWLISTED.to_string(),
        })
    }
}

fn pairing_paths_for_state_dir(state_dir: &Path) -> (PathBuf, PathBuf, bool) {
    let state_name = state_dir.file_name().and_then(|value| value.to_str());
    let tau_root = match state_name {
        Some("github")
        | Some("slack")
        | Some("events")
        | Some("channel-store")
        | Some("multi-channel") => state_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(state_dir),
        _ => state_dir,
    };
    let security_dir = tau_root.join("security");
    (
        security_dir.join("allowlist.json"),
        security_dir.join("pairings.json"),
        false,
    )
}

fn load_pairing_allowlist(path: &Path) -> Result<PairingAllowlistFile> {
    if !path.exists() {
        return Ok(PairingAllowlistFile {
            schema_version: 1,
            strict: false,
            channels: BTreeMap::new(),
        });
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let parsed = serde_json::from_str::<PairingAllowlistFile>(&raw)
        .with_context(|| format!("parse {}", path.display()))?;
    if parsed.schema_version != 1 {
        bail!(
            "unsupported allowlist schema_version {} in {} (expected 1)",
            parsed.schema_version,
            path.display()
        );
    }
    Ok(parsed)
}

fn load_pairing_registry(path: &Path) -> Result<PairingRegistryFile> {
    if !path.exists() {
        return Ok(PairingRegistryFile {
            schema_version: 1,
            pairings: Vec::new(),
        });
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let parsed = serde_json::from_str::<PairingRegistryFile>(&raw)
        .with_context(|| format!("parse {}", path.display()))?;
    if parsed.schema_version != 1 {
        bail!(
            "unsupported pairing schema_version {} in {} (expected 1)",
            parsed.schema_version,
            path.display()
        );
    }
    Ok(parsed)
}

fn channel_candidates(channel: &str) -> Vec<String> {
    let trimmed = channel.trim();
    if trimmed.is_empty() {
        return vec!["*".to_string()];
    }
    let mut candidates = vec![trimmed.to_string()];
    if let Some((prefix, _)) = trimmed.split_once(':') {
        if !prefix.is_empty() {
            candidates.push(prefix.to_string());
        }
    }
    candidates.push("*".to_string());
    candidates
}

fn channel_has_pairing_rules(
    allowlist: &PairingAllowlistFile,
    registry: &PairingRegistryFile,
    candidates: &[String],
) -> bool {
    let allowlist_has_entries = candidates.iter().any(|candidate| {
        allowlist
            .channels
            .get(candidate)
            .is_some_and(|actors| !actors.is_empty())
    });
    if allowlist_has_entries {
        return true;
    }
    registry
        .pairings
        .iter()
        .any(|entry| candidates.contains(&entry.channel))
}

fn allowlist_actor_allowed(
    allowlist: &PairingAllowlistFile,
    candidates: &[String],
    actor_id: &str,
) -> bool {
    candidates.iter().any(|candidate| {
        allowlist.channels.get(candidate).is_some_and(|actors| {
            actors
                .iter()
                .any(|actor| actor.trim().eq_ignore_ascii_case(actor_id))
        })
    })
}

fn pairing_actor_allowed(
    registry: &PairingRegistryFile,
    candidates: &[String],
    actor_id: &str,
    now_unix_ms: u64,
) -> bool {
    registry.pairings.iter().any(|entry| {
        candidates.contains(&entry.channel)
            && entry.actor_id.eq_ignore_ascii_case(actor_id)
            && !is_pairing_expired(entry, now_unix_ms)
    })
}

fn is_pairing_expired(entry: &PairingRecord, now_unix_ms: u64) -> bool {
    entry
        .expires_unix_ms
        .is_some_and(|expires| expires <= now_unix_ms)
}

fn approvals_root_for_state_dir(state_dir: &Path) -> PathBuf {
    let state_name = state_dir.file_name().and_then(|value| value.to_str());
    let tau_root = match state_name {
        Some("github")
        | Some("slack")
        | Some("events")
        | Some("channel-store")
        | Some("multi-channel") => state_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(state_dir),
        _ => state_dir,
    };
    tau_root.join("approvals")
}

fn execute_test_approvals_command(
    state_dir: &Path,
    args: &str,
    decision_actor: Option<&str>,
) -> Result<String> {
    let approvals_root = approvals_root_for_state_dir(state_dir);
    std::fs::create_dir_all(&approvals_root)
        .with_context(|| format!("create {}", approvals_root.display()))?;
    let store_path = approvals_root.join("requests.json");
    let mut store = if store_path.exists() {
        let raw = std::fs::read_to_string(&store_path)
            .with_context(|| format!("read {}", store_path.display()))?;
        serde_json::from_str::<Value>(&raw)
            .with_context(|| format!("parse {}", store_path.display()))?
    } else {
        serde_json::json!({
            "schema_version": 1,
            "next_request_id": 1,
            "requests": []
        })
    };

    let tokens = args.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("missing approvals action");
    }
    let action = tokens[0];
    let requests = store
        .get_mut("requests")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("approvals store missing requests array"))?;

    match action {
        "list" => {
            let mut status_filter: Option<&str> = None;
            let mut index = 1;
            while index < tokens.len() {
                if tokens[index] == "--status" {
                    status_filter = tokens.get(index + 1).copied();
                    index += 2;
                    continue;
                }
                index += 1;
            }
            let mut total = 0;
            let mut pending = 0;
            let mut approved = 0;
            let mut rejected = 0;
            let mut expired = 0;
            let mut consumed = 0;
            let mut lines = Vec::new();
            for request in requests.iter() {
                let status = request
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                total += 1;
                match status {
                    "pending" => pending += 1,
                    "approved" => approved += 1,
                    "rejected" => rejected += 1,
                    "expired" => expired += 1,
                    "consumed" => consumed += 1,
                    _ => {}
                }
                if status_filter.map(|filter| filter == status).unwrap_or(true) {
                    let id = request
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    lines.push(format!("request id={} status={}", id, status));
                }
            }
            let mut output = format!(
                    "approvals summary: total={} pending={} approved={} rejected={} expired={} consumed={}",
                    total, pending, approved, rejected, expired, consumed
                );
            if !lines.is_empty() {
                output.push('\n');
                output.push_str(&lines.join("\n"));
            }
            Ok(output)
        }
        "approve" | "reject" => {
            let request_id = tokens
                .get(1)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("missing approvals request id"))?;
            let reason = if tokens.len() > 2 {
                Some(tokens[2..].join(" "))
            } else {
                None
            };
            let mut target = None;
            for entry in requests.iter_mut() {
                if entry.get("id").and_then(Value::as_str) == Some(request_id) {
                    target = Some(entry);
                    break;
                }
            }
            let Some(entry) = target else {
                bail!("request {request_id} not found");
            };
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if status != "pending" {
                bail!("request {request_id} is not pending");
            }
            let new_status = if action == "approve" {
                "approved"
            } else {
                "rejected"
            };
            entry["status"] = Value::String(new_status.to_string());
            entry["decision_actor"] =
                Value::String(decision_actor.unwrap_or("local-command").to_string());
            entry["decision_reason"] = reason
                .as_deref()
                .map(|value| Value::String(value.to_string()))
                .unwrap_or(Value::Null);
            entry["decision_at_ms"] = Value::Number(current_unix_timestamp_ms().into());
            let payload =
                serde_json::to_string_pretty(&store).context("serialize approval store")?;
            std::fs::write(&store_path, payload)
                .with_context(|| format!("write {}", store_path.display()))?;
            Ok(format!(
                "approvals {action}: request {request_id} {new_status}"
            ))
        }
        _ => bail!("unknown approvals action '{action}'"),
    }
}

fn build_config(root: &Path) -> MultiChannelRuntimeConfig {
    MultiChannelRuntimeConfig {
        fixture_path: fixture_path("baseline-three-channel.json"),
        state_dir: root.join(".tau/multi-channel"),
        orchestrator_route_table_path: None,
        queue_limit: 64,
        processed_event_cap: 10_000,
        retry_max_attempts: 3,
        retry_base_delay_ms: 0,
        retry_jitter_ms: 0,
        outbound: MultiChannelOutboundConfig::default(),
        telemetry: MultiChannelTelemetryConfig::default(),
        media: crate::multi_channel_media::MultiChannelMediaUnderstandingConfig::default(),
        command_handlers: MultiChannelCommandHandlers {
            auth: Some(Arc::new(TestAuthHandler)),
            doctor: Some(Arc::new(TestDoctorHandler)),
            approvals: Some(Arc::new(TestApprovalsHandler)),
        },
        pairing_evaluator: Arc::new(FilePairingEvaluator),
    }
}

fn build_live_config(root: &Path) -> MultiChannelLiveRuntimeConfig {
    MultiChannelLiveRuntimeConfig {
        ingress_dir: root.join(".tau/multi-channel/live-ingress"),
        state_dir: root.join(".tau/multi-channel"),
        orchestrator_route_table_path: None,
        queue_limit: 64,
        processed_event_cap: 10_000,
        retry_max_attempts: 3,
        retry_base_delay_ms: 0,
        retry_jitter_ms: 0,
        outbound: MultiChannelOutboundConfig::default(),
        telemetry: MultiChannelTelemetryConfig::default(),
        media: crate::multi_channel_media::MultiChannelMediaUnderstandingConfig::default(),
        command_handlers: MultiChannelCommandHandlers {
            auth: Some(Arc::new(TestAuthHandler)),
            doctor: Some(Arc::new(TestDoctorHandler)),
            approvals: Some(Arc::new(TestApprovalsHandler)),
        },
        pairing_evaluator: Arc::new(FilePairingEvaluator),
    }
}

fn write_live_ingress_file(ingress_dir: &Path, transport: &str, fixture_name: &str) {
    std::fs::create_dir_all(ingress_dir).expect("create ingress directory");
    let file_name = format!("{transport}.ndjson");
    let fixture_raw =
        std::fs::read_to_string(live_fixture_path(fixture_name)).expect("read live fixture");
    let fixture_json: Value = serde_json::from_str(&fixture_raw).expect("parse fixture json");
    let fixture_line = serde_json::to_string(&fixture_json).expect("serialize fixture line");
    std::fs::write(ingress_dir.join(file_name), format!("{fixture_line}\n"))
        .expect("write ingress file");
}

fn write_pairing_allowlist(root: &Path, payload: &str) {
    let security_dir = root.join(".tau/security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(security_dir.join("allowlist.json"), payload).expect("write allowlist");
}

fn write_pairing_registry(root: &Path, payload: &str) {
    let security_dir = root.join(".tau/security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(security_dir.join("pairings.json"), payload).expect("write pairings");
}

fn secure_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[17; 32])
}

fn write_signed_envelope_trust_root(root: &Path, key_id: &str, signer: &SigningKey) {
    let security_dir = root.join(".tau/security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    save_trust_root_records(
        &security_dir.join("trust-roots.json"),
        &[TrustedRootRecord {
            id: key_id.to_string(),
            public_key: BASE64.encode(signer.verifying_key().to_bytes()),
            revoked: false,
            expires_unix: None,
            rotated_from: None,
        }],
    )
    .expect("write trust roots");
}

fn attach_signed_envelope(
    event: &mut MultiChannelInboundEvent,
    key_id: &str,
    nonce: &str,
    signer: &SigningKey,
) {
    let policy_channel = format!(
        "{}:{}",
        event.transport.as_str(),
        event.conversation_id.trim()
    );
    let message = signed_envelope_message_bytes(
        &policy_channel,
        event.actor_id.as_str(),
        event.event_id.as_str(),
        event.timestamp_ms,
        nonce,
        event.text.as_str(),
    );
    let signature = BASE64.encode(signer.sign(&message).to_bytes());
    event.metadata.insert(
        SIGNED_ENVELOPE_METADATA_KEY.to_string(),
        json!({
            "schema_version": 1,
            "key_id": key_id,
            "nonce": nonce,
            "timestamp_ms": event.timestamp_ms,
            "channel": policy_channel,
            "actor_id": event.actor_id.clone(),
            "event_id": event.event_id.clone(),
            "signature": signature,
        }),
    );
}

fn write_channel_policy(root: &Path, payload: &str) {
    let security_dir = root.join(".tau/security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(
        security_dir.join(crate::multi_channel_policy::MULTI_CHANNEL_POLICY_FILE_NAME),
        payload,
    )
    .expect("write channel policy");
}

fn write_approval_policy(root: &Path, payload: &str) {
    let approvals_dir = root.join(".tau/approvals");
    std::fs::create_dir_all(&approvals_dir).expect("create approvals dir");
    std::fs::write(approvals_dir.join("policy.json"), payload).expect("write approval policy");
}

fn write_approval_store(root: &Path, payload: &str) {
    let approvals_dir = root.join(".tau/approvals");
    std::fs::create_dir_all(&approvals_dir).expect("create approvals dir");
    std::fs::write(approvals_dir.join("requests.json"), payload).expect("write approval store");
}

fn write_multi_channel_route_bindings(root: &Path, payload: &str) {
    let security_dir = root.join(".tau/multi-channel/security");
    std::fs::create_dir_all(&security_dir).expect("create multi-channel security dir");
    std::fs::write(
        security_dir.join(crate::multi_channel_routing::MULTI_CHANNEL_ROUTE_BINDINGS_FILE_NAME),
        payload,
    )
    .expect("write multi-channel route bindings");
}

fn write_orchestrator_route_table(path: &Path, payload: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create orchestrator route table parent");
    }
    std::fs::write(path, payload).expect("write orchestrator route table");
}

fn sample_event(
    transport: MultiChannelTransport,
    event_id: &str,
    conversation_id: &str,
    actor_id: &str,
    text: &str,
) -> MultiChannelInboundEvent {
    MultiChannelInboundEvent {
        schema_version: 1,
        transport,
        event_kind: MultiChannelEventKind::Message,
        event_id: event_id.to_string(),
        conversation_id: conversation_id.to_string(),
        thread_id: String::new(),
        actor_id: actor_id.to_string(),
        actor_display: String::new(),
        timestamp_ms: 1_760_200_000_000,
        text: text.to_string(),
        attachments: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

#[test]
fn unit_retry_delay_ms_scales_with_attempt_number() {
    assert_eq!(retry_delay_ms(0, 0, 1, "seed"), 0);
    assert_eq!(retry_delay_ms(10, 0, 1, "seed"), 10);
    assert_eq!(retry_delay_ms(10, 0, 2, "seed"), 20);
    assert_eq!(retry_delay_ms(10, 0, 3, "seed"), 40);
}

#[test]
fn unit_retry_delay_ms_jitter_is_deterministic_for_seed() {
    let first = retry_delay_ms(20, 15, 2, "event-1");
    let second = retry_delay_ms(20, 15, 2, "event-1");
    assert_eq!(first, second);
    assert!(first >= 40);
    assert!(first <= 55);
}

#[test]
fn unit_pairing_policy_channel_includes_transport_prefix() {
    let event = sample_event(
        MultiChannelTransport::Discord,
        "evt-1",
        "ops-room",
        "discord-user-1",
        "hello",
    );
    assert_eq!(super::pairing_policy_channel(&event), "discord:ops-room");
}

#[test]
fn unit_should_emit_typing_presence_lifecycle_respects_threshold_and_force_flag() {
    let mut event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-telemetry-1",
        "chat-telemetry",
        "telegram-user-1",
        "hello",
    );
    let config = MultiChannelTelemetryConfig {
        typing_presence_min_response_chars: 200,
        ..MultiChannelTelemetryConfig::default()
    };
    assert!(!super::should_emit_typing_presence_lifecycle(
        &event,
        "short response",
        &config,
    ));

    event.metadata.insert(
        "telemetry_force_typing_presence".to_string(),
        Value::Bool(true),
    );
    assert!(super::should_emit_typing_presence_lifecycle(
        &event,
        "short response",
        &config,
    ));
}

#[test]
fn unit_extract_usage_estimated_cost_micros_parses_supported_metadata_fields() {
    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-cost-1",
        "ops-room",
        "discord-user-1",
        "cost metadata",
    );
    event
        .metadata
        .insert("usage_cost_usd".to_string(), Value::from(0.00125_f64));
    assert_eq!(
        super::extract_usage_estimated_cost_micros(&event),
        Some(1250)
    );

    event
        .metadata
        .insert("usage_cost_micros".to_string(), Value::from(777_u64));
    assert_eq!(
        super::extract_usage_estimated_cost_micros(&event),
        Some(777)
    );
}

#[test]
fn unit_parse_multi_channel_tau_command_supports_initial_command_set() {
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau").expect("parse"),
        Some(super::MultiChannelTauCommand::Help)
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau help").expect("parse"),
        Some(super::MultiChannelTauCommand::Help)
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau status").expect("parse"),
        Some(super::MultiChannelTauCommand::Status)
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau auth status openai").expect("parse"),
        Some(super::MultiChannelTauCommand::AuthStatus {
            provider: Some("openai".to_string())
        })
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau doctor --online").expect("parse"),
        Some(super::MultiChannelTauCommand::Doctor { online: true })
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau approvals list --json --status pending")
            .expect("parse"),
        Some(super::MultiChannelTauCommand::Approvals {
            action: super::MultiChannelTauApprovalsAction::List,
            args: "list --json --status pending".to_string(),
        })
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau approvals approve req-7 looks_safe")
            .expect("parse"),
        Some(super::MultiChannelTauCommand::Approvals {
            action: super::MultiChannelTauApprovalsAction::Approve,
            args: "approve req-7 looks_safe".to_string(),
        })
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau approvals reject req-8 blocked")
            .expect("parse"),
        Some(super::MultiChannelTauCommand::Approvals {
            action: super::MultiChannelTauApprovalsAction::Reject,
            args: "reject req-8 blocked".to_string(),
        })
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("plain text").expect("parse"),
        None
    );
}

#[test]
fn regression_parse_multi_channel_tau_command_rejects_invalid_forms() {
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau auth").expect_err("invalid args"),
        "command_invalid_args"
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau auth login").expect_err("invalid args"),
        "command_invalid_args"
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau auth status mystery")
            .expect_err("invalid provider"),
        "command_invalid_args"
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau unknown").expect_err("unknown command"),
        "command_unknown"
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau approvals").expect_err("missing action"),
        "command_invalid_args"
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau approvals list --status maybe")
            .expect_err("invalid status"),
        "command_invalid_args"
    );
    assert_eq!(
        super::parse_multi_channel_tau_command("/tau approvals approve")
            .expect_err("missing request id"),
        "command_invalid_args"
    );
}

#[tokio::test]
async fn functional_runner_executes_tau_status_command_and_persists_command_metadata() {
    let temp = tempdir().expect("tempdir");
    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-command-status-1",
        "telegram-command-room",
        "telegram-user-1",
        "/tau status",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "telegram-command-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["schema"].as_str(),
        Some("multi_channel_tau_command_v1")
    );
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("reported")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_status_reported")
    );
    let response = command_entry.payload["response"]
        .as_str()
        .expect("response string");
    assert!(response.contains("Tau command `/tau status`"));
    assert!(response.contains("reason_code `command_status_reported`"));
}

#[tokio::test]
async fn integration_runner_tau_doctor_requires_allowlisted_operator_scope() {
    let temp = tempdir().expect("tempdir");
    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Discord,
        "dc-command-doctor-1",
        "discord-command-room",
        "discord-user-1",
        "/tau doctor",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "discord-command-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("failed")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_rbac_denied")
    );
    let response = command_entry.payload["response"]
        .as_str()
        .expect("response");
    assert!(response.contains("command denied"));
}

#[tokio::test]
async fn integration_runner_executes_tau_doctor_for_allowlisted_operator() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "discord:discord-command-room": ["discord-allowed-user"]
  }
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Discord,
        "dc-command-doctor-allow-1",
        "discord-command-room",
        "discord-allowed-user",
        "/tau doctor",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "discord-command-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("reported")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_doctor_reported")
    );
}

#[tokio::test]
async fn functional_runner_executes_tau_approvals_list_for_allowlisted_operator() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
    );
    write_approval_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": []
}
"#,
    );
    write_approval_store(
        temp.path(),
        r#"{
  "schema_version": 1,
  "next_request_id": 2,
  "requests": [
    {
      "id": "req-1",
      "rule_id": "command-review",
      "action_kind": "command",
      "action_summary": "command name=/danger args='now'",
      "fingerprint": "seed",
      "status": "pending",
      "created_at_ms": 1,
      "expires_at_ms": 9999999999999,
      "decision_at_ms": null,
      "decision_reason": null,
      "decision_actor": null,
      "consumed_at_ms": null
    }
  ]
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-command-approvals-list-1",
        "approval-room",
        "telegram-operator",
        "/tau approvals list --status pending",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "approval-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("reported")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_approvals_list_reported")
    );
    let response = command_entry.payload["response"]
        .as_str()
        .expect("response");
    assert!(response.contains("approvals summary:"));
    assert!(response.contains("req-1"));
}

#[tokio::test]
async fn integration_runner_executes_tau_approvals_approve_and_persists_actor_mapping() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
    );
    write_approval_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": [
    {
      "id": "command-review",
      "action": "command",
      "command_names": ["/danger"]
    }
  ]
}
"#,
    );
    write_approval_store(
        temp.path(),
        r#"{
  "schema_version": 1,
  "next_request_id": 2,
  "requests": [
    {
      "id": "req-42",
      "rule_id": "command-review",
      "action_kind": "command",
      "action_summary": "command name=/danger args='now'",
      "fingerprint": "seed",
      "status": "pending",
      "created_at_ms": 1,
      "expires_at_ms": 9999999999999,
      "decision_at_ms": null,
      "decision_reason": null,
      "decision_actor": null,
      "consumed_at_ms": null
    }
  ]
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-command-approvals-approve-1",
        "approval-room",
        "telegram-operator",
        "/tau approvals approve req-42 approved_from_channel",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "approval-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("reported")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_approvals_approved")
    );

    let raw_store = std::fs::read_to_string(temp.path().join(".tau/approvals/requests.json"))
        .expect("read approval store");
    let store_json: Value = serde_json::from_str(&raw_store).expect("parse approval store");
    let request = store_json["requests"]
        .as_array()
        .expect("requests array")
        .iter()
        .find(|entry| entry["id"].as_str() == Some("req-42"))
        .expect("approval request");
    assert_eq!(request["status"].as_str(), Some("approved"));
    assert_eq!(
        request["decision_actor"].as_str(),
        Some("telegram:approval-room:telegram-operator")
    );
}

#[tokio::test]
async fn regression_runner_tau_approvals_approve_unknown_request_fails_closed() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
    );
    write_approval_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": []
}
"#,
    );
    write_approval_store(
        temp.path(),
        r#"{
  "schema_version": 1,
  "next_request_id": 1,
  "requests": []
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-command-approvals-unknown-1",
        "approval-room",
        "telegram-operator",
        "/tau approvals approve req-404 blocked",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "approval-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("failed")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_approvals_unknown_request")
    );
    let response = command_entry.payload["response"]
        .as_str()
        .expect("response");
    assert!(response.contains("approvals error:"));
}

#[tokio::test]
async fn regression_runner_tau_approvals_reject_stale_request_fails_closed() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
    );
    write_approval_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": []
}
"#,
    );
    write_approval_store(
        temp.path(),
        r#"{
  "schema_version": 1,
  "next_request_id": 2,
  "requests": [
    {
      "id": "req-9",
      "rule_id": "command-review",
      "action_kind": "command",
      "action_summary": "command name=/danger args='now'",
      "fingerprint": "seed",
      "status": "approved",
      "created_at_ms": 1,
      "expires_at_ms": 9999999999999,
      "decision_at_ms": 10,
      "decision_reason": "already approved",
      "decision_actor": "local-command",
      "consumed_at_ms": null
    }
  ]
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-command-approvals-stale-1",
        "approval-room",
        "telegram-operator",
        "/tau approvals reject req-9 blocked",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "approval-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("failed")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_approvals_stale_request")
    );
}

#[tokio::test]
async fn regression_runner_reports_unknown_tau_command_with_failure_reason_code() {
    let temp = tempdir().expect("tempdir");
    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Whatsapp,
        "wa-command-unknown-1",
        "whatsapp-command-room",
        "whatsapp-user-1",
        "/tau unknown",
    );

    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "whatsapp",
        "whatsapp-command-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let command_entry = logs
        .iter()
        .find(|entry| {
            entry.direction == "outbound"
                && entry
                    .payload
                    .get("command")
                    .and_then(Value::as_object)
                    .is_some()
        })
        .expect("command outbound log entry");
    assert_eq!(
        command_entry.payload["command"]["status"].as_str(),
        Some("failed")
    );
    assert_eq!(
        command_entry.payload["command"]["reason_code"].as_str(),
        Some("command_unknown")
    );
    let response = command_entry.payload["response"]
        .as_str()
        .expect("response");
    assert!(response.contains("/tau help"));
}

#[tokio::test]
async fn functional_runner_processes_fixture_and_persists_channel_store_entries() {
    let temp = tempdir().expect("tempdir");
    let config = build_config(temp.path());
    let fixture =
        load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");
    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

    assert_eq!(summary.discovered_events, 3);
    assert_eq!(summary.queued_events, 3);
    assert_eq!(summary.completed_events, 3);
    assert_eq!(summary.duplicate_skips, 0);
    assert_eq!(summary.failed_events, 0);
    assert_eq!(summary.policy_checked_events, 3);
    assert_eq!(summary.policy_enforced_events, 0);
    assert_eq!(summary.policy_allowed_events, 3);
    assert_eq!(summary.policy_denied_events, 0);

    let state =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("load state");
    assert_eq!(state.health.last_cycle_discovered, 3);
    assert_eq!(state.health.last_cycle_completed, 3);
    assert_eq!(state.health.last_cycle_failed, 0);
    assert_eq!(state.health.failure_streak, 0);
    assert_eq!(state.health.classify().state, TransportHealthState::Healthy);

    let events_log = std::fs::read_to_string(
        config
            .state_dir
            .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
    )
    .expect("read runtime events log");
    assert!(events_log.contains("healthy_cycle"));
    assert!(events_log.contains("\"health_state\":\"healthy\""));
    assert!(events_log.contains("pairing_policy_permissive"));

    for event in &fixture.events {
        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            event.transport.as_str(),
            &event.conversation_id,
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let context = store.load_context_entries().expect("load context");
        assert_eq!(logs.len(), 2);
        assert!(context.len() >= 2);
        assert_eq!(
            logs[0].payload["pairing"]["decision"].as_str(),
            Some("allow")
        );
        assert_eq!(
            logs[0].payload["pairing"]["reason_code"].as_str(),
            Some("allow_permissive_mode")
        );
    }
}

#[tokio::test]
async fn integration_runner_media_understanding_enriches_context_and_logs_reason_codes() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.outbound.mode = MultiChannelOutboundMode::DryRun;
    config.media.max_attachments_per_event = 3;
    config.media.max_summary_chars = 96;

    let mut event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-media-1",
        "chat-media",
        "telegram-user-1",
        "please inspect media",
    );
    event.attachments = vec![
        MultiChannelAttachment {
            attachment_id: "img-1".to_string(),
            url: "https://example.com/image.png".to_string(),
            content_type: "image/png".to_string(),
            file_name: "image.png".to_string(),
            size_bytes: 128,
        },
        MultiChannelAttachment {
            attachment_id: "aud-1".to_string(),
            url: "https://example.com/voice.wav".to_string(),
            content_type: "audio/wav".to_string(),
            file_name: "voice.wav".to_string(),
            size_bytes: 256,
        },
        MultiChannelAttachment {
            attachment_id: "doc-1".to_string(),
            url: "https://example.com/readme.txt".to_string(),
            content_type: "text/plain".to_string(),
            file_name: "readme.txt".to_string(),
            size_bytes: 32,
        },
    ];

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "telegram",
        "chat-media",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].payload["media_understanding"]["processed"], 2);
    assert_eq!(logs[0].payload["media_understanding"]["skipped"], 1);
    assert_eq!(
        logs[0].payload["media_understanding"]["reason_code_counts"]
            ["media_unsupported_attachment_type"],
        1
    );

    let context = store.load_context_entries().expect("load context");
    assert_eq!(context[0].role, "user");
    assert!(context[0].text.contains("Media understanding outcomes:"));
    assert!(context[0].text.contains("attachment_id=img-1"));
    assert!(context[0]
        .text
        .contains("reason_code=media_image_described"));
}

#[tokio::test]
async fn regression_runner_media_understanding_is_bounded_and_duplicate_event_idempotent() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.outbound.mode = MultiChannelOutboundMode::DryRun;
    config.media.max_attachments_per_event = 1;

    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-media-bounded-1",
        "discord-media-room",
        "discord-user-1",
        "bounded media processing",
    );
    event.attachments = vec![
        MultiChannelAttachment {
            attachment_id: "img-1".to_string(),
            url: "https://example.com/image.png".to_string(),
            content_type: "image/png".to_string(),
            file_name: "image.png".to_string(),
            size_bytes: 128,
        },
        MultiChannelAttachment {
            attachment_id: "vid-1".to_string(),
            url: "https://example.com/video.mp4".to_string(),
            content_type: "video/mp4".to_string(),
            file_name: "video.mp4".to_string(),
            size_bytes: 2048,
        },
    ];

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let first = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("first run");
    let second = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("second run");

    assert_eq!(first.completed_events, 1);
    assert_eq!(second.duplicate_skips, 1);

    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "discord",
        "discord-media-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].payload["media_understanding"]["processed"], 1);
    assert_eq!(logs[0].payload["media_understanding"]["skipped"], 1);
    assert_eq!(
        logs[0].payload["media_understanding"]["reason_code_counts"]
            ["media_attachment_limit_exceeded"],
        1
    );
}

#[tokio::test]
async fn functional_runner_allows_allowlisted_actor_in_strict_mode() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:chat-allow": ["telegram-allowed-user"]
  }
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let events = vec![sample_event(
        MultiChannelTransport::Telegram,
        "tg-allow-1",
        "chat-allow",
        "telegram-allowed-user",
        "allowlist actor",
    )];
    let summary = runtime.run_once_events(&events).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);
    assert_eq!(summary.policy_checked_events, 1);
    assert_eq!(summary.policy_enforced_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);
    assert_eq!(summary.policy_denied_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "chat-allow",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let context = store.load_context_entries().expect("load context");
    assert_eq!(logs.len(), 2);
    assert_eq!(context.len(), 2);
    assert_eq!(
        logs[0].payload["pairing"]["reason_code"].as_str(),
        Some("allow_allowlist")
    );

    let events_log = std::fs::read_to_string(
        temp.path()
            .join(".tau/multi-channel")
            .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
    )
    .expect("read events log");
    assert!(events_log.contains("pairing_policy_enforced"));
}

#[tokio::test]
async fn integration_runner_denies_unpaired_actor_in_strict_mode() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {}
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let events = vec![sample_event(
        MultiChannelTransport::Discord,
        "dc-deny-1",
        "discord-room-deny",
        "discord-unknown-user",
        "restricted actor",
    )];
    let summary = runtime.run_once_events(&events).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);
    assert_eq!(summary.policy_checked_events, 1);
    assert_eq!(summary.policy_enforced_events, 1);
    assert_eq!(summary.policy_allowed_events, 0);
    assert_eq!(summary.policy_denied_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "discord-room-deny",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let context = store.load_context_entries().expect("load context");
    assert_eq!(logs.len(), 2);
    assert_eq!(context.len(), 0);
    assert_eq!(logs[1].payload["status"].as_str(), Some("denied"));
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_actor_not_paired_or_allowlisted")
    );

    let events_log = std::fs::read_to_string(
        temp.path()
            .join(".tau/multi-channel")
            .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
    )
    .expect("read events log");
    assert!(events_log.contains("pairing_policy_denied_events"));
}

#[tokio::test]
async fn functional_runner_secure_messaging_preferred_falls_back_to_legacy_pairing_on_missing_envelope(
) {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": false,
  "secureMessaging": {
    "mode": "preferred",
    "timestampSkewSeconds": 300,
    "replayWindowSeconds": 300
  },
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {}
}
"#,
    );
    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Discord,
        "dc-secure-fallback-1",
        "secure-fallback-room",
        "discord-user-fallback",
        "legacy fallback path",
    );

    let summary = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);
    assert_eq!(summary.policy_denied_events, 0);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "secure-fallback-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(
        logs[0].payload["secure_messaging"]["status"].as_str(),
        Some("missing")
    );
    assert_eq!(
        logs[0].payload["secure_messaging"]["legacy_fallback"].as_bool(),
        Some(true)
    );
    assert_eq!(
        logs[0].payload["pairing"]["reason_code"].as_str(),
        Some("allow_permissive_mode")
    );
}

#[tokio::test]
async fn integration_runner_secure_messaging_required_allows_valid_signed_envelope() {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": true,
  "secureMessaging": {
    "mode": "required",
    "timestampSkewSeconds": 300,
    "replayWindowSeconds": 300
  },
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {}
}
"#,
    );
    let signer = secure_signing_key();
    write_signed_envelope_trust_root(temp.path(), "secure-root-v1", &signer);

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let mut event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-secure-allow-1",
        "secure-allow-room",
        "telegram-secure-user",
        "signed message",
    );
    event.timestamp_ms = current_unix_timestamp_ms();
    attach_signed_envelope(
        &mut event,
        "secure-root-v1",
        "nonce-secure-allow-1",
        &signer,
    );

    let summary = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);
    assert_eq!(summary.policy_denied_events, 0);
    assert_eq!(summary.policy_enforced_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "secure-allow-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(
        logs[0].payload["secure_messaging"]["status"].as_str(),
        Some("allow")
    );
    assert_eq!(
        logs[0].payload["secure_messaging"]["reason_code"].as_str(),
        Some("allow_signed_envelope_verified")
    );
    assert_eq!(
        logs[0].payload["secure_messaging"]["mode"].as_str(),
        Some("required")
    );
}

#[tokio::test]
async fn regression_runner_secure_messaging_required_rejects_forged_signature() {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": true,
  "secureMessaging": {
    "mode": "required",
    "timestampSkewSeconds": 300,
    "replayWindowSeconds": 300
  },
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {}
}
"#,
    );
    let signer = secure_signing_key();
    write_signed_envelope_trust_root(temp.path(), "secure-root-v1", &signer);

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-secure-forged-1",
        "secure-forged-room",
        "discord-secure-user",
        "original signed body",
    );
    event.timestamp_ms = current_unix_timestamp_ms();
    attach_signed_envelope(
        &mut event,
        "secure-root-v1",
        "nonce-secure-forged-1",
        &signer,
    );
    event.text = "tampered body".to_string();

    let summary = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_allowed_events, 0);
    assert_eq!(summary.policy_denied_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "secure-forged-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[1].payload["status"].as_str(), Some("denied"));
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_signed_envelope_invalid_signature")
    );
}

#[tokio::test]
async fn regression_runner_secure_messaging_required_rejects_replayed_nonce() {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": true,
  "secureMessaging": {
    "mode": "required",
    "timestampSkewSeconds": 300,
    "replayWindowSeconds": 300
  },
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {}
}
"#,
    );
    let signer = secure_signing_key();
    write_signed_envelope_trust_root(temp.path(), "secure-root-v1", &signer);

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let timestamp_ms = current_unix_timestamp_ms();
    let mut first = sample_event(
        MultiChannelTransport::Telegram,
        "tg-secure-replay-1",
        "secure-replay-room",
        "telegram-secure-user",
        "first signed message",
    );
    first.timestamp_ms = timestamp_ms;
    attach_signed_envelope(&mut first, "secure-root-v1", "nonce-shared-replay", &signer);

    let mut second = sample_event(
        MultiChannelTransport::Telegram,
        "tg-secure-replay-2",
        "secure-replay-room",
        "telegram-secure-user",
        "second signed message",
    );
    second.timestamp_ms = timestamp_ms.saturating_add(1);
    attach_signed_envelope(
        &mut second,
        "secure-root-v1",
        "nonce-shared-replay",
        &signer,
    );

    let summary = runtime
        .run_once_events(&[first, second])
        .await
        .expect("run once");
    assert_eq!(summary.completed_events, 2);
    assert_eq!(summary.policy_allowed_events, 1);
    assert_eq!(summary.policy_denied_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "secure-replay-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert!(logs.iter().any(|entry| {
        entry.direction == "outbound"
            && entry.payload["status"].as_str() == Some("denied")
            && entry.payload["reason_code"].as_str() == Some("deny_signed_envelope_replay")
    }));
}

#[tokio::test]
async fn integration_runner_denies_group_message_when_mention_required() {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "discord:ops-room": {
      "dmPolicy": "allow",
      "allowFrom": "any",
      "groupPolicy": "allow",
      "requireMention": true
    }
  }
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-mention-1",
        "ops-room",
        "discord-user-1",
        "hello team",
    );
    event
        .metadata
        .insert("guild_id".to_string(), Value::String("guild-1".to_string()));
    let summary = runtime.run_once_events(&[event]).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_denied_events, 1);
    assert_eq!(summary.policy_enforced_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "ops-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_channel_policy_mention_required")
    );
}

#[tokio::test]
async fn integration_runner_allows_group_message_when_mention_present_and_allow_from_any() {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "discord:ops-room": {
      "dmPolicy": "allow",
      "allowFrom": "any",
      "groupPolicy": "allow",
      "requireMention": true
    }
  }
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-mention-2",
        "ops-room",
        "discord-user-1",
        "@tau deploy status",
    );
    event
        .metadata
        .insert("guild_id".to_string(), Value::String("guild-1".to_string()));
    let summary = runtime.run_once_events(&[event]).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);
    assert_eq!(summary.policy_denied_events, 0);
    assert_eq!(summary.policy_enforced_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "discord",
        "ops-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs[0].payload["pairing"]["checked"].as_bool(), Some(false));
    assert_eq!(
        logs[0].payload["channel_policy"]["reason_code"].as_str(),
        Some("allow_channel_policy_allow_from_any")
    );
}

#[tokio::test]
async fn integration_runner_retries_transient_failure_then_recovers() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.retry_max_attempts = 4;
    let fixture_raw = r#"{
  "schema_version": 1,
  "name": "transient-retry",
  "events": [
    {
      "schema_version": 1,
      "transport": "telegram",
      "event_kind": "message",
      "event_id": "tg-transient-1",
      "conversation_id": "telegram-chat-transient",
      "actor_id": "telegram-user-1",
      "timestamp_ms": 1760100000000,
      "text": "hello",
      "metadata": { "simulate_transient_failures": 1 }
    }
  ]
}"#;
    let fixture = parse_multi_channel_contract_fixture(fixture_raw).expect("parse fixture");
    let mut runtime = MultiChannelRuntime::new(config).expect("runtime");
    let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.transient_failures, 1);
    assert_eq!(summary.retry_attempts, 1);
    assert_eq!(summary.failed_events, 0);
    assert_eq!(summary.policy_checked_events, 1);
    assert_eq!(summary.policy_allowed_events, 1);
}

#[tokio::test]
async fn functional_runner_dry_run_outbound_records_delivery_receipts() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.outbound.mode = MultiChannelOutboundMode::DryRun;
    config.outbound.max_chars = 12;

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-dry-run-1",
        "chat-dry-run",
        "telegram-user-1",
        "hello with dry run",
    );
    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "telegram",
        "chat-dry-run",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[1].payload["delivery"]["mode"], "dry_run");
    let receipts = logs[1].payload["delivery"]["receipts"]
        .as_array()
        .expect("delivery receipts");
    assert!(!receipts.is_empty());
    assert_eq!(receipts[0]["status"], "dry_run");
}

#[tokio::test]
async fn functional_runner_emits_typing_presence_telemetry_for_long_replies_in_dry_run_mode() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.outbound.mode = MultiChannelOutboundMode::DryRun;
    config.telemetry.typing_presence_min_response_chars = 1;

    let events = vec![
        sample_event(
            MultiChannelTransport::Telegram,
            "tg-typing-1",
            "chat-typing-telegram",
            "telegram-user-1",
            "hello",
        ),
        sample_event(
            MultiChannelTransport::Discord,
            "dc-typing-1",
            "chat-typing-discord",
            "discord-user-1",
            "hello",
        ),
        sample_event(
            MultiChannelTransport::Whatsapp,
            "wa-typing-1",
            "chat-typing-whatsapp",
            "15550001111",
            "hello",
        ),
    ];

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime.run_once_events(&events).await.expect("run once");
    assert_eq!(summary.completed_events, 3);
    assert_eq!(summary.typing_events_emitted, 6);
    assert_eq!(summary.presence_events_emitted, 6);
    assert_eq!(summary.usage_summary_records, 3);

    let state =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("load state");
    assert_eq!(state.telemetry.typing_events_emitted, 6);
    assert_eq!(state.telemetry.presence_events_emitted, 6);
    assert_eq!(state.telemetry.usage_summary_records, 3);
    assert_eq!(
        state.telemetry.typing_events_by_transport.get("telegram"),
        Some(&2)
    );
    assert_eq!(
        state.telemetry.typing_events_by_transport.get("discord"),
        Some(&2)
    );
    assert_eq!(
        state.telemetry.typing_events_by_transport.get("whatsapp"),
        Some(&2)
    );

    for event in &events {
        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            event.transport.as_str(),
            event.conversation_id.as_str(),
        )
        .expect("open channel store");
        let logs = store.load_log_entries().expect("load logs");
        assert!(logs.iter().any(|entry| {
            entry.payload.get("status").and_then(Value::as_str) == Some("typing_started")
        }));
        assert!(logs.iter().any(|entry| {
            entry.payload.get("status").and_then(Value::as_str) == Some("typing_stopped")
        }));
        assert!(logs.iter().any(|entry| {
            entry.payload.get("status").and_then(Value::as_str) == Some("presence_active")
        }));
        assert!(logs.iter().any(|entry| {
            entry.payload.get("status").and_then(Value::as_str) == Some("presence_idle")
        }));
    }
}

#[tokio::test]
async fn integration_runner_provider_outbound_posts_per_transport_adapter() {
    struct Scenario<'a> {
        transport: MultiChannelTransport,
        event_id: &'a str,
        conversation_id: &'a str,
        actor_id: &'a str,
        expected_path: &'a str,
        response_body: &'a str,
    }

    let scenarios = vec![
        Scenario {
            transport: MultiChannelTransport::Telegram,
            event_id: "tg-provider-1",
            conversation_id: "chat-200",
            actor_id: "telegram-user-1",
            expected_path: "/bottelegram-token/sendMessage",
            response_body: r#"{"ok":true,"result":{"message_id":42}}"#,
        },
        Scenario {
            transport: MultiChannelTransport::Discord,
            event_id: "dc-provider-1",
            conversation_id: "discord-room-1",
            actor_id: "discord-user-1",
            expected_path: "/channels/discord-room-1/messages",
            response_body: r#"{"id":"msg-22"}"#,
        },
        Scenario {
            transport: MultiChannelTransport::Whatsapp,
            event_id: "wa-provider-1",
            conversation_id: "whatsapp-room-1",
            actor_id: "15550001111",
            expected_path: "/15551234567/messages",
            response_body: r#"{"messages":[{"id":"wamid.1"}]}"#,
        },
    ];

    for scenario in scenarios {
        let server = MockServer::start();
        let sent = server.mock(|when, then| {
            when.method(POST).path(scenario.expected_path);
            then.status(200)
                .header("content-type", "application/json")
                .body(scenario.response_body);
        });

        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.outbound.mode = MultiChannelOutboundMode::Provider;
        config.outbound.http_timeout_ms = 3_000;
        match scenario.transport {
            MultiChannelTransport::Telegram => {
                config.outbound.telegram_api_base = server.base_url();
                config.outbound.telegram_bot_token = Some("telegram-token".to_string());
            }
            MultiChannelTransport::Discord => {
                config.outbound.discord_api_base = server.base_url();
                config.outbound.discord_bot_token = Some("discord-token".to_string());
            }
            MultiChannelTransport::Whatsapp => {
                config.outbound.whatsapp_api_base = server.base_url();
                config.outbound.whatsapp_access_token = Some("whatsapp-token".to_string());
                config.outbound.whatsapp_phone_number_id = Some("15551234567".to_string());
            }
        }
        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let event = sample_event(
            scenario.transport,
            scenario.event_id,
            scenario.conversation_id,
            scenario.actor_id,
            "provider integration event",
        );
        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);
        assert_eq!(summary.usage_summary_records, 1);
        sent.assert_calls(1);

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            scenario.transport.as_str(),
            scenario.conversation_id,
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(logs[1].payload["delivery"]["mode"], "provider");
        assert_eq!(logs[1].payload["delivery"]["receipts"][0]["status"], "sent");

        let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.telemetry.usage_summary_records, 1);
        assert_eq!(
            state
                .telemetry
                .usage_summary_records_by_transport
                .get(scenario.transport.as_str()),
            Some(&1)
        );
    }
}

#[tokio::test]
async fn regression_runner_provider_outbound_duplicate_event_suppresses_second_send() {
    let server = MockServer::start();
    let sent = server.mock(|when, then| {
        when.method(POST).path("/bottelegram-token/sendMessage");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"ok":true,"result":{"message_id":42}}"#);
    });

    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.outbound.mode = MultiChannelOutboundMode::Provider;
    config.outbound.telegram_api_base = server.base_url();
    config.outbound.telegram_bot_token = Some("telegram-token".to_string());
    config.telemetry.typing_presence_min_response_chars = 1;
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-dup-provider-1",
        "chat-dup-provider",
        "telegram-user-1",
        "duplicate suppression",
    );

    let mut runtime = MultiChannelRuntime::new(config).expect("runtime");
    let first = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("first run");
    let second = runtime
        .run_once_events(std::slice::from_ref(&event))
        .await
        .expect("second run");

    assert_eq!(first.completed_events, 1);
    assert_eq!(first.typing_events_emitted, 2);
    assert_eq!(first.presence_events_emitted, 2);
    assert_eq!(first.usage_summary_records, 1);
    assert_eq!(second.duplicate_skips, 1);
    assert_eq!(second.typing_events_emitted, 0);
    assert_eq!(second.presence_events_emitted, 0);
    assert_eq!(second.usage_summary_records, 0);
    sent.assert_calls(1);

    let state =
        load_multi_channel_runtime_state(&temp.path().join(".tau/multi-channel/state.json"))
            .expect("load state");
    assert_eq!(state.telemetry.typing_events_emitted, 2);
    assert_eq!(state.telemetry.presence_events_emitted, 2);
    assert_eq!(state.telemetry.usage_summary_records, 1);
}

#[tokio::test]
async fn regression_runner_provider_outbound_retry_exhaustion_surfaces_reason_code() {
    let server = MockServer::start();
    let failed = server.mock(|when, then| {
        when.method(POST).path("/bottelegram-token/sendMessage");
        then.status(503)
            .header("content-type", "application/json")
            .body(r#"{"error":"unavailable"}"#);
    });

    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.retry_max_attempts = 2;
    config.outbound.mode = MultiChannelOutboundMode::Provider;
    config.outbound.telegram_api_base = server.base_url();
    config.outbound.telegram_bot_token = Some("telegram-token".to_string());
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-retry-exhaust-1",
        "chat-retry-exhaust",
        "telegram-user-1",
        "should fail delivery",
    );

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.failed_events, 1);
    assert_eq!(summary.retry_attempts, 1);
    failed.assert_calls(2);

    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "telegram",
        "chat-retry-exhaust",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert!(logs.iter().any(|entry| {
        entry.payload.get("status").and_then(Value::as_str) == Some("delivery_failed")
            && entry.payload.get("reason_code").and_then(Value::as_str)
                == Some("delivery_provider_unavailable")
    }));
}

#[tokio::test]
async fn integration_runner_routes_event_to_bound_session_and_emits_route_trace() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    let route_table_path = temp.path().join("route-table.json");
    write_orchestrator_route_table(
        &route_table_path,
        r#"{
  "schema_version": 1,
  "roles": {
    "triage": {},
    "default": {}
  },
  "planner": { "role": "default" },
  "delegated": { "role": "default" },
  "delegated_categories": {
    "incident": { "role": "triage" }
  },
  "review": { "role": "default" }
}"#,
    );
    config.orchestrator_route_table_path = Some(route_table_path);

    write_multi_channel_route_bindings(
        temp.path(),
        r#"{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "discord-ops",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "*",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "session-{role}"
    }
  ]
}"#,
    );

    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-route-1",
        "ops-room",
        "discord-user-1",
        "please check latest incident",
    );
    event.metadata.insert(
        "account_id".to_string(),
        Value::String("discord-main".to_string()),
    );

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime.run_once_events(&[event]).await.expect("run once");
    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.failed_events, 0);

    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "discord",
        "session-triage",
    )
    .expect("open routed store");
    let logs = store.load_log_entries().expect("load routed logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].payload["route"]["binding_id"], "discord-ops");
    assert_eq!(logs[0].payload["route"]["selected_role"], "triage");
    assert_eq!(logs[0].payload["route_session_key"], "session-triage");

    let route_traces = std::fs::read_to_string(
        config
            .state_dir
            .join(super::MULTI_CHANNEL_ROUTE_TRACES_LOG_FILE),
    )
    .expect("read route traces");
    assert!(route_traces.contains("\"record_type\":\"multi_channel_route_trace_v1\""));
    assert!(route_traces.contains("\"binding_id\":\"discord-ops\""));
    assert!(route_traces.contains("\"selected_role\":\"triage\""));
}

#[tokio::test]
async fn integration_runner_respects_queue_limit_for_backpressure() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.queue_limit = 2;
    let fixture =
        load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");
    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

    assert_eq!(summary.discovered_events, 3);
    assert_eq!(summary.queued_events, 2);
    assert_eq!(summary.completed_events, 2);
    assert_eq!(summary.policy_checked_events, 2);
    assert_eq!(summary.policy_allowed_events, 2);
    let state =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("load state");
    assert_eq!(state.processed_event_keys.len(), 2);
}

#[tokio::test]
async fn regression_runner_skips_duplicate_events_from_persisted_state() {
    let temp = tempdir().expect("tempdir");
    let config = build_config(temp.path());
    let fixture =
        load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");

    let mut first_runtime = MultiChannelRuntime::new(config.clone()).expect("first runtime");
    let first_summary = first_runtime
        .run_once_fixture(&fixture)
        .await
        .expect("first run");
    assert_eq!(first_summary.completed_events, 3);

    let mut second_runtime = MultiChannelRuntime::new(config).expect("second runtime");
    let second_summary = second_runtime
        .run_once_fixture(&fixture)
        .await
        .expect("second run");
    assert_eq!(second_summary.completed_events, 0);
    assert_eq!(second_summary.duplicate_skips, 3);
    assert_eq!(second_summary.policy_checked_events, 0);
}

#[tokio::test]
async fn regression_runner_prefers_specific_route_binding_over_wildcard() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    let route_table_path = temp.path().join("route-table.json");
    write_orchestrator_route_table(
        &route_table_path,
        r#"{
  "schema_version": 1,
  "roles": {
    "specific": {},
    "fallback": {},
    "default": {}
  },
  "planner": { "role": "default" },
  "delegated": { "role": "fallback" },
  "delegated_categories": {
    "incident": { "role": "specific" }
  },
  "review": { "role": "default" }
}"#,
    );
    config.orchestrator_route_table_path = Some(route_table_path);

    write_multi_channel_route_bindings(
        temp.path(),
        r#"{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "wildcard",
      "transport": "discord",
      "account_id": "*",
      "conversation_id": "*",
      "actor_id": "*",
      "phase": "delegated_step",
      "session_key_template": "wildcard"
    },
    {
      "binding_id": "specific",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "discord-user-1",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "specific-{role}"
    }
  ]
}"#,
    );

    let mut event = sample_event(
        MultiChannelTransport::Discord,
        "dc-specific-1",
        "ops-room",
        "discord-user-1",
        "incident triage please",
    );
    event.metadata.insert(
        "account_id".to_string(),
        Value::String("discord-main".to_string()),
    );

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    runtime.run_once_events(&[event]).await.expect("run once");

    let specific_store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "discord",
        "specific-specific",
    )
    .expect("open specific store");
    let specific_logs = specific_store.load_log_entries().expect("specific logs");
    assert_eq!(specific_logs.len(), 2);
    assert_eq!(specific_logs[0].payload["route"]["binding_id"], "specific");
    assert_eq!(
        specific_logs[0].payload["route"]["selected_role"],
        "specific"
    );
}

#[tokio::test]
async fn regression_runner_denies_expired_pairing_in_strict_evaluation() {
    let temp = tempdir().expect("tempdir");
    write_pairing_registry(
        temp.path(),
        r#"{
  "schema_version": 1,
  "pairings": [
    {
      "channel": "whatsapp:incident-room",
      "actor_id": "15551234567",
      "paired_by": "ops",
      "issued_unix_ms": 1,
      "expires_unix_ms": 2
    }
  ]
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let events = vec![sample_event(
        MultiChannelTransport::Whatsapp,
        "wa-expired-1",
        "incident-room",
        "15551234567",
        "expired pairing should fail",
    )];
    let summary = runtime.run_once_events(&events).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_enforced_events, 1);
    assert_eq!(summary.policy_denied_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "whatsapp",
        "incident-room",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_actor_not_paired_or_allowlisted")
    );
}

#[tokio::test]
async fn regression_runner_allowlist_only_denies_pairing_only_actor() {
    let temp = tempdir().expect("tempdir");
    write_pairing_registry(
        temp.path(),
        r#"{
  "schema_version": 1,
  "pairings": [
    {
      "channel": "telegram:chat-allowlist-only",
      "actor_id": "telegram-paired-user",
      "paired_by": "ops",
      "issued_unix_ms": 1000,
      "expires_unix_ms": null
    }
  ]
}
"#,
    );
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "telegram:chat-allowlist-only": {
      "dmPolicy": "allow",
      "allowFrom": "allowlist_only",
      "groupPolicy": "allow",
      "requireMention": false
    }
  }
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let event = sample_event(
        MultiChannelTransport::Telegram,
        "tg-allowlist-only-1",
        "chat-allowlist-only",
        "telegram-paired-user",
        "paired actor should be denied by allowlist_only",
    );
    let summary = runtime.run_once_events(&[event]).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_denied_events, 1);
    assert_eq!(summary.policy_enforced_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "chat-allowlist-only",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_channel_policy_allow_from_allowlist_only")
    );
}

#[tokio::test]
async fn regression_runner_explicit_dm_deny_blocks_allow_from_any() {
    let temp = tempdir().expect("tempdir");
    write_channel_policy(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "deny",
    "allowFrom": "any",
    "groupPolicy": "allow",
    "requireMention": false
  }
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let mut event = sample_event(
        MultiChannelTransport::Whatsapp,
        "wa-dm-deny-1",
        "15551234567:15550001111",
        "15550001111",
        "dm should be denied",
    );
    event.metadata.insert(
        "conversation_mode".to_string(),
        Value::String("dm".to_string()),
    );
    let summary = runtime.run_once_events(&[event]).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_denied_events, 1);
    assert_eq!(summary.policy_allowed_events, 0);
    assert_eq!(summary.policy_enforced_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "whatsapp",
        "15551234567:15550001111",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_channel_policy_dm")
    );
}

#[tokio::test]
async fn regression_runner_denies_empty_actor_id_fail_closed_in_strict_mode() {
    let temp = tempdir().expect("tempdir");
    write_pairing_allowlist(
        temp.path(),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {}
}
"#,
    );

    let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
    let events = vec![sample_event(
        MultiChannelTransport::Telegram,
        "tg-empty-actor-1",
        "chat-empty-actor",
        "   ",
        "actor missing test",
    )];
    let summary = runtime.run_once_events(&events).await.expect("run once");

    assert_eq!(summary.completed_events, 1);
    assert_eq!(summary.policy_enforced_events, 1);
    assert_eq!(summary.policy_denied_events, 1);

    let store = ChannelStore::open(
        &temp.path().join(".tau/multi-channel/channel-store"),
        "telegram",
        "chat-empty-actor",
    )
    .expect("open store");
    let logs = store.load_log_entries().expect("load logs");
    let context = store.load_context_entries().expect("load context");
    assert_eq!(context.len(), 0);
    assert_eq!(
        logs[1].payload["reason_code"].as_str(),
        Some("deny_actor_id_missing")
    );
}

#[tokio::test]
async fn integration_runner_failure_streak_increments_and_resets_on_successful_cycle() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.retry_max_attempts = 2;

    let failing_fixture_raw = r#"{
  "schema_version": 1,
  "name": "persistent-failure",
  "events": [
    {
      "schema_version": 1,
      "transport": "discord",
      "event_kind": "message",
      "event_id": "discord-failing-1",
      "conversation_id": "discord-channel-failing",
      "actor_id": "discord-user-1",
      "timestamp_ms": 1760200000000,
      "text": "retry me",
      "metadata": { "simulate_transient_failures": 5 }
    }
  ]
}"#;
    let failing_fixture =
        parse_multi_channel_contract_fixture(failing_fixture_raw).expect("parse failing fixture");
    let success_fixture =
        load_multi_channel_contract_fixture(&config.fixture_path).expect("load success fixture");

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let first_failed = runtime
        .run_once_fixture(&failing_fixture)
        .await
        .expect("first failed cycle");
    assert_eq!(first_failed.failed_events, 1);
    let state_after_first = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
        .expect("state first");
    assert_eq!(state_after_first.health.failure_streak, 1);
    assert_eq!(
        state_after_first.health.classify().state,
        TransportHealthState::Degraded
    );

    let second_failed = runtime
        .run_once_fixture(&failing_fixture)
        .await
        .expect("second failed cycle");
    assert_eq!(second_failed.failed_events, 1);
    let state_after_second = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
        .expect("state second");
    assert_eq!(state_after_second.health.failure_streak, 2);

    let third_failed = runtime
        .run_once_fixture(&failing_fixture)
        .await
        .expect("third failed cycle");
    assert_eq!(third_failed.failed_events, 1);
    let state_after_third = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
        .expect("state third");
    assert_eq!(state_after_third.health.failure_streak, 3);
    assert_eq!(
        state_after_third.health.classify().state,
        TransportHealthState::Failing
    );

    let success = runtime
        .run_once_fixture(&success_fixture)
        .await
        .expect("successful cycle");
    assert_eq!(success.failed_events, 0);
    assert_eq!(success.completed_events, 3);
    let state_after_success =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("state success");
    assert_eq!(state_after_success.health.failure_streak, 0);
    assert_eq!(
        state_after_success.health.classify().state,
        TransportHealthState::Healthy
    );
}

#[tokio::test]
async fn regression_runner_emits_reason_codes_for_failed_cycles() {
    let temp = tempdir().expect("tempdir");
    let mut config = build_config(temp.path());
    config.retry_max_attempts = 2;
    let failing_fixture_raw = r#"{
  "schema_version": 1,
  "name": "failed-cycle-reasons",
  "events": [
    {
      "schema_version": 1,
      "transport": "whatsapp",
      "event_kind": "message",
      "event_id": "whatsapp-failing-1",
      "conversation_id": "whatsapp-chat-failing",
      "actor_id": "whatsapp-user-1",
      "timestamp_ms": 1760300000000,
      "text": "retries",
      "metadata": { "simulate_transient_failures": 5 }
    }
  ]
}"#;
    let failing_fixture =
        parse_multi_channel_contract_fixture(failing_fixture_raw).expect("parse fixture");

    let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
    let summary = runtime
        .run_once_fixture(&failing_fixture)
        .await
        .expect("run once");
    assert_eq!(summary.failed_events, 1);
    assert_eq!(summary.retry_attempts, 1);

    let events_log = std::fs::read_to_string(
        config
            .state_dir
            .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
    )
    .expect("read runtime events log");
    let first_line = events_log.lines().next().expect("at least one report line");
    let report: Value = serde_json::from_str(first_line).expect("parse report");
    assert_eq!(report["health_state"], "degraded");
    let reason_codes = report["reason_codes"]
        .as_array()
        .expect("reason code array");
    let reason_codes_set = reason_codes
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(reason_codes_set.contains("retry_attempted"));
    assert!(reason_codes_set.contains("transient_failures_observed"));
    assert!(reason_codes_set.contains("event_processing_failed"));
    assert!(reason_codes_set.contains("pairing_policy_permissive"));
}

#[test]
fn unit_live_ingress_loader_skips_invalid_rows_without_failing() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
    let telegram_raw =
        std::fs::read_to_string(live_fixture_path("telegram-valid.json")).expect("fixture");
    let telegram_json: Value = serde_json::from_str(&telegram_raw).expect("parse fixture");
    let telegram_line = serde_json::to_string(&telegram_json).expect("serialize fixture");
    std::fs::write(
        ingress_dir.join("telegram.ndjson"),
        format!("{telegram_line}\n{{\"transport\":\"slack\"}}\n"),
    )
    .expect("write telegram ingress");

    let events = load_multi_channel_live_events(&ingress_dir).expect("load live events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transport.as_str(), "telegram");
}

#[tokio::test]
async fn functional_live_runner_processes_ingress_files_and_persists_state() {
    let temp = tempdir().expect("tempdir");
    let config = build_live_config(temp.path());
    write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
    write_live_ingress_file(&config.ingress_dir, "discord", "discord-valid.json");
    write_live_ingress_file(&config.ingress_dir, "whatsapp", "whatsapp-valid.json");

    run_multi_channel_live_runner(config.clone())
        .await
        .expect("live runner should succeed");

    let state =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
    assert_eq!(state.health.last_cycle_discovered, 3);
    assert_eq!(state.health.last_cycle_completed, 3);
    assert_eq!(state.health.last_cycle_failed, 0);

    let events_log = std::fs::read_to_string(
        config
            .state_dir
            .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
    )
    .expect("read runtime events log");
    assert!(events_log.contains("\"health_state\":\"healthy\""));
}

#[tokio::test]
async fn integration_live_runner_is_idempotent_across_repeated_cycles() {
    let temp = tempdir().expect("tempdir");
    let config = build_live_config(temp.path());
    write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
    write_live_ingress_file(&config.ingress_dir, "discord", "discord-valid.json");

    run_multi_channel_live_runner(config.clone())
        .await
        .expect("first live run should succeed");
    run_multi_channel_live_runner(config.clone())
        .await
        .expect("second live run should succeed");

    let state =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
    assert_eq!(state.processed_event_keys.len(), 2);

    let channel_store_root = config.state_dir.join("channel-store");
    let telegram_store = ChannelStore::open(&channel_store_root, "telegram", "chat-100")
        .expect("open telegram store");
    let telegram_logs = telegram_store.load_log_entries().expect("telegram logs");
    assert_eq!(telegram_logs.len(), 2);
}

#[tokio::test]
async fn regression_live_runner_handles_invalid_transport_file_contents() {
    let temp = tempdir().expect("tempdir");
    let config = build_live_config(temp.path());
    write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
    std::fs::write(
            config.ingress_dir.join("discord.ndjson"),
            "{\"schema_version\":1,\"transport\":\"telegram\",\"provider\":\"telegram-bot-api\",\"payload\":{}}\n",
        )
        .expect("write mismatched ingress");

    run_multi_channel_live_runner(config.clone())
        .await
        .expect("live runner should continue despite mismatch");

    let state =
        load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
    assert_eq!(state.health.last_cycle_discovered, 1);
    assert_eq!(state.health.last_cycle_completed, 1);
}
