//! Credential store schema, encryption, and persistence helpers.
//!
//! This module defines the on-disk credential contract and keyed/plaintext
//! storage behavior. It validates schema/integrity and reports explicit errors
//! for decryption, checksum, and format failures.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, Payload},
    Aes256Gcm,
};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tau_ai::Provider;
use tau_cli::{Cli, CliCredentialStoreEncryptionMode};
use tau_core::write_text_atomic;

use crate::types::{CredentialStoreEncryptionMode, ProviderAuthMethod};

const CREDENTIAL_STORE_SCHEMA_VERSION: u32 = 1;
const CREDENTIAL_STORE_ENCRYPTED_V1_PREFIX: &str = "enc:v1:";
const CREDENTIAL_STORE_ENCRYPTED_V2_PREFIX: &str = "enc:v2:";
const CREDENTIAL_STORE_LEGACY_NONCE_BYTES: usize = 16;
const CREDENTIAL_STORE_LEGACY_TAG_BYTES: usize = 32;
const CREDENTIAL_STORE_AES_GCM_NONCE_BYTES: usize = 12;
const CREDENTIAL_STORE_AES_GCM_AAD: &[u8] = b"tau-credential-store-v2";
const CREDENTIAL_STORE_MACHINE_KEY_CONTEXT: &str = "tau-credential-store-machine-key-v1";
const CREDENTIAL_STORE_MACHINE_ID_CANDIDATE_PATHS: [&str; 2] =
    ["/etc/machine-id", "/var/lib/dbus/machine-id"];

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

/// Public struct `DecryptedSecret` used across Tau components.
#[derive(Clone, PartialEq, Eq)]
pub struct DecryptedSecret(String);

impl DecryptedSecret {
    /// Public `fn` `new` in `tau-provider`.
    ///
    /// This item is part of the G20 secret-store hardening API surface.
    /// Callers rely on redacted formatting semantics and non-empty validation.
    pub fn new(secret: impl Into<String>) -> Result<Self> {
        let secret = secret.into();
        if secret.trim().is_empty() {
            bail!("decrypted secret must not be empty");
        }
        Ok(Self(secret))
    }

    /// Public `fn` `expose` in `tau-provider`.
    ///
    /// This item returns plaintext secret bytes for explicit use sites.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Public `fn` `into_inner` in `tau-provider`.
    ///
    /// This item consumes the wrapper and returns plaintext.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for DecryptedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl fmt::Display for DecryptedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Public trait `SecretStore` in `tau-provider`.
///
/// This trait abstracts storage backends for provider/integration secrets while
/// preserving existing credential-store schema behavior.
pub trait SecretStore {
    /// Public `fn` `load` in `tau-provider`.
    fn load(
        &self,
        default_mode: CredentialStoreEncryptionMode,
        key: Option<&str>,
    ) -> Result<CredentialStoreData>;

    /// Public `fn` `save` in `tau-provider`.
    fn save(&self, store: &CredentialStoreData, key: Option<&str>) -> Result<()>;

    /// Public `fn` `read_integration_secret` in `tau-provider`.
    fn read_integration_secret(
        &self,
        integration_id: &str,
        default_mode: CredentialStoreEncryptionMode,
        key: Option<&str>,
    ) -> Result<Option<DecryptedSecret>> {
        let store = self.load(default_mode, key)?;
        let Some(record) = store.integrations.get(integration_id) else {
            return Ok(None);
        };
        let secret = record
            .secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(DecryptedSecret::new)
            .transpose()?;
        Ok(secret)
    }

