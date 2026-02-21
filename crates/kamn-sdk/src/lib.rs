//! KAMN SDK surface for browser-native DID bootstrapping.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use anyhow::{anyhow, Context, Result};
pub use kamn_core::{
    BrowserDidIdentity, BrowserDidIdentityRequest, DidMethod, KAMN_DID_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserDidInitRequest` used across Tau components.
pub struct BrowserDidInitRequest {
    pub method: DidMethod,
    pub network: String,
    pub subject: String,
    pub entropy: String,
}

impl Default for BrowserDidInitRequest {
    fn default() -> Self {
        Self {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "browser-agent".to_string(),
            entropy: "default-seed".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Public struct `BrowserDidInitReport` used across Tau components.
pub struct BrowserDidInitReport {
    pub schema_version: u32,
    pub runtime: String,
    pub interoperability_profile: String,
    pub identity: BrowserDidIdentity,
}

pub fn initialize_browser_did(request: &BrowserDidInitRequest) -> Result<BrowserDidInitReport> {
    let identity = kamn_core::build_browser_did_identity(&BrowserDidIdentityRequest {
        method: request.method,
        network: request.network.clone(),
        subject: request.subject.clone(),
        entropy: request.entropy.clone(),
    })
    .map_err(|error| anyhow!("failed to build browser DID identity: {error}"))?;

    Ok(BrowserDidInitReport {
        schema_version: KAMN_DID_SCHEMA_VERSION,
        runtime: runtime_label().to_string(),
        interoperability_profile: "kamn_browser_v1".to_string(),
        identity,
    })
}

pub fn render_browser_did_init_report(report: &BrowserDidInitReport) -> String {
    format!(
        "browser did init: runtime={} profile={} method={} did={} key_id={} fingerprint={}",
        report.runtime,
        report.interoperability_profile,
        report.identity.method.as_str(),
        report.identity.did,
        report.identity.key_id,
        report.identity.fingerprint
    )
}

pub fn render_browser_did_init_report_json(report: &BrowserDidInitReport) -> Result<String> {
    serde_json::to_string_pretty(report).context("failed to serialize browser did init report")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write_browser_did_init_report(path: &Path, report: &BrowserDidInitReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut payload = render_browser_did_init_report_json(report)?;
    payload.push('\n');
    std::fs::write(path, payload).with_context(|| format!("failed to write {}", path.display()))
}

fn runtime_label() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "wasm-browser"
    } else {
        "native-host"
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        initialize_browser_did, render_browser_did_init_report,
        render_browser_did_init_report_json, BrowserDidInitRequest, DidMethod,
    };

    fn base_request(method: DidMethod) -> BrowserDidInitRequest {
        BrowserDidInitRequest {
            method,
            network: "tau-devnet".to_string(),
            subject: "agent".to_string(),
            entropy: "seed".to_string(),
        }
    }

    #[test]
    fn unit_initialize_browser_did_uses_kamn_profile() {
        let report = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "agent".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("initialize did");

        assert_eq!(report.schema_version, 1);
        assert_eq!(report.interoperability_profile, "kamn_browser_v1");
    }

    #[test]
    fn functional_render_browser_did_init_report_includes_identity_fields() {
        let report = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Web,
            network: "edge.tau".to_string(),
            subject: "operator".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("initialize did");
        let rendered = render_browser_did_init_report(&report);

        assert!(rendered.contains("browser did init"));
        assert!(rendered.contains("did:web"));
        assert!(rendered.contains("fingerprint="));
    }

    #[test]
    fn integration_render_browser_did_init_report_json_roundtrips() {
        let report = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "agent".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("initialize did");
        let json = render_browser_did_init_report_json(&report).expect("render json");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(
            parsed
                .get("identity")
                .and_then(|value| value.get("did"))
                .and_then(serde_json::Value::as_str)
                .map(|value| value.starts_with("did:key:")),
            Some(true)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn integration_write_browser_did_init_report_persists_json() {
        let temp = tempdir().expect("tempdir");
        let output = temp.path().join("browser-did.json");
        let report = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "agent".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("initialize did");

        super::write_browser_did_init_report(&output, &report).expect("write report");
        let raw = std::fs::read_to_string(&output).expect("read output");
        assert!(raw.contains("\"interoperability_profile\": \"kamn_browser_v1\""));
    }

    #[test]
    fn regression_initialize_browser_did_rejects_empty_subject() {
        let error = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: " ".to_string(),
            entropy: "seed".to_string(),
        })
        .expect_err("empty subject should fail");

        assert!(error.to_string().contains("subject cannot be empty"));
    }

    #[test]
    fn spec_c01_initialize_browser_did_propagates_malformed_input_with_sdk_context() {
        let mut request = base_request(DidMethod::Web);
        request.network = "edge..tau".to_string();

        let error =
            initialize_browser_did(&request).expect_err("malformed dotted segment must fail");
        let message = error.to_string();
        assert!(message.contains("failed to build browser DID identity"));
        assert!(message.contains("network contains empty label segments"));
    }

    #[test]
    fn spec_c02_initialize_browser_did_is_deterministic_for_normalized_equivalent_inputs() {
        let canonical = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Key,
            network: "tau-devnet".to_string(),
            subject: "browser-agent".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("canonical request");
        let equivalent = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Key,
            network: "  TAU-DEVNET ".to_string(),
            subject: " BROWSER-AGENT ".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("normalized equivalent request");

        assert_eq!(canonical.identity.did, equivalent.identity.did);
        assert_eq!(
            canonical.identity.fingerprint,
            equivalent.identity.fingerprint
        );
        assert_eq!(
            canonical.identity.proof_material,
            equivalent.identity.proof_material
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn spec_c03_write_browser_did_init_report_persists_nested_json_with_trailing_newline() {
        let temp = tempdir().expect("tempdir");
        let output = temp
            .path()
            .join("nested")
            .join("reports")
            .join("browser-did.json");
        let report = initialize_browser_did(&BrowserDidInitRequest {
            method: DidMethod::Web,
            network: "edge.tau".to_string(),
            subject: "operator".to_string(),
            entropy: "seed".to_string(),
        })
        .expect("initialize report");

        super::write_browser_did_init_report(&output, &report).expect("write nested report");
        let raw = std::fs::read_to_string(&output).expect("read nested output");
        assert!(raw.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(raw.trim()).expect("parse output");
        assert_eq!(
            parsed
                .get("identity")
                .and_then(|value| value.get("did"))
                .and_then(serde_json::Value::as_str)
                .map(|did| did.starts_with("did:web:edge:tau:operator")),
            Some(true)
        );
    }
}
