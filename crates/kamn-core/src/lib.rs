//! Core KAMN DID primitives for browser- and edge-facing runtimes.

use anyhow::{bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const KAMN_DID_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `DidMethod` values.
pub enum DidMethod {
    Key,
    Web,
}

impl DidMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            DidMethod::Key => "key",
            DidMethod::Web => "web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserDidIdentityRequest` used across Tau components.
pub struct BrowserDidIdentityRequest {
    pub method: DidMethod,
    pub network: String,
    pub subject: String,
    pub entropy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Public struct `BrowserDidIdentity` used across Tau components.
pub struct BrowserDidIdentity {
    pub schema_version: u32,
    pub method: DidMethod,
    pub network: String,
    pub subject: String,
    pub did: String,
    pub key_id: String,
    pub fingerprint: String,
    pub proof_material: String,
}

pub fn build_browser_did_identity(
    request: &BrowserDidIdentityRequest,
) -> Result<BrowserDidIdentity> {
    let network = normalize_identifier("network", request.network.as_str())?;
    let subject = normalize_identifier("subject", request.subject.as_str())?;
    let entropy = request.entropy.trim();
    if entropy.is_empty() {
        bail!("entropy cannot be empty");
    }

    let digest = compute_digest(request.method, network.as_str(), subject.as_str(), entropy);
    let fingerprint = to_hex(&digest);
    let proof_material = BASE64_URL.encode(digest);
    let did = render_did(
        request.method,
        network.as_str(),
        subject.as_str(),
        proof_material.as_str(),
    );
    let key_id = format!("{did}#primary");

    Ok(BrowserDidIdentity {
        schema_version: KAMN_DID_SCHEMA_VERSION,
        method: request.method,
        network,
        subject,
        did,
        key_id,
        fingerprint,
        proof_material,
    })
}

fn render_did(method: DidMethod, network: &str, subject: &str, proof_material: &str) -> String {
    match method {
        DidMethod::Key => format!("did:key:z{proof_material}"),
        DidMethod::Web => {
            let network_component = network.replace('.', ":");
            format!("did:web:{network_component}:{subject}")
        }
    }
}

fn compute_digest(method: DidMethod, network: &str, subject: &str, entropy: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(method.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(network.as_bytes());
    hasher.update(b":");
    hasher.update(subject.as_bytes());
    hasher.update(b":");
    hasher.update(entropy.as_bytes());
    hasher.finalize().into()
}

fn normalize_identifier(label: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{label} cannot be empty");
    }
    if trimmed.starts_with('.') || trimmed.ends_with('.') || trimmed.contains("..") {
        bail!("{label} contains empty label segments");
    }
    if trimmed
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'))
    {
        bail!("{label} contains unsupported characters");
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{build_browser_did_identity, BrowserDidIdentityRequest, DidMethod};

    fn base_request(method: DidMethod) -> BrowserDidIdentityRequest {
        BrowserDidIdentityRequest {
            method,
            network: "tau-devnet".to_string(),
            subject: "agent_alpha".to_string(),
            entropy: "seed-001".to_string(),
        }
    }

    #[test]
    fn unit_build_browser_did_identity_requires_non_empty_entropy() {
        let error = build_browser_did_identity(&BrowserDidIdentityRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "agent".to_string(),
            entropy: " ".to_string(),
        })
        .expect_err("empty entropy should fail");
        assert!(error.to_string().contains("entropy cannot be empty"));
    }

    #[test]
    fn functional_build_browser_did_identity_key_method_produces_did_key() {
        let identity = build_browser_did_identity(&BrowserDidIdentityRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "agent_alpha".to_string(),
            entropy: "seed-001".to_string(),
        })
        .expect("build did identity");

        assert_eq!(identity.method, DidMethod::Key);
        assert!(identity.did.starts_with("did:key:z"));
        assert_eq!(identity.key_id, format!("{}#primary", identity.did));
        assert_eq!(identity.fingerprint.len(), 64);
    }

    #[test]
    fn integration_build_browser_did_identity_is_deterministic_for_same_input() {
        let request = BrowserDidIdentityRequest {
            method: DidMethod::Web,
            network: "edge.tau.local".to_string(),
            subject: "operator".to_string(),
            entropy: "stable-seed".to_string(),
        };
        let first = build_browser_did_identity(&request).expect("first");
        let second = build_browser_did_identity(&request).expect("second");

        assert_eq!(first.did, second.did);
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.proof_material, second.proof_material);
        assert!(first.did.starts_with("did:web:edge:tau:local:operator"));
    }

    #[test]
    fn regression_build_browser_did_identity_rejects_invalid_subject_chars() {
        let error = build_browser_did_identity(&BrowserDidIdentityRequest {
            method: DidMethod::Web,
            network: "tau-devnet".to_string(),
            subject: "operator with spaces".to_string(),
            entropy: "seed".to_string(),
        })
        .expect_err("subject with spaces should fail");

        assert!(error
            .to_string()
            .contains("subject contains unsupported characters"));
    }

    #[test]
    fn spec_c01_rejects_network_with_empty_dot_segment() {
        let mut request = base_request(DidMethod::Web);
        request.network = "edge..tau".to_string();

        let error = build_browser_did_identity(&request)
            .expect_err("network with empty segment should fail");
        assert!(error
            .to_string()
            .contains("network contains empty label segments"));
    }

    #[test]
    fn spec_c02_rejects_subject_with_empty_dot_segment() {
        let mut request = base_request(DidMethod::Web);
        request.network = "edge.tau.local".to_string();
        request.subject = "agent..primary".to_string();

        let error = build_browser_did_identity(&request)
            .expect_err("subject with empty segment should fail");
        assert!(error
            .to_string()
            .contains("subject contains empty label segments"));
    }

    #[test]
    fn spec_c03_normalizes_network_and_subject_to_canonical_lowercase() {
        let identity = build_browser_did_identity(&BrowserDidIdentityRequest {
            method: DidMethod::Web,
            network: "  EDGE.TAU.Local  ".to_string(),
            subject: "  Operator_One  ".to_string(),
            entropy: "seed-001".to_string(),
        })
        .expect("request with mixed case and padding should succeed");

        assert_eq!(identity.network, "edge.tau.local");
        assert_eq!(identity.subject, "operator_one");
    }

    #[test]
    fn spec_c04_key_method_outputs_are_stable_for_equivalent_normalized_inputs() {
        let canonical = build_browser_did_identity(&BrowserDidIdentityRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "agent_alpha".to_string(),
            entropy: "seed-001".to_string(),
        })
        .expect("canonical request should succeed");
        let normalized_equivalent = build_browser_did_identity(&BrowserDidIdentityRequest {
            method: DidMethod::Key,
            network: "  TAU-DEVNET ".to_string(),
            subject: " AGENT_ALPHA ".to_string(),
            entropy: "seed-001".to_string(),
        })
        .expect("equivalent normalized request should succeed");

        assert!(canonical.did.starts_with("did:key:z"));
        assert_eq!(canonical.key_id, format!("{}#primary", canonical.did));
        assert_eq!(canonical.did, normalized_equivalent.did);
        assert_eq!(canonical.fingerprint, normalized_equivalent.fingerprint);
        assert_eq!(
            canonical.proof_material,
            normalized_equivalent.proof_material
        );
    }
}
