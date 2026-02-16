//! Trust-chain and signature verification policy helpers for skills.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use super::{RegistryKeyEntry, RemoteSkillSource, SkillRegistryManifest, TrustedKey};

pub(super) fn validate_remote_skill_source_metadata(source: &RemoteSkillSource) -> Result<()> {
    if source.signature.is_some() ^ source.signer_public_key.is_some() {
        bail!(
            "incomplete signature metadata for '{}': both signature and signer public key are required",
            source.url
        );
    }
    Ok(())
}

pub(super) fn validate_remote_skill_payload(
    source: &RemoteSkillSource,
    bytes: &[u8],
) -> Result<()> {
    if let Some(expected_sha256) = &source.sha256 {
        let actual_sha256 = sha256_hex(bytes);
        let expected_sha256 = normalize_sha256(expected_sha256);
        if actual_sha256 != expected_sha256 {
            bail!(
                "sha256 mismatch for '{}': expected {}, got {}",
                source.url,
                expected_sha256,
                actual_sha256
            );
        }
    }

    if let (Some(signature), Some(signer_public_key)) =
        (&source.signature, &source.signer_public_key)
    {
        verify_ed25519_signature(bytes, signature, signer_public_key)
            .with_context(|| format!("signature verification failed for '{}'", source.url))?;
    }
    Ok(())
}

pub(super) fn build_trusted_key_map(
    manifest: &SkillRegistryManifest,
    trust_roots: &[TrustedKey],
) -> Result<HashMap<String, String>> {
    let now_unix = current_unix_timestamp();
    let mut trusted = HashMap::new();
    for root in trust_roots {
        let root_public_key = root.public_key.trim().to_string();
        let root_key_bytes =
            decode_base64_fixed::<32>("trusted root public key", &root_public_key)?;
        let _ = VerifyingKey::from_bytes(&root_key_bytes)
            .with_context(|| format!("invalid trusted root key '{}'", root.id))?;

        if let Some(existing) = trusted.get(&root.id) {
            if existing != &root_public_key {
                bail!(
                    "duplicate trusted root id '{}' has conflicting keys",
                    root.id
                );
            }
            continue;
        }
        trusted.insert(root.id.clone(), root_public_key);
    }

    let mut manifest_keys = HashMap::new();
    for key in &manifest.keys {
        if manifest_keys.insert(key.id.clone(), key).is_some() {
            bail!("registry contains duplicate key id '{}'", key.id);
        }
        if key.signed_by.is_some() ^ key.signature.is_some() {
            bail!(
                "registry key '{}' has incomplete signing metadata (signed_by and signature are required together)",
                key.id
            );
        }
    }

    for root in trust_roots {
        if let Some(manifest_root) = manifest_keys.get(&root.id) {
            if manifest_root.revoked {
                bail!("trusted root '{}' is revoked in registry manifest", root.id);
            }
            if is_expired(manifest_root.expires_unix, now_unix) {
                bail!("trusted root '{}' is expired in registry manifest", root.id);
            }
            if manifest_root.public_key.trim() != root.public_key.trim() {
                bail!(
                    "trusted root '{}' does not match registry key material",
                    root.id
                );
            }
        }
    }

    loop {
        let mut progressed = false;
        for key in &manifest.keys {
            if trusted.contains_key(&key.id) {
                continue;
            }
            if key.revoked || is_expired(key.expires_unix, now_unix) {
                continue;
            }

            let (Some(signed_by), Some(signature)) = (&key.signed_by, &key.signature) else {
                continue;
            };
            let Some(signer_public_key) = trusted.get(signed_by).cloned() else {
                continue;
            };

            verify_ed25519_signature(
                key_certificate_payload(key).as_bytes(),
                signature,
                &signer_public_key,
            )
            .with_context(|| {
                format!(
                    "failed to verify registry key '{}' signed by '{}'",
                    key.id, signed_by
                )
            })?;

            trusted.insert(key.id.clone(), key.public_key.trim().to_string());
            progressed = true;
        }

        if !progressed {
            break;
        }
    }

    Ok(trusted)
}

pub(super) fn verify_ed25519_signature(
    message: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> Result<()> {
    let public_key_bytes = decode_base64_fixed::<32>("public key", public_key_base64)?;
    let signature_bytes = decode_base64_fixed::<64>("signature", signature_base64)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .context("failed to decode ed25519 public key bytes")?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify_strict(message, &signature)
        .map_err(|error| anyhow!("invalid ed25519 signature: {error}"))?;
    Ok(())
}

fn decode_base64_fixed<const N: usize>(label: &str, value: &str) -> Result<[u8; N]> {
    let decoded = BASE64
        .decode(value.trim())
        .with_context(|| format!("failed to decode {label} from base64"))?;
    let bytes: [u8; N] = decoded
        .try_into()
        .map_err(|_| anyhow!("{label} must decode to {N} bytes"))?;
    Ok(bytes)
}

fn key_certificate_payload(key: &RegistryKeyEntry) -> String {
    format!("tau-skill-key-v1:{}:{}", key.id, key.public_key.trim())
}

pub(super) fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn is_expired(expires_unix: Option<u64>, now_unix: u64) -> bool {
    matches!(expires_unix, Some(expires_unix) if expires_unix <= now_unix)
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn signature_digest_sha256_hex(signature: &str) -> String {
    sha256_hex(signature.trim().as_bytes())
}

pub(super) fn normalize_sha256(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
