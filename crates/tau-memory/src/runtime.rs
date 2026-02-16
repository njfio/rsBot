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
    load_records_sqlite, open_memory_sqlite_connection, resolve_memory_backend,
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
    pub enable_hybrid_retrieval: bool,
    pub bm25_k1: f32,
    pub bm25_b: f32,
    pub bm25_min_score: f32,
    pub rrf_k: usize,
    pub rrf_vector_weight: f32,
    pub rrf_lexical_weight: f32,
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
            enable_embedding_migration: true,
            benchmark_against_hash: false,
            benchmark_against_vector_only: false,
        }
    }
}

/// Public struct `MemorySearchMatch` used across Tau components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchMatch {
    pub memory_id: String,
    pub score: f32,
    pub vector_score: Option<f32>,
    pub lexical_score: Option<f32>,
    pub fused_score: Option<f32>,
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
        self.append_record_backend(&record)?;
        Ok(MemoryWriteResult { record, created })
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

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        embed_text_vector, rank_text_candidates, rank_text_candidates_bm25, FileMemoryStore,
        MemoryEmbeddingProviderConfig, MemoryScopeFilter, MemorySearchOptions,
        MemoryStorageBackend, RankedTextCandidate, MEMORY_BACKEND_ENV,
        MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK,
    };
    use crate::memory_contract::{MemoryEntry, MemoryScope};
    use httpmock::{Method::POST, MockServer};
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