    /// Public `fn` `write_integration_secret` in `tau-provider`.
    fn write_integration_secret(
        &self,
        integration_id: &str,
        secret: &str,
        default_mode: CredentialStoreEncryptionMode,
        key: Option<&str>,
        updated_unix: Option<u64>,
    ) -> Result<()> {
        let secret = DecryptedSecret::new(secret)?;
        let mut store = self.load(default_mode, key)?;
        store.integrations.insert(
            integration_id.to_string(),
            IntegrationCredentialStoreRecord {
                secret: Some(secret.into_inner()),
                revoked: false,
                updated_unix,
            },
        );
        self.save(&store, key)
    }
}

/// Public struct `FileSecretStore` used across Tau components.
#[derive(Debug, Clone)]
pub struct FileSecretStore {
    path: PathBuf,
}

impl FileSecretStore {
    /// Public `fn` `new` in `tau-provider`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Public `fn` `path` in `tau-provider`.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SecretStore for FileSecretStore {
    fn load(
        &self,
        default_mode: CredentialStoreEncryptionMode,
        key: Option<&str>,
    ) -> Result<CredentialStoreData> {
        tracing::debug!(
            credential_store = %self.path.display(),
            encryption = ?default_mode,
            has_key = key.is_some_and(|value| !value.trim().is_empty()),
            "loading credential secret store"
        );
        load_credential_store(&self.path, default_mode, key)
    }

    fn save(&self, store: &CredentialStoreData, key: Option<&str>) -> Result<()> {
        tracing::debug!(
            credential_store = %self.path.display(),
            encryption = ?store.encryption,
            has_key = key.is_some_and(|value| !value.trim().is_empty()),
            provider_entries = store.providers.len(),
            integration_entries = store.integrations.len(),
            "saving credential secret store"
        );
        save_credential_store(&self.path, store, key)
    }
}

/// Public `fn` `resolve_credential_store_encryption_mode` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn resolve_credential_store_encryption_mode(cli: &Cli) -> CredentialStoreEncryptionMode {
    match cli.credential_store_encryption {
        CliCredentialStoreEncryptionMode::None => CredentialStoreEncryptionMode::None,
        CliCredentialStoreEncryptionMode::Keyed => CredentialStoreEncryptionMode::Keyed,
        CliCredentialStoreEncryptionMode::Auto => CredentialStoreEncryptionMode::Keyed,
    }
}

fn derive_credential_store_key_material(key: Option<&str>) -> Result<[u8; 32]> {
    let key_seed = match key.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            if value.len() < 8 {
                bail!("credential store key must be at least 8 characters");
            }
            value.to_string()
        }
        None => machine_derived_credential_store_key_seed(),
    };
    let digest = Sha256::digest(key_seed.as_bytes());
    let mut material = [0u8; 32];
    material.copy_from_slice(&digest);
    Ok(material)
}

fn machine_derived_credential_store_key_seed() -> String {
    let mut segments = vec![
        CREDENTIAL_STORE_MACHINE_KEY_CONTEXT.to_string(),
        format!("os={}", std::env::consts::OS),
        format!("arch={}", std::env::consts::ARCH),
    ];
    for variable in [
        "HOSTNAME",
        "COMPUTERNAME",
        "USER",
        "USERNAME",
        "HOME",
        "USERPROFILE",
    ] {
        if let Ok(value) = std::env::var(variable) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                segments.push(format!("{variable}={trimmed}"));
            }
        }
    }
    if let Some(machine_id) = read_machine_id_file() {
        segments.push(format!("machine_id={machine_id}"));
    }
    segments.join("|")
}

