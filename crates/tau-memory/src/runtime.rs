use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::memory_contract::{MemoryEntry, MemoryScope};

mod backend;
mod query;
mod ranking;

use backend::{
    append_record_jsonl, append_record_sqlite, initialize_memory_sqlite_schema, load_records_jsonl,
    load_records_sqlite, load_relation_map_sqlite, open_memory_sqlite_connection,
    resolve_memory_backend,
};
pub use ranking::{
    cosine_similarity, embed_text_vector, rank_text_candidates, rank_text_candidates_bm25,
};
use ranking::{
    reciprocal_rank_fuse, record_search_text, record_search_text_for_entry,
    resize_and_normalize_embedding,
};

const MEMORY_RUNTIME_SCHEMA_VERSION: u32 = 1;
const MEMORY_RUNTIME_ENTRIES_FILE_NAME: &str = "entries.jsonl";
const MEMORY_RUNTIME_ENTRIES_SQLITE_FILE_NAME: &str = "entries.sqlite";
const MEMORY_BACKEND_ENV: &str = "TAU_MEMORY_BACKEND";
const MEMORY_SCOPE_DEFAULT_WORKSPACE: &str = "default-workspace";
const MEMORY_SCOPE_DEFAULT_CHANNEL: &str = "default-channel";
const MEMORY_SCOPE_DEFAULT_ACTOR: &str = "default-actor";
const MEMORY_EMBEDDING_SOURCE_HASH: &str = "hash-fnv1a";
const MEMORY_EMBEDDING_SOURCE_PROVIDER: &str = "provider-openai-compatible";
const MEMORY_EMBEDDING_REASON_HASH_ONLY: &str = "memory_embedding_hash_only";
const MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS: &str = "memory_embedding_provider_success";
const MEMORY_EMBEDDING_REASON_PROVIDER_FAILED: &str = "memory_embedding_provider_failed";
const MEMORY_RETRIEVAL_BACKEND_VECTOR_ONLY: &str = "vector-only";
const MEMORY_RETRIEVAL_BACKEND_HYBRID_BM25_RRF: &str = "hybrid-bm25-rrf";
const MEMORY_RETRIEVAL_REASON_VECTOR_ONLY: &str = "memory_retrieval_vector_only";
const MEMORY_RETRIEVAL_REASON_HYBRID_ENABLED: &str = "memory_retrieval_hybrid_enabled";
const MEMORY_LIFECYCLE_DEFAULT_STALE_AFTER_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MEMORY_LIFECYCLE_DEFAULT_DECAY_RATE: f32 = 0.9;
const MEMORY_LIFECYCLE_DEFAULT_PRUNE_IMPORTANCE_FLOOR: f32 = 0.1;
const MEMORY_LIFECYCLE_DEFAULT_ORPHAN_IMPORTANCE_FLOOR: f32 = 0.2;
const MEMORY_STORAGE_REASON_PATH_JSONL: &str = "memory_storage_backend_path_jsonl";
const MEMORY_STORAGE_REASON_PATH_SQLITE: &str = "memory_storage_backend_path_sqlite";
const MEMORY_STORAGE_REASON_EXISTING_JSONL: &str = "memory_storage_backend_existing_jsonl";
const MEMORY_STORAGE_REASON_EXISTING_SQLITE: &str = "memory_storage_backend_existing_sqlite";
const MEMORY_STORAGE_REASON_DEFAULT_SQLITE: &str = "memory_storage_backend_default_sqlite";
const MEMORY_STORAGE_REASON_ENV_JSONL: &str = "memory_storage_backend_env_jsonl";
const MEMORY_STORAGE_REASON_ENV_SQLITE: &str = "memory_storage_backend_env_sqlite";
const MEMORY_STORAGE_REASON_ENV_AUTO: &str = "memory_storage_backend_env_auto";
const MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK: &str =
    "memory_storage_backend_env_invalid_fallback";
const MEMORY_STORAGE_REASON_INIT_IMPORT_FAILED: &str = "memory_storage_backend_import_failed";
const MEMORY_RELATION_TYPE_DEFAULT: &str = "relates_to";
const MEMORY_RELATION_TYPE_VALUES: &[&str] = &[
    "relates_to",
    "depends_on",
    "supports",
    "blocks",
    "references",
];
const MEMORY_GRAPH_SIGNAL_WEIGHT_DEFAULT: f32 = 0.25;
pub const MEMORY_INVALID_RELATION_REASON_CODE: &str = "memory_invalid_relation";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Enumerates supported `MemoryStorageBackend` values.
pub enum MemoryStorageBackend {
    Jsonl,
    Sqlite,
}

impl MemoryStorageBackend {
    pub fn label(self) -> &'static str {
        match self {
            MemoryStorageBackend::Jsonl => "jsonl",
            MemoryStorageBackend::Sqlite => "sqlite",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedMemoryBackend {
    backend: MemoryStorageBackend,
    storage_path: Option<PathBuf>,
    reason_code: String,
}

fn default_embedding_source() -> String {
    MEMORY_EMBEDDING_SOURCE_HASH.to_string()
}

fn default_embedding_reason_code() -> String {
    MEMORY_EMBEDDING_REASON_HASH_ONLY.to_string()
}

fn default_memory_importance() -> f32 {
    MemoryType::default().default_importance()
}

fn default_graph_signal_weight() -> f32 {
    MEMORY_GRAPH_SIGNAL_WEIGHT_DEFAULT
}

fn default_lifecycle_stale_after_unix_ms() -> u64 {
    MEMORY_LIFECYCLE_DEFAULT_STALE_AFTER_MS
}

fn default_lifecycle_decay_rate() -> f32 {
    MEMORY_LIFECYCLE_DEFAULT_DECAY_RATE
}

fn default_lifecycle_prune_importance_floor() -> f32 {
    MEMORY_LIFECYCLE_DEFAULT_PRUNE_IMPORTANCE_FLOOR
}

fn default_lifecycle_orphan_importance_floor() -> f32 {
    MEMORY_LIFECYCLE_DEFAULT_ORPHAN_IMPORTANCE_FLOOR
}

fn default_lifecycle_orphan_cleanup_enabled() -> bool {
    true
}

/// Public struct `MemoryRelation` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRelation {
    pub target_id: String,
    pub relation_type: String,
    pub weight: f32,
    pub effective_weight: f32,
}

/// Public struct `MemoryRelationInput` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRelationInput {
    pub target_id: String,
    #[serde(default)]
    pub relation_type: Option<String>,
    #[serde(default)]
    pub weight: Option<f32>,
}

/// Enumerates supported `MemoryType` values.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Identity,
    Goal,
    Decision,
    Todo,
    Preference,
    Fact,
    Event,
    Observation,
}

impl MemoryType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Goal => "goal",
            Self::Decision => "decision",
            Self::Todo => "todo",
            Self::Preference => "preference",
            Self::Fact => "fact",
            Self::Event => "event",
            Self::Observation => "observation",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "identity" => Some(Self::Identity),
            "goal" => Some(Self::Goal),
            "decision" => Some(Self::Decision),
            "todo" => Some(Self::Todo),
            "preference" => Some(Self::Preference),
            "fact" => Some(Self::Fact),
            "event" => Some(Self::Event),
            "observation" => Some(Self::Observation),
            _ => None,
        }
    }

    pub fn default_importance(self) -> f32 {
        match self {
            Self::Identity => 1.0,
            Self::Goal => 0.9,
            Self::Decision => 0.85,
            Self::Todo => 0.8,
            Self::Preference => 0.7,
            Self::Fact => 0.65,
            Self::Event => 0.55,
            Self::Observation => 0.3,
        }
    }
}

impl Default for MemoryType {
    fn default() -> Self {
        Self::Observation
    }
}

