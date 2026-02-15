//! Credential snapshot and secret-resolution primitives for multi-channel runtimes.
//!
//! Credential records are resolved per channel with explicit revoked-state
//! handling. Callers can distinguish missing versus revoked secrets so runtime
//! diagnostics and policy gates fail closed with actionable reasons.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
/// Public struct `MultiChannelCredentialRecord` used across Tau components.
pub struct MultiChannelCredentialRecord {
    pub secret: Option<String>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Default)]
/// Public struct `MultiChannelCredentialStoreSnapshot` used across Tau components.
pub struct MultiChannelCredentialStoreSnapshot {
    pub integrations: BTreeMap<String, MultiChannelCredentialRecord>,
}

#[derive(Debug, Clone)]
/// Public struct `ResolvedSecret` used across Tau components.
pub struct ResolvedSecret {
    pub value: Option<String>,
    pub source: String,
    pub credential_store_unreadable: bool,
}

fn resolve_non_empty_value(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn resolve_secret(
    direct_secret: Option<&str>,
    integration_id: &str,
    credential_store: Option<&MultiChannelCredentialStoreSnapshot>,
    credential_store_unreadable: bool,
) -> ResolvedSecret {
    if let Some(secret) = resolve_non_empty_value(direct_secret) {
        return ResolvedSecret {
            value: Some(secret),
            source: "cli_or_env".to_string(),
            credential_store_unreadable: false,
        };
    }
    if credential_store_unreadable {
        return ResolvedSecret {
            value: None,
            source: "credential_store_error".to_string(),
            credential_store_unreadable: true,
        };
    }
    let Some(store) = credential_store else {
        return ResolvedSecret {
            value: None,
            source: "missing".to_string(),
            credential_store_unreadable: false,
        };
    };
    let Some(record) = store.integrations.get(integration_id) else {
        return ResolvedSecret {
            value: None,
            source: "missing".to_string(),
            credential_store_unreadable: false,
        };
    };
    if record.revoked {
        return ResolvedSecret {
            value: None,
            source: "credential_store_revoked".to_string(),
            credential_store_unreadable: false,
        };
    }
    let value = resolve_non_empty_value(record.secret.as_deref());
    let source = if value.is_some() {
        "credential_store".to_string()
    } else {
        "missing".to_string()
    };
    ResolvedSecret {
        value,
        source,
        credential_store_unreadable: false,
    }
}
