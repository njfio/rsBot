use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};

const PAIRING_SCHEMA_VERSION: u32 = 1;
const ALLOWLIST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
/// Public struct `PairingPolicyConfig` used across Tau components.
pub struct PairingPolicyConfig {
    pub registry_path: PathBuf,
    pub allowlist_path: PathBuf,
    pub strict_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `PairingRecord` used across Tau components.
pub struct PairingRecord {
    pub channel: String,
    pub actor_id: String,
    pub paired_by: String,
    pub issued_unix_ms: u64,
    pub expires_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PairingRegistryFile {
    schema_version: u32,
    pairings: Vec<PairingRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PairingAllowlistFile {
    schema_version: u32,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    channels: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `PairingDecision` values.
pub enum PairingDecision {
    Allow { reason_code: String },
    Deny { reason_code: String },
}

impl PairingDecision {
    pub fn reason_code(&self) -> &str {
        match self {
            Self::Allow { reason_code } | Self::Deny { reason_code } => reason_code,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PairCommand {
    Add {
        channel: String,
        actor_id: String,
        ttl_seconds: Option<u64>,
    },
    Remove {
        channel: String,
        actor_id: String,
    },
    Status {
        channel: Option<String>,
        actor_id: Option<String>,
    },
}

pub fn default_pairing_policy_config() -> Result<PairingPolicyConfig> {
    let security_dir = PathBuf::from(".tau").join("security");
    Ok(PairingPolicyConfig {
        registry_path: security_dir.join("pairings.json"),
        allowlist_path: security_dir.join("allowlist.json"),
        strict_mode: false,
    })
}

pub fn pairing_policy_for_state_dir(state_dir: &Path) -> PairingPolicyConfig {
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
    PairingPolicyConfig {
        registry_path: security_dir.join("pairings.json"),
        allowlist_path: security_dir.join("allowlist.json"),
        strict_mode: false,
    }
}

pub fn evaluate_pairing_access(
    config: &PairingPolicyConfig,
    channel: &str,
    actor_id: &str,
    now_unix_ms: u64,
) -> Result<PairingDecision> {
    const ALLOW_PERMISSIVE_MODE: &str = "allow_permissive_mode";
    const ALLOW_ALLOWLIST_AND_PAIRING: &str = "allow_allowlist_and_pairing";
    const ALLOW_ALLOWLIST: &str = "allow_allowlist";
    const ALLOW_PAIRING: &str = "allow_pairing";
    const DENY_ACTOR_ID_MISSING: &str = "deny_actor_id_missing";
    const DENY_ACTOR_NOT_PAIRED_OR_ALLOWLISTED: &str = "deny_actor_not_paired_or_allowlisted";

    let actor_id = actor_id.trim();

    let allowlist = load_allowlist(&config.allowlist_path)?;
    let registry = load_pairing_registry(&config.registry_path)?;
    let candidates = channel_candidates(channel);
    let strict_effective = config.strict_mode
        || allowlist.strict
        || channel_has_pairing_rules(&allowlist, &registry, &candidates);

    if !strict_effective {
        return Ok(PairingDecision::Allow {
            reason_code: ALLOW_PERMISSIVE_MODE.to_string(),
        });
    }
    if actor_id.is_empty() {
        return Ok(PairingDecision::Deny {
            reason_code: DENY_ACTOR_ID_MISSING.to_string(),
        });
    }

    let allowed_by_allowlist = allowlist_actor_allowed(&allowlist, &candidates, actor_id);
    let allowed_by_pairing = pairing_actor_allowed(&registry, &candidates, actor_id, now_unix_ms);

    if allowed_by_allowlist && allowed_by_pairing {
        return Ok(PairingDecision::Allow {
            reason_code: ALLOW_ALLOWLIST_AND_PAIRING.to_string(),
        });
    }
    if allowed_by_allowlist {
        return Ok(PairingDecision::Allow {
            reason_code: ALLOW_ALLOWLIST.to_string(),
        });
    }
    if allowed_by_pairing {
        return Ok(PairingDecision::Allow {
            reason_code: ALLOW_PAIRING.to_string(),
        });
    }

    Ok(PairingDecision::Deny {
        reason_code: DENY_ACTOR_NOT_PAIRED_OR_ALLOWLISTED.to_string(),
    })
}

pub fn execute_pair_command(command_args: &str, actor_source: &str) -> String {
    let config = match default_pairing_policy_config() {
        Ok(config) => config,
        Err(error) => return format!("pair error: {error}"),
    };
    let command = match parse_pair_command(command_args) {
        Ok(command) => command,
        Err(error) => return format!("pair error: {error}"),
    };

    match command {
        PairCommand::Add {
            channel,
            actor_id,
            ttl_seconds,
        } => {
            let mut registry = match load_pairing_registry(&config.registry_path) {
                Ok(registry) => registry,
                Err(error) => return format!("pair error: {error}"),
            };
            registry
                .pairings
                .retain(|entry| !(entry.channel == channel && entry.actor_id == actor_id));
            let issued_unix_ms = current_unix_timestamp_ms();
            let expires_unix_ms = ttl_seconds
                .map(|seconds| issued_unix_ms.saturating_add(seconds.saturating_mul(1_000)));
            registry.pairings.push(PairingRecord {
                channel: channel.clone(),
                actor_id: actor_id.clone(),
                paired_by: actor_source.to_string(),
                issued_unix_ms,
                expires_unix_ms,
            });
            registry.pairings.sort_by(|left, right| {
                left.channel
                    .cmp(&right.channel)
                    .then(left.actor_id.cmp(&right.actor_id))
            });
            if let Err(error) = save_pairing_registry(&config.registry_path, &registry) {
                return format!("pair error: {error}");
            }
            format!(
                "pair add: channel={} actor={} ttl_seconds={} status=paired path={}",
                channel,
                actor_id,
                ttl_seconds
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                config.registry_path.display()
            )
        }
        PairCommand::Remove { channel, actor_id } => {
            execute_unpair_with_config(&config, &channel, &actor_id)
        }
        PairCommand::Status { channel, actor_id } => {
            let registry = match load_pairing_registry(&config.registry_path) {
                Ok(registry) => registry,
                Err(error) => return format!("pair error: {error}"),
            };
            let allowlist = match load_allowlist(&config.allowlist_path) {
                Ok(allowlist) => allowlist,
                Err(error) => return format!("pair error: {error}"),
            };
            render_pair_status(
                &config,
                &allowlist,
                &registry,
                channel.as_deref(),
                actor_id.as_deref(),
                current_unix_timestamp_ms(),
            )
        }
    }
}

pub fn execute_unpair_command(command_args: &str) -> String {
    let config = match default_pairing_policy_config() {
        Ok(config) => config,
        Err(error) => return format!("unpair error: {error}"),
    };
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    if tokens.len() != 2 {
        return "unpair error: usage: /unpair <channel> <actor_id>".to_string();
    }
    execute_unpair_with_config(&config, tokens[0], tokens[1])
}

fn execute_unpair_with_config(
    config: &PairingPolicyConfig,
    channel: &str,
    actor_id: &str,
) -> String {
    let mut registry = match load_pairing_registry(&config.registry_path) {
        Ok(registry) => registry,
        Err(error) => return format!("unpair error: {error}"),
    };
    let before = registry.pairings.len();
    registry
        .pairings
        .retain(|entry| !(entry.channel == channel && entry.actor_id == actor_id));
    let removed = before.saturating_sub(registry.pairings.len());
    if removed > 0 {
        if let Err(error) = save_pairing_registry(&config.registry_path, &registry) {
            return format!("unpair error: {error}");
        }
    }
    format!(
        "unpair: channel={} actor={} removed={} path={}",
        channel,
        actor_id,
        removed,
        config.registry_path.display()
    )
}

fn render_pair_status(
    config: &PairingPolicyConfig,
    allowlist: &PairingAllowlistFile,
    registry: &PairingRegistryFile,
    channel_filter: Option<&str>,
    actor_filter: Option<&str>,
    now_unix_ms: u64,
) -> String {
    let mut lines = vec![format!(
        "pair status: strict={} strict_allowlist={} registry={} allowlist={}",
        config.strict_mode,
        allowlist.strict,
        config.registry_path.display(),
        config.allowlist_path.display()
    )];

    let mut allowlist_rows = Vec::new();
    for (channel, actors) in &allowlist.channels {
        for actor in actors {
            if filter_pair_row(channel_filter, actor_filter, channel, actor) {
                allowlist_rows.push((channel.clone(), actor.clone()));
            }
        }
    }
    allowlist_rows.sort();
    if allowlist_rows.is_empty() {
        lines.push("allowlist: none".to_string());
    } else {
        for (channel, actor) in allowlist_rows {
            lines.push(format!("allowlist: channel={} actor={}", channel, actor));
        }
    }

    let mut pairing_rows = registry
        .pairings
        .iter()
        .filter(|entry| {
            filter_pair_row(
                channel_filter,
                actor_filter,
                &entry.channel,
                &entry.actor_id,
            )
        })
        .collect::<Vec<_>>();
    pairing_rows.sort_by(|left, right| {
        left.channel
            .cmp(&right.channel)
            .then(left.actor_id.cmp(&right.actor_id))
    });
    if pairing_rows.is_empty() {
        lines.push("pairings: none".to_string());
    } else {
        for entry in pairing_rows {
            let status = if is_pairing_expired(entry, now_unix_ms) {
                "expired"
            } else {
                "active"
            };
            lines.push(format!(
                "pairing: channel={} actor={} status={} paired_by={} issued_unix_ms={} expires_unix_ms={}",
                entry.channel,
                entry.actor_id,
                status,
                entry.paired_by,
                entry.issued_unix_ms,
                entry
                    .expires_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
        }
    }

    lines.join("\n")
}

fn filter_pair_row(
    channel_filter: Option<&str>,
    actor_filter: Option<&str>,
    channel: &str,
    actor: &str,
) -> bool {
    let channel_matches = channel_filter
        .map(|filter| channel == filter)
        .unwrap_or(true);
    let actor_matches = actor_filter.map(|filter| actor == filter).unwrap_or(true);
    channel_matches && actor_matches
}

fn parse_pair_command(command_args: &str) -> Result<PairCommand> {
    const USAGE: &str = "usage: /pair <add|remove|status> ...";
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{USAGE}");
    }

    match tokens[0] {
        "add" => parse_pair_add_command(&tokens),
        "remove" => {
            if tokens.len() != 3 {
                bail!("usage: /pair remove <channel> <actor_id>");
            }
            Ok(PairCommand::Remove {
                channel: tokens[1].to_string(),
                actor_id: tokens[2].to_string(),
            })
        }
        "status" => {
            if tokens.len() > 3 {
                bail!("usage: /pair status [channel] [actor_id]");
            }
            Ok(PairCommand::Status {
                channel: tokens.get(1).map(|value| value.to_string()),
                actor_id: tokens.get(2).map(|value| value.to_string()),
            })
        }
        "list" => {
            if tokens.len() != 1 {
                bail!("usage: /pair list");
            }
            Ok(PairCommand::Status {
                channel: None,
                actor_id: None,
            })
        }
        other => bail!("unknown pair subcommand '{}'; {USAGE}", other),
    }
}

fn parse_pair_add_command(tokens: &[&str]) -> Result<PairCommand> {
    if tokens.len() < 3 {
        bail!("usage: /pair add <channel> <actor_id> [--ttl-seconds <value>]");
    }
    let channel = tokens[1].to_string();
    let actor_id = tokens[2].to_string();
    let mut ttl_seconds = None;
    let mut index = 3;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--ttl-seconds" {
            let value = tokens
                .get(index + 1)
                .ok_or_else(|| anyhow!("missing value for --ttl-seconds"))?;
            let parsed = value
                .parse::<u64>()
                .with_context(|| format!("invalid --ttl-seconds value '{}'", value))?;
            if parsed == 0 {
                bail!("--ttl-seconds must be greater than 0");
            }
            ttl_seconds = Some(parsed);
            index += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--ttl-seconds=") {
            let parsed = value
                .parse::<u64>()
                .with_context(|| format!("invalid --ttl-seconds value '{}'", value))?;
            if parsed == 0 {
                bail!("--ttl-seconds must be greater than 0");
            }
            ttl_seconds = Some(parsed);
            index += 1;
            continue;
        }
        bail!(
            "unknown pair add flag '{}'; usage: /pair add <channel> <actor_id> [--ttl-seconds <value>]",
            token
        );
    }

    Ok(PairCommand::Add {
        channel,
        actor_id,
        ttl_seconds,
    })
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

fn load_pairing_registry(path: &Path) -> Result<PairingRegistryFile> {
    if !path.exists() {
        return Ok(PairingRegistryFile {
            schema_version: PAIRING_SCHEMA_VERSION,
            pairings: Vec::new(),
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read pairing registry {}", path.display()))?;
    let parsed = serde_json::from_str::<PairingRegistryFile>(&raw)
        .with_context(|| format!("failed to parse pairing registry {}", path.display()))?;
    if parsed.schema_version != PAIRING_SCHEMA_VERSION {
        bail!(
            "unsupported pairing schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            PAIRING_SCHEMA_VERSION
        );
    }
    Ok(parsed)
}

fn save_pairing_registry(path: &Path, registry: &PairingRegistryFile) -> Result<()> {
    let mut payload =
        serde_json::to_string_pretty(registry).context("failed to encode pairing registry")?;
    payload.push('\n');
    write_text_atomic(path, &payload)
        .with_context(|| format!("failed to write pairing registry {}", path.display()))
}

fn load_allowlist(path: &Path) -> Result<PairingAllowlistFile> {
    if !path.exists() {
        return Ok(PairingAllowlistFile {
            schema_version: ALLOWLIST_SCHEMA_VERSION,
            strict: false,
            channels: BTreeMap::new(),
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read pairing allowlist {}", path.display()))?;
    let parsed = serde_json::from_str::<PairingAllowlistFile>(&raw)
        .with_context(|| format!("failed to parse pairing allowlist {}", path.display()))?;
    if parsed.schema_version != ALLOWLIST_SCHEMA_VERSION {
        bail!(
            "unsupported pairing allowlist schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            ALLOWLIST_SCHEMA_VERSION
        );
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{
        default_pairing_policy_config, evaluate_pairing_access, execute_pair_command,
        execute_unpair_command, pairing_policy_for_state_dir, parse_pair_command,
        save_pairing_registry, PairCommand, PairingDecision, PairingPolicyConfig, PairingRecord,
        PairingRegistryFile, PAIRING_SCHEMA_VERSION,
    };
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn policy_config(root: &std::path::Path) -> PairingPolicyConfig {
        PairingPolicyConfig {
            registry_path: root.join("security/pairings.json"),
            allowlist_path: root.join("security/allowlist.json"),
            strict_mode: false,
        }
    }

    #[test]
    fn unit_parse_pair_command_supports_add_remove_and_status() {
        assert_eq!(
            parse_pair_command("add github:repo alice --ttl-seconds 30").expect("parse add"),
            PairCommand::Add {
                channel: "github:repo".to_string(),
                actor_id: "alice".to_string(),
                ttl_seconds: Some(30),
            }
        );
        assert_eq!(
            parse_pair_command("remove github:repo alice").expect("parse remove"),
            PairCommand::Remove {
                channel: "github:repo".to_string(),
                actor_id: "alice".to_string(),
            }
        );
        assert_eq!(
            parse_pair_command("status github:repo alice").expect("parse status"),
            PairCommand::Status {
                channel: Some("github:repo".to_string()),
                actor_id: Some("alice".to_string()),
            }
        );
    }

    #[test]
    fn functional_pair_command_add_status_and_unpair_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(temp.path()).expect("set current dir");

        let add = execute_pair_command("add github:tau alice --ttl-seconds 60", "test");
        assert!(add.contains("status=paired"), "{add}");

        let status = execute_pair_command("status github:tau alice", "test");
        assert!(
            status.contains("pairing: channel=github:tau actor=alice"),
            "{status}"
        );

        let unpair = execute_unpair_command("github:tau alice");
        assert!(unpair.contains("removed=1"), "{unpair}");
        std::env::set_current_dir(original).expect("restore cwd");
    }

    #[test]
    fn integration_evaluate_pairing_access_allows_active_pairing() {
        let temp = tempdir().expect("tempdir");
        let config = policy_config(temp.path());
        let now = 1_000_000_u64;
        let registry = PairingRegistryFile {
            schema_version: PAIRING_SCHEMA_VERSION,
            pairings: vec![PairingRecord {
                channel: "github:njfio/tau".to_string(),
                actor_id: "alice".to_string(),
                paired_by: "admin".to_string(),
                issued_unix_ms: now,
                expires_unix_ms: Some(now + 60_000),
            }],
        };
        save_pairing_registry(&config.registry_path, &registry).expect("save registry");

        let decision = evaluate_pairing_access(&config, "github:njfio/tau", "alice", now + 1_000)
            .expect("evaluate");
        assert_eq!(
            decision,
            PairingDecision::Allow {
                reason_code: "allow_pairing".to_string(),
            }
        );
    }

    #[test]
    fn regression_evaluate_pairing_access_denies_expired_pairing_when_strict() {
        let temp = tempdir().expect("tempdir");
        let mut config = policy_config(temp.path());
        config.strict_mode = true;
        let now = 2_000_000_u64;
        let registry = PairingRegistryFile {
            schema_version: PAIRING_SCHEMA_VERSION,
            pairings: vec![PairingRecord {
                channel: "slack:C123".to_string(),
                actor_id: "U999".to_string(),
                paired_by: "admin".to_string(),
                issued_unix_ms: now - 20_000,
                expires_unix_ms: Some(now - 1_000),
            }],
        };
        save_pairing_registry(&config.registry_path, &registry).expect("save registry");

        let decision =
            evaluate_pairing_access(&config, "slack:C123", "U999", now).expect("evaluate");
        assert_eq!(
            decision,
            PairingDecision::Deny {
                reason_code: "deny_actor_not_paired_or_allowlisted".to_string(),
            }
        );
    }

    #[test]
    fn regression_permissive_mode_allows_unknown_or_missing_actor_by_default() {
        let temp = tempdir().expect("tempdir");
        let config = policy_config(temp.path());
        let now = 3_000_000_u64;
        let decision =
            evaluate_pairing_access(&config, "github:njfio/tau", "", now).expect("evaluate");
        assert_eq!(
            decision,
            PairingDecision::Allow {
                reason_code: "allow_permissive_mode".to_string(),
            }
        );
    }

    #[test]
    fn unit_default_pairing_policy_config_uses_project_local_security_paths() {
        let temp = tempdir().expect("tempdir");
        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(temp.path()).expect("set current dir");
        let config = default_pairing_policy_config().expect("config");
        std::env::set_current_dir(original).expect("restore cwd");
        assert_eq!(
            config.registry_path,
            PathBuf::from(".tau/security/pairings.json")
        );
        assert_eq!(
            config.allowlist_path,
            PathBuf::from(".tau/security/allowlist.json")
        );
    }

    #[test]
    fn unit_pairing_policy_for_state_dir_supports_transport_and_test_state_roots() {
        let transport_root = PathBuf::from(".tau/github");
        let transport_policy = pairing_policy_for_state_dir(&transport_root);
        assert_eq!(
            transport_policy.registry_path,
            PathBuf::from(".tau/security/pairings.json")
        );

        let multi_channel_root = PathBuf::from(".tau/multi-channel");
        let multi_channel_policy = pairing_policy_for_state_dir(&multi_channel_root);
        assert_eq!(
            multi_channel_policy.registry_path,
            PathBuf::from(".tau/security/pairings.json")
        );

        let temp = tempdir().expect("tempdir");
        let test_root = temp.path().join("runtime-state");
        let test_policy = pairing_policy_for_state_dir(&test_root);
        assert_eq!(
            test_policy.registry_path,
            test_root.join("security/pairings.json")
        );
    }
}
