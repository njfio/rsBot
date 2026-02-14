use anyhow::{bail, Result};

const DID_SCHEME: &str = "did";
const DID_METHOD_KAMN: &str = "kamn";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Canonical KAMN DID identity with trust-root and subject components.
pub struct KamnDid {
    canonical: String,
    canonical_base: String,
    trust_root_id: String,
    subject: String,
    service_fragment: Option<String>,
}

impl KamnDid {
    /// Parses a KAMN DID in the shape `did:kamn:<trust_root_id>:<subject>[#fragment]`.
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            bail!("KAMN DID cannot be empty");
        }

        let (base, fragment) = match raw.split_once('#') {
            Some((base, fragment)) => (base.trim(), Some(fragment.trim())),
            None => (raw, None),
        };
        if base.is_empty() {
            bail!("KAMN DID base identifier cannot be empty");
        }

        let parts = base.split(':').collect::<Vec<_>>();
        if parts.len() < 4 {
            bail!("invalid KAMN DID '{raw}': expected did:kamn:<trust_root_id>:<subject>");
        }
        if !parts[0].eq_ignore_ascii_case(DID_SCHEME) {
            bail!("invalid KAMN DID '{raw}': identifier must start with 'did:'");
        }
        if !parts[1].eq_ignore_ascii_case(DID_METHOD_KAMN) {
            bail!("invalid KAMN DID '{raw}': unsupported DID method");
        }

        let trust_root_id = parts[2].trim();
        let subject = parts[3..].join(":");
        if trust_root_id.is_empty() {
            bail!("invalid KAMN DID '{raw}': trust root id cannot be empty");
        }
        if subject.trim().is_empty() {
            bail!("invalid KAMN DID '{raw}': subject cannot be empty");
        }
        if !is_valid_component(trust_root_id, true) {
            bail!("invalid KAMN DID '{raw}': trust root id contains unsupported characters");
        }
        if !is_valid_component(subject.as_str(), false) {
            bail!("invalid KAMN DID '{raw}': subject contains unsupported characters");
        }
        if let Some(fragment) = fragment {
            if fragment.is_empty() {
                bail!("invalid KAMN DID '{raw}': fragment cannot be empty");
            }
            if !is_valid_component(fragment, false) {
                bail!("invalid KAMN DID '{raw}': fragment contains unsupported characters");
            }
        }

        let canonical_base = format!("did:kamn:{trust_root_id}:{subject}");
        let canonical = match fragment {
            Some(fragment) => format!("{canonical_base}#{fragment}"),
            None => canonical_base.clone(),
        };

        Ok(Self {
            canonical,
            canonical_base,
            trust_root_id: trust_root_id.to_string(),
            subject,
            service_fragment: fragment.map(str::to_string),
        })
    }

    pub fn as_str(&self) -> &str {
        self.canonical.as_str()
    }

    pub fn canonical_base(&self) -> &str {
        self.canonical_base.as_str()
    }

    pub fn trust_root_id(&self) -> &str {
        self.trust_root_id.as_str()
    }

    pub fn subject(&self) -> &str {
        self.subject.as_str()
    }

    pub fn service_fragment(&self) -> Option<&str> {
        self.service_fragment.as_deref()
    }

    /// Builds a deterministic DID service endpoint reference for a runtime channel.
    pub fn service_endpoint_for_channel(&self, channel: &str) -> String {
        let channel = channel.trim();
        let fragment = if channel.is_empty() {
            "default".to_string()
        } else {
            channel
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        };
        format!("{}#{fragment}", self.canonical_base())
    }
}

fn is_valid_component(value: &str, strict: bool) -> bool {
    value.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' || (!strict && ch == ':')
    })
}

#[cfg(test)]
mod tests {
    use super::KamnDid;

    #[test]
    fn unit_kamn_did_parse_extracts_trust_root_and_subject() {
        let did = KamnDid::parse("did:kamn:root-alpha:agent-1").expect("parse canonical KAMN DID");
        assert_eq!(did.trust_root_id(), "root-alpha");
        assert_eq!(did.subject(), "agent-1");
        assert_eq!(did.as_str(), "did:kamn:root-alpha:agent-1");
        assert!(did.service_fragment().is_none());
    }

    #[test]
    fn unit_kamn_did_parse_rejects_invalid_shapes() {
        let error = KamnDid::parse("did:kamn:root-alpha").expect_err("missing subject should fail");
        assert!(error.to_string().contains("expected did:kamn:"));

        let error =
            KamnDid::parse("did:kamn:root alpha:agent").expect_err("space should fail closed");
        assert!(error.to_string().contains("unsupported characters"));
    }

    #[test]
    fn functional_kamn_did_service_endpoint_maps_channel_slug() {
        let did = KamnDid::parse("did:kamn:root-alpha:agent-1").expect("parse DID");
        assert_eq!(
            did.service_endpoint_for_channel("github:njfio/Tau"),
            "did:kamn:root-alpha:agent-1#github-njfio-Tau"
        );
    }

    #[test]
    fn regression_kamn_did_parse_accepts_case_insensitive_prefix() {
        let did =
            KamnDid::parse("DID:KAMN:root-alpha:agent-1").expect("accept case-insensitive DID");
        assert_eq!(did.as_str(), "did:kamn:root-alpha:agent-1");
    }
}
