//! Media attachment policy and enrichment helpers for multi-channel ingress.
//!
//! Media handling enforces attachment limits, dedupes repeated assets, and
//! records machine-readable reason codes for accept/reject/enrichment outcomes.
//! Runtime and telemetry layers consume these reason codes for diagnostics.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::multi_channel_contract::{MultiChannelAttachment, MultiChannelInboundEvent};

const MEDIA_REASON_UNDERSTANDING_DISABLED: &str = "media_understanding_disabled";
const MEDIA_REASON_DUPLICATE_ATTACHMENT: &str = "media_duplicate_attachment";
const MEDIA_REASON_ATTACHMENT_LIMIT_EXCEEDED: &str = "media_attachment_limit_exceeded";
const MEDIA_REASON_UNSUPPORTED_ATTACHMENT_TYPE: &str = "media_unsupported_attachment_type";
const MEDIA_REASON_PROVIDER_ERROR: &str = "media_provider_error";
const MEDIA_REASON_IMAGE_DESCRIBED: &str = "media_image_described";
const MEDIA_REASON_AUDIO_TRANSCRIBED: &str = "media_audio_transcribed";
const MEDIA_REASON_VIDEO_SUMMARIZED: &str = "media_video_summarized";

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "m4a", "ogg", "flac", "aac", "opus"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "avi", "mkv", "webm"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelMediaUnderstandingConfig` used across Tau components.
pub struct MultiChannelMediaUnderstandingConfig {
    pub enabled: bool,
    pub max_attachments_per_event: usize,
    pub max_summary_chars: usize,
}

impl Default for MultiChannelMediaUnderstandingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attachments_per_event: 4,
            max_summary_chars: 280,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MediaUnderstandingDecision` values.
pub enum MediaUnderstandingDecision {
    Processed,
    Skipped,
    Failed,
}

