use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use tau_cli::Cli;
use tau_core::current_unix_timestamp;

use crate::{
    load_credential_store, resolve_credential_store_encryption_mode, save_credential_store,
    AuthCommandConfig, IntegrationCredentialStoreRecord,
};

pub fn resolve_non_empty_cli_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn resolve_secret_from_cli_or_store_id(
    cli: &Cli,
    direct_secret: Option<&str>,
    secret_id: Option<&str>,
    secret_id_flag: &str,
) -> Result<Option<String>> {
    if let Some(secret) = resolve_non_empty_cli_value(direct_secret) {
        return Ok(Some(secret));
    }
    let Some(raw_secret_id) = secret_id else {
        return Ok(None);
    };
    let normalized_secret_id = normalize_integration_credential_id(raw_secret_id)?;
    let store = load_credential_store(
        &cli.credential_store,
        resolve_credential_store_encryption_mode(cli),
        cli.credential_store_key.as_deref(),
    )
    .with_context(|| {
        format!(
            "failed to resolve {} from credential store {}",
            secret_id_flag,
            cli.credential_store.display()
        )
    })?;
    let entry = store
        .integrations
        .get(&normalized_secret_id)
        .ok_or_else(|| {
            anyhow!(
                "integration credential id '{}' from {} was not found in credential store {}",
                normalized_secret_id,
                secret_id_flag,
                cli.credential_store.display()
            )
        })?;
    if entry.revoked {
        bail!(
            "integration credential id '{}' from {} is revoked in credential store {}",
            normalized_secret_id,
            secret_id_flag,
            cli.credential_store.display()
        );
    }
    let secret = entry
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "integration credential id '{}' from {} has no secret value in credential store {}",
                normalized_secret_id,
                secret_id_flag,
                cli.credential_store.display()
            )
        })?;
    Ok(Some(secret.to_string()))
}

const INTEGRATION_AUTH_USAGE: &str = "usage: /integration-auth <set|status|rotate|revoke> ...";
const INTEGRATION_AUTH_SET_USAGE: &str =
    "usage: /integration-auth set <integration-id> <secret> [--json]";
const INTEGRATION_AUTH_STATUS_USAGE: &str =
    "usage: /integration-auth status [integration-id] [--json]";
const INTEGRATION_AUTH_ROTATE_USAGE: &str =
    "usage: /integration-auth rotate <integration-id> <secret> [--json]";
const INTEGRATION_AUTH_REVOKE_USAGE: &str =
    "usage: /integration-auth revoke <integration-id> [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `IntegrationAuthCommand` values.
pub enum IntegrationAuthCommand {
    Set {
        integration_id: String,
        secret: String,
        json_output: bool,
    },
    Status {
        integration_id: Option<String>,
        json_output: bool,
    },
    Rotate {
        integration_id: String,
        secret: String,
        json_output: bool,
    },
    Revoke {
        integration_id: String,
        json_output: bool,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct IntegrationAuthStatusRow {
    integration_id: String,
    available: bool,
    state: String,
    source: String,
    reason: String,
    updated_unix: Option<u64>,
    revoked: bool,
}

pub fn normalize_integration_credential_id(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("integration credential id must not be empty");
    }

    let mut normalized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            normalized.push(ch.to_ascii_lowercase());
            continue;
        }
        bail!(
            "integration credential id '{}' contains unsupported character '{}'; use only [a-z0-9._-]",
            trimmed,
            ch
        );
    }
    Ok(normalized)
}

