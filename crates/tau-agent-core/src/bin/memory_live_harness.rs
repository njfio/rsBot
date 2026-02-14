use std::{
    collections::VecDeque,
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use serde::Serialize;
use tau_agent_core::{Agent, AgentConfig};
use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, MessageRole, TauAiError};
use tokio::sync::Mutex as AsyncMutex;

const HARNESS_SCHEMA_VERSION: u32 = 1;
const MEMORY_RECALL_PREFIX: &str = "[Tau memory recall]";

#[derive(Debug, Clone)]
struct CliArgs {
    output_dir: PathBuf,
    state_dir: PathBuf,
    summary_json_out: PathBuf,
    quality_report_json_out: PathBuf,
    artifact_manifest_json_out: PathBuf,
    workspace_id: String,
}

impl CliArgs {
    fn parse() -> Result<Self, String> {
        let mut output_dir = PathBuf::from(".tau/demo-memory-live");
        let mut state_dir: Option<PathBuf> = None;
        let mut summary_json_out: Option<PathBuf> = None;
        let mut quality_report_json_out: Option<PathBuf> = None;
        let mut artifact_manifest_json_out: Option<PathBuf> = None;
        let mut workspace_id = "demo-workspace".to_string();

        let args = env::args().skip(1).collect::<Vec<_>>();
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--output-dir" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "missing value for --output-dir".to_string())?;
                    output_dir = PathBuf::from(value);
                    index += 2;
                }
                "--state-dir" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "missing value for --state-dir".to_string())?;
                    state_dir = Some(PathBuf::from(value));
                    index += 2;
                }
                "--summary-json-out" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "missing value for --summary-json-out".to_string())?;
                    summary_json_out = Some(PathBuf::from(value));
                    index += 2;
                }
                "--quality-report-json-out" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "missing value for --quality-report-json-out".to_string())?;
                    quality_report_json_out = Some(PathBuf::from(value));
                    index += 2;
                }
                "--artifact-manifest-json-out" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "missing value for --artifact-manifest-json-out".to_string()
                    })?;
                    artifact_manifest_json_out = Some(PathBuf::from(value));
                    index += 2;
                }
                "--workspace-id" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "missing value for --workspace-id".to_string())?;
                    workspace_id = value.to_string();
                    index += 2;
                }
                "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                unknown => {
                    return Err(format!("unknown argument: {unknown}"));
                }
            }
        }

        let state_dir = state_dir.unwrap_or_else(|| output_dir.join("state"));
        let summary_json_out =
            summary_json_out.unwrap_or_else(|| output_dir.join("memory-live-summary.json"));
        let quality_report_json_out = quality_report_json_out
            .unwrap_or_else(|| output_dir.join("memory-live-quality-report.json"));
        let artifact_manifest_json_out = artifact_manifest_json_out
            .unwrap_or_else(|| output_dir.join("memory-live-artifact-manifest.json"));

        Ok(Self {
            output_dir,
            state_dir,
            summary_json_out,
            quality_report_json_out,
            artifact_manifest_json_out,
            workspace_id,
        })
    }
}

fn print_usage() {
    println!(
        "Usage: memory_live_harness [--output-dir PATH] [--state-dir PATH] [--summary-json-out PATH] [--quality-report-json-out PATH] [--artifact-manifest-json-out PATH] [--workspace-id VALUE]"
    );
}

#[derive(Clone)]
struct CapturingMockClient {
    responses: Arc<AsyncMutex<VecDeque<ChatResponse>>>,
    requests: Arc<AsyncMutex<Vec<ChatRequest>>>,
}

impl CapturingMockClient {
    fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Arc::new(AsyncMutex::new(VecDeque::from(responses))),
            requests: Arc::new(AsyncMutex::new(Vec::new())),
        }
    }

    async fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().await.clone()
    }
}

#[async_trait]
impl LlmClient for CapturingMockClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        self.requests.lock().await.push(request);
        self.responses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| TauAiError::InvalidResponse("mock response queue exhausted".to_string()))
    }
}

#[derive(Debug, Clone)]
struct EvaluationCase {
    case_id: &'static str,
    query: &'static str,
    expected_keywords: &'static [&'static str],
}