impl MediaUnderstandingDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Processed => "processed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MediaUnderstandingOutcome` used across Tau components.
pub struct MediaUnderstandingOutcome {
    pub attachment_id: String,
    pub decision: MediaUnderstandingDecision,
    pub reason_code: String,
    pub media_kind: String,
    pub operation: String,
    pub content_type: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub url: String,
    pub summary: Option<String>,
    pub summary_chars: usize,
    pub truncated: bool,
    pub retryable: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `MediaUnderstandingReport` used across Tau components.
pub struct MediaUnderstandingReport {
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub truncated_summaries: usize,
    pub outcomes: Vec<MediaUnderstandingOutcome>,
    pub reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Audio,
    Video,
    Unsupported,
}

impl MediaKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Unsupported => "unsupported",
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::Image => "describe",
            Self::Audio => "transcribe",
            Self::Video => "summarize",
            Self::Unsupported => "none",
        }
    }

    fn success_reason_code(self) -> &'static str {
        match self {
            Self::Image => MEDIA_REASON_IMAGE_DESCRIBED,
            Self::Audio => MEDIA_REASON_AUDIO_TRANSCRIBED,
            Self::Video => MEDIA_REASON_VIDEO_SUMMARIZED,
            Self::Unsupported => MEDIA_REASON_UNSUPPORTED_ATTACHMENT_TYPE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `MediaUnderstandingProviderError` used across Tau components.
pub struct MediaUnderstandingProviderError {
    pub reason_code: String,
    pub detail: String,
    pub retryable: bool,
}

impl MediaUnderstandingProviderError {
    fn non_retryable(reason_code: &str, detail: &str) -> Self {
        Self {
            reason_code: reason_code.trim().to_string(),
            detail: detail.trim().to_string(),
            retryable: false,
        }
    }
}

/// Trait contract for `MediaUnderstandingProvider` behavior.
pub trait MediaUnderstandingProvider {
    fn describe_image(
        &self,
        attachment: &MultiChannelAttachment,
    ) -> Result<String, MediaUnderstandingProviderError>;

    fn transcribe_audio(
        &self,
        attachment: &MultiChannelAttachment,
    ) -> Result<String, MediaUnderstandingProviderError>;

    fn summarize_video(
        &self,
        attachment: &MultiChannelAttachment,
    ) -> Result<String, MediaUnderstandingProviderError>;
}

#[derive(Debug, Clone, Copy, Default)]
/// Public struct `DeterministicMediaUnderstandingProvider` used across Tau components.
pub struct DeterministicMediaUnderstandingProvider;

impl MediaUnderstandingProvider for DeterministicMediaUnderstandingProvider {
    fn describe_image(
        &self,
        attachment: &MultiChannelAttachment,
    ) -> Result<String, MediaUnderstandingProviderError> {
        Ok(format!(
            "Image '{}' described from metadata (content_type='{}', size_bytes={}, source='{}').",
            attachment_display_name(attachment),
            normalized_content_type(attachment),
            attachment.size_bytes,
            attachment.url.trim()
        ))
    }

    fn transcribe_audio(
        &self,
        attachment: &MultiChannelAttachment,
    ) -> Result<String, MediaUnderstandingProviderError> {
        Ok(format!(
            "Audio '{}' transcribed from metadata envelope (content_type='{}', size_bytes={}, source='{}').",
            attachment_display_name(attachment),
            normalized_content_type(attachment),
            attachment.size_bytes,
            attachment.url.trim()
        ))
    }

    fn summarize_video(
        &self,
        attachment: &MultiChannelAttachment,
    ) -> Result<String, MediaUnderstandingProviderError> {
        Ok(format!(
            "Video '{}' summarized from metadata envelope (content_type='{}', size_bytes={}, source='{}').",
            attachment_display_name(attachment),
            normalized_content_type(attachment),
            attachment.size_bytes,
            attachment.url.trim()
        ))
    }
}

pub fn process_media_attachments(
    event: &MultiChannelInboundEvent,
    config: &MultiChannelMediaUnderstandingConfig,
) -> MediaUnderstandingReport {
    process_media_attachments_with_provider(event, config, &DeterministicMediaUnderstandingProvider)
}

pub fn process_media_attachments_with_provider<P: MediaUnderstandingProvider>(
    event: &MultiChannelInboundEvent,
    config: &MultiChannelMediaUnderstandingConfig,
    provider: &P,
) -> MediaUnderstandingReport {
    let mut report = MediaUnderstandingReport::default();
    if event.attachments.is_empty() {
        return report;
    }

    let max_attachments = config.max_attachments_per_event.max(1);
    let max_summary_chars = config.max_summary_chars.max(16);
    let mut seen_attachments = HashSet::new();
    let mut processed_candidates = 0_usize;

    for attachment in &event.attachments {
        let identity = format!(
            "{}:{}",
            attachment.attachment_id.trim(),
            attachment.url.trim().to_ascii_lowercase()
        );
        let media_kind = classify_media_kind(attachment);
        let mut outcome = base_outcome(attachment, media_kind);

        if !config.enabled {
            outcome.decision = MediaUnderstandingDecision::Skipped;
            outcome.reason_code = MEDIA_REASON_UNDERSTANDING_DISABLED.to_string();
            push_outcome(&mut report, outcome);
            continue;
        }

        if !seen_attachments.insert(identity) {
            outcome.decision = MediaUnderstandingDecision::Skipped;
            outcome.reason_code = MEDIA_REASON_DUPLICATE_ATTACHMENT.to_string();
            push_outcome(&mut report, outcome);
            continue;
        }

        if processed_candidates >= max_attachments {
            outcome.decision = MediaUnderstandingDecision::Skipped;
            outcome.reason_code = MEDIA_REASON_ATTACHMENT_LIMIT_EXCEEDED.to_string();
            push_outcome(&mut report, outcome);
            continue;
        }
        processed_candidates = processed_candidates.saturating_add(1);

        if media_kind == MediaKind::Unsupported {
            outcome.decision = MediaUnderstandingDecision::Skipped;
            outcome.reason_code = MEDIA_REASON_UNSUPPORTED_ATTACHMENT_TYPE.to_string();
            push_outcome(&mut report, outcome);
            continue;
        }

        match process_supported_attachment(provider, attachment, media_kind) {
            Ok(summary) => {
                let (bounded_summary, truncated) = truncate_summary(&summary, max_summary_chars);
                outcome.decision = MediaUnderstandingDecision::Processed;
                outcome.reason_code = media_kind.success_reason_code().to_string();
                outcome.summary_chars = bounded_summary.chars().count();
                outcome.summary = Some(bounded_summary);
                outcome.truncated = truncated;
                if truncated {
                    report.truncated_summaries = report.truncated_summaries.saturating_add(1);
                }
            }
            Err(error) => {
                outcome.decision = MediaUnderstandingDecision::Failed;
                outcome.reason_code = if error.reason_code.trim().is_empty() {
                    MEDIA_REASON_PROVIDER_ERROR.to_string()
                } else {
                    error.reason_code.trim().to_string()
                };
                outcome.retryable = error.retryable;
                if !error.detail.trim().is_empty() {
                    outcome.detail = Some(error.detail.trim().to_string());
                }
            }
        }

        push_outcome(&mut report, outcome);
    }

    report
}

pub fn render_media_prompt_context(report: &MediaUnderstandingReport) -> Option<String> {
    if report.outcomes.is_empty() {
        return None;
    }

    let mut lines = Vec::with_capacity(report.outcomes.len().saturating_add(2));
    lines.push("Media understanding outcomes:".to_string());
    for outcome in &report.outcomes {
        let mut line = format!(
            "- attachment_id={} decision={} media_kind={} operation={} reason_code={}",
            outcome.attachment_id,
            outcome.decision.as_str(),
            outcome.media_kind,
            outcome.operation,
            outcome.reason_code
        );
        if let Some(summary) = &outcome.summary {
            line.push_str(&format!(" summary={}", normalize_inline_summary(summary)));
        }
        if outcome.retryable {
            line.push_str(" retryable=true");
        }
        if let Some(detail) = &outcome.detail {
            line.push_str(&format!(" detail={}", normalize_inline_summary(detail)));
        }
        lines.push(line);
    }
    Some(lines.join("\n"))
}

fn push_outcome(report: &mut MediaUnderstandingReport, outcome: MediaUnderstandingOutcome) {
    match outcome.decision {
        MediaUnderstandingDecision::Processed => {
            report.processed = report.processed.saturating_add(1);
        }
        MediaUnderstandingDecision::Skipped => {
            report.skipped = report.skipped.saturating_add(1);
        }
        MediaUnderstandingDecision::Failed => {
            report.failed = report.failed.saturating_add(1);
        }
    }
    let reason_entry = report
        .reason_code_counts
        .entry(outcome.reason_code.clone())
        .or_insert(0);
    *reason_entry = reason_entry.saturating_add(1);
    report.outcomes.push(outcome);
}

fn base_outcome(
    attachment: &MultiChannelAttachment,
    media_kind: MediaKind,
) -> MediaUnderstandingOutcome {
    MediaUnderstandingOutcome {
        attachment_id: attachment.attachment_id.trim().to_string(),
        decision: MediaUnderstandingDecision::Skipped,
        reason_code: String::new(),
        media_kind: media_kind.as_str().to_string(),
        operation: media_kind.operation().to_string(),
        content_type: attachment.content_type.trim().to_string(),
        file_name: attachment.file_name.trim().to_string(),
        size_bytes: attachment.size_bytes,
        url: attachment.url.trim().to_string(),
        summary: None,
        summary_chars: 0,
        truncated: false,
        retryable: false,
        detail: None,
    }
}

fn process_supported_attachment<P: MediaUnderstandingProvider>(
    provider: &P,
    attachment: &MultiChannelAttachment,
    media_kind: MediaKind,
) -> Result<String, MediaUnderstandingProviderError> {
    match media_kind {
        MediaKind::Image => provider.describe_image(attachment),
        MediaKind::Audio => provider.transcribe_audio(attachment),
        MediaKind::Video => provider.summarize_video(attachment),
        MediaKind::Unsupported => Err(MediaUnderstandingProviderError::non_retryable(
            MEDIA_REASON_UNSUPPORTED_ATTACHMENT_TYPE,
            "unsupported attachment type",
        )),
    }
}

fn classify_media_kind(attachment: &MultiChannelAttachment) -> MediaKind {
    let content_type = attachment.content_type.trim().to_ascii_lowercase();
    if content_type.starts_with("image/") {
        return MediaKind::Image;
    }
    if content_type.starts_with("audio/") {
        return MediaKind::Audio;
    }
    if content_type.starts_with("video/") {
        return MediaKind::Video;
    }

    let extension = attachment_extension(attachment);
    let Some(extension) = extension else {
        return MediaKind::Unsupported;
    };
    if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return MediaKind::Image;
    }
    if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return MediaKind::Audio;
    }
    if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        return MediaKind::Video;
    }
    MediaKind::Unsupported
}