pub fn parse_integration_auth_command(command_args: &str) -> Result<IntegrationAuthCommand> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{INTEGRATION_AUTH_USAGE}");
    }

    match tokens[0] {
        "set" => {
            if tokens.len() < 3 {
                bail!("{INTEGRATION_AUTH_SET_USAGE}");
            }
            let integration_id = normalize_integration_credential_id(tokens[1])?;
            let mut secret: Option<String> = None;
            let mut json_output = false;
            for token in tokens.into_iter().skip(2) {
                if token == "--json" {
                    json_output = true;
                    continue;
                }
                if secret.is_some() {
                    bail!(
                        "unexpected argument '{}'; {INTEGRATION_AUTH_SET_USAGE}",
                        token
                    );
                }
                secret = Some(token.to_string());
            }
            let Some(secret) = secret else {
                bail!("{INTEGRATION_AUTH_SET_USAGE}");
            };
            Ok(IntegrationAuthCommand::Set {
                integration_id,
                secret,
                json_output,
            })
        }
        "status" => {
            let mut integration_id: Option<String> = None;
            let mut json_output = false;
            for token in tokens.into_iter().skip(1) {
                if token == "--json" {
                    json_output = true;
                    continue;
                }
                if integration_id.is_some() {
                    bail!(
                        "unexpected argument '{}'; {INTEGRATION_AUTH_STATUS_USAGE}",
                        token
                    );
                }
                integration_id = Some(normalize_integration_credential_id(token)?);
            }
            Ok(IntegrationAuthCommand::Status {
                integration_id,
                json_output,
            })
        }
        "rotate" => {
            if tokens.len() < 3 {
                bail!("{INTEGRATION_AUTH_ROTATE_USAGE}");
            }
            let integration_id = normalize_integration_credential_id(tokens[1])?;
            let mut secret: Option<String> = None;
            let mut json_output = false;
            for token in tokens.into_iter().skip(2) {
                if token == "--json" {
                    json_output = true;
                    continue;
                }
                if secret.is_some() {
                    bail!(
                        "unexpected argument '{}'; {INTEGRATION_AUTH_ROTATE_USAGE}",
                        token
                    );
                }
                secret = Some(token.to_string());
            }
            let Some(secret) = secret else {
                bail!("{INTEGRATION_AUTH_ROTATE_USAGE}");
            };
            Ok(IntegrationAuthCommand::Rotate {
                integration_id,
                secret,
                json_output,
            })
        }
        "revoke" => {
            if tokens.len() < 2 {
                bail!("{INTEGRATION_AUTH_REVOKE_USAGE}");
            }
            let integration_id = normalize_integration_credential_id(tokens[1])?;
            let mut json_output = false;
            for token in tokens.into_iter().skip(2) {
                if token == "--json" {
                    json_output = true;
                } else {
                    bail!(
                        "unexpected argument '{}'; {INTEGRATION_AUTH_REVOKE_USAGE}",
                        token
                    );
                }
            }
            Ok(IntegrationAuthCommand::Revoke {
                integration_id,
                json_output,
            })
        }
        other => bail!("unknown subcommand '{}'; {INTEGRATION_AUTH_USAGE}", other),
    }
}

fn integration_auth_error(command: &str, integration_id: &str, error: anyhow::Error) -> String {
    format!("integration auth {command} error: integration_id={integration_id} error={error}")
}

fn execute_integration_auth_set_or_rotate_command(
    config: &AuthCommandConfig,
    integration_id: String,
    secret: String,
    json_output: bool,
    rotate: bool,
) -> String {
    let secret = secret.trim();
    if secret.is_empty() {
        if json_output {
            return serde_json::json!({
                "command": if rotate {
                    "integration_auth.rotate"
                } else {
                    "integration_auth.set"
                },
                "integration_id": integration_id,
                "status": "error",
                "reason": "integration secret must not be empty",
            })
            .to_string();
        }
        return format!(
            "integration auth {} error: integration_id={} error=integration secret must not be empty",
            if rotate { "rotate" } else { "set" },
            integration_id,
        );
    }

    let mut store = match load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    ) {
        Ok(store) => store,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": if rotate {
                        "integration_auth.rotate"
                    } else {
                        "integration_auth.set"
                    },
                    "integration_id": integration_id,
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return integration_auth_error(
                if rotate { "rotate" } else { "set" },
                &integration_id,
                error,
            );
        }
    };

    let existed = store.integrations.contains_key(&integration_id);
    let updated_unix = Some(current_unix_timestamp());
    store.integrations.insert(
        integration_id.clone(),
        IntegrationCredentialStoreRecord {
            secret: Some(secret.to_string()),
            revoked: false,
            updated_unix,
        },
    );
    if let Err(error) = save_credential_store(
        &config.credential_store,
        &store,
        config.credential_store_key.as_deref(),
    ) {
        if json_output {
            return serde_json::json!({
                "command": if rotate {
                    "integration_auth.rotate"
                } else {
                    "integration_auth.set"
                },
                "integration_id": integration_id,
                "status": "error",
                "reason": error.to_string(),
            })
            .to_string();
        }
        return integration_auth_error(
            if rotate { "rotate" } else { "set" },
            &integration_id,
            error,
        );
    }

    let status = if rotate {
        if existed {
            "rotated"
        } else {
            "created"
        }
    } else {
        "saved"
    };
    if json_output {
        return serde_json::json!({
            "command": if rotate {
                "integration_auth.rotate"
            } else {
                "integration_auth.set"
            },
            "integration_id": integration_id,
            "status": status,
            "credential_store": config.credential_store.display().to_string(),
            "updated_unix": updated_unix,
        })
        .to_string();
    }

    format!(
        "integration auth {}: integration_id={} status={} credential_store={} updated_unix={}",
        if rotate { "rotate" } else { "set" },
        integration_id,
        status,
        config.credential_store.display(),
        updated_unix
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    )
}