pub(super) fn importance_rank_multiplier(importance: f32) -> f32 {
    1.0 + importance.clamp(0.0, 1.0)
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
    #[serde(default)]
    pub memory_type: MemoryType,
    #[serde(default = "default_memory_importance")]
    pub importance: f32,
    #[serde(default = "default_embedding_source")]
    pub embedding_source: String,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub embedding_vector: Vec<f32>,
    #[serde(default = "default_embedding_reason_code")]
    pub embedding_reason_code: String,
    #[serde(default)]
    pub last_accessed_at_unix_ms: u64,
    #[serde(default)]
    pub access_count: u64,
    #[serde(default)]
    pub forgotten: bool,
    #[serde(default)]
    pub relations: Vec<MemoryRelation>,
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
    pub enable_hybrid_retrieval: bool,
    pub bm25_k1: f32,
    pub bm25_b: f32,
    pub bm25_min_score: f32,
    pub rrf_k: usize,
    pub rrf_vector_weight: f32,
    pub rrf_lexical_weight: f32,
    #[serde(default = "default_graph_signal_weight")]
    pub graph_signal_weight: f32,
    pub enable_embedding_migration: bool,
    pub benchmark_against_hash: bool,
    pub benchmark_against_vector_only: bool,
}

impl Default for MemorySearchOptions {
    fn default() -> Self {
        Self {
            scope: MemoryScopeFilter::default(),
            limit: 5,
            embedding_dimensions: 128,
            min_similarity: 0.55,
            enable_hybrid_retrieval: false,
            bm25_k1: 1.2,
            bm25_b: 0.75,
            bm25_min_score: 0.0,
            rrf_k: 60,
            rrf_vector_weight: 1.0,
            rrf_lexical_weight: 1.0,
            graph_signal_weight: MEMORY_GRAPH_SIGNAL_WEIGHT_DEFAULT,
            enable_embedding_migration: true,
            benchmark_against_hash: false,
            benchmark_against_vector_only: false,
        }
    }
}

/// Public struct `MemoryLifecycleMaintenancePolicy` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryLifecycleMaintenancePolicy {
    #[serde(default = "default_lifecycle_stale_after_unix_ms")]
    pub stale_after_unix_ms: u64,
    #[serde(default = "default_lifecycle_decay_rate")]
    pub decay_rate: f32,
    #[serde(default = "default_lifecycle_prune_importance_floor")]
    pub prune_importance_floor: f32,
    #[serde(default = "default_lifecycle_orphan_cleanup_enabled")]
    pub orphan_cleanup_enabled: bool,
    #[serde(default = "default_lifecycle_orphan_importance_floor")]
    pub orphan_importance_floor: f32,
}

impl Default for MemoryLifecycleMaintenancePolicy {
    fn default() -> Self {
        Self {
            stale_after_unix_ms: default_lifecycle_stale_after_unix_ms(),
            decay_rate: default_lifecycle_decay_rate(),
            prune_importance_floor: default_lifecycle_prune_importance_floor(),
            orphan_cleanup_enabled: default_lifecycle_orphan_cleanup_enabled(),
            orphan_importance_floor: default_lifecycle_orphan_importance_floor(),
        }
    }
}

/// Public struct `MemoryLifecycleMaintenanceResult` used across Tau components.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryLifecycleMaintenanceResult {
    pub scanned_records: usize,
    pub decayed_records: usize,
    pub pruned_records: usize,
    pub orphan_forgotten_records: usize,
    pub identity_exempt_records: usize,
    pub updated_records: usize,
    pub unchanged_records: usize,
}

/// Public struct `MemorySearchMatch` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchMatch {
    pub memory_id: String,
    pub score: f32,
    pub vector_score: Option<f32>,
    pub lexical_score: Option<f32>,
    pub fused_score: Option<f32>,
    pub graph_score: Option<f32>,
    pub scope: MemoryScope,
    pub summary: String,
    pub memory_type: MemoryType,
    pub importance: f32,
    pub tags: Vec<String>,
    pub facts: Vec<String>,
    pub source_event_key: String,
    pub embedding_source: String,
    pub embedding_model: Option<String>,
    pub relations: Vec<MemoryRelation>,
}

/// Public struct `MemorySearchResult` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchResult {
    pub query: String,
    pub scanned: usize,
    pub returned: usize,
    pub retrieval_backend: String,
    pub retrieval_reason_code: String,
    pub embedding_backend: String,
    pub embedding_reason_code: String,
    pub migrated_entries: usize,
    pub query_embedding_latency_ms: u64,
    pub ranking_latency_ms: u64,
    pub lexical_ranking_latency_ms: u64,
    pub fusion_latency_ms: u64,
    pub vector_candidates: usize,
    pub lexical_candidates: usize,
    pub baseline_hash_overlap: Option<usize>,
    pub baseline_vector_overlap: Option<usize>,
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
    storage_backend: MemoryStorageBackend,
    storage_path: Option<PathBuf>,
    backend_reason_code: String,
    backend_init_error: Option<String>,
}

