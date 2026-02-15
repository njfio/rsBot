use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::memory_contract::{MemoryEntry, MemoryScope};

const MEMORY_RUNTIME_SCHEMA_VERSION: u32 = 1;
const MEMORY_RUNTIME_ENTRIES_FILE_NAME: &str = "entries.jsonl";
const MEMORY_SCOPE_DEFAULT_WORKSPACE: &str = "default-workspace";
const MEMORY_SCOPE_DEFAULT_CHANNEL: &str = "default-channel";
const MEMORY_SCOPE_DEFAULT_ACTOR: &str = "default-actor";
const MEMORY_EMBEDDING_SOURCE_HASH: &str = "hash-fnv1a";
const MEMORY_EMBEDDING_SOURCE_PROVIDER: &str = "provider-openai-compatible";
const MEMORY_EMBEDDING_REASON_HASH_ONLY: &str = "memory_embedding_hash_only";
const MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS: &str = "memory_embedding_provider_success";
const MEMORY_EMBEDDING_REASON_PROVIDER_FAILED: &str = "memory_embedding_provider_failed";

fn default_embedding_source() -> String {
    MEMORY_EMBEDDING_SOURCE_HASH.to_string()
}

fn default_embedding_reason_code() -> String {
    MEMORY_EMBEDDING_REASON_HASH_ONLY.to_string()
}

/// Public struct `MemoryEmbeddingProviderConfig` used across Tau components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEmbeddingProviderConfig {
    pub provider: String,
    pub model: String,
    pub api_base: String,
    pub api_key: String,
    pub dimensions: usize,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct ComputedEmbedding {
    vector: Vec<f32>,
    backend: String,
    model: Option<String>,
    reason_code: String,
}

/// Public struct `RuntimeMemoryRecord` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeMemoryRecord {
    pub schema_version: u32,
    pub updated_unix_ms: u64,
    pub scope: MemoryScope,
    pub entry: MemoryEntry,
    #[serde(default = "default_embedding_source")]
    pub embedding_source: String,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub embedding_vector: Vec<f32>,
    #[serde(default = "default_embedding_reason_code")]
    pub embedding_reason_code: String,
}

/// Public struct `MemoryScopeFilter` used across Tau components.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryScopeFilter {
    pub workspace_id: Option<String>,
    pub channel_id: Option<String>,
    pub actor_id: Option<String>,
}

impl MemoryScopeFilter {
    /// Returns true when `scope` satisfies the configured filter dimensions.
    pub fn matches_scope(&self, scope: &MemoryScope) -> bool {
        let matches_workspace = self
            .workspace_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value == scope.workspace_id)
            .unwrap_or(true);
        if !matches_workspace {
            return false;
        }

        let matches_channel = self
            .channel_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value == scope.channel_id)
            .unwrap_or(true);
        if !matches_channel {
            return false;
        }

        self.actor_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value == scope.actor_id)
            .unwrap_or(true)
    }
}

/// Public struct `MemoryWriteResult` used across Tau components.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryWriteResult {
    pub record: RuntimeMemoryRecord,
    pub created: bool,
}

/// Public struct `MemorySearchOptions` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchOptions {
    pub scope: MemoryScopeFilter,
    pub limit: usize,
    pub embedding_dimensions: usize,
    pub min_similarity: f32,
    pub enable_embedding_migration: bool,
    pub benchmark_against_hash: bool,
}

impl Default for MemorySearchOptions {
    fn default() -> Self {
        Self {
            scope: MemoryScopeFilter::default(),
            limit: 5,
            embedding_dimensions: 128,
            min_similarity: 0.55,
            enable_embedding_migration: true,
            benchmark_against_hash: false,
        }
    }
}

/// Public struct `MemorySearchMatch` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchMatch {
    pub memory_id: String,
    pub score: f32,
    pub scope: MemoryScope,
    pub summary: String,
    pub tags: Vec<String>,
    pub facts: Vec<String>,
    pub source_event_key: String,
    pub embedding_source: String,
    pub embedding_model: Option<String>,
}

/// Public struct `MemorySearchResult` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchResult {
    pub query: String,
    pub scanned: usize,
    pub returned: usize,
    pub embedding_backend: String,
    pub embedding_reason_code: String,
    pub migrated_entries: usize,
    pub query_embedding_latency_ms: u64,
    pub ranking_latency_ms: u64,
    pub baseline_hash_overlap: Option<usize>,
    pub matches: Vec<MemorySearchMatch>,
}

/// Public struct `MemoryTreeNode` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryTreeNode {
    pub id: String,
    pub level: String,
    pub entry_count: usize,
    pub children: Vec<MemoryTreeNode>,
}

/// Public struct `MemoryTree` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryTree {
    pub total_entries: usize,
    pub workspaces: Vec<MemoryTreeNode>,
}