fn integration_status_row_for_entry(
    integration_id: &str,
    entry: Option<&IntegrationCredentialStoreRecord>,
) -> IntegrationAuthStatusRow {
    let Some(entry) = entry else {
        return IntegrationAuthStatusRow {
            integration_id: integration_id.to_string(),
            available: false,
            state: "missing_credential".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry is missing".to_string(),
            updated_unix: None,
            revoked: false,
        };
    };

    if entry.revoked {
        return IntegrationAuthStatusRow {
            integration_id: integration_id.to_string(),
            available: false,
            state: "revoked".to_string(),
            source: "credential_store".to_string(),
            reason: "credential has been revoked".to_string(),
            updated_unix: entry.updated_unix,
            revoked: true,
        };
    }

    if entry
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return IntegrationAuthStatusRow {
            integration_id: integration_id.to_string(),
            available: false,
            state: "missing_secret".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry has no secret".to_string(),
            updated_unix: entry.updated_unix,
            revoked: false,
        };
    }

    IntegrationAuthStatusRow {
        integration_id: integration_id.to_string(),
        available: true,
        state: "ready".to_string(),
        source: "credential_store".to_string(),
        reason: "credential available".to_string(),
        updated_unix: entry.updated_unix,
        revoked: false,
    }
}

fn integration_state_counts(rows: &[IntegrationAuthStatusRow]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        *counts.entry(row.state.clone()).or_insert(0) += 1;
    }
    counts
}

fn integration_revoked_counts(rows: &[IntegrationAuthStatusRow]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        let key = if row.revoked {
            "revoked"
        } else {
            "not_revoked"
        };
        *counts.entry(key.to_string()).or_insert(0) += 1;
    }
    counts
}

fn format_auth_state_counts(state_counts: &BTreeMap<String, usize>) -> String {
    if state_counts.is_empty() {
        return "none".to_string();
    }
    state_counts
        .iter()
        .map(|(state, count)| format!("{state}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn execute_integration_auth_status_command(
    config: &AuthCommandConfig,
    integration_id: Option<String>,
    json_output: bool,
) -> String {
    let store = match load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    ) {
        Ok(store) => store,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": "integration_auth.status",
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return format!("integration auth status error: {error}");
        }
    };

    let all_rows = store
        .integrations
        .iter()
        .map(|(integration_id, entry)| {
            integration_status_row_for_entry(integration_id, Some(entry))
        })
        .collect::<Vec<_>>();
    let rows = match integration_id {
        Some(integration_id) => vec![integration_status_row_for_entry(
            &integration_id,
            store.integrations.get(&integration_id),
        )],
        None => all_rows.clone(),
    };
    let integrations_total = all_rows.len();
    let available_total = all_rows.iter().filter(|row| row.available).count();
    let unavailable_total = integrations_total.saturating_sub(available_total);
    let state_counts_total = integration_state_counts(&all_rows);
    let revoked_counts_total = integration_revoked_counts(&all_rows);
    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);
    let state_counts = integration_state_counts(&rows);
    let revoked_counts = integration_revoked_counts(&rows);

    if json_output {
        return serde_json::json!({
            "command": "integration_auth.status",
            "integrations_total": integrations_total,
            "integrations": rows.len(),
            "available_total": available_total,
            "unavailable_total": unavailable_total,
            "available": available,
            "unavailable": unavailable,
            "state_counts_total": state_counts_total,
            "state_counts": state_counts,
            "revoked_counts_total": revoked_counts_total,
            "revoked_counts": revoked_counts,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "integration auth status: integrations={} integrations_total={} available={} unavailable={} available_total={} unavailable_total={} state_counts={} state_counts_total={} revoked_counts={} revoked_counts_total={}",
        rows.len(),
        integrations_total,
        available,
        unavailable,
        available_total,
        unavailable_total,
        format_auth_state_counts(&state_counts),
        format_auth_state_counts(&state_counts_total),
        format_auth_state_counts(&revoked_counts),
        format_auth_state_counts(&revoked_counts_total)
    )];
    for row in rows {
        lines.push(format!(
            "integration credential: id={} available={} state={} source={} reason={} updated_unix={} revoked={}",
            row.integration_id,
            row.available,
            row.state,
            row.source,
            row.reason,
            row.updated_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.revoked
        ));
    }
    lines.join("\n")
}

