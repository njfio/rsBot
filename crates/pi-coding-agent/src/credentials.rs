use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use pi_ai::Provider;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    current_unix_timestamp, write_text_atomic, AuthCommandConfig, Cli,
    CliCredentialStoreEncryptionMode, CredentialStoreEncryptionMode, ProviderAuthMethod,
};

pub(crate) fn resolve_non_empty_cli_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn resolve_secret_from_cli_or_store_id(
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
pub(crate) enum IntegrationAuthCommand {
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

pub(crate) fn normalize_integration_credential_id(raw: &str) -> Result<String> {
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

pub(crate) fn parse_integration_auth_command(command_args: &str) -> Result<IntegrationAuthCommand> {
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

    let rows = match integration_id {
        Some(integration_id) => {
            vec![integration_status_row_for_entry(
                &integration_id,
                store.integrations.get(&integration_id),
            )]
        }
        None => store
            .integrations
            .iter()
            .map(|(integration_id, entry)| {
                integration_status_row_for_entry(integration_id, Some(entry))
            })
            .collect::<Vec<_>>(),
    };
    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);

    if json_output {
        return serde_json::json!({
            "command": "integration_auth.status",
            "integrations": rows.len(),
            "available": available,
            "unavailable": unavailable,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "integration auth status: integrations={} available={} unavailable={}",
        rows.len(),
        available,
        unavailable
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

pub(crate) fn execute_integration_auth_command(
    config: &AuthCommandConfig,
    command_args: &str,
) -> String {
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

const CREDENTIAL_STORE_SCHEMA_VERSION: u32 = 1;
const CREDENTIAL_STORE_ENCRYPTED_PREFIX: &str = "enc:v1:";
const CREDENTIAL_STORE_NONCE_BYTES: usize = 16;
const CREDENTIAL_STORE_TAG_BYTES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CredentialStoreFile {
    schema_version: u32,
    encryption: CredentialStoreEncryptionMode,
    providers: BTreeMap<String, StoredProviderCredential>,
    #[serde(default)]
    integrations: BTreeMap<String, StoredIntegrationCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredProviderCredential {
    auth_method: ProviderAuthMethod,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_unix: Option<u64>,
    revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredIntegrationCredential {
    secret: Option<String>,
    revoked: bool,
    updated_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderCredentialStoreRecord {
    pub(crate) auth_method: ProviderAuthMethod,
    pub(crate) access_token: Option<String>,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_unix: Option<u64>,
    pub(crate) revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntegrationCredentialStoreRecord {
    pub(crate) secret: Option<String>,
    pub(crate) revoked: bool,
    pub(crate) updated_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CredentialStoreData {
    pub(crate) encryption: CredentialStoreEncryptionMode,
    pub(crate) providers: BTreeMap<String, ProviderCredentialStoreRecord>,
    pub(crate) integrations: BTreeMap<String, IntegrationCredentialStoreRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefreshedProviderCredential {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_unix: Option<u64>,
}

pub(crate) fn resolve_credential_store_encryption_mode(cli: &Cli) -> CredentialStoreEncryptionMode {
    match cli.credential_store_encryption {
        CliCredentialStoreEncryptionMode::None => CredentialStoreEncryptionMode::None,
        CliCredentialStoreEncryptionMode::Keyed => CredentialStoreEncryptionMode::Keyed,
        CliCredentialStoreEncryptionMode::Auto => {
            if cli
                .credential_store_key
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                CredentialStoreEncryptionMode::Keyed
            } else {
                CredentialStoreEncryptionMode::None
            }
        }
    }
}

fn derive_credential_store_key_material(key: Option<&str>) -> Result<[u8; 32]> {
    let key = key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("credential store key is required for keyed encryption (set --credential-store-key or PI_CREDENTIAL_STORE_KEY)")
        })?;
    if key.len() < 8 {
        bail!("credential store key must be at least 8 characters");
    }

    let digest = Sha256::digest(key.as_bytes());
    let mut material = [0u8; 32];
    material.copy_from_slice(&digest);
    Ok(material)
}

fn derive_credential_store_nonce() -> [u8; CREDENTIAL_STORE_NONCE_BYTES] {
    let mut seed = Vec::new();
    seed.extend_from_slice(&current_unix_timestamp().to_le_bytes());
    seed.extend_from_slice(&(std::process::id() as u64).to_le_bytes());
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    seed.extend_from_slice(&now_nanos.to_le_bytes());
    let digest = Sha256::digest(seed);
    let mut nonce = [0u8; CREDENTIAL_STORE_NONCE_BYTES];
    nonce.copy_from_slice(&digest[..CREDENTIAL_STORE_NONCE_BYTES]);
    nonce
}

fn xor_with_keyed_stream(data: &[u8], key: &[u8; 32], nonce: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(data.len());
    let mut offset = 0usize;
    let mut counter = 0u64;
    while offset < data.len() {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(nonce);
        hasher.update(counter.to_le_bytes());
        let block = hasher.finalize();
        for byte in block {
            if offset >= data.len() {
                break;
            }
            output.push(data[offset] ^ byte);
            offset += 1;
        }
        counter = counter.saturating_add(1);
    }
    output
}

fn credential_store_tag(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(nonce);
    hasher.update(ciphertext);
    hasher.update(b"pi-credential-store-v1");
    let digest = hasher.finalize();
    let mut tag = [0u8; 32];
    tag.copy_from_slice(&digest);
    tag
}

fn timing_safe_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (lhs, rhs) in left.iter().zip(right) {
        diff |= lhs ^ rhs;
    }
    diff == 0
}

pub(crate) fn encrypt_credential_store_secret(
    secret: &str,
    mode: CredentialStoreEncryptionMode,
    key: Option<&str>,
) -> Result<String> {
    let secret = secret.trim();
    if secret.is_empty() {
        bail!("credential secret must not be empty");
    }

    match mode {
        CredentialStoreEncryptionMode::None => Ok(secret.to_string()),
        CredentialStoreEncryptionMode::Keyed => {
            let key = derive_credential_store_key_material(key)?;
            let nonce = derive_credential_store_nonce();
            let ciphertext = xor_with_keyed_stream(secret.as_bytes(), &key, &nonce);
            let tag = credential_store_tag(&key, &nonce, &ciphertext);

            let mut payload = Vec::with_capacity(
                CREDENTIAL_STORE_NONCE_BYTES + CREDENTIAL_STORE_TAG_BYTES + ciphertext.len(),
            );
            payload.extend_from_slice(&nonce);
            payload.extend_from_slice(&tag);
            payload.extend_from_slice(&ciphertext);

            Ok(format!(
                "{CREDENTIAL_STORE_ENCRYPTED_PREFIX}{}",
                BASE64_STANDARD.encode(payload)
            ))
        }
    }
}

pub(crate) fn decrypt_credential_store_secret(
    encoded: &str,
    mode: CredentialStoreEncryptionMode,
    key: Option<&str>,
) -> Result<String> {
    match mode {
        CredentialStoreEncryptionMode::None => {
            let value = encoded.trim();
            if value.is_empty() {
                bail!("credential secret must not be empty");
            }
            Ok(value.to_string())
        }
        CredentialStoreEncryptionMode::Keyed => {
            let key = derive_credential_store_key_material(key)?;
            let payload = encoded
                .strip_prefix(CREDENTIAL_STORE_ENCRYPTED_PREFIX)
                .ok_or_else(|| anyhow!("credential payload prefix is invalid"))?;
            let raw = BASE64_STANDARD
                .decode(payload)
                .map_err(|_| anyhow!("credential payload encoding is invalid"))?;
            if raw.len() < CREDENTIAL_STORE_NONCE_BYTES + CREDENTIAL_STORE_TAG_BYTES {
                bail!("credential payload is truncated");
            }

            let nonce_end = CREDENTIAL_STORE_NONCE_BYTES;
            let tag_end = nonce_end + CREDENTIAL_STORE_TAG_BYTES;
            let nonce = &raw[..nonce_end];
            let tag = &raw[nonce_end..tag_end];
            let ciphertext = &raw[tag_end..];

            let expected_tag = credential_store_tag(&key, nonce, ciphertext);
            if !timing_safe_equal(tag, &expected_tag) {
                bail!("credential payload integrity check failed");
            }

            let plaintext = xor_with_keyed_stream(ciphertext, &key, nonce);
            let secret = String::from_utf8(plaintext)
                .map_err(|_| anyhow!("credential payload is not valid UTF-8"))?;
            if secret.trim().is_empty() {
                bail!("credential payload resolves to an empty secret");
            }
            Ok(secret)
        }
    }
}

pub(crate) fn load_credential_store(
    path: &Path,
    default_mode: CredentialStoreEncryptionMode,
    key: Option<&str>,
) -> Result<CredentialStoreData> {
    if !path.exists() {
        return Ok(CredentialStoreData {
            encryption: default_mode,
            providers: BTreeMap::new(),
            integrations: BTreeMap::new(),
        });
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read credential store {}", path.display()))?;
    let parsed = serde_json::from_str::<CredentialStoreFile>(&raw)
        .with_context(|| format!("failed to parse credential store {}", path.display()))?;
    if parsed.schema_version != CREDENTIAL_STORE_SCHEMA_VERSION {
        bail!(
            "unsupported credential store schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            CREDENTIAL_STORE_SCHEMA_VERSION
        );
    }

    let mut providers = BTreeMap::new();
    for (provider, record) in parsed.providers {
        let access_token = record
            .access_token
            .map(|value| {
                decrypt_credential_store_secret(&value, parsed.encryption, key).with_context(|| {
                    format!(
                        "credential store entry '{}' access token is invalid or corrupted",
                        provider
                    )
                })
            })
            .transpose()?;
        let refresh_token = record
            .refresh_token
            .map(|value| {
                decrypt_credential_store_secret(&value, parsed.encryption, key).with_context(|| {
                    format!(
                        "credential store entry '{}' refresh token is invalid or corrupted",
                        provider
                    )
                })
            })
            .transpose()?;
        providers.insert(
            provider,
            ProviderCredentialStoreRecord {
                auth_method: record.auth_method,
                access_token,
                refresh_token,
                expires_unix: record.expires_unix,
                revoked: record.revoked,
            },
        );
    }

    let mut integrations = BTreeMap::new();
    for (integration_id, record) in parsed.integrations {
        let secret = record
            .secret
            .map(|value| {
                decrypt_credential_store_secret(&value, parsed.encryption, key).with_context(|| {
                    format!(
                        "integration credential store entry '{}' secret is invalid or corrupted",
                        integration_id
                    )
                })
            })
            .transpose()?;
        integrations.insert(
            integration_id,
            IntegrationCredentialStoreRecord {
                secret,
                revoked: record.revoked,
                updated_unix: record.updated_unix,
            },
        );
    }

    Ok(CredentialStoreData {
        encryption: parsed.encryption,
        providers,
        integrations,
    })
}

pub(crate) fn save_credential_store(
    path: &Path,
    store: &CredentialStoreData,
    key: Option<&str>,
) -> Result<()> {
    let mut providers = BTreeMap::new();
    for (provider, record) in &store.providers {
        let access_token = record
            .access_token
            .as_deref()
            .map(|value| {
                encrypt_credential_store_secret(value, store.encryption, key).with_context(|| {
                    format!(
                        "failed to encode credential store entry '{}' access token",
                        provider
                    )
                })
            })
            .transpose()?;
        let refresh_token = record
            .refresh_token
            .as_deref()
            .map(|value| {
                encrypt_credential_store_secret(value, store.encryption, key).with_context(|| {
                    format!(
                        "failed to encode credential store entry '{}' refresh token",
                        provider
                    )
                })
            })
            .transpose()?;
        providers.insert(
            provider.clone(),
            StoredProviderCredential {
                auth_method: record.auth_method,
                access_token,
                refresh_token,
                expires_unix: record.expires_unix,
                revoked: record.revoked,
            },
        );
    }

    let mut integrations = BTreeMap::new();
    for (integration_id, record) in &store.integrations {
        let secret = record
            .secret
            .as_deref()
            .map(|value| {
                encrypt_credential_store_secret(value, store.encryption, key).with_context(|| {
                    format!(
                        "failed to encode integration credential store entry '{}' secret",
                        integration_id
                    )
                })
            })
            .transpose()?;
        integrations.insert(
            integration_id.clone(),
            StoredIntegrationCredential {
                secret,
                revoked: record.revoked,
                updated_unix: record.updated_unix,
            },
        );
    }

    let payload = CredentialStoreFile {
        schema_version: CREDENTIAL_STORE_SCHEMA_VERSION,
        encryption: store.encryption,
        providers,
        integrations,
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode credential store")?;
    encoded.push('\n');
    write_text_atomic(path, &encoded)
}

pub(crate) fn refresh_provider_access_token(
    provider: Provider,
    refresh_token: &str,
    now_unix: u64,
) -> Result<RefreshedProviderCredential> {
    let refresh_token = refresh_token.trim();
    if refresh_token.is_empty() {
        bail!("refresh token is empty");
    }
    if refresh_token.starts_with("revoked") {
        bail!("refresh token revoked");
    }
    if refresh_token.starts_with("invalid") {
        bail!("refresh token invalid");
    }

    let seed = format!("{}:{refresh_token}:{now_unix}", provider.as_str());
    let digest = format!("{:x}", Sha256::digest(seed.as_bytes()));
    Ok(RefreshedProviderCredential {
        access_token: format!("{}_access_{}", provider.as_str(), &digest[..24]),
        refresh_token: Some(format!("{}_refresh_{}", provider.as_str(), &digest[24..48])),
        expires_unix: Some(now_unix.saturating_add(3600)),
    })
}

pub(crate) fn reauth_required_error(provider: Provider, reason: &str) -> anyhow::Error {
    anyhow!(
        "provider '{}' requires re-authentication: {reason}",
        provider.as_str()
    )
}