impl FileMemoryStore {
    /// Creates a file-backed store rooted at `root_dir`.
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self::new_with_embedding_provider(root_dir, None)
    }

    /// Creates a file-backed store rooted at `root_dir` with optional embedding provider config.
    pub fn new_with_embedding_provider(
        root_dir: impl Into<PathBuf>,
        embedding_provider: Option<MemoryEmbeddingProviderConfig>,
    ) -> Self {
        let root_dir = root_dir.into();
        let resolved = resolve_memory_backend(&root_dir);
        let mut store = Self {
            root_dir,
            embedding_provider,
            storage_backend: resolved.backend,
            storage_path: resolved.storage_path,
            backend_reason_code: resolved.reason_code,
            backend_init_error: None,
        };
        if store.storage_backend == MemoryStorageBackend::Sqlite {
            if let Err(error) = store.maybe_import_legacy_jsonl_into_sqlite() {
                store.backend_init_error = Some(error.to_string());
                store.backend_reason_code = MEMORY_STORAGE_REASON_INIT_IMPORT_FAILED.to_string();
            }
        }
        store
    }

    /// Returns the store root directory.
    pub fn root_dir(&self) -> &Path {
        self.root_dir.as_path()
    }

    /// Returns the active storage backend.
    pub fn storage_backend(&self) -> MemoryStorageBackend {
        self.storage_backend
    }

    /// Returns the active storage backend label.
    pub fn storage_backend_label(&self) -> &'static str {
        self.storage_backend.label()
    }

    /// Returns the backend selection reason code.
    pub fn storage_backend_reason_code(&self) -> &str {
        self.backend_reason_code.as_str()
    }

    /// Returns the resolved storage file path, when applicable.
    pub fn storage_path(&self) -> Option<&Path> {
        self.storage_path.as_deref()
    }

    /// Imports JSONL artifacts into the active backend.
    pub fn import_jsonl_artifact(&self, source: &Path) -> Result<usize> {
        let records = load_records_jsonl(source)?;
        if records.is_empty() {
            return Ok(0);
        }

        self.ensure_backend_ready()?;
        match self.storage_backend {
            MemoryStorageBackend::Jsonl => {
                for record in &records {
                    append_record_jsonl(self.storage_path_required()?, record)?;
                }
                Ok(records.len())
            }
            MemoryStorageBackend::Sqlite => {
                let mut connection = open_memory_sqlite_connection(self.storage_path_required()?)?;
                initialize_memory_sqlite_schema(&connection)?;
                let transaction = connection.transaction()?;
                for record in &records {
                    let encoded =
                        serde_json::to_string(record).context("failed to encode memory record")?;
                    transaction.execute(
                        r#"
                        INSERT INTO memory_records (memory_id, updated_unix_ms, record_json)
                        VALUES (?1, ?2, ?3)
                        "#,
                        params![record.entry.memory_id, record.updated_unix_ms, encoded],
                    )?;
                }
                transaction.commit()?;
                Ok(records.len())
            }
        }
    }

    /// Writes or updates a memory entry under `scope`.
    pub fn write_entry(
        &self,
        scope: &MemoryScope,
        entry: MemoryEntry,
    ) -> Result<MemoryWriteResult> {
        self.write_entry_with_metadata(scope, entry, None, None)
    }

    /// Writes or updates a memory entry with optional typed-memory metadata.
    pub fn write_entry_with_metadata(
        &self,
        scope: &MemoryScope,
        entry: MemoryEntry,
        memory_type: Option<MemoryType>,
        importance: Option<f32>,
    ) -> Result<MemoryWriteResult> {
        self.write_entry_with_metadata_and_relations(scope, entry, memory_type, importance, &[])
    }

    /// Writes or updates a memory entry with metadata and explicit relations.
    pub fn write_entry_with_metadata_and_relations(
        &self,
        scope: &MemoryScope,
        entry: MemoryEntry,
        memory_type: Option<MemoryType>,
        importance: Option<f32>,
        relations: &[MemoryRelationInput],
    ) -> Result<MemoryWriteResult> {
        let normalized_scope = normalize_scope(scope);
        let normalized_entry = normalize_entry(entry)?;
        let resolved_memory_type = memory_type.unwrap_or_default();
        let resolved_importance = match importance {
            Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => value,
            Some(value) => {
                bail!("importance must be within 0.0..=1.0 (received {value})")
            }
            None => resolved_memory_type.default_importance(),
        };
        let existing_records = self.load_latest_records()?;
        let known_memory_ids = existing_records
            .iter()
            .map(|record| record.entry.memory_id.clone())
            .collect::<BTreeSet<_>>();
        let normalized_relations = normalize_relations(
            normalized_entry.memory_id.as_str(),
            relations,
            &known_memory_ids,
        )?;

        let created = existing_records.iter().all(|record| {
            record.entry.memory_id != normalized_entry.memory_id || record.scope != normalized_scope
        });

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
            memory_type: resolved_memory_type,
            importance: resolved_importance,
            embedding_source: computed_embedding.backend,
            embedding_model: computed_embedding.model,
            embedding_vector: computed_embedding.vector,
            embedding_reason_code: computed_embedding.reason_code,
            last_accessed_at_unix_ms: 0,
            access_count: 0,
            forgotten: false,
            relations: normalized_relations,
        };
        self.append_record_backend(&record)?;
        Ok(MemoryWriteResult { record, created })
    }

    /// Marks the latest active memory record as forgotten without removing historical data.
    pub fn soft_delete_entry(
        &self,
        memory_id: &str,
        scope_filter: Option<&MemoryScopeFilter>,
    ) -> Result<Option<RuntimeMemoryRecord>> {
        let normalized_memory_id = memory_id.trim();
        if normalized_memory_id.is_empty() {
            bail!("memory_id must not be empty");
        }
        let records = self.load_latest_records_including_forgotten()?;
        let Some(existing) = records.into_iter().find(|record| {
            record.entry.memory_id == normalized_memory_id
                && !record.forgotten
                && scope_filter
                    .map(|filter| filter.matches_scope(&record.scope))
                    .unwrap_or(true)
        }) else {
            return Ok(None);
        };

        let mut forgotten_record = existing;
        forgotten_record.updated_unix_ms = current_unix_timestamp_ms();
        forgotten_record.forgotten = true;
        self.append_record_backend(&forgotten_record)?;
        Ok(Some(forgotten_record))
    }

    pub(super) fn touch_entry_access(
        &self,
        record: &RuntimeMemoryRecord,
    ) -> Result<RuntimeMemoryRecord> {
        let mut touched = record.clone();
        let now_unix_ms = current_unix_timestamp_ms();
        touched.updated_unix_ms = now_unix_ms;
        touched.last_accessed_at_unix_ms = now_unix_ms;
        touched.access_count = touched.access_count.saturating_add(1);
        self.append_record_backend(&touched)?;
        Ok(touched)
    }

    fn ensure_backend_ready(&self) -> Result<()> {
        if let Some(error) = &self.backend_init_error {
            bail!(
                "memory storage backend initialization failed (reason_code={}): {}",
                self.backend_reason_code,
                error
            );
        }
        Ok(())
    }

    fn storage_path_required(&self) -> Result<&Path> {
        self.storage_path.as_deref().ok_or_else(|| {
            anyhow!(
                "memory storage backend '{}' does not define a filesystem path",
                self.storage_backend.label()
            )
        })
    }

    fn append_record_backend(&self, record: &RuntimeMemoryRecord) -> Result<()> {
        self.ensure_backend_ready()?;
        match self.storage_backend {
            MemoryStorageBackend::Jsonl => {
                append_record_jsonl(self.storage_path_required()?, record)
            }
            MemoryStorageBackend::Sqlite => {
                append_record_sqlite(self.storage_path_required()?, record)
            }
        }
    }

    fn load_records_backend(&self) -> Result<Vec<RuntimeMemoryRecord>> {
        self.ensure_backend_ready()?;
        match self.storage_backend {
            MemoryStorageBackend::Jsonl => load_records_jsonl(self.storage_path_required()?),
            MemoryStorageBackend::Sqlite => load_records_sqlite(self.storage_path_required()?),
        }
    }

    fn load_relation_map_backend(&self) -> Result<HashMap<String, Vec<MemoryRelation>>> {
        self.ensure_backend_ready()?;
        match self.storage_backend {
            MemoryStorageBackend::Jsonl => Ok(HashMap::new()),
            MemoryStorageBackend::Sqlite => load_relation_map_sqlite(self.storage_path_required()?),
        }
    }

    fn maybe_import_legacy_jsonl_into_sqlite(&self) -> Result<usize> {
        if self.storage_backend != MemoryStorageBackend::Sqlite {
            return Ok(0);
        }
        let Some(sqlite_path) = self.storage_path.as_deref() else {
            return Ok(0);
        };
        let Some(legacy_path) = self.legacy_jsonl_import_path() else {
            return Ok(0);
        };
        if !legacy_path.exists() {
            return Ok(0);
        }

        let mut connection = open_memory_sqlite_connection(sqlite_path)?;
        initialize_memory_sqlite_schema(&connection)?;
        let existing_count: u64 = connection
            .query_row("SELECT COUNT(1) FROM memory_records", [], |row| row.get(0))
            .context("failed to inspect sqlite memory record count")?;
        if existing_count > 0 {
            return Ok(0);
        }

        let records = load_records_jsonl(&legacy_path)?;
        if records.is_empty() {
            return Ok(0);
        }

        let transaction = connection.transaction()?;
        for record in &records {
            let encoded =
                serde_json::to_string(record).context("failed to encode memory sqlite record")?;
            transaction.execute(
                r#"
                INSERT INTO memory_records (memory_id, updated_unix_ms, record_json)
                VALUES (?1, ?2, ?3)
                "#,
                params![record.entry.memory_id, record.updated_unix_ms, encoded],
            )?;
        }
        transaction.commit()?;
        Ok(records.len())
    }

    fn legacy_jsonl_import_path(&self) -> Option<PathBuf> {
        match self.storage_backend {
            MemoryStorageBackend::Jsonl => None,
            MemoryStorageBackend::Sqlite => {
                if self.root_dir.extension().and_then(|value| value.to_str()) == Some("sqlite")
                    || self.root_dir.extension().and_then(|value| value.to_str()) == Some("db")
                {
                    let legacy = self.root_dir.with_extension("jsonl");
                    if self.storage_path.as_deref() == Some(legacy.as_path()) {
                        None
                    } else {
                        Some(legacy)
                    }
                } else {
                    let legacy = self.root_dir.join(MEMORY_RUNTIME_ENTRIES_FILE_NAME);
                    if self.storage_path.as_deref() == Some(legacy.as_path()) {
                        None
                    } else {
                        Some(legacy)
                    }
                }
            }
        }
    }
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