fn read_machine_id_file() -> Option<String> {
    for path in CREDENTIAL_STORE_MACHINE_ID_CANDIDATE_PATHS {
        if let Ok(raw) = std::fs::read_to_string(path) {
            let value = raw.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
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

fn legacy_credential_store_tag(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(nonce);
    hasher.update(ciphertext);
    hasher.update(b"tau-credential-store-legacy-v1");
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

/// Public `fn` `encrypt_credential_store_secret` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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
        CredentialStoreEncryptionMode::Keyed => encrypt_credential_store_secret_v2(secret, key),
    }
}

fn encrypt_credential_store_secret_v2(secret: &str, key: Option<&str>) -> Result<String> {
    let key_material = derive_credential_store_key_material(key)?;
    let cipher = Aes256Gcm::new_from_slice(&key_material)
        .map_err(|_| anyhow!("credential key material has invalid length"))?;
    let mut nonce = [0u8; CREDENTIAL_STORE_AES_GCM_NONCE_BYTES];
    use aes_gcm::aead::rand_core::RngCore as _;
    OsRng.fill_bytes(&mut nonce);

    let ciphertext = cipher
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: secret.as_bytes(),
                aad: CREDENTIAL_STORE_AES_GCM_AAD,
            },
        )
        .map_err(|_| anyhow!("credential payload encryption failed"))?;

    let mut payload = Vec::with_capacity(CREDENTIAL_STORE_AES_GCM_NONCE_BYTES + ciphertext.len());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&ciphertext);
    Ok(format!(
        "{CREDENTIAL_STORE_ENCRYPTED_V2_PREFIX}{}",
        BASE64_STANDARD.encode(payload)
    ))
}

fn decrypt_credential_store_secret_v2(payload: &str, key: Option<&str>) -> Result<String> {
    let key_material = derive_credential_store_key_material(key)?;
    let cipher = Aes256Gcm::new_from_slice(&key_material)
        .map_err(|_| anyhow!("credential key material has invalid length"))?;
    let raw = BASE64_STANDARD
        .decode(payload)
        .map_err(|_| anyhow!("credential payload encoding is invalid"))?;
    if raw.len() <= CREDENTIAL_STORE_AES_GCM_NONCE_BYTES {
        bail!("credential payload is truncated");
    }

    let nonce = &raw[..CREDENTIAL_STORE_AES_GCM_NONCE_BYTES];
    let ciphertext = &raw[CREDENTIAL_STORE_AES_GCM_NONCE_BYTES..];
    let plaintext = cipher
        .decrypt(
            nonce.into(),
            Payload {
                msg: ciphertext,
                aad: CREDENTIAL_STORE_AES_GCM_AAD,
            },
        )
        .map_err(|_| anyhow!("credential payload integrity check failed"))?;
    let secret = String::from_utf8(plaintext)
        .map_err(|_| anyhow!("credential payload is not valid UTF-8"))?;
    if secret.trim().is_empty() {
        bail!("credential payload resolves to an empty secret");
    }
    Ok(secret)
}

fn decrypt_credential_store_secret_legacy(payload: &str, key: Option<&str>) -> Result<String> {
    let key_material = derive_credential_store_key_material(key)?;
    let raw = BASE64_STANDARD
        .decode(payload)
        .map_err(|_| anyhow!("credential payload encoding is invalid"))?;
    if raw.len() < CREDENTIAL_STORE_LEGACY_NONCE_BYTES + CREDENTIAL_STORE_LEGACY_TAG_BYTES {
        bail!("credential payload is truncated");
    }

    let nonce_end = CREDENTIAL_STORE_LEGACY_NONCE_BYTES;
    let tag_end = nonce_end + CREDENTIAL_STORE_LEGACY_TAG_BYTES;
    let nonce = &raw[..nonce_end];
    let tag = &raw[nonce_end..tag_end];
    let ciphertext = &raw[tag_end..];

    let expected_tag = legacy_credential_store_tag(&key_material, nonce, ciphertext);
    if !timing_safe_equal(tag, &expected_tag) {
        bail!("credential payload integrity check failed");
    }

    let plaintext = xor_with_keyed_stream(ciphertext, &key_material, nonce);
    let secret = String::from_utf8(plaintext)
        .map_err(|_| anyhow!("credential payload is not valid UTF-8"))?;
    if secret.trim().is_empty() {
        bail!("credential payload resolves to an empty secret");
    }
    Ok(secret)
}

