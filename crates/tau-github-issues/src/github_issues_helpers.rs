use std::collections::HashSet;

const GITHUB_ATTACHMENT_SUPPORTED_EXTENSIONS: &[&str] = &[
    "txt", "md", "json", "yaml", "yml", "toml", "log", "csv", "tsv", "rs", "py", "js", "ts", "tsx",
    "jsx", "go", "java", "c", "cpp", "h", "hpp", "sh", "zsh", "bash", "sql", "xml", "html", "css",
    "scss", "diff", "patch", "png", "jpg", "jpeg", "gif", "bmp", "webp", "pdf", "zip", "gz", "tar",
    "tgz",
];
const GITHUB_ATTACHMENT_DENIED_EXTENSIONS: &[&str] = &[
    "exe", "dll", "dylib", "so", "bat", "cmd", "com", "msi", "apk", "ipa", "ps1", "jar", "scr",
    "vb", "vbs",
];
const GITHUB_ATTACHMENT_DENIED_CONTENT_TYPES: &[&str] = &[
    "application/x-msdownload",
    "application/x-dosexec",
    "application/vnd.microsoft.portable-executable",
    "application/x-executable",
    "application/x-bat",
    "application/x-msdos-program",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `AttachmentPolicyDecision` used across Tau components.
pub struct AttachmentPolicyDecision {
    pub accepted: bool,
    pub reason_code: &'static str,
}

pub fn extract_attachment_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    for token in text.split_whitespace() {
        if let Some(markdown_url) = extract_markdown_link_url(token) {
            push_attachment_url(markdown_url, &mut urls, &mut seen);
        }
        push_attachment_url(token, &mut urls, &mut seen);
    }
    urls
}

pub fn evaluate_attachment_url_policy(url: &str) -> AttachmentPolicyDecision {
    let parsed = match reqwest::Url::parse(url) {
        Ok(parsed) => parsed,
        Err(_) => {
            return AttachmentPolicyDecision {
                accepted: false,
                reason_code: "deny_invalid_url",
            };
        }
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return AttachmentPolicyDecision {
            accepted: false,
            reason_code: "deny_non_http_scheme",
        };
    }
    let extension = attachment_extension_from_parsed_url(&parsed);
    let Some(extension) = extension else {
        return AttachmentPolicyDecision {
            accepted: false,
            reason_code: "deny_missing_extension",
        };
    };
    if GITHUB_ATTACHMENT_DENIED_EXTENSIONS.contains(&extension.as_str()) {
        return AttachmentPolicyDecision {
            accepted: false,
            reason_code: "deny_extension_denylist",
        };
    }
    if !GITHUB_ATTACHMENT_SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        return AttachmentPolicyDecision {
            accepted: false,
            reason_code: "deny_extension_not_allowlisted",
        };
    }
    AttachmentPolicyDecision {
        accepted: true,
        reason_code: "allow_extension_allowlist",
    }
}

pub fn evaluate_attachment_content_type_policy(
    content_type: Option<&str>,
) -> AttachmentPolicyDecision {
    let Some(content_type) = content_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return AttachmentPolicyDecision {
            accepted: true,
            reason_code: "allow_content_type_default",
        };
    };
    let normalized = content_type.to_ascii_lowercase();
    let normalized_base = normalized
        .split(';')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(normalized.as_str());
    if GITHUB_ATTACHMENT_DENIED_CONTENT_TYPES.contains(&normalized_base) {
        return AttachmentPolicyDecision {
            accepted: false,
            reason_code: "deny_content_type_dangerous",
        };
    }
    AttachmentPolicyDecision {
        accepted: true,
        reason_code: "allow_content_type_default",
    }
}

pub fn is_supported_attachment_url(url: &str) -> bool {
    evaluate_attachment_url_policy(url).accepted
}

pub fn attachment_filename_from_url(url: &str, index: usize) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(name) = parsed
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .filter(|name| !name.trim().is_empty())
        {
            return name.to_string();
        }
    }
    format!("attachment-{}.bin", index)
}

pub fn split_at_char_index(text: &str, index: usize) -> (String, String) {
    let mut iter = text.chars();
    let mut left = String::new();
    for _ in 0..index {
        if let Some(ch) = iter.next() {
            left.push(ch);
        } else {
            break;
        }
    }
    let right: String = iter.collect();
    (left, right)
}

pub fn chunk_text_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_chars {
            chunks.push(current);
            current = String::new();
            count = 0;
        }
        current.push(ch);
        count = count.saturating_add(1);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn extract_markdown_link_url(token: &str) -> Option<&str> {
    let start = token.find("](")?;
    let remainder = &token[start + 2..];
    let end = remainder.find(')')?;
    Some(&remainder[..end])
}

fn push_attachment_url(raw: &str, urls: &mut Vec<String>, seen: &mut HashSet<String>) {
    let candidate = raw.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
        )
    });
    if !candidate.starts_with("http://") && !candidate.starts_with("https://") {
        return;
    }
    let candidate = candidate.trim_end_matches(['.', ',', ';', ':']);
    if !is_supported_attachment_url(candidate) {
        return;
    }
    if seen.insert(candidate.to_string()) {
        urls.push(candidate.to_string());
    }
}

fn attachment_extension_from_parsed_url(parsed: &reqwest::Url) -> Option<String> {
    parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|segment| segment.rsplit('.').next())
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|extension| extension.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{
        chunk_text_by_chars, evaluate_attachment_content_type_policy,
        evaluate_attachment_url_policy, extract_attachment_urls, split_at_char_index,
    };

    #[test]
    fn unit_extract_attachment_urls_deduplicates_and_parses_markdown_links() {
        let text = "See [trace](https://example.com/files/trace.log) and https://example.com/files/trace.log";
        let urls = extract_attachment_urls(text);
        assert_eq!(
            urls,
            vec!["https://example.com/files/trace.log".to_string()]
        );
    }

    #[test]
    fn unit_attachment_policies_enforce_extension_and_content_type_rules() {
        assert!(!evaluate_attachment_url_policy("https://example.com/file.exe").accepted);
        assert!(evaluate_attachment_url_policy("https://example.com/file.log").accepted);
        assert!(
            !evaluate_attachment_content_type_policy(Some("application/x-msdownload")).accepted
        );
        assert!(evaluate_attachment_content_type_policy(Some("text/plain")).accepted);
    }

    #[test]
    fn unit_split_at_char_index_handles_unicode_boundaries() {
        let text = "ta🌊u";
        let (left, right) = split_at_char_index(text, 3);
        assert_eq!(left, "ta🌊");
        assert_eq!(right, "u");
    }

    #[test]
    fn unit_chunk_text_by_chars_splits_into_bounded_segments() {
        let chunks = chunk_text_by_chars("abcdef", 2);
        assert_eq!(chunks, vec!["ab", "cd", "ef"]);
    }
}