fn execute_integration_auth_revoke_command(
    config: &AuthCommandConfig,
    integration_id: String,
    json_output: bool,
) -> String {
    let mut store = match load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    ) {
        Ok(store) => store,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": "integration_auth.revoke",
                    "integration_id": integration_id,
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return integration_auth_error("revoke", &integration_id, error);
        }
    };

    let status = if let Some(entry) = store.integrations.get_mut(&integration_id) {
        entry.secret = None;
        entry.revoked = true;
        entry.updated_unix = Some(current_unix_timestamp());
        "revoked"
    } else {
        "not_found"
    };
    if status == "revoked" {
        if let Err(error) = save_credential_store(
            &config.credential_store,
            &store,
            config.credential_store_key.as_deref(),
        ) {
            if json_output {
                return serde_json::json!({
                    "command": "integration_auth.revoke",
                    "integration_id": integration_id,
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return integration_auth_error("revoke", &integration_id, error);
        }
    }

    if json_output {
        return serde_json::json!({
            "command": "integration_auth.revoke",
            "integration_id": integration_id,
            "status": status,
            "credential_store": config.credential_store.display().to_string(),
        })
        .to_string();
    }

    format!(
        "integration auth revoke: integration_id={} status={} credential_store={}",
        integration_id,
        status,
        config.credential_store.display()
    )
}

pub fn execute_integration_auth_command(config: &AuthCommandConfig, command_args: &str) -> String {
    let command = match parse_integration_auth_command(command_args) {
        Ok(command) => command,
        Err(error) => return format!("integration auth error: {error}"),
    };

    match command {
        IntegrationAuthCommand::Set {
            integration_id,
            secret,
            json_output,
        } => execute_integration_auth_set_or_rotate_command(
            config,
            integration_id,
            secret,
            json_output,
            false,
        ),
        IntegrationAuthCommand::Status {
            integration_id,
            json_output,
        } => execute_integration_auth_status_command(config, integration_id, json_output),
        IntegrationAuthCommand::Rotate {
            integration_id,
            secret,
            json_output,
        } => execute_integration_auth_set_or_rotate_command(
            config,
            integration_id,
            secret,
            json_output,
            true,
        ),
        IntegrationAuthCommand::Revoke {
            integration_id,
            json_output,
        } => execute_integration_auth_revoke_command(config, integration_id, json_output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_integration_auth_histogram_helpers_are_deterministic() {
        let rows = vec![
            IntegrationAuthStatusRow {
                integration_id: "alpha".to_string(),
                available: true,
                state: "ready".to_string(),
                source: "credential_store".to_string(),
                reason: "ok".to_string(),
                updated_unix: Some(123),
                revoked: false,
            },
            IntegrationAuthStatusRow {
                integration_id: "beta".to_string(),
                available: false,
                state: "revoked".to_string(),
                source: "credential_store".to_string(),
                reason: "revoked".to_string(),
                updated_unix: Some(456),
                revoked: true,
            },
            IntegrationAuthStatusRow {
                integration_id: "gamma".to_string(),
                available: false,
                state: "missing_secret".to_string(),
                source: "credential_store".to_string(),
                reason: "missing".to_string(),
                updated_unix: None,
                revoked: false,
            },
        ];

        let state_counts = integration_state_counts(&rows);
        assert_eq!(state_counts.get("ready"), Some(&1));
        assert_eq!(state_counts.get("revoked"), Some(&1));
        assert_eq!(state_counts.get("missing_secret"), Some(&1));
        assert_eq!(
            format_auth_state_counts(&state_counts),
            "missing_secret:1,ready:1,revoked:1"
        );

        let revoked_counts = integration_revoked_counts(&rows);
        assert_eq!(revoked_counts.get("revoked"), Some(&1));
        assert_eq!(revoked_counts.get("not_revoked"), Some(&2));
        assert_eq!(
            format_auth_state_counts(&revoked_counts),
            "not_revoked:2,revoked:1"
        );
    }
}
