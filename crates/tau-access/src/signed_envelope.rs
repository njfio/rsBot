use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tau_core::write_text_atomic;

use crate::trust_roots::load_trust_root_records;

const SIGNED_ENVELOPE_SCHEMA_VERSION: u32 = 1;
const SIGNED_ENVELOPE_REPLAY_SCHEMA_VERSION: u32 = 1;
const DEFAULT_TIMESTAMP_SKEW_SECONDS: u64 = 300;
const DEFAULT_REPLAY_WINDOW_SECONDS: u64 = 300;

const REASON_ALLOW_SIGNED_ENVELOPE_VERIFIED: &str = "allow_signed_envelope_verified";
const REASON_MISSING_SIGNED_ENVELOPE: &str = "signed_envelope_missing";
const REASON_DENY_ENVELOPE_INVALID_METADATA: &str = "deny_signed_envelope_invalid_metadata";
const REASON_DENY_ENVELOPE_UNSUPPORTED_SCHEMA: &str = "deny_signed_envelope_unsupported_schema";
const REASON_DENY_ENVELOPE_CHANNEL_MISMATCH: &str = "deny_signed_envelope_channel_mismatch";
const REASON_DENY_ENVELOPE_ACTOR_MISMATCH: &str = "deny_signed_envelope_actor_mismatch";
const REASON_DENY_ENVELOPE_EVENT_MISMATCH: &str = "deny_signed_envelope_event_mismatch";
const REASON_DENY_ENVELOPE_TIMESTAMP_MISMATCH: &str = "deny_signed_envelope_timestamp_mismatch";
const REASON_DENY_ENVELOPE_TIMESTAMP_OUT_OF_WINDOW: &str =
    "deny_signed_envelope_timestamp_out_of_window";
const REASON_DENY_ENVELOPE_UNTRUSTED_KEY: &str = "deny_signed_envelope_untrusted_key";
const REASON_DENY_ENVELOPE_REVOKED_KEY: &str = "deny_signed_envelope_revoked_key";
const REASON_DENY_ENVELOPE_EXPIRED_KEY: &str = "deny_signed_envelope_expired_key";
const REASON_DENY_ENVELOPE_INVALID_SIGNATURE: &str = "deny_signed_envelope_invalid_signature";
const REASON_DENY_ENVELOPE_REPLAY: &str = "deny_signed_envelope_replay";
const REASON_DENY_ENVELOPE_REPLAY_GUARD_ERROR: &str = "deny_signed_envelope_replay_guard_error";
const REASON_DENY_ENVELOPE_TRUST_STORE_ERROR: &str = "deny_signed_envelope_trust_store_error";

pub const SIGNED_ENVELOPE_METADATA_KEY: &str = "signed_envelope";

#[derive(Debug, Clone)]
/// Public struct `SignedEnvelopePolicyConfig` used across Tau components.
pub struct SignedEnvelopePolicyConfig {
    pub trust_root_path: PathBuf,
    pub replay_guard_path: PathBuf,
    pub timestamp_skew_seconds: u64,
    pub replay_window_seconds: u64,
}

#[derive(Debug, Clone)]
/// Public struct `SignedEnvelopeContext` used across Tau components.
pub struct SignedEnvelopeContext<'a> {
    pub policy_channel: &'a str,
    pub actor_id: &'a str,
    pub event_id: &'a str,
    pub event_timestamp_ms: u64,
    pub text: &'a str,
    pub metadata: &'a BTreeMap<String, Value>,
    pub now_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `SignedEnvelope` used across Tau components.
pub struct SignedEnvelope {
    #[serde(default = "signed_envelope_schema_version")]
    pub schema_version: u32,
    pub key_id: String,
    pub nonce: String,
    pub timestamp_ms: u64,
    pub channel: String,
    pub actor_id: String,
    pub event_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `SignedEnvelopeDecision` values.
pub enum SignedEnvelopeDecision {
    Allow {
        reason_code: String,
        key_id: String,
        nonce: String,
    },
    Missing {
        reason_code: String,
    },
    Deny {
        reason_code: String,
    },
}

impl SignedEnvelopeDecision {
    pub fn reason_code(&self) -> &str {
        match self {
            Self::Allow { reason_code, .. }
            | Self::Missing { reason_code }
            | Self::Deny { reason_code } => reason_code,
        }
    }