#[derive(Debug, Serialize)]
struct RequestCaptureMessage {
    role: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct RequestCapture {
    case_id: String,
    query: String,
    request_messages: Vec<RequestCaptureMessage>,
}

#[derive(Debug, Serialize)]
struct EvaluationCaseResult {
    case_id: String,
    query: String,
    expected_keywords: Vec<String>,
    recall_count: usize,
    top1_score: Option<f32>,
    top1_text: Option<String>,
    top1_hit: bool,
    topk_hit: bool,
}

#[derive(Debug, Serialize)]
struct QualityThresholds {
    top1_relevance_min: f64,
    topk_relevance_min: f64,
}

#[derive(Debug, Serialize)]
struct QualityMetrics {
    total_cases: usize,
    top1_hits: usize,
    topk_hits: usize,
    top1_relevance_rate: f64,
    topk_relevance_rate: f64,
    quality_gate_passed: bool,
}

#[derive(Debug, Serialize)]
struct QualityReport {
    schema_version: u32,
    generated_unix_ms: u64,
    workspace_id: String,
    backend_state_file: String,
    thresholds: QualityThresholds,
    metrics: QualityMetrics,
    cases: Vec<EvaluationCaseResult>,
}

#[derive(Debug, Serialize)]
struct SummaryReport {
    schema_version: u32,
    generated_unix_ms: u64,
    workspace_id: String,
    total_cases: usize,
    persisted_entry_count: usize,
    top1_hits: usize,
    topk_hits: usize,
    top1_relevance_rate: f64,
    topk_relevance_rate: f64,
    quality_gate_passed: bool,
    request_captures_path: String,
}

#[derive(Debug, Serialize)]
struct ArtifactRecord {
    label: String,
    path: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct ArtifactManifest {
    schema_version: u32,
    generated_unix_ms: u64,
    artifacts: Vec<ArtifactRecord>,
    missing_artifacts: Vec<String>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("[memory-live-harness] {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args = CliArgs::parse()?;
    fs::create_dir_all(&args.output_dir).map_err(|error| {
        format!(
            "failed to create output directory '{}': {error}",
            args.output_dir.display()
        )
    })?;
    fs::create_dir_all(&args.state_dir).map_err(|error| {
        format!(
            "failed to create state directory '{}': {error}",
            args.state_dir.display()
        )
    })?;

    let normalized_workspace_id = normalize_workspace_id(&args.workspace_id);
    let request_captures_path = args.output_dir.join("memory-live-request-captures.json");
    let backend_state_file = args
        .state_dir
        .join("live-backend")
        .join(format!("{normalized_workspace_id}.jsonl"));

    seed_persisted_backend(args.state_dir.as_path(), normalized_workspace_id.as_str()).await?;

    let evaluation_cases = vec![
        EvaluationCase {
            case_id: "postgres-failover",
            query: "What is the postgres failover lag checklist?",
            expected_keywords: &["postgres", "failover"],
        },
        EvaluationCase {
            case_id: "redis-warmup",
            query: "How do we run redis cache warmup before cutover?",
            expected_keywords: &["redis", "warmup"],
        },
        EvaluationCase {
            case_id: "kafka-lag",
            query: "Remind me of the kafka lag remediation steps.",
            expected_keywords: &["kafka", "lag"],
        },
    ];

    let mut case_results = Vec::new();
    let mut request_captures = Vec::new();
    for case in &evaluation_cases {
        let request_response = ChatResponse {
            message: Message::assistant_text("ack"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        };
        let reader_client = Arc::new(CapturingMockClient::new(vec![request_response]));
        let mut reader = Agent::new(
            reader_client.clone(),
            AgentConfig {
                max_context_messages: Some(2),
                memory_retrieval_limit: 3,
                memory_min_similarity: 0.0,
                memory_backend_state_dir: Some(args.state_dir.clone()),
                memory_backend_workspace_id: normalized_workspace_id.clone(),
                ..AgentConfig::default()
            },
        );
        reader
            .prompt(case.query)
            .await
            .map_err(|error| format!("reader prompt failed for '{}': {error}", case.case_id))?;

        let requests = reader_client.requests().await;
        let request = requests
            .first()
            .ok_or_else(|| format!("missing captured request for '{}'", case.case_id))?;
        let recall_block = request
            .messages
            .iter()
            .find(|message| {
                message.role == MessageRole::System
                    && message.text_content().starts_with(MEMORY_RECALL_PREFIX)
            })
            .map(|message| message.text_content())
            .unwrap_or_default();
        let recall_entries = parse_recall_entries(recall_block.as_str());

        let top1_text = recall_entries.first().map(|(_, text)| text.clone());
        let top1_score = recall_entries.first().map(|(score, _)| *score);
        let top1_hit = top1_text
            .as_deref()
            .map(|text| keywords_match(text, case.expected_keywords))
            .unwrap_or(false);
        let topk_hit = recall_entries
            .iter()
            .any(|(_, text)| keywords_match(text, case.expected_keywords));

        case_results.push(EvaluationCaseResult {
            case_id: case.case_id.to_string(),
            query: case.query.to_string(),
            expected_keywords: case
                .expected_keywords
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            recall_count: recall_entries.len(),
            top1_score,
            top1_text,
            top1_hit,
            topk_hit,
        });

        request_captures.push(RequestCapture {
            case_id: case.case_id.to_string(),
            query: case.query.to_string(),
            request_messages: request
                .messages
                .iter()
                .map(|message| RequestCaptureMessage {
                    role: role_label(message.role).to_string(),
                    text: message.text_content(),
                })
                .collect(),
        });
    }

    write_json(request_captures_path.as_path(), &request_captures)?;

    let total_cases = case_results.len();
    let top1_hits = case_results.iter().filter(|result| result.top1_hit).count();
    let topk_hits = case_results.iter().filter(|result| result.topk_hit).count();
    let top1_relevance_rate = ratio(top1_hits, total_cases);
    let topk_relevance_rate = ratio(topk_hits, total_cases);
    let thresholds = QualityThresholds {
        top1_relevance_min: 0.66,
        topk_relevance_min: 1.0,
    };
    let quality_gate_passed = top1_relevance_rate >= thresholds.top1_relevance_min
        && topk_relevance_rate >= thresholds.topk_relevance_min
        && case_results.iter().all(|case| case.recall_count > 0);

    let persisted_entry_count = read_nonempty_lines(backend_state_file.as_path())?.len();
    let quality_report = QualityReport {
        schema_version: HARNESS_SCHEMA_VERSION,
        generated_unix_ms: current_unix_timestamp_ms(),
        workspace_id: normalized_workspace_id.clone(),
        backend_state_file: backend_state_file.display().to_string(),
        thresholds,
        metrics: QualityMetrics {
            total_cases,
            top1_hits,
            topk_hits,
            top1_relevance_rate,
            topk_relevance_rate,
            quality_gate_passed,
        },
        cases: case_results,
    };
    write_json(args.quality_report_json_out.as_path(), &quality_report)?;

    let summary_report = SummaryReport {
        schema_version: HARNESS_SCHEMA_VERSION,
        generated_unix_ms: current_unix_timestamp_ms(),
        workspace_id: normalized_workspace_id.clone(),
        total_cases,
        persisted_entry_count,
        top1_hits,
        topk_hits,
        top1_relevance_rate,
        topk_relevance_rate,
        quality_gate_passed,
        request_captures_path: request_captures_path.display().to_string(),
    };
    write_json(args.summary_json_out.as_path(), &summary_report)?;

    let artifact_manifest = build_artifact_manifest(
        vec![
            ("summary", args.summary_json_out.as_path()),
            ("quality_report", args.quality_report_json_out.as_path()),
            ("request_captures", request_captures_path.as_path()),
            ("backend_state", backend_state_file.as_path()),
        ],
        current_unix_timestamp_ms(),
    );
    write_json(
        args.artifact_manifest_json_out.as_path(),
        &artifact_manifest,
    )?;

    if !quality_gate_passed {
        return Err(format!(
            "quality gate failed: top1={:.3} topk={:.3}",
            top1_relevance_rate, topk_relevance_rate
        ));
    }

    println!(
        "[memory-live-harness] quality_gate_passed=true total_cases={} top1_rate={:.3} topk_rate={:.3}",
        total_cases, top1_relevance_rate, topk_relevance_rate
    );
    Ok(())
}

async fn seed_persisted_backend(state_dir: &Path, workspace_id: &str) -> Result<(), String> {
    let seed_prompts = vec![
        (
            "postgres-failover-seed",
            "Postgres failover checklist: promote the replica and verify replication lag metrics.",
            "Postgres failover requires promotion, lag checks, and write verification.",
        ),
        (
            "redis-warmup-seed",
            "Redis warmup runbook: preload hot keys before serving production traffic.",
            "Redis warmup includes preload, cache hit validation, and staged traffic ramp.",
        ),
        (
            "kafka-lag-seed",
            "Kafka lag remediation: inspect partitions and trigger consumer-group rebalance.",
            "Kafka lag response includes partition inspection, rebalance, and backlog drain.",
        ),
    ];

    let responses = seed_prompts
        .iter()
        .map(|(_, _, response)| ChatResponse {
            message: Message::assistant_text(*response),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })
        .collect::<Vec<_>>();
    let writer_client = Arc::new(CapturingMockClient::new(responses));
    let mut writer = Agent::new(
        writer_client,
        AgentConfig {
            max_context_messages: Some(2),
            memory_retrieval_limit: 3,
            memory_min_similarity: 0.0,
            memory_backend_state_dir: Some(state_dir.to_path_buf()),
            memory_backend_workspace_id: workspace_id.to_string(),
            memory_backend_max_entries: 512,
            ..AgentConfig::default()
        },
    );

    for (_, prompt, _) in &seed_prompts {
        writer
            .prompt(*prompt)
            .await
            .map_err(|error| format!("seed prompt failed: {error}"))?;
    }

    Ok(())
}

fn build_artifact_manifest(
    artifacts: Vec<(&'static str, &Path)>,
    generated_unix_ms: u64,
) -> ArtifactManifest {
    let mut records = Vec::new();
    let mut missing = Vec::new();
    for (label, path) in artifacts {
        if let Ok(metadata) = fs::metadata(path) {
            records.push(ArtifactRecord {
                label: label.to_string(),
                path: path.display().to_string(),
                bytes: metadata.len(),
            });
        } else {
            missing.push(path.display().to_string());
        }
    }
    ArtifactManifest {
        schema_version: HARNESS_SCHEMA_VERSION,
        generated_unix_ms,
        artifacts: records,
        missing_artifacts: missing,
    }
}

fn write_json<T: Serialize>(path: &Path, payload: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create parent directory for '{}': {error}",
                path.display()
            )
        })?;
    }
    let body = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("failed to serialize JSON for '{}': {error}", path.display()))?;
    fs::write(path, body)
        .map_err(|error| format!("failed to write JSON file '{}': {error}", path.display()))
}

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_recall_entries(block: &str) -> Vec<(f32, String)> {
    block
        .lines()
        .skip(1)
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let score = trimmed
                .split_whitespace()
                .find_map(|part| part.strip_prefix("score="))
                .and_then(|raw| raw.parse::<f32>().ok())
                .unwrap_or(0.0);
            let text = trimmed
                .split("text=")
                .nth(1)
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                None
            } else {
                Some((score, text))
            }
        })
        .collect()
}

fn keywords_match(text: &str, expected_keywords: &[&str]) -> bool {
    let lowered = text.to_ascii_lowercase();
    expected_keywords
        .iter()
        .all(|keyword| lowered.contains(keyword.to_ascii_lowercase().as_str()))
}

fn role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}

fn read_nonempty_lines(path: &Path) -> Result<Vec<String>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn normalize_workspace_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "default".to_string();
    }
    let mut normalized = String::with_capacity(trimmed.len());
    for character in trimmed.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            normalized.push(character.to_ascii_lowercase());
        } else {
            normalized.push('-');
        }
    }
    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "default".to_string()
    } else {
        normalized
    }
}