fn attachment_extension(attachment: &MultiChannelAttachment) -> Option<String> {
    extract_extension(&attachment.file_name).or_else(|| extract_extension(&attachment.url))
}

fn extract_extension(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let path_tail = without_query.rsplit('/').next().unwrap_or(without_query);
    let (_, ext) = path_tail.rsplit_once('.')?;
    if ext.trim().is_empty() {
        return None;
    }
    Some(ext.trim().to_ascii_lowercase())
}

fn attachment_display_name(attachment: &MultiChannelAttachment) -> String {
    let file_name = attachment.file_name.trim();
    if !file_name.is_empty() {
        return file_name.to_string();
    }
    let url_tail = attachment
        .url
        .trim()
        .split('#')
        .next()
        .unwrap_or(attachment.url.trim())
        .split('?')
        .next()
        .unwrap_or(attachment.url.trim())
        .rsplit('/')
        .next()
        .unwrap_or(attachment.url.trim())
        .trim();
    if !url_tail.is_empty() {
        return url_tail.to_string();
    }
    attachment.attachment_id.trim().to_string()
}

fn normalized_content_type(attachment: &MultiChannelAttachment) -> String {
    let content_type = attachment.content_type.trim();
    if content_type.is_empty() {
        "unknown".to_string()
    } else {
        content_type.to_string()
    }
}