/// Public struct `RankedTextCandidate` used across Tau components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedTextCandidate {
    pub key: String,
    pub text: String,
}

/// Public struct `RankedTextMatch` used across Tau components.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedTextMatch {
    pub key: String,
    pub text: String,
    pub score: f32,
}

/// Public struct `FileMemoryStore` used across Tau components.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMemoryStore {
    root_dir: PathBuf,
    embedding_provider: Option<MemoryEmbeddingProviderConfig>,
}

impl FileMemoryStore {
    /// Creates a file-backed store rooted at `root_dir`.
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
            embedding_provider: None,
        }
    }

    /// Creates a file-backed store rooted at `root_dir` with optional embedding provider config.
    pub fn new_with_embedding_provider(
        root_dir: impl Into<PathBuf>,
        embedding_provider: Option<MemoryEmbeddingProviderConfig>,
    ) -> Self {
        Self {
            root_dir: root_dir.into(),
            embedding_provider,
        }
    }

    /// Returns the store root directory.
    pub fn root_dir(&self) -> &Path {
        self.root_dir.as_path()
    }

    /// Writes or updates a memory entry under `scope`.
    pub fn write_entry(
        &self,
        scope: &MemoryScope,
        entry: MemoryEntry,
    ) -> Result<MemoryWriteResult> {
        let normalized_scope = normalize_scope(scope);
        let normalized_entry = normalize_entry(entry)?;

        let created = self
            .read_entry(
                normalized_entry.memory_id.as_str(),
                Some(&MemoryScopeFilter {
                    workspace_id: Some(normalized_scope.workspace_id.clone()),
                    channel_id: Some(normalized_scope.channel_id.clone()),
                    actor_id: Some(normalized_scope.actor_id.clone()),
                }),
            )?
            .is_none();

        let embedding_text = record_search_text_for_entry(&normalized_entry);
        let embedding_dimensions = self
            .embedding_provider
            .as_ref()
            .map(|config| config.dimensions)
            .unwrap_or(128);
        let computed_embedding =
            self.compute_embedding(&embedding_text, embedding_dimensions, true);
        let record = RuntimeMemoryRecord {
            schema_version: MEMORY_RUNTIME_SCHEMA_VERSION,
            updated_unix_ms: current_unix_timestamp_ms(),
            scope: normalized_scope,
            entry: normalized_entry,
            embedding_source: computed_embedding.backend,
            embedding_model: computed_embedding.model,
            embedding_vector: computed_embedding.vector,
            embedding_reason_code: computed_embedding.reason_code,
        };
        append_record(self.entries_path().as_path(), &record)?;
        Ok(MemoryWriteResult { record, created })
    }

    /// Reads the latest record for `memory_id`, optionally constrained by `scope_filter`.
    pub fn read_entry(
        &self,
        memory_id: &str,
        scope_filter: Option<&MemoryScopeFilter>,
    ) -> Result<Option<RuntimeMemoryRecord>> {
        let normalized_memory_id = memory_id.trim();
        if normalized_memory_id.is_empty() {
            bail!("memory_id must not be empty");
        }
        let records = self.load_latest_records()?;
        Ok(records.into_iter().find(|record| {
            record.entry.memory_id == normalized_memory_id
                && scope_filter
                    .map(|filter| filter.matches_scope(&record.scope))
                    .unwrap_or(true)
        }))
    }

    /// Returns latest records filtered by scope and bounded by `limit`.
    pub fn list_latest_records(
        &self,
        scope_filter: Option<&MemoryScopeFilter>,
        limit: usize,
    ) -> Result<Vec<RuntimeMemoryRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut records = self.load_latest_records()?;
        if let Some(filter) = scope_filter {
            records.retain(|record| filter.matches_scope(&record.scope));
        }
        records.truncate(limit);
        Ok(records)
    }

    /// Performs deterministic semantic search over latest records.
    pub fn search(&self, query: &str, options: &MemorySearchOptions) -> Result<MemorySearchResult> {
        let normalized_query = query.trim();
        if normalized_query.is_empty() {
            bail!("query must not be empty");
        }

        let mut migrated_entries = 0usize;
        let mut embedding_reason_code = MEMORY_EMBEDDING_REASON_HASH_ONLY.to_string();
        if options.enable_embedding_migration {
            let current = self.list_latest_records(Some(&options.scope), usize::MAX)?;
            match self.migrate_records_to_provider_embeddings(&current) {
                Ok(migrated) => {
                    migrated_entries = migrated;
                }
                Err(_) => {
                    embedding_reason_code = MEMORY_EMBEDDING_REASON_PROVIDER_FAILED.to_string();
                }
            }
        }

        let records = self.list_latest_records(Some(&options.scope), usize::MAX)?;
        let by_memory_id = records
            .into_iter()
            .map(|record| (record.entry.memory_id.clone(), record))
            .collect::<HashMap<_, _>>();

        let query_embedding_started = Instant::now();
        let computed_query =
            self.compute_embedding(normalized_query, options.embedding_dimensions, true);
        let query_embedding_latency_ms = query_embedding_started.elapsed().as_millis() as u64;
        if computed_query.reason_code != MEMORY_EMBEDDING_REASON_HASH_ONLY {
            embedding_reason_code = computed_query.reason_code.clone();
        }
        if computed_query
            .vector
            .iter()
            .all(|component| *component == 0.0)
        {
            return Ok(MemorySearchResult {
                query: normalized_query.to_string(),
                scanned: by_memory_id.len(),
                returned: 0,
                embedding_backend: computed_query.backend,
                embedding_reason_code,
                migrated_entries,
                query_embedding_latency_ms,
                ranking_latency_ms: 0,
                baseline_hash_overlap: options.benchmark_against_hash.then_some(0),
                matches: Vec::new(),
            });
        }

        let ranking_started = Instant::now();
        let mut ranked = by_memory_id
            .iter()
            .filter_map(|(memory_id, record)| {
                let candidate_embedding = if record.embedding_vector.is_empty() {
                    embed_text_vector(
                        record_search_text(record).as_str(),
                        options.embedding_dimensions,
                    )
                } else {
                    resize_and_normalize_embedding(
                        record.embedding_vector.as_slice(),
                        options.embedding_dimensions,
                    )
                };
                let score = cosine_similarity(&computed_query.vector, &candidate_embedding);
                if score >= options.min_similarity {
                    Some(RankedTextMatch {
                        key: memory_id.clone(),
                        text: record_search_text(record),
                        score,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.key.cmp(&right.key))
        });
        ranked.truncate(options.limit);
        let ranking_latency_ms = ranking_started.elapsed().as_millis() as u64;

        let mut matches = Vec::with_capacity(ranked.len());
        for item in &ranked {
            let Some(record) = by_memory_id.get(&item.key) else {
                continue;
            };
            matches.push(MemorySearchMatch {
                memory_id: record.entry.memory_id.clone(),
                score: item.score,
                scope: record.scope.clone(),
                summary: record.entry.summary.clone(),
                tags: record.entry.tags.clone(),
                facts: record.entry.facts.clone(),
                source_event_key: record.entry.source_event_key.clone(),
                embedding_source: record.embedding_source.clone(),
                embedding_model: record.embedding_model.clone(),
            });
        }

        let baseline_hash_overlap = if options.benchmark_against_hash {
            let baseline = rank_text_candidates(
                normalized_query,
                by_memory_id
                    .iter()
                    .map(|(memory_id, record)| RankedTextCandidate {
                        key: memory_id.clone(),
                        text: record_search_text(record),
                    })
                    .collect::<Vec<_>>(),
                options.limit,
                options.embedding_dimensions,
                options.min_similarity,
            );
            let selected = matches
                .iter()
                .map(|item| item.memory_id.as_str())
                .collect::<BTreeSet<_>>();
            Some(
                baseline
                    .into_iter()
                    .filter(|candidate| selected.contains(candidate.key.as_str()))
                    .count(),
            )
        } else {
            None
        };

        Ok(MemorySearchResult {
            query: normalized_query.to_string(),
            scanned: by_memory_id.len(),
            returned: matches.len(),
            embedding_backend: computed_query.backend,
            embedding_reason_code,
            migrated_entries,
            query_embedding_latency_ms,
            ranking_latency_ms,
            baseline_hash_overlap,
            matches,
        })
    }

    fn compute_embedding(
        &self,
        text: &str,
        dimensions: usize,
        prefer_provider: bool,
    ) -> ComputedEmbedding {
        if prefer_provider {
            if let Some(config) = &self.embedding_provider {
                let provider = config.provider.trim().to_ascii_lowercase();
                if provider == "openai" || provider == "openai-compatible" {
                    if let Ok(vectors) =
                        embed_text_vectors_via_provider(&[text.to_string()], dimensions, config)
                    {
                        if let Some(first) = vectors.first() {
                            return ComputedEmbedding {
                                vector: first.clone(),
                                backend: MEMORY_EMBEDDING_SOURCE_PROVIDER.to_string(),
                                model: Some(config.model.clone()),
                                reason_code: MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS.to_string(),
                            };
                        }
                    }
                    return ComputedEmbedding {
                        vector: embed_text_vector(text, dimensions),
                        backend: MEMORY_EMBEDDING_SOURCE_HASH.to_string(),
                        model: None,
                        reason_code: MEMORY_EMBEDDING_REASON_PROVIDER_FAILED.to_string(),
                    };
                }
            }
        }

        ComputedEmbedding {
            vector: embed_text_vector(text, dimensions),
            backend: MEMORY_EMBEDDING_SOURCE_HASH.to_string(),
            model: None,
            reason_code: MEMORY_EMBEDDING_REASON_HASH_ONLY.to_string(),
        }
    }

    fn migrate_records_to_provider_embeddings(
        &self,
        records: &[RuntimeMemoryRecord],
    ) -> Result<usize> {
        let Some(config) = &self.embedding_provider else {
            return Ok(0);
        };
        let provider = config.provider.trim().to_ascii_lowercase();
        if provider != "openai" && provider != "openai-compatible" {
            return Ok(0);
        }

        let to_migrate = records
            .iter()
            .filter(|record| {
                record.embedding_vector.is_empty()
                    || record
                        .embedding_source
                        .starts_with(MEMORY_EMBEDDING_SOURCE_HASH)
            })
            .cloned()
            .collect::<Vec<_>>();
        if to_migrate.is_empty() {
            return Ok(0);
        }

        let inputs = to_migrate
            .iter()
            .map(record_search_text)
            .collect::<Vec<_>>();
        let vectors = embed_text_vectors_via_provider(&inputs, config.dimensions, config)
            .map_err(|error| anyhow::anyhow!(error))?;

        let mut migrated = 0usize;
        for (record, vector) in to_migrate.into_iter().zip(vectors.into_iter()) {
            let migrated_record = RuntimeMemoryRecord {
                schema_version: MEMORY_RUNTIME_SCHEMA_VERSION,
                updated_unix_ms: current_unix_timestamp_ms(),
                scope: record.scope,
                entry: record.entry,
                embedding_source: MEMORY_EMBEDDING_SOURCE_PROVIDER.to_string(),
                embedding_model: Some(config.model.clone()),
                embedding_vector: vector,
                embedding_reason_code: MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS.to_string(),
            };
            append_record(self.entries_path().as_path(), &migrated_record)?;
            migrated = migrated.saturating_add(1);
        }

        Ok(migrated)
    }

    /// Returns a hierarchical workspace/channel/actor tree for latest records.
    pub fn tree(&self) -> Result<MemoryTree> {
        let records = self.load_latest_records()?;
        let mut by_scope = BTreeMap::<String, BTreeMap<String, BTreeMap<String, usize>>>::new();

        for record in records {
            let workspace = record.scope.workspace_id;
            let channel = record.scope.channel_id;
            let actor = record.scope.actor_id;
            *by_scope
                .entry(workspace)
                .or_default()
                .entry(channel)
                .or_default()
                .entry(actor)
                .or_default() += 1;
        }

        let mut total_entries = 0usize;
        let mut workspaces = Vec::with_capacity(by_scope.len());
        for (workspace_id, channels) in by_scope {
            let mut workspace_count = 0usize;
            let mut channel_nodes = Vec::with_capacity(channels.len());
            for (channel_id, actors) in channels {
                let mut channel_count = 0usize;
                let mut actor_nodes = Vec::with_capacity(actors.len());
                for (actor_id, actor_count) in actors {
                    channel_count = channel_count.saturating_add(actor_count);
                    actor_nodes.push(MemoryTreeNode {
                        id: actor_id,
                        level: "actor".to_string(),
                        entry_count: actor_count,
                        children: Vec::new(),
                    });
                }
                workspace_count = workspace_count.saturating_add(channel_count);
                channel_nodes.push(MemoryTreeNode {
                    id: channel_id,
                    level: "channel".to_string(),
                    entry_count: channel_count,
                    children: actor_nodes,
                });
            }
            total_entries = total_entries.saturating_add(workspace_count);
            workspaces.push(MemoryTreeNode {
                id: workspace_id,
                level: "workspace".to_string(),
                entry_count: workspace_count,
                children: channel_nodes,
            });
        }

        Ok(MemoryTree {
            total_entries,
            workspaces,
        })
    }

    fn load_latest_records(&self) -> Result<Vec<RuntimeMemoryRecord>> {
        let records = load_records(self.entries_path().as_path())?;
        let mut seen = BTreeSet::new();
        let mut latest = Vec::new();
        for record in records.into_iter().rev() {
            if seen.insert(record.entry.memory_id.clone()) {
                latest.push(record);
            }
        }
        latest.sort_by(|left, right| {
            right
                .updated_unix_ms
                .cmp(&left.updated_unix_ms)
                .then_with(|| left.entry.memory_id.cmp(&right.entry.memory_id))
        });
        Ok(latest)
    }

    fn entries_path(&self) -> PathBuf {
        self.root_dir.join(MEMORY_RUNTIME_ENTRIES_FILE_NAME)
    }
}

/// Ranks text candidates using deterministic hash embeddings and cosine similarity.
pub fn rank_text_candidates(
    query: &str,
    candidates: Vec<RankedTextCandidate>,
    limit: usize,
    dimensions: usize,
    min_similarity: f32,
) -> Vec<RankedTextMatch> {
    if limit == 0 {
        return Vec::new();
    }
    let normalized_query = query.trim();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let query_embedding = embed_text_vector(normalized_query, dimensions);
    if query_embedding.iter().all(|component| *component == 0.0) {
        return Vec::new();
    }

    let mut matches = candidates
        .into_iter()
        .filter_map(|candidate| {
            let candidate_embedding = embed_text_vector(candidate.text.as_str(), dimensions);
            let score = cosine_similarity(&query_embedding, &candidate_embedding);
            if score >= min_similarity {
                Some(RankedTextMatch {
                    key: candidate.key,
                    text: candidate.text,
                    score,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.key.cmp(&right.key))
    });
    matches.truncate(limit);
    matches
}

/// Converts text to a normalized fixed-size vector using FNV-1a token hashing.
pub fn embed_text_vector(text: &str, dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(1);
    let mut vector = vec![0.0f32; dimensions];
    for raw_token in text.split(|character: char| !character.is_alphanumeric()) {
        if raw_token.is_empty() {
            continue;
        }
        let token = raw_token.to_ascii_lowercase();
        let hash = fnv1a_hash(token.as_bytes());
        let index = (hash as usize) % dimensions;
        let sign = if (hash & 1) == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }

    let magnitude = vector
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if magnitude > 0.0 {
        for component in &mut vector {
            *component /= magnitude;
        }
    }
    vector
}

fn embed_text_vectors_via_provider(
    inputs: &[String],
    dimensions: usize,
    config: &MemoryEmbeddingProviderConfig,
) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let timeout_ms = config.timeout_ms.max(1);
    let api_base = config.api_base.trim_end_matches('/');
    if api_base.is_empty() {
        return Err("embedding api_base must not be empty".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|error| format!("failed to build embedding client: {error}"))?;
    let response = client
        .post(format!("{api_base}/embeddings"))
        .bearer_auth(config.api_key.as_str())
        .json(&serde_json::json!({
            "model": config.model,
            "input": inputs,
        }))
        .send()
        .map_err(|error| format!("embedding request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "embedding request failed with status {}: {}",
            status.as_u16(),
            body.chars().take(240).collect::<String>()
        ));
    }

    let payload = response
        .json::<serde_json::Value>()
        .map_err(|error| format!("failed to parse embedding response json: {error}"))?;
    let data = payload
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "embedding response missing data array".to_string())?;
    if data.len() != inputs.len() {
        return Err(format!(
            "embedding response size mismatch: expected {}, got {}",
            inputs.len(),
            data.len()
        ));
    }

    let mut vectors = Vec::with_capacity(data.len());
    for item in data {
        let raw_embedding = item
            .get("embedding")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "embedding item missing embedding array".to_string())?;
        let parsed = raw_embedding
            .iter()
            .map(|component| {
                component
                    .as_f64()
                    .map(|value| value as f32)
                    .ok_or_else(|| "embedding component must be numeric".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        vectors.push(resize_and_normalize_embedding(&parsed, dimensions));
    }
    Ok(vectors)
}

fn resize_and_normalize_embedding(values: &[f32], dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(1);
    let mut resized = vec![0.0f32; dimensions];
    for (index, value) in values.iter().enumerate() {
        let bucket = index % dimensions;
        resized[bucket] += *value;
    }

    let magnitude = resized
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if magnitude > 0.0 {
        for component in &mut resized {
            *component /= magnitude;
        }
    }
    resized
}

/// Computes cosine similarity for equal-length vectors.
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() {
        return 0.0;
    }
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn record_search_text(record: &RuntimeMemoryRecord) -> String {
    let mut parts = Vec::with_capacity(3);
    parts.push(record.entry.summary.clone());
    if !record.entry.tags.is_empty() {
        parts.push(record.entry.tags.join(" "));
    }
    if !record.entry.facts.is_empty() {
        parts.push(record.entry.facts.join(" "));
    }
    parts.join("\n")
}

fn record_search_text_for_entry(entry: &MemoryEntry) -> String {
    let mut parts = Vec::with_capacity(3);
    parts.push(entry.summary.clone());
    if !entry.tags.is_empty() {
        parts.push(entry.tags.join(" "));
    }
    if !entry.facts.is_empty() {
        parts.push(entry.facts.join(" "));
    }
    parts.join("\n")
}

fn normalize_scope(scope: &MemoryScope) -> MemoryScope {
    MemoryScope {
        workspace_id: normalize_scope_component(
            &scope.workspace_id,
            MEMORY_SCOPE_DEFAULT_WORKSPACE,
        ),
        channel_id: normalize_scope_component(&scope.channel_id, MEMORY_SCOPE_DEFAULT_CHANNEL),
        actor_id: normalize_scope_component(&scope.actor_id, MEMORY_SCOPE_DEFAULT_ACTOR),
    }
}

fn normalize_scope_component(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_entry(entry: MemoryEntry) -> Result<MemoryEntry> {
    let memory_id = entry.memory_id.trim().to_string();
    if memory_id.is_empty() {
        bail!("memory_id must not be empty");
    }
    let summary = entry.summary.trim().to_string();
    if summary.is_empty() {
        bail!("summary must not be empty");
    }

    let tags = entry
        .tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let facts = entry
        .facts
        .into_iter()
        .map(|fact| fact.trim().to_string())
        .filter(|fact| !fact.is_empty())
        .collect::<Vec<_>>();

    Ok(MemoryEntry {
        memory_id,
        summary,
        tags,
        facts,
        source_event_key: entry.source_event_key.trim().to_string(),
        recency_weight_bps: entry.recency_weight_bps,
        confidence_bps: entry.confidence_bps,
    })
}

fn append_record(path: &Path, record: &RuntimeMemoryRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create memory store root {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open memory entries file {}", path.display()))?;
    let encoded = serde_json::to_string(record).context("failed to encode memory record")?;
    file.write_all(encoded.as_bytes())
        .with_context(|| format!("failed to write memory record to {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to write newline to {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush memory entries file {}", path.display()))?;
    Ok(())
}

fn load_records(path: &Path) -> Result<Vec<RuntimeMemoryRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open memory entries file {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "failed to read memory entries file {} at line {}",
                path.display(),
                index + 1
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<RuntimeMemoryRecord>(trimmed).with_context(|| {
            format!(
                "failed to parse memory entries file {} at line {}",
                path.display(),
                index + 1
            )
        })?;
        records.push(record);
    }
    Ok(records)
}

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        embed_text_vector, rank_text_candidates, FileMemoryStore, MemoryEmbeddingProviderConfig,
        MemoryScopeFilter, MemorySearchOptions, RankedTextCandidate,
    };
    use crate::memory_contract::{MemoryEntry, MemoryScope};
    use httpmock::{Method::POST, MockServer};
    use tempfile::tempdir;

    #[test]
    fn unit_embed_text_vector_normalizes_non_empty_inputs() {
        let vector = embed_text_vector("release checklist", 32);
        let magnitude = vector
            .iter()
            .map(|component| component * component)
            .sum::<f32>()
            .sqrt();
        assert!(magnitude > 0.99);
        assert!(magnitude <= 1.001);
    }

    #[test]
    fn functional_file_memory_store_write_and_read_round_trips_latest_record() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "channel-1".to_string(),
            actor_id: "assistant".to_string(),
        };

        let first = MemoryEntry {
            memory_id: "memory-1".to_string(),
            summary: "remember release checklist owner".to_string(),
            tags: vec!["release".to_string()],
            facts: vec!["owner=ops".to_string()],
            source_event_key: "evt-1".to_string(),
            recency_weight_bps: 120,
            confidence_bps: 880,
        };
        let second = MemoryEntry {
            summary: "remember release checklist owner + rollout order".to_string(),
            source_event_key: "evt-2".to_string(),
            ..first.clone()
        };

        let first_result = store.write_entry(&scope, first).expect("first write");
        assert!(first_result.created);
        let second_result = store.write_entry(&scope, second).expect("second write");
        assert!(!second_result.created);

        let loaded = store
            .read_entry("memory-1", None)
            .expect("read")
            .expect("existing");
        assert_eq!(
            loaded.entry.summary,
            "remember release checklist owner + rollout order"
        );
        assert_eq!(loaded.entry.source_event_key, "evt-2");
    }

    #[test]
    fn functional_memory_store_persists_provider_embedding_metadata() {
        let server = MockServer::start();
        let embeddings = server.mock(|when, then| {
            when.method(POST).path("/embeddings");
            then.status(200).json_body_obj(&serde_json::json!({
                "data": [
                    { "embedding": [0.4, 0.1, -0.3, 0.2] }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new_with_embedding_provider(
            temp.path(),
            Some(MemoryEmbeddingProviderConfig {
                provider: "openai-compatible".to_string(),
                model: "text-embedding-3-small".to_string(),
                api_base: server.url(""),
                api_key: "test-key".to_string(),
                dimensions: 8,
                timeout_ms: 5_000,
            }),
        );
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };
        let write_result = store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-provider".to_string(),
                    summary: "release checklist with provider embeddings".to_string(),
                    tags: vec!["release".to_string()],
                    facts: vec!["owner=ops".to_string()],
                    source_event_key: "evt-provider".to_string(),
                    recency_weight_bps: 100,
                    confidence_bps: 900,
                },
            )
            .expect("provider-backed write");

        embeddings.assert();
        assert_eq!(
            write_result.record.embedding_source,
            "provider-openai-compatible"
        );
        assert_eq!(
            write_result.record.embedding_model,
            Some("text-embedding-3-small".to_string())
        );
        assert_eq!(
            write_result.record.embedding_reason_code,
            "memory_embedding_provider_success"
        );
        assert_eq!(write_result.record.embedding_vector.len(), 8);
        assert!(write_result
            .record
            .embedding_vector
            .iter()
            .any(|value| *value != 0.0));
    }

    #[test]
    fn regression_memory_store_falls_back_to_hash_embeddings_on_provider_failure() {
        let server = MockServer::start();
        let _embeddings = server.mock(|when, then| {
            when.method(POST).path("/embeddings");
            then.status(500).json_body_obj(&serde_json::json!({
                "error": "provider outage"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new_with_embedding_provider(
            temp.path(),
            Some(MemoryEmbeddingProviderConfig {
                provider: "openai".to_string(),
                model: "text-embedding-3-small".to_string(),
                api_base: server.url(""),
                api_key: "test-key".to_string(),
                dimensions: 16,
                timeout_ms: 5_000,
            }),
        );
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };
        let result = store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-fallback".to_string(),
                    summary: "fallback should keep memory writes online".to_string(),
                    tags: vec!["incident".to_string()],
                    facts: vec!["provider=down".to_string()],
                    source_event_key: "evt-fallback".to_string(),
                    recency_weight_bps: 100,
                    confidence_bps: 900,
                },
            )
            .expect("fallback write");

        assert_eq!(result.record.embedding_source, "hash-fnv1a");
        assert_eq!(result.record.embedding_model, None);
        assert_eq!(
            result.record.embedding_reason_code,
            "memory_embedding_provider_failed"
        );
        assert_eq!(result.record.embedding_vector.len(), 16);
    }

    #[test]
    fn integration_memory_search_migrates_hash_records_to_provider_embeddings() {
        let temp = tempdir().expect("tempdir");
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };
        let hash_store = FileMemoryStore::new(temp.path());
        hash_store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-1".to_string(),
                    summary: "release workflow validation".to_string(),
                    tags: vec!["release".to_string()],
                    facts: vec!["check smoke tests".to_string()],
                    source_event_key: "evt-1".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write first hash record");
        hash_store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-2".to_string(),
                    summary: "release freeze checklist".to_string(),
                    tags: vec!["freeze".to_string()],
                    facts: vec!["validate rollback".to_string()],
                    source_event_key: "evt-2".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write second hash record");

        let server = MockServer::start();
        let migration_call = server.mock(|when, then| {
            when.method(POST)
                .path("/embeddings")
                .body_includes("release workflow validation");
            then.status(200).json_body_obj(&serde_json::json!({
                "data": [
                    { "embedding": [0.9, 0.0, 0.1, 0.0] },
                    { "embedding": [0.8, 0.0, 0.2, 0.0] }
                ]
            }));
        });
        let query_call = server.mock(|when, then| {
            when.method(POST)
                .path("/embeddings")
                .body_includes("release workflow");
            then.status(200).json_body_obj(&serde_json::json!({
                "data": [
                    { "embedding": [0.95, 0.0, 0.05, 0.0] }
                ]
            }));
        });

        let provider_store = FileMemoryStore::new_with_embedding_provider(
            temp.path(),
            Some(MemoryEmbeddingProviderConfig {
                provider: "openai-compatible".to_string(),
                model: "text-embedding-3-small".to_string(),
                api_base: server.url(""),
                api_key: "test-key".to_string(),
                dimensions: 4,
                timeout_ms: 5_000,
            }),
        );
        let result = provider_store
            .search(
                "release workflow",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 4,
                    min_similarity: 0.0,
                    enable_embedding_migration: true,
                    benchmark_against_hash: false,
                },
            )
            .expect("search with migration");

        migration_call.assert();
        query_call.assert();
        assert_eq!(result.migrated_entries, 2);
        assert_eq!(result.embedding_backend, "provider-openai-compatible");
        assert_eq!(
            result.embedding_reason_code,
            "memory_embedding_provider_success"
        );
        assert!(result.returned >= 1);

        let migrated_first = provider_store
            .read_entry("memory-1", None)
            .expect("read migrated first")
            .expect("first exists");
        let migrated_second = provider_store
            .read_entry("memory-2", None)
            .expect("read migrated second")
            .expect("second exists");
        assert_eq!(
            migrated_first.embedding_source,
            "provider-openai-compatible"
        );
        assert_eq!(
            migrated_second.embedding_source,
            "provider-openai-compatible"
        );
        assert_eq!(
            migrated_first.embedding_reason_code,
            "memory_embedding_provider_success"
        );
    }

    #[test]
    fn integration_memory_search_uses_ranked_candidates_with_scope_filter() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope_a = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };
        let scope_b = MemoryScope {
            workspace_id: "workspace-b".to_string(),
            channel_id: "support".to_string(),
            actor_id: "assistant".to_string(),
        };

        store
            .write_entry(
                &scope_a,
                MemoryEntry {
                    memory_id: "memory-release".to_string(),
                    summary: "Nightly release checklist requires smoke tests".to_string(),
                    tags: vec!["release".to_string(), "nightly".to_string()],
                    facts: vec!["run smoke".to_string()],
                    source_event_key: "evt-a".to_string(),
                    recency_weight_bps: 90,
                    confidence_bps: 820,
                },
            )
            .expect("write release memory");
        store
            .write_entry(
                &scope_b,
                MemoryEntry {
                    memory_id: "memory-support".to_string(),
                    summary: "Support rotation uses weekend escalation".to_string(),
                    tags: vec!["support".to_string()],
                    facts: vec!["pager escalation".to_string()],
                    source_event_key: "evt-b".to_string(),
                    recency_weight_bps: 70,
                    confidence_bps: 700,
                },
            )
            .expect("write support memory");

        let result = store
            .search(
                "release smoke checklist",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter {
                        workspace_id: Some("workspace-a".to_string()),
                        channel_id: None,
                        actor_id: None,
                    },
                    limit: 5,
                    embedding_dimensions: 128,
                    min_similarity: 0.1,
                    enable_embedding_migration: true,
                    benchmark_against_hash: false,
                },
            )
            .expect("search");
        assert_eq!(result.returned, 1);
        assert_eq!(result.matches[0].memory_id, "memory-release");
        assert_eq!(result.matches[0].scope.workspace_id, "workspace-a");
    }

    #[test]
    fn regression_memory_search_reports_baseline_overlap_when_benchmark_enabled() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };
        store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-release".to_string(),
                    summary: "release smoke checklist".to_string(),
                    tags: vec!["release".to_string()],
                    facts: vec!["smoke".to_string()],
                    source_event_key: "evt-1".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write release memory");
        store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-unrelated".to_string(),
                    summary: "office lunch planning".to_string(),
                    tags: vec!["social".to_string()],
                    facts: vec!["pizza".to_string()],
                    source_event_key: "evt-2".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write unrelated memory");

        let benchmarked = store
            .search(
                "release smoke",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 64,
                    min_similarity: 0.0,
                    enable_embedding_migration: false,
                    benchmark_against_hash: true,
                },
            )
            .expect("benchmarked search");
        let regular = store
            .search(
                "release smoke",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 64,
                    min_similarity: 0.0,
                    enable_embedding_migration: false,
                    benchmark_against_hash: false,
                },
            )
            .expect("regular search");

        assert!(benchmarked.baseline_hash_overlap.is_some());
        assert_eq!(regular.baseline_hash_overlap, None);
    }

    #[test]
    fn regression_memory_tree_counts_latest_entry_versions_once() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };

        let first = MemoryEntry {
            memory_id: "memory-1".to_string(),
            summary: "first".to_string(),
            tags: Vec::new(),
            facts: Vec::new(),
            source_event_key: "evt-1".to_string(),
            recency_weight_bps: 0,
            confidence_bps: 0,
        };
        store
            .write_entry(&scope, first.clone())
            .expect("write first version");
        store
            .write_entry(
                &scope,
                MemoryEntry {
                    summary: "second".to_string(),
                    source_event_key: "evt-2".to_string(),
                    ..first
                },
            )
            .expect("write second version");

        let tree = store.tree().expect("tree");
        assert_eq!(tree.total_entries, 1);
        assert_eq!(tree.workspaces.len(), 1);
        assert_eq!(tree.workspaces[0].entry_count, 1);
        assert_eq!(tree.workspaces[0].children[0].entry_count, 1);
        assert_eq!(tree.workspaces[0].children[0].children[0].entry_count, 1);
    }

    #[test]
    fn unit_rank_text_candidates_returns_highest_similarity_first() {
        let ranked = rank_text_candidates(
            "release checklist",
            vec![
                RankedTextCandidate {
                    key: "a".to_string(),
                    text: "release checklist smoke tests".to_string(),
                },
                RankedTextCandidate {
                    key: "b".to_string(),
                    text: "team lunch planning".to_string(),
                },
            ],
            2,
            128,
            0.1,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].key, "a");
    }
}