fn normalize_relations(
    source_memory_id: &str,
    relations: &[MemoryRelationInput],
    known_memory_ids: &BTreeSet<String>,
) -> Result<Vec<MemoryRelation>> {
    if relations.is_empty() {
        return Ok(Vec::new());
    }

    let mut deduped = BTreeMap::<(String, String), MemoryRelation>::new();
    for relation in relations {
        let target_id = relation.target_id.trim();
        if target_id.is_empty() {
            bail!("{MEMORY_INVALID_RELATION_REASON_CODE}: relation target_id must not be empty");
        }
        if target_id == source_memory_id {
            bail!("{MEMORY_INVALID_RELATION_REASON_CODE}: source_id and target_id must differ");
        }
        if !known_memory_ids.contains(target_id) {
            bail!(
                "{MEMORY_INVALID_RELATION_REASON_CODE}: unknown target_id '{}'",
                target_id
            );
        }

        let relation_type = normalize_relation_type(relation.relation_type.as_deref())?;
        let raw_weight = relation.weight.unwrap_or(1.0);
        if !raw_weight.is_finite() {
            bail!("{MEMORY_INVALID_RELATION_REASON_CODE}: relation weight must be finite");
        }
        if !(0.0..=1.0).contains(&raw_weight) {
            bail!(
                "{MEMORY_INVALID_RELATION_REASON_CODE}: relation weight must be in range 0.0..=1.0"
            );
        }
        let effective_weight = raw_weight.clamp(0.0, 1.0);
        deduped.insert(
            (target_id.to_string(), relation_type.clone()),
            MemoryRelation {
                target_id: target_id.to_string(),
                relation_type,
                weight: raw_weight,
                effective_weight,
            },
        );
    }

    Ok(deduped.into_values().collect())
}