fn truncate_summary(summary: &str, max_chars: usize) -> (String, bool) {
    let normalized = summary.trim().replace('\n', " ");
    let total_chars = normalized.chars().count();
    if total_chars <= max_chars {
        return (normalized, false);
    }
    let truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    (format!("{truncated}..."), true)
}

fn normalize_inline_summary(raw: &str) -> String {
    raw.trim().replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        classify_media_kind, process_media_attachments_with_provider, render_media_prompt_context,
        MediaKind, MediaUnderstandingProvider, MediaUnderstandingProviderError,
        MultiChannelMediaUnderstandingConfig,
    };
    use crate::multi_channel_contract::{
        MultiChannelAttachment, MultiChannelEventKind, MultiChannelInboundEvent,
        MultiChannelTransport,
    };

    #[derive(Debug)]
    struct MockProvider;

    impl MediaUnderstandingProvider for MockProvider {
        fn describe_image(
            &self,
            attachment: &MultiChannelAttachment,
        ) -> Result<String, MediaUnderstandingProviderError> {
            Ok(format!("mock-image-summary:{}", attachment.attachment_id))
        }

        fn transcribe_audio(
            &self,
            attachment: &MultiChannelAttachment,
        ) -> Result<String, MediaUnderstandingProviderError> {
            Err(MediaUnderstandingProviderError {
                reason_code: "media_provider_timeout".to_string(),
                detail: format!("timed out while transcribing {}", attachment.attachment_id),
                retryable: true,
            })
        }

        fn summarize_video(
            &self,
            attachment: &MultiChannelAttachment,
        ) -> Result<String, MediaUnderstandingProviderError> {
            Ok(format!("mock-video-summary:{}", attachment.attachment_id))
        }
    }

    fn sample_event(attachments: Vec<MultiChannelAttachment>) -> MultiChannelInboundEvent {
        MultiChannelInboundEvent {
            schema_version: 1,
            transport: MultiChannelTransport::Discord,
            event_kind: MultiChannelEventKind::Message,
            event_id: "event-1".to_string(),
            conversation_id: "room-1".to_string(),
            thread_id: String::new(),
            actor_id: "user-1".to_string(),
            actor_display: "user".to_string(),
            timestamp_ms: 1_760_000_000_000,
            text: "inspect media".to_string(),
            attachments,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn unit_classify_media_kind_detects_content_type_and_extension() {
        let image = MultiChannelAttachment {
            attachment_id: "a1".to_string(),
            url: "https://example.com/file.bin".to_string(),
            content_type: "image/png".to_string(),
            file_name: "file.bin".to_string(),
            size_bytes: 1,
        };
        assert_eq!(classify_media_kind(&image), MediaKind::Image);

        let audio = MultiChannelAttachment {
            attachment_id: "a2".to_string(),
            url: "https://example.com/voice-note".to_string(),
            content_type: String::new(),
            file_name: "voice-note.wav".to_string(),
            size_bytes: 1,
        };
        assert_eq!(classify_media_kind(&audio), MediaKind::Audio);

        let video = MultiChannelAttachment {
            attachment_id: "a3".to_string(),
            url: "https://example.com/clip.mp4?token=abc".to_string(),
            content_type: String::new(),
            file_name: String::new(),
            size_bytes: 1,
        };
        assert_eq!(classify_media_kind(&video), MediaKind::Video);

        let unsupported = MultiChannelAttachment {
            attachment_id: "a4".to_string(),
            url: "https://example.com/document.txt".to_string(),
            content_type: "text/plain".to_string(),
            file_name: "document.txt".to_string(),
            size_bytes: 1,
        };
        assert_eq!(classify_media_kind(&unsupported), MediaKind::Unsupported);
    }

    #[test]
    fn functional_media_processing_contract_handles_success_unsupported_and_retryable_failure() {
        let event = sample_event(vec![
            MultiChannelAttachment {
                attachment_id: "image-1".to_string(),
                url: "https://example.com/image.png".to_string(),
                content_type: "image/png".to_string(),
                file_name: "image.png".to_string(),
                size_bytes: 128,
            },
            MultiChannelAttachment {
                attachment_id: "doc-1".to_string(),
                url: "https://example.com/doc.txt".to_string(),
                content_type: "text/plain".to_string(),
                file_name: "doc.txt".to_string(),
                size_bytes: 32,
            },
            MultiChannelAttachment {
                attachment_id: "audio-1".to_string(),
                url: "https://example.com/audio.wav".to_string(),
                content_type: "audio/wav".to_string(),
                file_name: "audio.wav".to_string(),
                size_bytes: 256,
            },
        ]);
        let report = process_media_attachments_with_provider(
            &event,
            &MultiChannelMediaUnderstandingConfig::default(),
            &MockProvider,
        );

        assert_eq!(report.processed, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(
            report.reason_code_counts.get("media_image_described"),
            Some(&1)
        );
        assert_eq!(
            report
                .reason_code_counts
                .get("media_unsupported_attachment_type"),
            Some(&1)
        );
        assert_eq!(
            report.reason_code_counts.get("media_provider_timeout"),
            Some(&1)
        );
        assert!(report.outcomes.iter().any(|outcome| outcome.retryable));
    }

    #[test]
    fn integration_media_prompt_context_renders_deterministic_lines() {
        let event = sample_event(vec![MultiChannelAttachment {
            attachment_id: "image-1".to_string(),
            url: "https://example.com/image.png".to_string(),
            content_type: "image/png".to_string(),
            file_name: "image.png".to_string(),
            size_bytes: 128,
        }]);
        let report = process_media_attachments_with_provider(
            &event,
            &MultiChannelMediaUnderstandingConfig::default(),
            &MockProvider,
        );
        let prompt_context =
            render_media_prompt_context(&report).expect("prompt context should be populated");
        assert!(prompt_context.contains("Media understanding outcomes:"));
        assert!(prompt_context.contains("attachment_id=image-1"));
        assert!(prompt_context.contains("decision=processed"));
        assert!(prompt_context.contains("reason_code=media_image_described"));
    }

    #[test]
    fn regression_media_processing_is_bounded_and_deduplicated_per_event() {
        let event = sample_event(vec![
            MultiChannelAttachment {
                attachment_id: "image-1".to_string(),
                url: "https://example.com/image.png".to_string(),
                content_type: "image/png".to_string(),
                file_name: "image.png".to_string(),
                size_bytes: 128,
            },
            MultiChannelAttachment {
                attachment_id: "image-1".to_string(),
                url: "https://example.com/image.png".to_string(),
                content_type: "image/png".to_string(),
                file_name: "image.png".to_string(),
                size_bytes: 128,
            },
            MultiChannelAttachment {
                attachment_id: "video-1".to_string(),
                url: "https://example.com/clip.mp4".to_string(),
                content_type: "video/mp4".to_string(),
                file_name: "clip.mp4".to_string(),
                size_bytes: 1024,
            },
        ]);
        let report = process_media_attachments_with_provider(
            &event,
            &MultiChannelMediaUnderstandingConfig {
                enabled: true,
                max_attachments_per_event: 1,
                max_summary_chars: 120,
            },
            &MockProvider,
        );

        assert_eq!(report.processed, 1);
        assert_eq!(report.skipped, 2);
        assert_eq!(report.failed, 0);
        assert_eq!(
            report.reason_code_counts.get("media_duplicate_attachment"),
            Some(&1)
        );
        assert_eq!(
            report
                .reason_code_counts
                .get("media_attachment_limit_exceeded"),
            Some(&1)
        );
    }
}