/// Public `fn` `decrypt_credential_store_secret` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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
            if let Some(payload) = encoded.strip_prefix(CREDENTIAL_STORE_ENCRYPTED_V2_PREFIX) {
                return decrypt_credential_store_secret_v2(payload, key);
            }
            if let Some(payload) = encoded.strip_prefix(CREDENTIAL_STORE_ENCRYPTED_V1_PREFIX) {
                return decrypt_credential_store_secret_legacy(payload, key);
            }
            bail!("credential payload prefix is invalid");
        }
    }
}

/// Public `fn` `load_credential_store` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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

/// Public `fn` `save_credential_store` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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

/// Public `fn` `refresh_provider_access_token` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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

/// Public `fn` `reauth_required_error` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn reauth_required_error(provider: Provider, reason: &str) -> anyhow::Error {
    anyhow!(
        "provider '{}' requires re-authentication: {reason}",
        provider.as_str()
    )
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    use super::{
        decrypt_credential_store_secret, encrypt_credential_store_secret, xor_with_keyed_stream,
        CredentialStoreEncryptionMode, BASE64_STANDARD, CREDENTIAL_STORE_ENCRYPTED_V1_PREFIX,
        CREDENTIAL_STORE_LEGACY_NONCE_BYTES,
    };
    use super::{derive_credential_store_key_material, legacy_credential_store_tag};

    #[test]
    fn spec_2774_c02_keyed_payload_uses_v2_prefix_and_roundtrips() {
        let encoded = encrypt_credential_store_secret(
            "super-secret-token",
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-store-passphrase"),
        )
        .expect("keyed encryption must succeed");
        assert!(
            encoded.starts_with("enc:v2:"),
            "expected v2 envelope prefix for keyed payloads"
        );
        let decoded = decrypt_credential_store_secret(
            &encoded,
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-store-passphrase"),
        )
        .expect("keyed decrypt must roundtrip");
        assert_eq!(decoded, "super-secret-token");
    }

    #[test]
    fn regression_spec_2774_c03_legacy_v1_payload_still_decrypts() {
        let encoded = build_legacy_v1_payload("legacy-secret", Some("credential-store-passphrase"));
        let decoded = decrypt_credential_store_secret(
            &encoded,
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-store-passphrase"),
        )
        .expect("legacy v1 payload must remain readable");
        assert_eq!(decoded, "legacy-secret");
    }

    #[test]
    fn regression_spec_2774_c04_tampered_v2_payload_fails_closed() {
        let encoded = encrypt_credential_store_secret(
            "super-secret-token",
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-store-passphrase"),
        )
        .expect("keyed encryption must succeed");
        let payload = encoded
            .strip_prefix("enc:v2:")
            .expect("v2 prefix should be present for keyed payload");
        let mut raw = BASE64_STANDARD
            .decode(payload)
            .expect("payload must be base64");
        let last = raw
            .last_mut()
            .expect("encrypted payload should include at least one ciphertext byte");
        *last ^= 0xAA;
        let tampered = format!("enc:v2:{}", BASE64_STANDARD.encode(raw));
        let error = decrypt_credential_store_secret(
            &tampered,
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-store-passphrase"),
        )
        .expect_err("tampered payload must fail closed");
        assert!(
            error.to_string().contains("integrity check failed"),
            "expected integrity failure, got: {error}"
        );
    }

    fn build_legacy_v1_payload(secret: &str, key: Option<&str>) -> String {
        let key_material = derive_credential_store_key_material(key).expect("derive key");
        let nonce = [7u8; CREDENTIAL_STORE_LEGACY_NONCE_BYTES];
        let ciphertext = xor_with_keyed_stream(secret.as_bytes(), &key_material, &nonce);
        let tag = legacy_credential_store_tag(&key_material, &nonce, &ciphertext);
        let mut payload = Vec::with_capacity(nonce.len() + tag.len() + ciphertext.len());
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&tag);
        payload.extend_from_slice(&ciphertext);
        format!(
            "{CREDENTIAL_STORE_ENCRYPTED_V1_PREFIX}{}",
            BASE64_STANDARD.encode(payload)
        )
    }
}