fn normalize_relation_type(value: Option<&str>) -> Result<String> {
    let normalized = value
        .unwrap_or(MEMORY_RELATION_TYPE_DEFAULT)
        .trim()
        .to_ascii_lowercase();
    if normalized.is_empty() {
        bail!("{MEMORY_INVALID_RELATION_REASON_CODE}: relation_type must not be empty");
    }
    if MEMORY_RELATION_TYPE_VALUES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        bail!(
            "{MEMORY_INVALID_RELATION_REASON_CODE}: unsupported relation_type '{}'",
            normalized
        );
    }
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
        embed_text_vector, importance_rank_multiplier, rank_text_candidates,
        rank_text_candidates_bm25, FileMemoryStore, MemoryEmbeddingProviderConfig,
        MemoryLifecycleMaintenancePolicy, MemoryLifecycleMaintenanceResult, MemoryRelationInput,
        MemoryScopeFilter, MemorySearchOptions, MemoryStorageBackend, MemoryType,
        RankedTextCandidate, RuntimeMemoryRecord, MEMORY_BACKEND_ENV,
        MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK,
    };
    use crate::memory_contract::{MemoryEntry, MemoryScope};
    use httpmock::{Method::POST, MockServer};
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    struct ScopedEnvVar {
        key: &'static str,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            match self.previous.as_deref() {
                Some(previous) => std::env::set_var(self.key, previous),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn memory_backend_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn lifecycle_scope() -> MemoryScope {
        MemoryScope {
            workspace_id: "workspace-lifecycle".to_string(),
            channel_id: "channel-lifecycle".to_string(),
            actor_id: "assistant".to_string(),
        }
    }

    fn lifecycle_entry(memory_id: &str, summary: &str) -> MemoryEntry {
        MemoryEntry {
            memory_id: memory_id.to_string(),
            summary: summary.to_string(),
            tags: vec!["lifecycle".to_string()],
            facts: vec!["phase=2".to_string()],
            source_event_key: format!("evt-{memory_id}"),
            recency_weight_bps: 0,
            confidence_bps: 1_000,
        }
    }

    fn append_lifecycle_snapshot(
        store: &FileMemoryStore,
        memory_id: &str,
        updated_unix_ms: u64,
        last_accessed_at_unix_ms: u64,
        importance: f32,
    ) {
        let mut record = store
            .list_latest_records(None, usize::MAX)
            .expect("list latest lifecycle records")
            .into_iter()
            .find(|candidate| candidate.entry.memory_id == memory_id)
            .expect("record exists for lifecycle snapshot");
        record.updated_unix_ms = updated_unix_ms;
        record.last_accessed_at_unix_ms = last_accessed_at_unix_ms;
        record.importance = importance;
        store
            .append_record_backend(&record)
            .expect("append lifecycle snapshot");
    }

    #[test]
    fn spec_2455_c01_lifecycle_maintenance_policy_defaults_and_empty_result_are_deterministic() {
        let policy = MemoryLifecycleMaintenancePolicy::default();
        assert_eq!(
            policy.stale_after_unix_ms,
            7_u64 * 24 * 60 * 60 * 1_000,
            "default stale threshold should be seven days"
        );
        assert!((policy.decay_rate - 0.9).abs() <= 0.000_001);
        assert!((policy.prune_importance_floor - 0.1).abs() <= 0.000_001);
        assert!(policy.orphan_cleanup_enabled);
        assert!((policy.orphan_importance_floor - 0.2).abs() <= 0.000_001);

        let zero = MemoryLifecycleMaintenanceResult::default();
        assert_eq!(zero.scanned_records, 0);
        assert_eq!(zero.decayed_records, 0);
        assert_eq!(zero.pruned_records, 0);
        assert_eq!(zero.orphan_forgotten_records, 0);
        assert_eq!(zero.identity_exempt_records, 0);
        assert_eq!(zero.updated_records, 0);
        assert_eq!(zero.unchanged_records, 0);

        let store = FileMemoryStore::new(tempdir().expect("tempdir").path());
        let run = store
            .run_lifecycle_maintenance(&policy, 10_000)
            .expect("run lifecycle maintenance");
        assert_eq!(run.scanned_records, 0);
        assert_eq!(run.updated_records, 0);
    }

    #[test]
    fn spec_2455_c02_stale_non_identity_records_decay_while_identity_is_exempt() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = lifecycle_scope();
        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-stale-observation", "stale observation"),
                Some(MemoryType::Observation),
                Some(0.6),
            )
            .expect("write stale observation");
        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-stale-identity", "stale identity"),
                Some(MemoryType::Identity),
                Some(0.2),
            )
            .expect("write stale identity");

        append_lifecycle_snapshot(&store, "memory-stale-observation", 1_000, 1_000, 0.6);
        append_lifecycle_snapshot(&store, "memory-stale-identity", 1_000, 1_000, 0.2);

        let run = store
            .run_lifecycle_maintenance(
                &MemoryLifecycleMaintenancePolicy {
                    stale_after_unix_ms: 1_000,
                    decay_rate: 0.5,
                    prune_importance_floor: 0.05,
                    orphan_cleanup_enabled: false,
                    orphan_importance_floor: 0.0,
                },
                10_000,
            )
            .expect("run lifecycle maintenance");
        assert_eq!(run.scanned_records, 2);
        assert_eq!(run.decayed_records, 1);
        assert_eq!(run.identity_exempt_records, 1);
        assert_eq!(run.pruned_records, 0);

        let latest = store
            .list_latest_records(None, usize::MAX)
            .expect("list post-maintenance");
        let observation = latest
            .iter()
            .find(|record| record.entry.memory_id == "memory-stale-observation")
            .expect("observation record present");
        assert!((observation.importance - 0.3).abs() <= 0.000_001);
        let identity = latest
            .iter()
            .find(|record| record.entry.memory_id == "memory-stale-identity")
            .expect("identity record present");
        assert!((identity.importance - 0.2).abs() <= 0.000_001);
        assert!(!identity.forgotten);
    }

    #[test]
    fn spec_2455_c03_prune_floor_marks_low_importance_records_as_forgotten() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = lifecycle_scope();
        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-prune-low", "low importance"),
                Some(MemoryType::Observation),
                Some(0.05),
            )
            .expect("write low importance memory");
        append_lifecycle_snapshot(&store, "memory-prune-low", 9_500, 9_500, 0.05);

        let run = store
            .run_lifecycle_maintenance(
                &MemoryLifecycleMaintenancePolicy {
                    stale_after_unix_ms: 60_000,
                    decay_rate: 1.0,
                    prune_importance_floor: 0.1,
                    orphan_cleanup_enabled: false,
                    orphan_importance_floor: 0.0,
                },
                10_000,
            )
            .expect("run lifecycle maintenance");
        assert_eq!(run.pruned_records, 1);

        let listed = store
            .list_latest_records(None, usize::MAX)
            .expect("list latest records after prune");
        assert!(
            listed
                .iter()
                .all(|record| record.entry.memory_id != "memory-prune-low"),
            "forgotten memory must be excluded from default list"
        );
        let read = store
            .read_entry("memory-prune-low", None)
            .expect("read after prune");
        assert!(
            read.is_none(),
            "forgotten memory must be excluded from default read"
        );
        let search = store
            .search("low importance", &MemorySearchOptions::default())
            .expect("search after prune");
        assert!(
            search
                .matches
                .iter()
                .all(|record| record.memory_id != "memory-prune-low"),
            "forgotten memory must be excluded from default search"
        );
    }

    #[test]
    fn regression_2455_prune_floor_boundary_keeps_equal_importance_active() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = lifecycle_scope();
        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-prune-boundary", "boundary importance"),
                Some(MemoryType::Observation),
                Some(0.1),
            )
            .expect("write boundary importance memory");

        let run = store
            .run_lifecycle_maintenance(
                &MemoryLifecycleMaintenancePolicy {
                    stale_after_unix_ms: u64::MAX,
                    decay_rate: 1.0,
                    prune_importance_floor: 0.1,
                    orphan_cleanup_enabled: false,
                    orphan_importance_floor: 0.0,
                },
                10_000,
            )
            .expect("run lifecycle maintenance");
        assert_eq!(run.pruned_records, 0);
        assert_eq!(run.updated_records, 0);
        assert_eq!(run.unchanged_records, 1);

        let listed = store
            .list_latest_records(None, usize::MAX)
            .expect("list latest records after boundary prune");
        assert!(
            listed
                .iter()
                .any(|record| record.entry.memory_id == "memory-prune-boundary"),
            "importance equal to prune floor must remain active"
        );
    }

    #[test]
    fn spec_2455_c04_orphan_cleanup_forgets_low_importance_orphans_only() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = lifecycle_scope();

        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-linked-target", "linked target"),
                Some(MemoryType::Goal),
                Some(0.9),
            )
            .expect("write linked target");
        store
            .write_entry_with_metadata_and_relations(
                &scope,
                lifecycle_entry("memory-linked-low", "linked low importance"),
                Some(MemoryType::Observation),
                Some(0.15),
                &[MemoryRelationInput {
                    target_id: "memory-linked-target".to_string(),
                    relation_type: Some("depends_on".to_string()),
                    weight: Some(1.0),
                }],
            )
            .expect("write linked low-importance record");
        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-orphan-low", "orphan low importance"),
                Some(MemoryType::Observation),
                Some(0.15),
            )
            .expect("write orphan low-importance record");

        let run = store
            .run_lifecycle_maintenance(
                &MemoryLifecycleMaintenancePolicy {
                    stale_after_unix_ms: u64::MAX,
                    decay_rate: 1.0,
                    prune_importance_floor: 0.1,
                    orphan_cleanup_enabled: true,
                    orphan_importance_floor: 0.2,
                },
                10_000,
            )
            .expect("run lifecycle maintenance");
        assert_eq!(run.orphan_forgotten_records, 1);
        assert_eq!(run.pruned_records, 0);

        let listed = store
            .list_latest_records(None, usize::MAX)
            .expect("list post orphan cleanup");
        assert!(
            listed
                .iter()
                .any(|record| record.entry.memory_id == "memory-linked-low"),
            "edge-connected low-importance memory should remain active"
        );
        assert!(
            listed
                .iter()
                .all(|record| record.entry.memory_id != "memory-orphan-low"),
            "orphan low-importance memory should be forgotten"
        );
    }

    #[test]
    fn spec_2455_c05_identity_records_are_exempt_from_decay_prune_and_orphan_cleanup() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = lifecycle_scope();
        store
            .write_entry_with_metadata(
                &scope,
                lifecycle_entry("memory-identity-critical", "identity memory"),
                Some(MemoryType::Identity),
                Some(0.01),
            )
            .expect("write identity memory");
        append_lifecycle_snapshot(&store, "memory-identity-critical", 1_000, 1_000, 0.01);

        let run = store
            .run_lifecycle_maintenance(
                &MemoryLifecycleMaintenancePolicy {
                    stale_after_unix_ms: 1_000,
                    decay_rate: 0.5,
                    prune_importance_floor: 0.1,
                    orphan_cleanup_enabled: true,
                    orphan_importance_floor: 0.2,
                },
                10_000,
            )
            .expect("run lifecycle maintenance");
        assert_eq!(run.identity_exempt_records, 1);
        assert_eq!(run.decayed_records, 0);
        assert_eq!(run.pruned_records, 0);
        assert_eq!(run.orphan_forgotten_records, 0);

        let listed = store
            .list_latest_records(None, usize::MAX)
            .expect("list post maintenance");
        let identity = listed
            .iter()
            .find(|record| record.entry.memory_id == "memory-identity-critical")
            .expect("identity record remains");
        assert!((identity.importance - 0.01).abs() <= 0.000_001);
        assert!(!identity.forgotten);
    }

    #[test]
    fn unit_lifecycle_maintenance_rejects_invalid_policy_values() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());

        let invalid_decay = store.run_lifecycle_maintenance(
            &MemoryLifecycleMaintenancePolicy {
                stale_after_unix_ms: 1_000,
                decay_rate: 1.5,
                prune_importance_floor: 0.1,
                orphan_cleanup_enabled: true,
                orphan_importance_floor: 0.2,
            },
            10_000,
        );
        assert!(invalid_decay.is_err(), "out-of-range decay_rate must fail");

        let invalid_prune_floor = store.run_lifecycle_maintenance(
            &MemoryLifecycleMaintenancePolicy {
                stale_after_unix_ms: 1_000,
                decay_rate: 0.9,
                prune_importance_floor: -0.1,
                orphan_cleanup_enabled: true,
                orphan_importance_floor: 0.2,
            },
            10_000,
        );
        assert!(
            invalid_prune_floor.is_err(),
            "negative prune_importance_floor must fail"
        );

        let invalid_orphan_floor = store.run_lifecycle_maintenance(
            &MemoryLifecycleMaintenancePolicy {
                stale_after_unix_ms: 1_000,
                decay_rate: 0.9,
                prune_importance_floor: 0.1,
                orphan_cleanup_enabled: true,
                orphan_importance_floor: 1.1,
            },
            10_000,
        );
        assert!(
            invalid_orphan_floor.is_err(),
            "out-of-range orphan_importance_floor must fail"
        );
    }

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
    fn unit_memory_type_parse_and_display_roundtrip() {
        let cases = [
            (MemoryType::Identity, "identity"),
            (MemoryType::Goal, "goal"),
            (MemoryType::Decision, "decision"),
            (MemoryType::Todo, "todo"),
            (MemoryType::Preference, "preference"),
            (MemoryType::Fact, "fact"),
            (MemoryType::Event, "event"),
            (MemoryType::Observation, "observation"),
        ];
        for (memory_type, label) in cases {
            let uppercase = label.to_ascii_uppercase();
            let padded = format!(" {label} ");
            assert_eq!(memory_type.as_str(), label);
            assert_eq!(MemoryType::parse(label), Some(memory_type));
            assert_eq!(MemoryType::parse(uppercase.as_str()), Some(memory_type));
            assert_eq!(MemoryType::parse(padded.as_str()), Some(memory_type));
        }
        assert_eq!(MemoryType::parse("unknown"), None);
    }

    #[test]
    fn unit_memory_type_default_importance_profile_and_record_defaults() {
        let expectations = [
            (MemoryType::Identity, 1.0f32),
            (MemoryType::Goal, 0.9f32),
            (MemoryType::Decision, 0.85f32),
            (MemoryType::Todo, 0.8f32),
            (MemoryType::Preference, 0.7f32),
            (MemoryType::Fact, 0.65f32),
            (MemoryType::Event, 0.55f32),
            (MemoryType::Observation, 0.3f32),
        ];
        for (memory_type, expected_importance) in expectations {
            assert!(
                (memory_type.default_importance() - expected_importance).abs() <= 0.000_001,
                "default importance mismatch for {}",
                memory_type.as_str()
            );
        }
        assert_eq!(MemoryType::default(), MemoryType::Observation);

        let decoded: RuntimeMemoryRecord = serde_json::from_value(json!({
            "schema_version": 1,
            "updated_unix_ms": 123,
            "scope": {
                "workspace_id": "workspace",
                "channel_id": "channel",
                "actor_id": "assistant"
            },
            "entry": {
                "memory_id": "memory-default",
                "summary": "default metadata",
                "tags": [],
                "facts": [],
                "source_event_key": "evt-default",
                "recency_weight_bps": 0,
                "confidence_bps": 1000
            }
        }))
        .expect("deserialize runtime record with defaults");
        assert_eq!(decoded.memory_type, MemoryType::Observation);
        assert!((decoded.importance - 0.3).abs() <= 0.000_001);
        assert!(decoded.relations.is_empty());
        assert_eq!(decoded.last_accessed_at_unix_ms, 0);
        assert_eq!(decoded.access_count, 0);
        assert!(!decoded.forgotten);
    }

    #[test]
    fn unit_memory_search_options_serde_default_sets_graph_signal_weight() {
        let decoded: MemorySearchOptions = serde_json::from_value(json!({
            "scope": {
                "workspace_id": null,
                "channel_id": null,
                "actor_id": null
            },
            "limit": 5,
            "embedding_dimensions": 128,
            "min_similarity": 0.55,
            "enable_hybrid_retrieval": false,
            "bm25_k1": 1.2,
            "bm25_b": 0.75,
            "bm25_min_score": 0.0,
            "rrf_k": 60,
            "rrf_vector_weight": 1.0,
            "rrf_lexical_weight": 1.0,
            "enable_embedding_migration": true,
            "benchmark_against_hash": false,
            "benchmark_against_vector_only": false
        }))
        .expect("deserialize search options with graph default");
        assert!((decoded.graph_signal_weight - 0.25).abs() <= 0.000_001);
    }

    #[test]
    fn unit_normalize_relations_validates_target_type_and_weight() {
        let known_memory_ids = std::collections::BTreeSet::from([String::from("target-memory")]);
        let valid = super::normalize_relations(
            "source-memory",
            &[super::MemoryRelationInput {
                target_id: "target-memory".to_string(),
                relation_type: Some("depends_on".to_string()),
                weight: Some(0.75),
            }],
            &known_memory_ids,
        )
        .expect("valid relation normalization");
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].target_id, "target-memory");
        assert_eq!(valid[0].relation_type, "depends_on");
        assert!((valid[0].weight - 0.75).abs() <= 0.000_001);
        assert!((valid[0].effective_weight - 0.75).abs() <= 0.000_001);

        let default_type = super::normalize_relations(
            "source-memory",
            &[super::MemoryRelationInput {
                target_id: "target-memory".to_string(),
                relation_type: None,
                weight: None,
            }],
            &known_memory_ids,
        )
        .expect("default relation type and weight");
        assert_eq!(default_type[0].relation_type, "relates_to");
        assert!((default_type[0].weight - 1.0).abs() <= 0.000_001);
        assert!((default_type[0].effective_weight - 1.0).abs() <= 0.000_001);

        let unknown_target = super::normalize_relations(
            "source-memory",
            &[super::MemoryRelationInput {
                target_id: "missing-target".to_string(),
                relation_type: Some("depends_on".to_string()),
                weight: Some(0.5),
            }],
            &known_memory_ids,
        )
        .expect_err("unknown target must fail");
        assert!(unknown_target
            .to_string()
            .contains("memory_invalid_relation"));

        let self_target_known = std::collections::BTreeSet::from([String::from("source-memory")]);
        let self_target = super::normalize_relations(
            "source-memory",
            &[super::MemoryRelationInput {
                target_id: "source-memory".to_string(),
                relation_type: Some("depends_on".to_string()),
                weight: Some(0.5),
            }],
            &self_target_known,
        )
        .expect_err("self target must fail");
        assert!(self_target.to_string().contains("must differ"));

        let invalid_type = super::normalize_relations(
            "source-memory",
            &[super::MemoryRelationInput {
                target_id: "target-memory".to_string(),
                relation_type: Some("unknown".to_string()),
                weight: Some(0.5),
            }],
            &known_memory_ids,
        )
        .expect_err("invalid type must fail");
        assert!(invalid_type
            .to_string()
            .contains("unsupported relation_type"));

        let invalid_weight = super::normalize_relations(
            "source-memory",
            &[super::MemoryRelationInput {
                target_id: "target-memory".to_string(),
                relation_type: Some("depends_on".to_string()),
                weight: Some(1.5),
            }],
            &known_memory_ids,
        )
        .expect_err("invalid weight must fail");
        assert!(invalid_weight.to_string().contains("0.0..=1.0"));
    }

    #[test]
    fn regression_write_entry_with_relations_created_flag_tracks_scope_membership() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope_a = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "ops".to_string(),
            actor_id: "assistant".to_string(),
        };
        let scope_b = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "ops-secondary".to_string(),
            actor_id: "assistant".to_string(),
        };
        let entry = MemoryEntry {
            memory_id: "shared-memory".to_string(),
            summary: "shared summary".to_string(),
            tags: Vec::new(),
            facts: Vec::new(),
            source_event_key: "evt-shared".to_string(),
            recency_weight_bps: 0,
            confidence_bps: 1_000,
        };

        let first = store
            .write_entry_with_metadata_and_relations(
                &scope_a,
                entry.clone(),
                Some(MemoryType::Fact),
                Some(0.65),
                &[],
            )
            .expect("first write");
        assert!(first.created);

        let second_same_scope = store
            .write_entry_with_metadata_and_relations(
                &scope_a,
                entry.clone(),
                Some(MemoryType::Fact),
                Some(0.65),
                &[],
            )
            .expect("second write same scope");
        assert!(!second_same_scope.created);

        let third_other_scope = store
            .write_entry_with_metadata_and_relations(
                &scope_b,
                entry,
                Some(MemoryType::Fact),
                Some(0.65),
                &[],
            )
            .expect("third write other scope");
        assert!(third_other_scope.created);
    }

    #[test]
    fn integration_read_entry_hydrates_relations_from_sqlite_relation_table() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let sqlite_path = store.storage_path().expect("sqlite path").to_path_buf();
        let connection =
            super::open_memory_sqlite_connection(&sqlite_path).expect("open sqlite memory store");
        super::initialize_memory_sqlite_schema(&connection).expect("initialize schema");

        let source_json = json!({
            "schema_version": 1,
            "updated_unix_ms": 100,
            "scope": {
                "workspace_id": "workspace-a",
                "channel_id": "ops",
                "actor_id": "assistant"
            },
            "entry": {
                "memory_id": "source-legacy",
                "summary": "legacy source entry",
                "tags": [],
                "facts": [],
                "source_event_key": "evt-source",
                "recency_weight_bps": 0,
                "confidence_bps": 1000
            },
            "memory_type": "observation",
            "importance": 0.3,
            "embedding_source": "hash-fnv1a",
            "embedding_model": null,
            "embedding_vector": [0.1, 0.2],
            "embedding_reason_code": "memory_embedding_hash_only"
        })
        .to_string();
        connection
            .execute(
                r#"
                INSERT INTO memory_records (memory_id, updated_unix_ms, record_json)
                VALUES (?1, ?2, ?3)
                "#,
                rusqlite::params!["source-legacy", 100_u64, source_json],
            )
            .expect("insert source legacy record");

        let target_json = json!({
            "schema_version": 1,
            "updated_unix_ms": 90,
            "scope": {
                "workspace_id": "workspace-a",
                "channel_id": "ops",
                "actor_id": "assistant"
            },
            "entry": {
                "memory_id": "target-legacy",
                "summary": "legacy target entry",
                "tags": [],
                "facts": [],
                "source_event_key": "evt-target",
                "recency_weight_bps": 0,
                "confidence_bps": 1000
            },
            "memory_type": "goal",
            "importance": 1.0,
            "embedding_source": "hash-fnv1a",
            "embedding_model": null,
            "embedding_vector": [0.1, 0.2],
            "embedding_reason_code": "memory_embedding_hash_only"
        })
        .to_string();
        connection
            .execute(
                r#"
                INSERT INTO memory_records (memory_id, updated_unix_ms, record_json)
                VALUES (?1, ?2, ?3)
                "#,
                rusqlite::params!["target-legacy", 90_u64, target_json],
            )
            .expect("insert target legacy record");

        connection
            .execute(
                r#"
                INSERT INTO memory_relations (
                    source_memory_id,
                    target_memory_id,
                    relation_type,
                    weight,
                    effective_weight,
                    updated_unix_ms
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                rusqlite::params![
                    "source-legacy",
                    "target-legacy",
                    "depends_on",
                    0.7_f32,
                    0.7_f32,
                    100_u64
                ],
            )
            .expect("insert relation edge");

        let read = store
            .read_entry("source-legacy", None)
            .expect("read source")
            .expect("source exists");
        assert_eq!(read.relations.len(), 1);
        assert_eq!(read.relations[0].target_id, "target-legacy");
        assert_eq!(read.relations[0].relation_type, "depends_on");
        assert!((read.relations[0].effective_weight - 0.7).abs() <= 0.000_001);
    }

    #[test]
    fn unit_importance_rank_multiplier_clamps_to_expected_range() {
        assert!((importance_rank_multiplier(-1.0) - 1.0).abs() <= 0.000_001);
        assert!((importance_rank_multiplier(0.0) - 1.0).abs() <= 0.000_001);
        assert!((importance_rank_multiplier(0.5) - 1.5).abs() <= 0.000_001);
        assert!((importance_rank_multiplier(1.0) - 2.0).abs() <= 0.000_001);
        assert!((importance_rank_multiplier(3.0) - 2.0).abs() <= 0.000_001);
    }

    #[test]
    fn regression_write_entry_with_metadata_rejects_invalid_importance_range() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "channel-1".to_string(),
            actor_id: "assistant".to_string(),
        };
        let base_entry = MemoryEntry {
            memory_id: "memory-invalid-importance".to_string(),
            summary: "importance must remain bounded".to_string(),
            tags: vec!["validation".to_string()],
            facts: vec!["range=0..1".to_string()],
            source_event_key: "evt-invalid".to_string(),
            recency_weight_bps: 0,
            confidence_bps: 1_000,
        };

        let below = store.write_entry_with_metadata(
            &scope,
            base_entry.clone(),
            Some(MemoryType::Goal),
            Some(-0.01),
        );
        assert!(below.is_err());

        let above = store.write_entry_with_metadata(
            &scope,
            base_entry.clone(),
            Some(MemoryType::Goal),
            Some(1.01),
        );
        assert!(above.is_err());

        let nan = store.write_entry_with_metadata(
            &scope,
            base_entry.clone(),
            Some(MemoryType::Goal),
            Some(f32::NAN),
        );
        assert!(nan.is_err());

        let valid = store
            .write_entry_with_metadata(
                &scope,
                MemoryEntry {
                    memory_id: "memory-valid-importance".to_string(),
                    ..base_entry
                },
                Some(MemoryType::Goal),
                Some(0.95),
            )
            .expect("valid importance should write successfully");
        assert_eq!(valid.record.memory_type, MemoryType::Goal);
        assert!((valid.record.importance - 0.95).abs() <= 0.000_001);
    }

    #[test]
    fn integration_memory_search_importance_multiplier_prioritizes_high_importance_match() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "deploy".to_string(),
            actor_id: "assistant".to_string(),
        };

        let shared_summary = "release smoke checklist".to_string();
        let shared_tags = vec!["release".to_string()];
        let shared_facts = vec!["run smoke tests".to_string()];

        store
            .write_entry_with_metadata(
                &scope,
                MemoryEntry {
                    memory_id: "a-low".to_string(),
                    summary: shared_summary.clone(),
                    tags: shared_tags.clone(),
                    facts: shared_facts.clone(),
                    source_event_key: "evt-low".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Observation),
                Some(0.0),
            )
            .expect("write low importance");
        store
            .write_entry_with_metadata(
                &scope,
                MemoryEntry {
                    memory_id: "z-high".to_string(),
                    summary: shared_summary,
                    tags: shared_tags,
                    facts: shared_facts,
                    source_event_key: "evt-high".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Goal),
                Some(1.0),
            )
            .expect("write high importance");

        let result = store
            .search(
                "release smoke checklist",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 64,
                    min_similarity: 0.0,
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.0,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: false,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: false,
                },
            )
            .expect("search with importance ranking");

        assert_eq!(result.returned, 2);
        assert_eq!(result.matches[0].memory_id, "z-high");
        assert!(result.matches[0].score > result.matches[1].score);
        let low = result
            .matches
            .iter()
            .find(|item| item.memory_id == "a-low")
            .expect("low memory in ranked matches")
            .score;
        let high = result
            .matches
            .iter()
            .find(|item| item.memory_id == "z-high")
            .expect("high memory in ranked matches")
            .score;
        assert!(low > 0.0);
        let ratio = high / low;
        assert!(
            (ratio - 2.0).abs() <= 0.05,
            "importance multiplier ratio drifted: {ratio}"
        );
    }

    #[test]
    fn integration_migrate_records_to_provider_embeddings_reports_count_and_preserves_metadata() {
        let server = MockServer::start();
        let embeddings = server.mock(|when, then| {
            when.method(POST).path("/embeddings");
            then.status(200).json_body_obj(&json!({
                "data": [
                    { "embedding": [0.9, 0.1, 0.0, 0.0] },
                    { "embedding": [0.8, 0.2, 0.0, 0.0] }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "ops".to_string(),
            actor_id: "assistant".to_string(),
        };
        let seed_store = FileMemoryStore::new(temp.path());
        seed_store
            .write_entry_with_metadata(
                &scope,
                MemoryEntry {
                    memory_id: "memory-migrate-a".to_string(),
                    summary: "provider migration candidate a".to_string(),
                    tags: vec!["migration".to_string()],
                    facts: vec!["priority=high".to_string()],
                    source_event_key: "evt-migrate-a".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Goal),
                Some(0.9),
            )
            .expect("write migration candidate a");
        seed_store
            .write_entry_with_metadata(
                &scope,
                MemoryEntry {
                    memory_id: "memory-migrate-b".to_string(),
                    summary: "provider migration candidate b".to_string(),
                    tags: vec!["migration".to_string()],
                    facts: vec!["priority=medium".to_string()],
                    source_event_key: "evt-migrate-b".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Fact),
                Some(0.65),
            )
            .expect("write migration candidate b");

        let records = seed_store
            .list_latest_records(None, usize::MAX)
            .expect("list seeded records");
        assert_eq!(records.len(), 2);

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
        let migrated = provider_store
            .migrate_records_to_provider_embeddings(&records)
            .expect("migrate records to provider embeddings");
        assert_eq!(migrated, 2);
        embeddings.assert();

        let migrated_a = provider_store
            .read_entry("memory-migrate-a", None)
            .expect("read migrated a")
            .expect("migrated a exists");
        let migrated_b = provider_store
            .read_entry("memory-migrate-b", None)
            .expect("read migrated b")
            .expect("migrated b exists");
        assert_eq!(migrated_a.embedding_source, "provider-openai-compatible");
        assert_eq!(migrated_b.embedding_source, "provider-openai-compatible");
        assert_eq!(migrated_a.memory_type, MemoryType::Goal);
        assert_eq!(migrated_b.memory_type, MemoryType::Fact);
        assert!((migrated_a.importance - 0.9).abs() <= 0.000_001);
        assert!((migrated_b.importance - 0.65).abs() <= 0.000_001);
    }

    #[test]
    fn functional_memory_store_defaults_to_sqlite_backend_for_directory_roots() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path().join(".tau/memory"));
        assert_eq!(store.storage_backend(), MemoryStorageBackend::Sqlite);
        assert_eq!(
            store.storage_backend_reason_code(),
            "memory_storage_backend_default_sqlite"
        );
        assert!(store
            .storage_path()
            .expect("sqlite storage path")
            .ends_with("entries.sqlite"));
    }

    #[test]
    fn regression_memory_store_treats_postgres_env_backend_as_invalid_and_falls_back() {
        let _guard = memory_backend_env_lock()
            .lock()
            .expect("memory backend env lock");
        let _backend_env = ScopedEnvVar::set(MEMORY_BACKEND_ENV, "postgres");

        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path().join(".tau/memory"));
        assert_eq!(store.storage_backend(), MemoryStorageBackend::Sqlite);
        assert_eq!(
            store.storage_backend_reason_code(),
            MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK
        );
    }

    #[test]
    fn integration_memory_store_imports_legacy_jsonl_into_sqlite() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join(".tau/memory");
        let legacy_jsonl = root.join("entries.jsonl");
        let legacy_store = FileMemoryStore::new_with_embedding_provider(legacy_jsonl.clone(), None);
        let scope = MemoryScope {
            workspace_id: "workspace".to_string(),
            channel_id: "channel".to_string(),
            actor_id: "assistant".to_string(),
        };
        let entry = MemoryEntry {
            memory_id: "memory-legacy".to_string(),
            summary: "legacy-jsonl-entry".to_string(),
            tags: vec!["legacy".to_string()],
            facts: vec!["imported=true".to_string()],
            source_event_key: "evt-legacy".to_string(),
            recency_weight_bps: 0,
            confidence_bps: 1_000,
        };
        legacy_store
            .write_entry(&scope, entry)
            .expect("seed legacy jsonl");

        let sqlite_store = FileMemoryStore::new_with_embedding_provider(root.clone(), None);
        assert_eq!(sqlite_store.storage_backend(), MemoryStorageBackend::Sqlite);
        assert_eq!(
            sqlite_store.storage_backend_reason_code(),
            "memory_storage_backend_existing_jsonl"
        );
        let loaded = sqlite_store
            .read_entry("memory-legacy", None)
            .expect("read legacy")
            .expect("legacy should import");
        assert_eq!(loaded.entry.summary, "legacy-jsonl-entry");
        assert!(root.join("entries.sqlite").exists());
        assert!(legacy_jsonl.exists());
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
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.0,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: true,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: false,
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
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.0,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: true,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: false,
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
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.0,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: false,
                    benchmark_against_hash: true,
                    benchmark_against_vector_only: false,
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
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.0,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: false,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: false,
                },
            )
            .expect("regular search");

        assert!(benchmarked.baseline_hash_overlap.is_some());
        assert_eq!(regular.baseline_hash_overlap, None);
    }

    #[test]
    fn integration_memory_search_hybrid_returns_lexical_match_when_vector_filter_excludes() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "ops".to_string(),
            actor_id: "assistant".to_string(),
        };
        store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-hybrid".to_string(),
                    summary: "kubernetes incident playbook for oncall".to_string(),
                    tags: vec!["kubernetes".to_string()],
                    facts: vec!["incident escalation".to_string()],
                    source_event_key: "evt-hybrid".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write hybrid memory");
        store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-other".to_string(),
                    summary: "office kitchen cleanup schedule".to_string(),
                    tags: vec!["office".to_string()],
                    facts: vec!["cleanup rota".to_string()],
                    source_event_key: "evt-other".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write other memory");

        let vector_only = store
            .search(
                "kubernetes incident",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 64,
                    min_similarity: 1.1,
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.1,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: false,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: false,
                },
            )
            .expect("vector-only search");
        let hybrid = store
            .search(
                "kubernetes incident",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 64,
                    min_similarity: 1.1,
                    enable_hybrid_retrieval: true,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.1,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: false,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: true,
                },
            )
            .expect("hybrid search");

        assert_eq!(vector_only.returned, 0);
        assert_eq!(hybrid.returned, 1);
        assert_eq!(hybrid.matches[0].memory_id, "memory-hybrid");
        assert_eq!(hybrid.retrieval_backend, "hybrid-bm25-rrf");
        assert_eq!(
            hybrid.retrieval_reason_code,
            "memory_retrieval_hybrid_enabled"
        );
        assert!(hybrid.matches[0]
            .lexical_score
            .is_some_and(|score| score > 0.0));
        assert!(hybrid.matches[0].vector_score.is_none());
        assert!(hybrid.baseline_vector_overlap.is_some());
    }

    #[test]
    fn regression_memory_search_vector_only_matches_hash_baseline_order() {
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
                    memory_id: "memory-a".to_string(),
                    summary: "release checklist smoke tests".to_string(),
                    tags: vec!["release".to_string()],
                    facts: vec!["smoke".to_string()],
                    source_event_key: "evt-a".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write memory a");
        store
            .write_entry(
                &scope,
                MemoryEntry {
                    memory_id: "memory-b".to_string(),
                    summary: "deployment rollback strategy".to_string(),
                    tags: vec!["rollback".to_string()],
                    facts: vec!["rollback drill".to_string()],
                    source_event_key: "evt-b".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 0,
                },
            )
            .expect("write memory b");

        let result = store
            .search(
                "release smoke",
                &MemorySearchOptions {
                    scope: MemoryScopeFilter::default(),
                    limit: 5,
                    embedding_dimensions: 64,
                    min_similarity: 0.0,
                    enable_hybrid_retrieval: false,
                    bm25_k1: 1.2,
                    bm25_b: 0.75,
                    bm25_min_score: 0.0,
                    rrf_k: 60,
                    rrf_vector_weight: 1.0,
                    rrf_lexical_weight: 1.0,
                    graph_signal_weight: 0.25,
                    enable_embedding_migration: false,
                    benchmark_against_hash: false,
                    benchmark_against_vector_only: false,
                },
            )
            .expect("vector-only search");
        let records = store
            .list_latest_records(None, usize::MAX)
            .expect("list latest records");
        let baseline = rank_text_candidates(
            "release smoke",
            records
                .iter()
                .map(|record| RankedTextCandidate {
                    key: record.entry.memory_id.clone(),
                    text: format!(
                        "{}\n{}\n{}",
                        record.entry.summary,
                        record.entry.tags.join(" "),
                        record.entry.facts.join(" ")
                    ),
                })
                .collect::<Vec<_>>(),
            5,
            64,
            0.0,
        );
        let result_ids = result
            .matches
            .iter()
            .map(|item| item.memory_id.as_str())
            .collect::<Vec<_>>();
        let baseline_ids = baseline
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(result_ids, baseline_ids);
        assert_eq!(result.retrieval_backend, "vector-only");
        assert_eq!(result.retrieval_reason_code, "memory_retrieval_vector_only");
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

    #[test]
    fn unit_rank_text_candidates_bm25_prefers_exact_lexical_overlap() {
        let ranked = rank_text_candidates_bm25(
            "tokio runtime",
            vec![
                RankedTextCandidate {
                    key: "match".to_string(),
                    text: "tokio runtime troubleshooting checklist".to_string(),
                },
                RankedTextCandidate {
                    key: "other".to_string(),
                    text: "garden watering schedule".to_string(),
                },
            ],
            5,
            1.2,
            0.75,
            0.001,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].key, "match");
        assert!(ranked[0].score > 0.0);
    }
}
