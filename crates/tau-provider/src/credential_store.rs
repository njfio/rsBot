//! Credential store schema, encryption, and persistence helpers.
//!
//! This module defines the on-disk credential contract and keyed/plaintext
//! storage behavior. It validates schema/integrity and reports explicit errors
//! for decryption, checksum, and format failures.

use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tau_ai::Provider;
use tau_cli::{Cli, CliCredentialStoreEncryptionMode};
use tau_core::{current_unix_timestamp, write_text_atomic};

use crate::types::{CredentialStoreEncryptionMode, ProviderAuthMethod};

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
/// Public struct `ProviderCredentialStoreRecord` used across Tau components.
pub struct ProviderCredentialStoreRecord {
    pub auth_method: ProviderAuthMethod,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_unix: Option<u64>,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `IntegrationCredentialStoreRecord` used across Tau components.
pub struct IntegrationCredentialStoreRecord {
    pub secret: Option<String>,
    pub revoked: bool,
    pub updated_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `CredentialStoreData` used across Tau components.
pub struct CredentialStoreData {
    pub encryption: CredentialStoreEncryptionMode,
    pub providers: BTreeMap<String, ProviderCredentialStoreRecord>,
    pub integrations: BTreeMap<String, IntegrationCredentialStoreRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `RefreshedProviderCredential` used across Tau components.
pub struct RefreshedProviderCredential {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_unix: Option<u64>,
}

pub fn resolve_credential_store_encryption_mode(cli: &Cli) -> CredentialStoreEncryptionMode {
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
            anyhow!("credential store key is required for keyed encryption (set --credential-store-key or TAU_CREDENTIAL_STORE_KEY)")
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
    hasher.update(b"tau-credential-store-v1");
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

pub fn encrypt_credential_store_secret(
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

pub fn decrypt_credential_store_secret(
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

pub fn load_credential_store(
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
                        "credential store entry '{}' secret is invalid or corrupted",
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

pub fn save_credential_store(
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

pub fn refresh_provider_access_token(
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

pub fn reauth_required_error(provider: Provider, reason: &str) -> anyhow::Error {
    anyhow!(
        "provider '{}' requires re-authentication: {reason}",
        provider.as_str()
    )
}
