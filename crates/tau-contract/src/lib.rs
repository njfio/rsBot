//! Shared contract fixture helpers used by Tau contract/runtime crates.
//!
//! This crate centralizes common parsing/loading and baseline fixture
//! validation so contract crates keep behavior consistent.
//!
//! Architecture reference:
//! - [`docs/guides/contract-pattern-lifecycle.md`](../../../docs/guides/contract-pattern-lifecycle.md)
//!
//! ```rust
//! use tau_contract::{parse_fixture_with_validation, validate_fixture_header};
//!
//! #[derive(serde::Deserialize)]
//! struct ExampleFixture {
//!     schema_version: u32,
//!     name: String,
//!     cases: Vec<String>,
//! }
//!
//! let fixture = parse_fixture_with_validation::<ExampleFixture>(
//!     r#"{"schema_version":1,"name":"demo","cases":["case-a"]}"#,
//!     "failed to parse demo fixture",
//!     |parsed| {
//!         validate_fixture_header(
//!             "demo",
//!             parsed.schema_version,
//!             1,
//!             &parsed.name,
//!             parsed.cases.len(),
//!         )
//!     },
//! )?;
//!
//! assert_eq!(fixture.cases.len(), 1);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

/// Parses a contract fixture payload and runs caller-provided validation.
pub fn parse_fixture_with_validation<F>(
    raw: &str,
    parse_error_context: &str,
    validate: impl FnOnce(&F) -> Result<()>,
) -> Result<F>
where
    F: DeserializeOwned,
{
    let fixture =
        serde_json::from_str::<F>(raw).with_context(|| parse_error_context.to_string())?;
    validate(&fixture)?;
    Ok(fixture)
}

/// Loads a fixture file from disk and delegates parsing to the caller.
pub fn load_fixture_from_path<F>(path: &Path, parse: impl FnOnce(&str) -> Result<F>) -> Result<F> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse(&raw).with_context(|| format!("invalid fixture {}", path.display()))
}

/// Validates common contract fixture header fields.
pub fn validate_fixture_header(
    contract_label: &str,
    schema_version: u32,
    expected_schema_version: u32,
    fixture_name: &str,
    case_count: usize,
) -> Result<()> {
    validate_fixture_header_with_empty_message(
        contract_label,
        schema_version,
        expected_schema_version,
        fixture_name,
        case_count,
        "fixture must include at least one case",
    )
}

/// Validates common contract fixture header fields with a custom empty-items message.
pub fn validate_fixture_header_with_empty_message(
    contract_label: &str,
    schema_version: u32,
    expected_schema_version: u32,
    fixture_name: &str,
    item_count: usize,
    empty_items_message: &str,
) -> Result<()> {
    if schema_version != expected_schema_version {
        bail!(
            "unsupported {} contract schema version {} (expected {})",
            contract_label,
            schema_version,
            expected_schema_version
        );
    }
    if fixture_name.trim().is_empty() {
        bail!("fixture name cannot be empty");
    }
    if item_count == 0 {
        bail!("{empty_items_message}");
    }
    Ok(())
}

/// Validates that case identifiers are unique after trimming whitespace.
pub fn ensure_unique_case_ids<'a>(case_ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let mut ids = HashSet::new();
    for raw_case_id in case_ids {
        let case_id = raw_case_id.trim().to_string();
        if !ids.insert(case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", case_id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use serde::{Deserialize, Serialize};

    use super::{ensure_unique_case_ids, parse_fixture_with_validation, validate_fixture_header};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct Fixture {
        schema_version: u32,
        name: String,
        cases: Vec<String>,
    }

    #[test]
    fn unit_parse_fixture_with_validation_parses_and_validates() {
        let fixture = parse_fixture_with_validation::<Fixture>(
            r#"{"schema_version":1,"name":"fixture","cases":["a"]}"#,
            "failed to parse test fixture",
            |fixture| {
                if fixture.cases.is_empty() {
                    return Err(anyhow!("fixture missing cases"));
                }
                Ok(())
            },
        )
        .expect("fixture should parse");
        assert_eq!(fixture.name, "fixture");
    }

    #[test]
    fn functional_validate_fixture_header_accepts_valid_values() {
        validate_fixture_header("custom-command", 1, 1, "fixture", 1)
            .expect("header should validate");
    }

    #[test]
    fn regression_validate_fixture_header_rejects_invalid_inputs() {
        let schema_error = validate_fixture_header("memory", 2, 1, "fixture", 1)
            .expect_err("schema mismatch should fail");
        assert!(schema_error
            .to_string()
            .contains("unsupported memory contract schema"));

        let name_error =
            validate_fixture_header("memory", 1, 1, "   ", 1).expect_err("blank name should fail");
        assert!(name_error
            .to_string()
            .contains("fixture name cannot be empty"));

        let cases_error = validate_fixture_header("memory", 1, 1, "fixture", 0)
            .expect_err("missing cases should fail");
        assert!(cases_error
            .to_string()
            .contains("fixture must include at least one case"));
    }

    #[test]
    fn integration_ensure_unique_case_ids_accepts_and_rejects_expected_inputs() {
        ensure_unique_case_ids(["case-a", "case-b"]).expect("unique ids should pass");
        let error =
            ensure_unique_case_ids(["case-a", " case-a "]).expect_err("duplicate ids should fail");
        assert!(error.to_string().contains("duplicate case_id"));
    }

    #[test]
    fn regression_parse_fixture_with_validation_preserves_parse_context() {
        let error = parse_fixture_with_validation::<Fixture>(
            "not-json",
            "parse failure",
            |_fixture| Ok(()),
        )
        .expect_err("invalid json should fail");
        assert!(error.to_string().contains("parse failure"));
    }
}
