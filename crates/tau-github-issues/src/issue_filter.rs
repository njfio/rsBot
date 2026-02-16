use std::collections::HashSet;

/// Normalize issue label filters for case-insensitive matching.
pub fn normalize_issue_label(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Build the normalized set of required labels from CLI or configuration values.
pub fn build_required_issue_labels<'a>(
    labels: impl IntoIterator<Item = &'a str>,
) -> HashSet<String> {
    labels
        .into_iter()
        .map(normalize_issue_label)
        .filter(|label| !label.is_empty())
        .collect::<HashSet<_>>()
}

/// Return true when required issue-number filters are empty or contain the issue.
pub fn issue_matches_required_numbers(issue_number: u64, required: &HashSet<u64>) -> bool {
    required.is_empty() || required.contains(&issue_number)
}

/// Return true when issue labels satisfy required label filters.
pub fn issue_matches_required_labels<'a>(
    labels: impl IntoIterator<Item = &'a str>,
    required: &HashSet<String>,
) -> bool {
    if required.is_empty() {
        return true;
    }
    labels
        .into_iter()
        .map(normalize_issue_label)
        .any(|label| required.contains(&label))
}

#[cfg(test)]
mod tests {
    use super::{
        build_required_issue_labels, issue_matches_required_labels, issue_matches_required_numbers,
        normalize_issue_label,
    };
    use std::collections::HashSet;

    #[test]
    fn unit_normalize_issue_label_trims_and_lowercases() {
        assert_eq!(
            normalize_issue_label("  Needs-Attention  "),
            "needs-attention"
        );
    }

    #[test]
    fn functional_build_required_issue_labels_deduplicates_and_ignores_blank_values() {
        let labels = vec!["  Bug  ", "bug", "", "  "];
        let normalized = build_required_issue_labels(labels);
        assert_eq!(normalized.len(), 1);
        assert!(normalized.contains("bug"));
    }

    #[test]
    fn integration_issue_matches_required_labels_is_case_insensitive() {
        let required = HashSet::from([String::from("priority:high")]);
        let labels = ["Priority:High", "enhancement"];
        assert!(issue_matches_required_labels(labels, &required));
    }

    #[test]
    fn regression_issue_matches_required_numbers_respects_empty_and_filtered_sets() {
        let required = HashSet::from([7_u64, 11_u64]);
        assert!(issue_matches_required_numbers(7, &required));
        assert!(!issue_matches_required_numbers(5, &required));

        let no_filter = HashSet::new();
        assert!(issue_matches_required_numbers(42, &no_filter));
    }
}