    pub fn status(&self) -> &'static str {
        match self {
            Self::Allow { .. } => "allow",
            Self::Missing { .. } => "missing",
            Self::Deny { .. } => "deny",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedEnvelopeReplayGuardState {
    schema_version: u32,
    #[serde(default)]
    nonce_last_seen_unix_ms: BTreeMap<String, u64>,
}

impl Default for SignedEnvelopeReplayGuardState {
    fn default() -> Self {
        Self {
            schema_version: SIGNED_ENVELOPE_REPLAY_SCHEMA_VERSION,
            nonce_last_seen_unix_ms: BTreeMap::new(),
        }
    }
}

fn signed_envelope_schema_version() -> u32 {
    SIGNED_ENVELOPE_SCHEMA_VERSION
}

fn security_root_for_state_dir(state_dir: &Path) -> PathBuf {
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
    tau_root.join("security")
}

pub fn default_signed_envelope_policy_config() -> SignedEnvelopePolicyConfig {
    let security_root = PathBuf::from(".tau/security");
    SignedEnvelopePolicyConfig {
        trust_root_path: security_root.join("trust-roots.json"),
        replay_guard_path: security_root.join("signed-envelope-replay.json"),
        timestamp_skew_seconds: DEFAULT_TIMESTAMP_SKEW_SECONDS,
        replay_window_seconds: DEFAULT_REPLAY_WINDOW_SECONDS,
    }
}

pub fn signed_envelope_policy_for_state_dir(state_dir: &Path) -> SignedEnvelopePolicyConfig {
    let security_root = security_root_for_state_dir(state_dir);
    SignedEnvelopePolicyConfig {
        trust_root_path: security_root.join("trust-roots.json"),
        replay_guard_path: security_root.join("signed-envelope-replay.json"),
        timestamp_skew_seconds: DEFAULT_TIMESTAMP_SKEW_SECONDS,
        replay_window_seconds: DEFAULT_REPLAY_WINDOW_SECONDS,
    }
}

pub fn signed_envelope_message_bytes(
    policy_channel: &str,
    actor_id: &str,
    event_id: &str,
    timestamp_ms: u64,
    nonce: &str,
    text: &str,
) -> Vec<u8> {
    let text_sha256 = sha256_hex(text.as_bytes());
    format!(
        "v1\nchannel={}\nactor_id={}\nevent_id={}\ntimestamp_ms={}\nnonce={}\ntext_sha256={}",
        policy_channel.trim(),
        actor_id.trim(),
        event_id.trim(),
        timestamp_ms,
        nonce.trim(),
        text_sha256
    )
    .into_bytes()
}

pub fn evaluate_signed_envelope_access(
    config: &SignedEnvelopePolicyConfig,
    context: &SignedEnvelopeContext<'_>,
) -> SignedEnvelopeDecision {
    let Some(raw_envelope) = context.metadata.get(SIGNED_ENVELOPE_METADATA_KEY) else {
        return SignedEnvelopeDecision::Missing {
            reason_code: REASON_MISSING_SIGNED_ENVELOPE.to_string(),
        };
    };

    let envelope = match serde_json::from_value::<SignedEnvelope>(raw_envelope.clone()) {
        Ok(parsed) => parsed,
        Err(_) => {
            return SignedEnvelopeDecision::Deny {
                reason_code: REASON_DENY_ENVELOPE_INVALID_METADATA.to_string(),
            };
        }
    };

    let envelope_channel = envelope.channel.trim();
    let envelope_actor = envelope.actor_id.trim();
    let envelope_event = envelope.event_id.trim();
    let envelope_key_id = envelope.key_id.trim();
    let envelope_nonce = envelope.nonce.trim();
    let envelope_signature = envelope.signature.trim();
    let expected_actor = context.actor_id.trim();
    let expected_event = context.event_id.trim();
    let expected_channel = context.policy_channel.trim();

    if envelope.schema_version != SIGNED_ENVELOPE_SCHEMA_VERSION {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_UNSUPPORTED_SCHEMA.to_string(),
        };
    }
    if envelope_channel.is_empty()
        || envelope_actor.is_empty()
        || envelope_event.is_empty()
        || envelope_key_id.is_empty()
        || envelope_nonce.is_empty()
        || envelope_signature.is_empty()
        || envelope.timestamp_ms == 0
    {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_INVALID_METADATA.to_string(),
        };
    }
    if envelope_channel != expected_channel {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_CHANNEL_MISMATCH.to_string(),
        };
    }
    if !envelope_actor.eq_ignore_ascii_case(expected_actor) {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_ACTOR_MISMATCH.to_string(),
        };
    }
    if envelope_event != expected_event {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_EVENT_MISMATCH.to_string(),
        };
    }
    if envelope.timestamp_ms != context.event_timestamp_ms {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_TIMESTAMP_MISMATCH.to_string(),
        };
    }

    if config.timestamp_skew_seconds > 0 {
        let window_ms = config.timestamp_skew_seconds.saturating_mul(1_000);
        if envelope.timestamp_ms > context.now_unix_ms.saturating_add(window_ms)
            || context.now_unix_ms > envelope.timestamp_ms.saturating_add(window_ms)
        {
            return SignedEnvelopeDecision::Deny {
                reason_code: REASON_DENY_ENVELOPE_TIMESTAMP_OUT_OF_WINDOW.to_string(),
            };
        }
    }

    let trust_roots = match load_trust_root_records(&config.trust_root_path) {
        Ok(records) => records,
        Err(_) => {
            return SignedEnvelopeDecision::Deny {
                reason_code: REASON_DENY_ENVELOPE_TRUST_STORE_ERROR.to_string(),
            };
        }
    };

    let Some(trusted_root) = trust_roots
        .iter()
        .find(|record| record.id.trim().eq_ignore_ascii_case(envelope_key_id))
    else {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_UNTRUSTED_KEY.to_string(),
        };
    };
    if trusted_root.revoked {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_REVOKED_KEY.to_string(),
        };
    }
    if trusted_root
        .expires_unix
        .is_some_and(|expires| expires <= context.now_unix_ms / 1_000)
    {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_EXPIRED_KEY.to_string(),
        };
    }

    let message = signed_envelope_message_bytes(
        expected_channel,
        expected_actor,
        expected_event,
        envelope.timestamp_ms,
        envelope_nonce,
        context.text,
    );
    if verify_ed25519_signature(&message, envelope_signature, trusted_root.public_key.trim())
        .is_err()
    {
        return SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_INVALID_SIGNATURE.to_string(),
        };
    }

    match enforce_signed_envelope_replay_guard(
        config,
        envelope_key_id,
        envelope_nonce,
        context.now_unix_ms,
    ) {
        Ok(ReplayGuardOutcome::ReplayDetected) => SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_REPLAY.to_string(),
        },
        Ok(ReplayGuardOutcome::Accepted) => SignedEnvelopeDecision::Allow {
            reason_code: REASON_ALLOW_SIGNED_ENVELOPE_VERIFIED.to_string(),
            key_id: envelope_key_id.to_string(),
            nonce: envelope_nonce.to_string(),
        },
        Err(_) => SignedEnvelopeDecision::Deny {
            reason_code: REASON_DENY_ENVELOPE_REPLAY_GUARD_ERROR.to_string(),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayGuardOutcome {
    Accepted,
    ReplayDetected,
}

fn enforce_signed_envelope_replay_guard(
    config: &SignedEnvelopePolicyConfig,
    key_id: &str,
    nonce: &str,
    now_unix_ms: u64,
) -> Result<ReplayGuardOutcome> {
    let mut state = load_signed_envelope_replay_guard_state(&config.replay_guard_path)?;
    let replay_window_ms = config.replay_window_seconds.max(1).saturating_mul(1_000);
    let retain_window_ms = replay_window_ms.saturating_mul(3);
    state
        .nonce_last_seen_unix_ms
        .retain(|_key, seen| now_unix_ms.saturating_sub(*seen) <= retain_window_ms);

    let replay_key = format!(
        "{}:{}",
        key_id.trim().to_ascii_lowercase(),
        nonce.trim().to_ascii_lowercase()
    );
    if let Some(last_seen) = state.nonce_last_seen_unix_ms.get(&replay_key) {
        if now_unix_ms.saturating_sub(*last_seen) <= replay_window_ms {
            return Ok(ReplayGuardOutcome::ReplayDetected);
        }
    }
    state
        .nonce_last_seen_unix_ms
        .insert(replay_key, now_unix_ms);
    save_signed_envelope_replay_guard_state(&config.replay_guard_path, &state)?;
    Ok(ReplayGuardOutcome::Accepted)
}

fn load_signed_envelope_replay_guard_state(path: &Path) -> Result<SignedEnvelopeReplayGuardState> {
    if !path.exists() {
        return Ok(SignedEnvelopeReplayGuardState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let state = serde_json::from_str::<SignedEnvelopeReplayGuardState>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if state.schema_version != SIGNED_ENVELOPE_REPLAY_SCHEMA_VERSION {
        bail!(
            "unsupported signed-envelope replay schema_version {} in {} (expected {})",
            state.schema_version,
            path.display(),
            SIGNED_ENVELOPE_REPLAY_SCHEMA_VERSION
        );
    }
    Ok(state)
}

fn save_signed_envelope_replay_guard_state(
    path: &Path,
    state: &SignedEnvelopeReplayGuardState,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut payload = serde_json::to_string_pretty(state)
        .context("failed to encode signed-envelope replay state")?;
    payload.push('\n');
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

fn verify_ed25519_signature(
    message: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> Result<()> {
    let signature_bytes = decode_base64_fixed::<64>("signature", signature_base64)?;
    let public_key_bytes = decode_base64_fixed::<32>("public key", public_key_base64)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .context("failed to decode ed25519 public key bytes")?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify_strict(message, &signature)
        .map_err(|error| anyhow!("invalid ed25519 signature: {error}"))?;
    Ok(())
}

fn decode_base64_fixed<const N: usize>(label: &str, raw: &str) -> Result<[u8; N]> {
    let decoded = BASE64
        .decode(raw.trim())
        .with_context(|| format!("failed to decode base64 {}", label))?;
    let decoded_len = decoded.len();
    let array: [u8; N] = decoded.try_into().map_err(|_| {
        anyhow!(
            "{} decoded to {} bytes (expected {})",
            label,
            decoded_len,
            N
        )
    })?;
    Ok(array)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{
        default_signed_envelope_policy_config, evaluate_signed_envelope_access,
        signed_envelope_message_bytes, SignedEnvelope, SignedEnvelopeContext,
        SignedEnvelopeDecision, SignedEnvelopePolicyConfig, SIGNED_ENVELOPE_METADATA_KEY,
    };
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::Path;
    use tempfile::tempdir;

    use crate::trust_roots::{save_trust_root_records, TrustedRootRecord};

    fn policy_config(root: &Path) -> SignedEnvelopePolicyConfig {
        SignedEnvelopePolicyConfig {
            trust_root_path: root.join("security/trust-roots.json"),
            replay_guard_path: root.join("security/signed-envelope-replay.json"),
            timestamp_skew_seconds: 300,
            replay_window_seconds: 300,
        }
    }

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn write_trust_root(root: &Path, key_id: &str, revoked: bool, expires_unix: Option<u64>) {
        let signer = signing_key();
        save_trust_root_records(
            &root.join("security/trust-roots.json"),
            &[TrustedRootRecord {
                id: key_id.to_string(),
                public_key: BASE64.encode(signer.verifying_key().to_bytes()),
                revoked,
                expires_unix,
                rotated_from: None,
            }],
        )
        .expect("save trust roots");
    }

    fn signed_metadata(
        policy_channel: &str,
        actor_id: &str,
        event_id: &str,
        timestamp_ms: u64,
        text: &str,
        key_id: &str,
        nonce: &str,
    ) -> BTreeMap<String, serde_json::Value> {
        let signer = signing_key();
        let message = signed_envelope_message_bytes(
            policy_channel,
            actor_id,
            event_id,
            timestamp_ms,
            nonce,
            text,
        );
        let signature = BASE64.encode(signer.sign(&message).to_bytes());
        let envelope = SignedEnvelope {
            schema_version: 1,
            key_id: key_id.to_string(),
            nonce: nonce.to_string(),
            timestamp_ms,
            channel: policy_channel.to_string(),
            actor_id: actor_id.to_string(),
            event_id: event_id.to_string(),
            signature,
        };
        let mut metadata = BTreeMap::new();
        metadata.insert(
            SIGNED_ENVELOPE_METADATA_KEY.to_string(),
            serde_json::to_value(&envelope).expect("serialize envelope"),
        );
        metadata
    }

    #[test]
    fn unit_evaluate_signed_envelope_access_reports_missing_metadata() {
        let temp = tempdir().expect("tempdir");
        let config = policy_config(temp.path());
        let metadata = BTreeMap::new();
        let decision = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-1",
                event_timestamp_ms: 1_000,
                text: "hello",
                metadata: &metadata,
                now_unix_ms: 1_000,
            },
        );
        assert_eq!(
            decision,
            SignedEnvelopeDecision::Missing {
                reason_code: "signed_envelope_missing".to_string(),
            }
        );
    }

    #[test]
    fn functional_evaluate_signed_envelope_access_allows_verified_message() {
        let temp = tempdir().expect("tempdir");
        write_trust_root(temp.path(), "root-v1", false, None);
        let config = policy_config(temp.path());
        let metadata = signed_metadata(
            "discord:ops-room",
            "alice",
            "evt-allow-1",
            5_000,
            "hello signed world",
            "root-v1",
            "nonce-allow-1",
        );

        let decision = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-allow-1",
                event_timestamp_ms: 5_000,
                text: "hello signed world",
                metadata: &metadata,
                now_unix_ms: 5_000,
            },
        );
        assert_eq!(
            decision,
            SignedEnvelopeDecision::Allow {
                reason_code: "allow_signed_envelope_verified".to_string(),
                key_id: "root-v1".to_string(),
                nonce: "nonce-allow-1".to_string(),
            }
        );
    }

    #[test]
    fn integration_evaluate_signed_envelope_access_rejects_replayed_nonce() {
        let temp = tempdir().expect("tempdir");
        write_trust_root(temp.path(), "root-v1", false, None);
        let config = policy_config(temp.path());
        let metadata = signed_metadata(
            "discord:ops-room",
            "alice",
            "evt-replay-1",
            7_000,
            "hello replay",
            "root-v1",
            "nonce-replay-1",
        );

        let first = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-replay-1",
                event_timestamp_ms: 7_000,
                text: "hello replay",
                metadata: &metadata,
                now_unix_ms: 7_000,
            },
        );
        assert!(matches!(first, SignedEnvelopeDecision::Allow { .. }));

        let second = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-replay-1",
                event_timestamp_ms: 7_000,
                text: "hello replay",
                metadata: &metadata,
                now_unix_ms: 7_100,
            },
        );
        assert_eq!(
            second,
            SignedEnvelopeDecision::Deny {
                reason_code: "deny_signed_envelope_replay".to_string(),
            }
        );
    }

    #[test]
    fn regression_evaluate_signed_envelope_access_rejects_forged_payload_signature() {
        let temp = tempdir().expect("tempdir");
        write_trust_root(temp.path(), "root-v1", false, None);
        let config = policy_config(temp.path());
        let metadata = signed_metadata(
            "discord:ops-room",
            "alice",
            "evt-forged-1",
            9_000,
            "original body",
            "root-v1",
            "nonce-forged-1",
        );

        let decision = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-forged-1",
                event_timestamp_ms: 9_000,
                text: "tampered body",
                metadata: &metadata,
                now_unix_ms: 9_000,
            },
        );
        assert_eq!(
            decision,
            SignedEnvelopeDecision::Deny {
                reason_code: "deny_signed_envelope_invalid_signature".to_string(),
            }
        );
    }

    #[test]
    fn unit_default_signed_envelope_policy_config_uses_project_security_paths() {
        let config = default_signed_envelope_policy_config();
        assert_eq!(
            config.trust_root_path,
            std::path::PathBuf::from(".tau/security/trust-roots.json")
        );
        assert_eq!(
            config.replay_guard_path,
            std::path::PathBuf::from(".tau/security/signed-envelope-replay.json")
        );
    }

    #[test]
    fn regression_evaluate_signed_envelope_access_rejects_revoked_key() {
        let temp = tempdir().expect("tempdir");
        write_trust_root(temp.path(), "root-revoked", true, None);
        let config = policy_config(temp.path());
        let metadata = signed_metadata(
            "discord:ops-room",
            "alice",
            "evt-revoked-1",
            10_000,
            "hello",
            "root-revoked",
            "nonce-revoked-1",
        );
        let decision = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-revoked-1",
                event_timestamp_ms: 10_000,
                text: "hello",
                metadata: &metadata,
                now_unix_ms: 10_000,
            },
        );
        assert_eq!(
            decision,
            SignedEnvelopeDecision::Deny {
                reason_code: "deny_signed_envelope_revoked_key".to_string(),
            }
        );
    }

    #[test]
    fn unit_evaluate_signed_envelope_access_rejects_invalid_metadata_shape() {
        let temp = tempdir().expect("tempdir");
        write_trust_root(temp.path(), "root-v1", false, None);
        let config = policy_config(temp.path());
        let mut metadata = BTreeMap::new();
        metadata.insert(
            SIGNED_ENVELOPE_METADATA_KEY.to_string(),
            json!({"bad":"shape"}),
        );

        let decision = evaluate_signed_envelope_access(
            &config,
            &SignedEnvelopeContext {
                policy_channel: "discord:ops-room",
                actor_id: "alice",
                event_id: "evt-1",
                event_timestamp_ms: 1_000,
                text: "hello",
                metadata: &metadata,
                now_unix_ms: 1_000,
            },
        );
        assert_eq!(
            decision,
            SignedEnvelopeDecision::Deny {
                reason_code: "deny_signed_envelope_invalid_metadata".to_string(),
            }
        );
    }
}
