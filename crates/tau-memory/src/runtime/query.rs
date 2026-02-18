use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::{Instant, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};

use crate::memory_contract::MemoryEntry;

use super::{
    importance_rank_multiplier, rank_text_candidates, rank_text_candidates_bm25,
    reciprocal_rank_fuse, record_search_text, resize_and_normalize_embedding, FileMemoryStore,
    MemoryIngestionLlmOptions, MemoryIngestionOptions, MemoryIngestionResult,
    MemoryIngestionWatchPollingState, MemoryLifecycleMaintenancePolicy,
    MemoryLifecycleMaintenanceResult, MemoryScopeFilter, MemorySearchMatch, MemorySearchOptions,
    MemorySearchResult, MemoryStorageBackend, MemoryTree, MemoryTreeNode, MemoryType,
    RankedTextCandidate, RankedTextMatch, RuntimeMemoryRecord, MEMORY_EMBEDDING_REASON_HASH_ONLY,
    MEMORY_EMBEDDING_REASON_PROVIDER_FAILED, MEMORY_RETRIEVAL_BACKEND_HYBRID_BM25_RRF,
    MEMORY_RETRIEVAL_BACKEND_VECTOR_ONLY, MEMORY_RETRIEVAL_REASON_HYBRID_ENABLED,
    MEMORY_RETRIEVAL_REASON_VECTOR_ONLY,
};

const MEMORY_INGESTION_SOURCE_EVENT_KEY_PREFIX: &str = "ingestion:chunk:";
const MEMORY_INGESTION_DEFAULT_RECENCY_WEIGHT_BPS: u16 = 0;
const MEMORY_INGESTION_DEFAULT_CONFIDENCE_BPS: u16 = 1_000;
const MEMORY_INGESTION_WATCH_NO_CHANGE_REASON: &str = "ingestion_watch_poll_no_changes";
const MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON: &str = "ingestion_chunk_llm_parse_failed";
const MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON: &str = "ingestion_chunk_llm_request_failed";
const MEMORY_INGESTION_LLM_EMPTY_TOOL_CALLS_REASON: &str = "ingestion_chunk_llm_no_tool_calls";
const MEMORY_INGESTION_SUPPORTED_EXTENSIONS: &[&str] = &[
    "txt", "md", "json", "jsonl", "csv", "tsv", "log", "xml", "yaml", "yml", "toml",
];

#[derive(Debug, Clone)]
struct IngestionChunkWritePlan {
    memory_id: String,
    summary: String,
    tags: Vec<String>,
    facts: Vec<String>,
    memory_type: Option<MemoryType>,
    importance: Option<f32>,
}

impl FileMemoryStore {
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
        let matched = records.into_iter().find(|record| {
            record.entry.memory_id == normalized_memory_id
                && scope_filter
                    .map(|filter| filter.matches_scope(&record.scope))
                    .unwrap_or(true)
        });
        match matched {
            Some(record) => self.touch_entry_access(&record).map(Some),
            None => Ok(None),
        }
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

    /// Runs one deterministic ingestion pass through a worker-oriented entrypoint.
    #[tracing::instrument(
        name = "tau_memory.ingestion.worker_once",
        skip(self, options),
        fields(
            ingest_dir = %ingest_dir.display(),
            chunk_line_count = options.chunk_line_count,
            delete_source_on_success = options.delete_source_on_success
        )
    )]
    pub fn ingest_directory_worker_once(
        &self,
        ingest_dir: &Path,
        options: &MemoryIngestionOptions,
    ) -> Result<MemoryIngestionResult> {
        let result = self.ingest_directory_once_with_processor(ingest_dir, options, None)?;
        tracing::debug!(
            discovered_files = result.discovered_files,
            supported_files = result.supported_files,
            processed_files = result.processed_files,
            failed_files = result.failed_files,
            chunks_discovered = result.chunks_discovered,
            chunks_ingested = result.chunks_ingested,
            chunks_skipped_existing = result.chunks_skipped_existing,
            "completed ingestion worker run"
        );
        Ok(result)
    }

    /// Runs one deterministic ingestion pass where each chunk is extracted via LLM `memory_write` tool calls.
    #[tracing::instrument(
        name = "tau_memory.ingestion.worker_once_llm_memory_save",
        skip(self, options, llm_options),
        fields(
            ingest_dir = %ingest_dir.display(),
            chunk_line_count = options.chunk_line_count,
            delete_source_on_success = options.delete_source_on_success,
            provider = %llm_options.provider,
            model = %llm_options.model
        )
    )]
    pub fn ingest_directory_worker_once_with_llm_memory_save(
        &self,
        ingest_dir: &Path,
        options: &MemoryIngestionOptions,
        llm_options: &MemoryIngestionLlmOptions,
    ) -> Result<MemoryIngestionResult> {
        let result =
            self.ingest_directory_once_with_processor(ingest_dir, options, Some(llm_options))?;
        tracing::debug!(
            discovered_files = result.discovered_files,
            supported_files = result.supported_files,
            processed_files = result.processed_files,
            failed_files = result.failed_files,
            chunks_discovered = result.chunks_discovered,
            chunks_ingested = result.chunks_ingested,
            chunks_skipped_existing = result.chunks_skipped_existing,
            "completed ingestion worker run with llm memory_save extraction"
        );
        Ok(result)
    }

    /// Executes one heartbeat-style watch poll cycle and ingests only when directory fingerprints change.
    #[tracing::instrument(
        name = "tau_memory.ingestion.watch_poll_once",
        skip(self, options, polling_state),
        fields(
            ingest_dir = %ingest_dir.display(),
            chunk_line_count = options.chunk_line_count,
            delete_source_on_success = options.delete_source_on_success
        )
    )]
    pub fn ingest_directory_watch_poll_once(
        &self,
        ingest_dir: &Path,
        options: &MemoryIngestionOptions,
        polling_state: &mut MemoryIngestionWatchPollingState,
    ) -> Result<MemoryIngestionResult> {
        if !ingest_dir.exists() {
            polling_state.file_fingerprints.clear();
            return self.ingest_directory_worker_once(ingest_dir, options);
        }
        let fingerprints = collect_ingest_directory_fingerprints(ingest_dir)?;
        if fingerprints == polling_state.file_fingerprints {
            return Ok(MemoryIngestionResult {
                diagnostics: vec![format!(
                    "{MEMORY_INGESTION_WATCH_NO_CHANGE_REASON}: path={}",
                    ingest_dir.display()
                )],
                ..MemoryIngestionResult::default()
            });
        }
        polling_state.file_fingerprints = fingerprints;
        self.ingest_directory_worker_once(ingest_dir, options)
    }

    /// Executes one watch poll cycle with LLM-based chunk extraction via `memory_write` tool calls.
    #[tracing::instrument(
        name = "tau_memory.ingestion.watch_poll_once_llm_memory_save",
        skip(self, options, polling_state, llm_options),
        fields(
            ingest_dir = %ingest_dir.display(),
            chunk_line_count = options.chunk_line_count,
            delete_source_on_success = options.delete_source_on_success,
            provider = %llm_options.provider,
            model = %llm_options.model
        )
    )]
    pub fn ingest_directory_watch_poll_once_with_llm_memory_save(
        &self,
        ingest_dir: &Path,
        options: &MemoryIngestionOptions,
        polling_state: &mut MemoryIngestionWatchPollingState,
        llm_options: &MemoryIngestionLlmOptions,
    ) -> Result<MemoryIngestionResult> {
        if !ingest_dir.exists() {
            polling_state.file_fingerprints.clear();
            return self.ingest_directory_worker_once_with_llm_memory_save(
                ingest_dir,
                options,
                llm_options,
            );
        }
        let fingerprints = collect_ingest_directory_fingerprints(ingest_dir)?;
        if fingerprints == polling_state.file_fingerprints {
            return Ok(MemoryIngestionResult {
                diagnostics: vec![format!(
                    "{MEMORY_INGESTION_WATCH_NO_CHANGE_REASON}: path={}",
                    ingest_dir.display()
                )],
                ..MemoryIngestionResult::default()
            });
        }
        polling_state.file_fingerprints = fingerprints;
        self.ingest_directory_worker_once_with_llm_memory_save(ingest_dir, options, llm_options)
    }

    /// Ingests supported files from `ingest_dir` once using deterministic chunk checkpoints.
    pub fn ingest_directory_once(
        &self,
        ingest_dir: &Path,
        options: &MemoryIngestionOptions,
    ) -> Result<MemoryIngestionResult> {
        self.ingest_directory_once_with_processor(ingest_dir, options, None)
    }

    fn ingest_directory_once_with_processor(
        &self,
        ingest_dir: &Path,
        options: &MemoryIngestionOptions,
        llm_options: Option<&MemoryIngestionLlmOptions>,
    ) -> Result<MemoryIngestionResult> {
        if options.chunk_line_count == 0 {
            bail!("ingestion chunk_line_count must be greater than zero");
        }
        if !ingest_dir.exists() {
            return Ok(MemoryIngestionResult {
                diagnostics: vec![format!(
                    "ingestion_directory_missing: path={}",
                    ingest_dir.display()
                )],
                ..MemoryIngestionResult::default()
            });
        }
        if !ingest_dir.is_dir() {
            bail!(
                "ingestion path must be a directory (received {})",
                ingest_dir.display()
            );
        }

        let mut result = MemoryIngestionResult::default();
        let mut known_checkpoint_keys = self.load_ingestion_checkpoint_keys()?;

        let mut source_paths = Vec::new();
        for entry in fs::read_dir(ingest_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                source_paths.push(path);
            }
        }
        source_paths.sort();

        for source_path in source_paths {
            result.discovered_files = result.discovered_files.saturating_add(1);

            let Some(extension) = supported_ingest_extension(source_path.as_path()) else {
                result.skipped_unsupported_files =
                    result.skipped_unsupported_files.saturating_add(1);
                result.diagnostics.push(format!(
                    "ingestion_file_unsupported_extension: path={}",
                    source_path.display()
                ));
                continue;
            };
            result.supported_files = result.supported_files.saturating_add(1);

            let raw = match fs::read_to_string(&source_path) {
                Ok(raw) => raw,
                Err(error) => {
                    result.failed_files = result.failed_files.saturating_add(1);
                    result.diagnostics.push(format!(
                        "ingestion_file_read_failed: path={} error={error}",
                        source_path.display()
                    ));
                    continue;
                }
            };

            let chunks = chunk_text_by_lines(raw.as_str(), options.chunk_line_count);
            result.chunks_discovered = result.chunks_discovered.saturating_add(chunks.len());

            let mut file_failed = false;
            for (chunk_index, chunk_text) in chunks.iter().enumerate() {
                let checkpoint_key =
                    ingestion_chunk_checkpoint_key(source_path.as_path(), chunk_index, chunk_text);
                if known_checkpoint_keys.contains(checkpoint_key.as_str()) {
                    result.chunks_skipped_existing =
                        result.chunks_skipped_existing.saturating_add(1);
                    continue;
                }

                let write_plans = if let Some(llm_options) = llm_options {
                    match llm_memory_write_plans_for_chunk(
                        source_path.as_path(),
                        chunk_index,
                        chunk_text,
                        extension.as_str(),
                        checkpoint_key.as_str(),
                        llm_options,
                    ) {
                        Ok(plans) => plans,
                        Err(error) => {
                            result.failed_files = result.failed_files.saturating_add(1);
                            result.diagnostics.push(format!(
                                "ingestion_chunk_llm_processing_failed: path={} chunk_index={} error={error}",
                                source_path.display(),
                                chunk_index
                            ));
                            file_failed = true;
                            break;
                        }
                    }
                } else {
                    vec![default_ingestion_chunk_write_plan(
                        source_path.as_path(),
                        chunk_index,
                        chunk_text,
                        extension.as_str(),
                        checkpoint_key.as_str(),
                    )]
                };

                let mut chunk_write_failed = false;
                for write_plan in write_plans {
                    let entry = MemoryEntry {
                        memory_id: write_plan.memory_id,
                        summary: write_plan.summary,
                        tags: write_plan.tags,
                        facts: write_plan.facts,
                        source_event_key: checkpoint_key.clone(),
                        recency_weight_bps: MEMORY_INGESTION_DEFAULT_RECENCY_WEIGHT_BPS,
                        confidence_bps: MEMORY_INGESTION_DEFAULT_CONFIDENCE_BPS,
                    };

                    if let Err(error) = self.write_entry_with_metadata(
                        &options.scope,
                        entry,
                        write_plan.memory_type,
                        write_plan.importance,
                    ) {
                        result.failed_files = result.failed_files.saturating_add(1);
                        result.diagnostics.push(format!(
                            "ingestion_chunk_write_failed: path={} chunk_index={} error={error}",
                            source_path.display(),
                            chunk_index
                        ));
                        chunk_write_failed = true;
                        break;
                    }
                }
                if chunk_write_failed {
                    file_failed = true;
                    break;
                }

                if let Err(error) = self.persist_ingestion_checkpoint(
                    checkpoint_key.as_str(),
                    source_path.as_path(),
                    chunk_index,
                ) {
                    result.failed_files = result.failed_files.saturating_add(1);
                    result.diagnostics.push(format!(
                        "ingestion_checkpoint_write_failed: path={} chunk_index={} error={error}",
                        source_path.display(),
                        chunk_index
                    ));
                    file_failed = true;
                    break;
                }

                known_checkpoint_keys.insert(checkpoint_key);
                result.chunks_ingested = result.chunks_ingested.saturating_add(1);
            }

            if file_failed {
                continue;
            }

            result.processed_files = result.processed_files.saturating_add(1);
            if options.delete_source_on_success {
                match fs::remove_file(&source_path) {
                    Ok(_) => {
                        result.deleted_files = result.deleted_files.saturating_add(1);
                    }
                    Err(error) => {
                        result.failed_files = result.failed_files.saturating_add(1);
                        result.diagnostics.push(format!(
                            "ingestion_file_delete_failed: path={} error={error}",
                            source_path.display()
                        ));
                    }
                }
            }
        }

        Ok(result)
    }

    fn load_ingestion_checkpoint_keys(&self) -> Result<BTreeSet<String>> {
        let mut known_checkpoint_keys = self
            .list_latest_records(None, usize::MAX)?
            .into_iter()
            .map(|record| record.entry.source_event_key)
            .collect::<BTreeSet<_>>();
        if self.storage_backend == MemoryStorageBackend::Sqlite {
            let sqlite_keys = super::backend::load_ingestion_checkpoint_keys_sqlite(
                self.storage_path_required()?,
            )?;
            known_checkpoint_keys.extend(sqlite_keys);
        }
        Ok(known_checkpoint_keys)
    }

    fn persist_ingestion_checkpoint(
        &self,
        checkpoint_key: &str,
        source_path: &Path,
        chunk_index: usize,
    ) -> Result<()> {
        if self.storage_backend != MemoryStorageBackend::Sqlite {
            return Ok(());
        }
        let digest = checkpoint_key
            .strip_prefix(MEMORY_INGESTION_SOURCE_EVENT_KEY_PREFIX)
            .unwrap_or(checkpoint_key);
        super::backend::upsert_ingestion_checkpoint_sqlite(
            self.storage_path_required()?,
            checkpoint_key,
            digest,
            source_path,
            chunk_index,
            super::current_unix_timestamp_ms(),
        )
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
        let has_query_embedding = computed_query
            .vector
            .iter()
            .any(|component| *component != 0.0);

        let ranking_started = Instant::now();
        let mut vector_ranked = if has_query_embedding {
            by_memory_id
                .iter()
                .filter_map(|(memory_id, record)| {
                    let candidate_embedding = if record.embedding_vector.is_empty() {
                        super::embed_text_vector(
                            record_search_text(record).as_str(),
                            options.embedding_dimensions,
                        )
                    } else {
                        resize_and_normalize_embedding(
                            record.embedding_vector.as_slice(),
                            options.embedding_dimensions,
                        )
                    };
                    let score =
                        super::cosine_similarity(&computed_query.vector, &candidate_embedding);
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
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        vector_ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.key.cmp(&right.key))
        });

        let mut lexical_ranking_latency_ms = 0u64;
        let mut lexical_ranked = Vec::new();
        if options.enable_hybrid_retrieval {
            let lexical_started = Instant::now();
            lexical_ranked = rank_text_candidates_bm25(
                normalized_query,
                by_memory_id
                    .iter()
                    .map(|(memory_id, record)| RankedTextCandidate {
                        key: memory_id.clone(),
                        text: record_search_text(record),
                    })
                    .collect::<Vec<_>>(),
                by_memory_id.len(),
                options.bm25_k1,
                options.bm25_b,
                options.bm25_min_score,
            );
            lexical_ranking_latency_ms = lexical_started.elapsed().as_millis() as u64;
        }

        let mut vector_scores = HashMap::new();
        for item in &vector_ranked {
            vector_scores.insert(item.key.clone(), item.score);
        }
        let mut lexical_scores = HashMap::new();
        for item in &lexical_ranked {
            lexical_scores.insert(item.key.clone(), item.score);
        }

        let vector_candidates = vector_ranked.len();
        let lexical_candidates = lexical_ranked.len();

        let mut fusion_latency_ms = 0u64;
        let mut fused_scores = HashMap::new();
        let mut ranked = if options.enable_hybrid_retrieval {
            let fusion_started = Instant::now();
            let fused = reciprocal_rank_fuse(
                &vector_ranked,
                &lexical_ranked,
                options.limit,
                options.rrf_k,
                options.rrf_vector_weight,
                options.rrf_lexical_weight,
            );
            fusion_latency_ms = fusion_started.elapsed().as_millis() as u64;
            for item in &fused {
                fused_scores.insert(item.key.clone(), item.score);
            }
            fused
        } else {
            vector_ranked.clone()
        };
        let graph_scores = compute_graph_scores(&by_memory_id);
        let safe_graph_weight = options.graph_signal_weight.max(0.0);
        for item in &mut ranked {
            if let Some(record) = by_memory_id.get(item.key.as_str()) {
                item.score *= importance_rank_multiplier(record.importance);
            }
            if let Some(graph_score) = graph_scores.get(item.key.as_str()) {
                item.score += safe_graph_weight * graph_score;
            }
        }
        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.key.cmp(&right.key))
        });

        ranked.truncate(options.limit);
        let ranking_latency_ms = ranking_started.elapsed().as_millis() as u64;

        let mut touched_records = HashMap::<String, RuntimeMemoryRecord>::new();
        for item in &ranked {
            if let Some(record) = by_memory_id.get(item.key.as_str()) {
                let touched = self.touch_entry_access(record)?;
                touched_records.insert(item.key.clone(), touched);
            }
        }

        let mut matches = Vec::with_capacity(ranked.len());
        for item in &ranked {
            let Some(record) = touched_records
                .get(item.key.as_str())
                .or_else(|| by_memory_id.get(&item.key))
            else {
                continue;
            };
            matches.push(MemorySearchMatch {
                memory_id: record.entry.memory_id.clone(),
                score: item.score,
                vector_score: vector_scores.get(item.key.as_str()).copied(),
                lexical_score: lexical_scores.get(item.key.as_str()).copied(),
                fused_score: options
                    .enable_hybrid_retrieval
                    .then(|| fused_scores.get(item.key.as_str()).copied())
                    .flatten(),
                graph_score: graph_scores.get(item.key.as_str()).copied(),
                scope: record.scope.clone(),
                summary: record.entry.summary.clone(),
                memory_type: record.memory_type,
                importance: record.importance,
                tags: record.entry.tags.clone(),
                facts: record.entry.facts.clone(),
                source_event_key: record.entry.source_event_key.clone(),
                embedding_source: record.embedding_source.clone(),
                embedding_model: record.embedding_model.clone(),
                relations: record.relations.clone(),
            });
        }
        let selected = matches
            .iter()
            .map(|item| item.memory_id.as_str())
            .collect::<BTreeSet<_>>();

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
            Some(
                baseline
                    .into_iter()
                    .filter(|candidate| selected.contains(candidate.key.as_str()))
                    .count(),
            )
        } else {
            None
        };
        let baseline_vector_overlap = if options.benchmark_against_vector_only {
            Some(
                vector_ranked
                    .iter()
                    .take(options.limit)
                    .filter(|candidate| selected.contains(candidate.key.as_str()))
                    .count(),
            )
        } else {
            None
        };
        let retrieval_backend = if options.enable_hybrid_retrieval {
            MEMORY_RETRIEVAL_BACKEND_HYBRID_BM25_RRF.to_string()
        } else {
            MEMORY_RETRIEVAL_BACKEND_VECTOR_ONLY.to_string()
        };
        let retrieval_reason_code = if options.enable_hybrid_retrieval {
            MEMORY_RETRIEVAL_REASON_HYBRID_ENABLED.to_string()
        } else {
            MEMORY_RETRIEVAL_REASON_VECTOR_ONLY.to_string()
        };

        Ok(MemorySearchResult {
            query: normalized_query.to_string(),
            scanned: by_memory_id.len(),
            returned: matches.len(),
            retrieval_backend,
            retrieval_reason_code,
            embedding_backend: computed_query.backend,
            embedding_reason_code,
            migrated_entries,
            query_embedding_latency_ms,
            ranking_latency_ms,
            lexical_ranking_latency_ms,
            fusion_latency_ms,
            vector_candidates,
            lexical_candidates,
            baseline_hash_overlap,
            baseline_vector_overlap,
            matches,
        })
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

    /// Applies lifecycle maintenance (decay/prune/orphan cleanup) to active records.
    pub fn run_lifecycle_maintenance(
        &self,
        policy: &MemoryLifecycleMaintenancePolicy,
        now_unix_ms: u64,
    ) -> Result<MemoryLifecycleMaintenanceResult> {
        validate_lifecycle_maintenance_policy(policy)?;

        let active_records = self
            .load_latest_records_including_forgotten()?
            .into_iter()
            .filter(|record| !record.forgotten)
            .collect::<Vec<_>>();
        let active_ids = active_records
            .iter()
            .map(|record| record.entry.memory_id.clone())
            .collect::<BTreeSet<_>>();
        let mut has_outgoing = BTreeSet::<String>::new();
        let mut has_incoming = BTreeSet::<String>::new();
        for record in &active_records {
            for relation in &record.relations {
                if relation.effective_weight <= 0.0 {
                    continue;
                }
                if !active_ids.contains(relation.target_id.as_str()) {
                    continue;
                }
                has_outgoing.insert(record.entry.memory_id.clone());
                has_incoming.insert(relation.target_id.clone());
            }
        }
        let duplicate_forgotten_ids = if policy.duplicate_cleanup_enabled {
            collect_duplicate_memory_ids(
                active_records.as_slice(),
                policy.duplicate_similarity_threshold,
            )
        } else {
            BTreeSet::new()
        };

        let mut result = MemoryLifecycleMaintenanceResult::default();
        for record in active_records {
            result.scanned_records = result.scanned_records.saturating_add(1);
            if record.memory_type == MemoryType::Identity {
                result.identity_exempt_records = result.identity_exempt_records.saturating_add(1);
                result.unchanged_records = result.unchanged_records.saturating_add(1);
                continue;
            }

            let mut updated_record = record.clone();
            let mut changed = false;
            let clamped_importance = updated_record.importance.clamp(0.0, 1.0);
            if updated_record.importance != clamped_importance {
                updated_record.importance = clamped_importance;
                changed = true;
            }

            let memory_id = updated_record.entry.memory_id.clone();
            if duplicate_forgotten_ids.contains(memory_id.as_str()) {
                if !updated_record.forgotten {
                    updated_record.forgotten = true;
                    result.duplicate_forgotten_records =
                        result.duplicate_forgotten_records.saturating_add(1);
                    changed = true;
                }
            } else {
                let last_accessed_unix_ms = updated_record
                    .last_accessed_at_unix_ms
                    .max(updated_record.updated_unix_ms);
                if now_unix_ms.saturating_sub(last_accessed_unix_ms) >= policy.stale_after_unix_ms {
                    let decayed_importance =
                        (updated_record.importance * policy.decay_rate).clamp(0.0, 1.0);
                    if decayed_importance != updated_record.importance {
                        updated_record.importance = decayed_importance;
                        result.decayed_records = result.decayed_records.saturating_add(1);
                        changed = true;
                    }
                }

                if updated_record.importance < policy.prune_importance_floor {
                    updated_record.forgotten = true;
                    result.pruned_records = result.pruned_records.saturating_add(1);
                    changed = true;
                } else if policy.orphan_cleanup_enabled
                    && updated_record.importance <= policy.orphan_importance_floor
                    && !has_outgoing.contains(memory_id.as_str())
                    && !has_incoming.contains(memory_id.as_str())
                {
                    updated_record.forgotten = true;
                    result.orphan_forgotten_records =
                        result.orphan_forgotten_records.saturating_add(1);
                    changed = true;
                }
            }

            if changed {
                updated_record.updated_unix_ms = now_unix_ms;
                self.append_record_backend(&updated_record)?;
                result.updated_records = result.updated_records.saturating_add(1);
            } else {
                result.unchanged_records = result.unchanged_records.saturating_add(1);
            }
        }

        Ok(result)
    }

    pub(super) fn load_latest_records(&self) -> Result<Vec<RuntimeMemoryRecord>> {
        self.load_latest_records_internal(false)
    }

    pub(super) fn load_latest_records_including_forgotten(
        &self,
    ) -> Result<Vec<RuntimeMemoryRecord>> {
        self.load_latest_records_internal(true)
    }

    fn load_latest_records_internal(
        &self,
        include_forgotten: bool,
    ) -> Result<Vec<RuntimeMemoryRecord>> {
        let records = self.load_records_backend()?;
        let relation_map = self.load_relation_map_backend()?;
        let mut seen = BTreeSet::new();
        let mut latest = Vec::new();
        for record in records.into_iter().rev() {
            if seen.insert(record.entry.memory_id.clone()) {
                latest.push(record);
            }
        }
        if !relation_map.is_empty() {
            for record in &mut latest {
                if let Some(relations) = relation_map.get(record.entry.memory_id.as_str()) {
                    record.relations = relations.clone();
                }
            }
        }
        if !include_forgotten {
            latest.retain(|record| !record.forgotten);
        }
        latest.sort_by(|left, right| {
            right
                .updated_unix_ms
                .cmp(&left.updated_unix_ms)
                .then_with(|| left.entry.memory_id.cmp(&right.entry.memory_id))
        });
        Ok(latest)
    }
}

fn validate_lifecycle_maintenance_policy(policy: &MemoryLifecycleMaintenancePolicy) -> Result<()> {
    if !policy.decay_rate.is_finite() || !(0.0..=1.0).contains(&policy.decay_rate) {
        bail!(
            "lifecycle decay_rate must be finite and within 0.0..=1.0 (received {})",
            policy.decay_rate
        );
    }
    if !policy.prune_importance_floor.is_finite()
        || !(0.0..=1.0).contains(&policy.prune_importance_floor)
    {
        bail!(
            "lifecycle prune_importance_floor must be finite and within 0.0..=1.0 (received {})",
            policy.prune_importance_floor
        );
    }
    if !policy.orphan_importance_floor.is_finite()
        || !(0.0..=1.0).contains(&policy.orphan_importance_floor)
    {
        bail!(
            "lifecycle orphan_importance_floor must be finite and within 0.0..=1.0 (received {})",
            policy.orphan_importance_floor
        );
    }
    if !policy.duplicate_similarity_threshold.is_finite()
        || !(0.0..=1.0).contains(&policy.duplicate_similarity_threshold)
    {
        bail!(
            "lifecycle duplicate_similarity_threshold must be finite and within 0.0..=1.0 (received {})",
            policy.duplicate_similarity_threshold
        );
    }
    Ok(())
}

fn collect_ingest_directory_fingerprints(ingest_dir: &Path) -> Result<BTreeMap<String, String>> {
    if !ingest_dir.exists() {
        return Ok(BTreeMap::new());
    }
    if !ingest_dir.is_dir() {
        bail!(
            "ingestion path must be a directory (received {})",
            ingest_dir.display()
        );
    }

    let mut fingerprints = BTreeMap::new();
    for entry in fs::read_dir(ingest_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified_unix_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let signature = format!("{}:{modified_unix_ms}", metadata.len());
        fingerprints.insert(path.display().to_string(), signature);
    }
    Ok(fingerprints)
}

fn default_ingestion_chunk_write_plan(
    source_path: &Path,
    chunk_index: usize,
    chunk_text: &str,
    extension: &str,
    checkpoint_key: &str,
) -> IngestionChunkWritePlan {
    IngestionChunkWritePlan {
        memory_id: ingestion_chunk_memory_id(source_path, chunk_index, checkpoint_key),
        summary: ingestion_chunk_summary(source_path, chunk_index, chunk_text),
        tags: vec![
            "ingestion".to_string(),
            format!("ingestion_extension:{extension}"),
        ],
        facts: vec![chunk_text.to_string()],
        memory_type: Some(MemoryType::Fact),
        importance: None,
    }
}

fn llm_memory_write_plans_for_chunk(
    source_path: &Path,
    chunk_index: usize,
    chunk_text: &str,
    extension: &str,
    checkpoint_key: &str,
    llm_options: &MemoryIngestionLlmOptions,
) -> Result<Vec<IngestionChunkWritePlan>> {
    let provider = llm_options.provider.trim().to_ascii_lowercase();
    if provider != "openai" && provider != "openai-compatible" {
        return Err(anyhow!(
            "{MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON}: unsupported provider '{}'",
            llm_options.provider
        ));
    }

    let api_base = llm_options.api_base.trim_end_matches('/');
    if api_base.is_empty() {
        return Err(anyhow!(
            "{MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON}: api_base must not be empty"
        ));
    }
    if llm_options.model.trim().is_empty() {
        return Err(anyhow!(
            "{MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON}: model must not be empty"
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(
            llm_options.timeout_ms.max(1),
        ))
        .build()
        .map_err(|error| {
            anyhow!(
                "{MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON}: failed to build llm client: {error}"
            )
        })?;

    let response = client
        .post(format!("{api_base}/chat/completions"))
        .bearer_auth(llm_options.api_key.as_str())
        .json(&serde_json::json!({
            "model": llm_options.model,
            "temperature": 0,
            "tool_choice": "required",
            "messages": [
                {
                    "role": "system",
                    "content": "Extract durable semantic memories from each chunk and emit only memory_write tool calls."
                },
                {
                    "role": "user",
                    "content": format!(
                        "source_path={}\nchunk_index={}\nchunk_text:\n{}",
                        source_path.display(),
                        chunk_index,
                        chunk_text
                    )
                }
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "memory_write",
                        "description": "Persist memory records extracted from an ingestion chunk.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "memory_id": { "type": "string" },
                                "summary": { "type": "string" },
                                "tags": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "facts": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "memory_type": { "type": "string" },
                                "importance": { "type": "number" }
                            },
                            "required": ["summary"],
                            "additionalProperties": false
                        }
                    }
                }
            ]
        }))
        .send()
        .map_err(|error| {
            anyhow!("{MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON}: request failed: {error}")
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "{MEMORY_INGESTION_LLM_REQUEST_FAILURE_REASON}: status={} body={}",
            status.as_u16(),
            body.chars().take(240).collect::<String>()
        ));
    }

    let payload = response.json::<serde_json::Value>().map_err(|error| {
        anyhow!("{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: invalid json payload: {error}")
    })?;
    let tool_calls = payload
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("tool_calls"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            anyhow!("{MEMORY_INGESTION_LLM_EMPTY_TOOL_CALLS_REASON}: response missing tool_calls")
        })?;

    let mut plans = Vec::new();
    for tool_call in tool_calls {
        let function = tool_call
            .get("function")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| {
                anyhow!("{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: tool call missing function")
            })?;
        let name = function
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if name != "memory_write" {
            continue;
        }
        let arguments = function
            .get("arguments")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                anyhow!(
                    "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: memory_write missing arguments string"
                )
            })?;
        let parsed_arguments = serde_json::from_str::<serde_json::Value>(arguments).map_err(|error| {
            anyhow!(
                "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: invalid memory_write arguments json: {error}"
            )
        })?;

        let summary = parsed_arguments
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: memory_write summary must be non-empty"
                )
            })?
            .to_string();

        let memory_id = parsed_arguments
            .get("memory_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                ingestion_chunk_memory_id_with_variant(
                    source_path,
                    chunk_index,
                    checkpoint_key,
                    plans.len(),
                )
            });

        let mut tags = parsed_arguments
            .get("tags")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|value| {
                        value.as_str().map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).ok_or_else(|| {
                            anyhow!(
                                "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: tag values must be non-empty strings"
                            )
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        tags.push("ingestion".to_string());
        tags.push(format!("ingestion_extension:{extension}"));
        tags.push("ingestion_llm".to_string());
        tags.sort();
        tags.dedup();

        let facts = parsed_arguments
            .get("facts")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|value| {
                        value.as_str().map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).ok_or_else(|| {
                            anyhow!(
                                "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: fact values must be non-empty strings"
                            )
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let facts = if facts.is_empty() {
            vec![chunk_text.to_string()]
        } else {
            facts
        };

        let memory_type = match parsed_arguments
            .get("memory_type")
            .and_then(serde_json::Value::as_str)
        {
            Some(raw) => Some(MemoryType::parse(raw).ok_or_else(|| {
                anyhow!(
                    "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: unsupported memory_type '{raw}'"
                )
            })?),
            None => Some(MemoryType::Fact),
        };

        let importance = match parsed_arguments.get("importance").and_then(serde_json::Value::as_f64)
        {
            Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => {
                Some(value as f32)
            }
            Some(value) => {
                return Err(anyhow!(
                    "{MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON}: importance must be within 0.0..=1.0 (received {value})"
                ))
            }
            None => None,
        };

        plans.push(IngestionChunkWritePlan {
            memory_id,
            summary,
            tags,
            facts,
            memory_type,
            importance,
        });
    }

    if plans.is_empty() {
        return Err(anyhow!(
            "{MEMORY_INGESTION_LLM_EMPTY_TOOL_CALLS_REASON}: no memory_write tool calls were produced"
        ));
    }
    Ok(plans)
}

fn supported_ingest_extension(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_ascii_lowercase())?;
    MEMORY_INGESTION_SUPPORTED_EXTENSIONS
        .contains(&extension.as_str())
        .then_some(extension)
}

fn chunk_text_by_lines(raw: &str, chunk_line_count: usize) -> Vec<String> {
    if chunk_line_count == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current_lines = Vec::with_capacity(chunk_line_count);
    for line in raw.lines() {
        current_lines.push(line.to_string());
        if current_lines.len() == chunk_line_count {
            chunks.push(current_lines.join("\n"));
            current_lines.clear();
        }
    }
    if !current_lines.is_empty() {
        chunks.push(current_lines.join("\n"));
    }
    if chunks.is_empty() && !raw.trim().is_empty() {
        chunks.push(raw.trim().to_string());
    }
    chunks
}

fn ingestion_chunk_checkpoint_key(path: &Path, chunk_index: usize, chunk_text: &str) -> String {
    let material = format!("{}|{}|{}", path.display(), chunk_index, chunk_text);
    let digest = sha256_hex(material.as_bytes());
    format!("{MEMORY_INGESTION_SOURCE_EVENT_KEY_PREFIX}{digest}")
}

fn ingestion_chunk_memory_id(path: &Path, chunk_index: usize, checkpoint_key: &str) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("chunk");
    let sanitized = sanitize_memory_id_component(stem);
    let digest = checkpoint_key.rsplit(':').next().unwrap_or("checkpoint");
    let digest_prefix = digest.chars().take(12).collect::<String>();
    format!(
        "ingest-{}-{:04}-{}",
        if sanitized.is_empty() {
            "chunk"
        } else {
            sanitized.as_str()
        },
        chunk_index.saturating_add(1),
        digest_prefix
    )
}

fn ingestion_chunk_memory_id_with_variant(
    path: &Path,
    chunk_index: usize,
    checkpoint_key: &str,
    variant_index: usize,
) -> String {
    let base = ingestion_chunk_memory_id(path, chunk_index, checkpoint_key);
    if variant_index == 0 {
        base
    } else {
        format!("{base}-{:02}", variant_index.saturating_add(1))
    }
}

fn ingestion_chunk_summary(path: &Path, chunk_index: usize, chunk_text: &str) -> String {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-file");
    let preview = chunk_text.lines().next().unwrap_or("").trim();
    let preview = truncate_chars(preview, 80);
    if preview.is_empty() {
        format!(
            "Ingested chunk {} from {}",
            chunk_index.saturating_add(1),
            file_name
        )
    } else {
        format!(
            "Ingested chunk {} from {}: {}",
            chunk_index.saturating_add(1),
            file_name,
            preview
        )
    }
}

fn sanitize_memory_id_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
fn fnv1a64_hex(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn collect_duplicate_memory_ids(
    records: &[RuntimeMemoryRecord],
    similarity_threshold: f32,
) -> BTreeSet<String> {
    let mut by_scope = BTreeMap::<(String, String, String), Vec<&RuntimeMemoryRecord>>::new();
    for record in records {
        if record.memory_type == MemoryType::Identity || record.embedding_vector.is_empty() {
            continue;
        }
        by_scope
            .entry((
                record.scope.workspace_id.clone(),
                record.scope.channel_id.clone(),
                record.scope.actor_id.clone(),
            ))
            .or_default()
            .push(record);
    }

    let mut duplicate_forgotten_ids = BTreeSet::<String>::new();
    for scoped_records in by_scope.values_mut() {
        scoped_records.sort_by(|left, right| {
            right
                .importance
                .total_cmp(&left.importance)
                .then_with(|| left.entry.memory_id.cmp(&right.entry.memory_id))
        });

        for (canonical_index, canonical) in scoped_records.iter().enumerate() {
            if duplicate_forgotten_ids.contains(canonical.entry.memory_id.as_str()) {
                continue;
            }
            for candidate in scoped_records
                .iter()
                .skip(canonical_index.saturating_add(1))
            {
                if duplicate_forgotten_ids.contains(candidate.entry.memory_id.as_str()) {
                    continue;
                }
                let similarity = super::cosine_similarity(
                    &canonical.embedding_vector,
                    &candidate.embedding_vector,
                );
                if similarity >= similarity_threshold {
                    duplicate_forgotten_ids.insert(candidate.entry.memory_id.clone());
                }
            }
        }
    }

    duplicate_forgotten_ids
}

fn compute_graph_scores(records: &HashMap<String, RuntimeMemoryRecord>) -> HashMap<String, f32> {
    let mut scores = HashMap::<String, f32>::new();
    for source_record in records.values() {
        for relation in &source_record.relations {
            let Some(target_record) = records.get(relation.target_id.as_str()) else {
                continue;
            };
            let relation_weight = relation.effective_weight.clamp(0.0, 1.0);
            if relation_weight <= 0.0 {
                continue;
            }
            let source_importance = source_record.importance.clamp(0.0, 1.0);
            if source_importance > 0.0 {
                *scores.entry(relation.target_id.clone()).or_default() +=
                    source_importance * relation_weight;
            }
            let target_importance = target_record.importance.clamp(0.0, 1.0);
            if target_importance > 0.0 {
                *scores
                    .entry(source_record.entry.memory_id.clone())
                    .or_default() += target_importance * relation_weight;
            }
        }
    }
    for score in scores.values_mut() {
        *score = score.clamp(0.0, 1.0);
    }
    scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_contract::{MemoryEntry, MemoryScope};
    use crate::runtime::{
        FileMemoryStore, MemoryIngestionLlmOptions, MemoryIngestionWatchPollingState,
        MemoryRelation, MemorySearchOptions, MemoryType,
    };
    use httpmock::{Method::POST, MockServer};
    use serde_json::json;
    use tempfile::tempdir;

    fn build_record(
        memory_id: &str,
        importance: f32,
        relations: Vec<MemoryRelation>,
    ) -> RuntimeMemoryRecord {
        RuntimeMemoryRecord {
            schema_version: 1,
            updated_unix_ms: 1,
            scope: MemoryScope {
                workspace_id: "workspace-a".to_string(),
                channel_id: "ops".to_string(),
                actor_id: "assistant".to_string(),
            },
            entry: MemoryEntry {
                memory_id: memory_id.to_string(),
                summary: format!("summary-{memory_id}"),
                tags: Vec::new(),
                facts: Vec::new(),
                source_event_key: format!("evt-{memory_id}"),
                recency_weight_bps: 0,
                confidence_bps: 1_000,
            },
            memory_type: MemoryType::Observation,
            importance,
            embedding_source: "hash-fnv1a".to_string(),
            embedding_model: None,
            embedding_vector: vec![1.0, 0.0],
            embedding_reason_code: "memory_embedding_hash_only".to_string(),
            last_accessed_at_unix_ms: 0,
            access_count: 0,
            forgotten: false,
            relations,
        }
    }

    #[test]
    fn unit_compute_graph_scores_accumulates_bidirectional_weighted_importance() {
        let source_id = "source-memory".to_string();
        let target_id = "target-memory".to_string();
        let records = HashMap::from([
            (
                source_id.clone(),
                build_record(
                    source_id.as_str(),
                    0.8,
                    vec![MemoryRelation {
                        target_id: target_id.clone(),
                        relation_type: "depends_on".to_string(),
                        weight: 0.5,
                        effective_weight: 0.5,
                    }],
                ),
            ),
            (
                target_id.clone(),
                build_record(target_id.as_str(), 0.6, Vec::new()),
            ),
        ]);

        let scores = compute_graph_scores(&records);
        assert_eq!(
            scores.len(),
            2,
            "expected exactly source + target graph scores"
        );
        assert!(
            (scores.get(target_id.as_str()).copied().unwrap_or_default() - 0.4).abs() <= 0.000_001
        );
        assert!(
            (scores.get(source_id.as_str()).copied().unwrap_or_default() - 0.3).abs() <= 0.000_001
        );
    }

    #[test]
    fn unit_compute_graph_scores_ignores_zero_importance_zero_weight_and_missing_targets() {
        let zero_source_id = "zero-source".to_string();
        let zero_target_id = "zero-target".to_string();
        let missing_target_id = "missing-target".to_string();
        let zero_weight_source_id = "zero-weight-source".to_string();
        let records = HashMap::from([
            (
                zero_source_id.clone(),
                build_record(
                    zero_source_id.as_str(),
                    0.0,
                    vec![MemoryRelation {
                        target_id: zero_target_id.clone(),
                        relation_type: "depends_on".to_string(),
                        weight: 0.7,
                        effective_weight: 0.7,
                    }],
                ),
            ),
            (
                zero_target_id.clone(),
                build_record(zero_target_id.as_str(), 0.0, Vec::new()),
            ),
            (
                "missing-target-source".to_string(),
                build_record(
                    "missing-target-source",
                    0.9,
                    vec![MemoryRelation {
                        target_id: missing_target_id,
                        relation_type: "depends_on".to_string(),
                        weight: 0.9,
                        effective_weight: 0.9,
                    }],
                ),
            ),
            (
                zero_weight_source_id.clone(),
                build_record(
                    zero_weight_source_id.as_str(),
                    0.9,
                    vec![MemoryRelation {
                        target_id: zero_target_id,
                        relation_type: "depends_on".to_string(),
                        weight: 0.0,
                        effective_weight: 0.0,
                    }],
                ),
            ),
        ]);

        let scores = compute_graph_scores(&records);
        assert!(
            scores.is_empty(),
            "invalid/zero-weight/zero-importance edges should not produce graph scores"
        );
    }

    #[test]
    fn integration_search_score_uses_vector_importance_and_graph_signal_additively() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "ops".to_string(),
            actor_id: "assistant".to_string(),
        };
        store
            .write_entry_with_metadata_and_relations(
                &scope,
                MemoryEntry {
                    memory_id: "target-memory".to_string(),
                    summary: "target anchor memory".to_string(),
                    tags: Vec::new(),
                    facts: Vec::new(),
                    source_event_key: "evt-target".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Goal),
                Some(0.8),
                &[],
            )
            .expect("write target memory");
        store
            .write_entry_with_metadata_and_relations(
                &scope,
                MemoryEntry {
                    memory_id: "source-memory".to_string(),
                    summary: "alpha graph source memory".to_string(),
                    tags: Vec::new(),
                    facts: Vec::new(),
                    source_event_key: "evt-source".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Goal),
                Some(1.0),
                &[crate::runtime::MemoryRelationInput {
                    target_id: "target-memory".to_string(),
                    relation_type: Some("depends_on".to_string()),
                    weight: Some(0.5),
                }],
            )
            .expect("write source memory with relation");

        let options = MemorySearchOptions {
            graph_signal_weight: 0.5,
            min_similarity: -1.0,
            enable_hybrid_retrieval: false,
            enable_embedding_migration: false,
            ..MemorySearchOptions::default()
        };
        let result = store
            .search("alpha graph source memory", &options)
            .expect("search succeeds");
        let source_match = result
            .matches
            .iter()
            .find(|item| item.memory_id == "source-memory")
            .expect("source memory match");
        let vector_score = source_match
            .vector_score
            .expect("vector score should be present for vector retrieval");
        let graph_score = source_match
            .graph_score
            .expect("graph score should be present for related memory");
        assert!(graph_score > 0.0);

        let expected = vector_score * importance_rank_multiplier(source_match.importance)
            + options.graph_signal_weight * graph_score;
        assert!((source_match.score - expected).abs() <= 0.000_001);
    }

    #[test]
    fn spec_2450_c02_read_and_search_touch_lifecycle_metadata() {
        let temp = tempdir().expect("tempdir");
        let store = FileMemoryStore::new(temp.path());
        let scope = MemoryScope {
            workspace_id: "workspace-a".to_string(),
            channel_id: "ops".to_string(),
            actor_id: "assistant".to_string(),
        };
        store
            .write_entry_with_metadata_and_relations(
                &scope,
                MemoryEntry {
                    memory_id: "memory-lifecycle".to_string(),
                    summary: "lifecycle metadata sample".to_string(),
                    tags: vec!["lifecycle".to_string()],
                    facts: vec!["touch updates access counters".to_string()],
                    source_event_key: "evt-lifecycle".to_string(),
                    recency_weight_bps: 0,
                    confidence_bps: 1_000,
                },
                Some(MemoryType::Fact),
                Some(0.65),
                &[],
            )
            .expect("write lifecycle memory");

        let before = store
            .list_latest_records(None, usize::MAX)
            .expect("list before touch");
        let before_record = before
            .iter()
            .find(|record| record.entry.memory_id == "memory-lifecycle")
            .expect("memory-lifecycle exists before touch");
        assert_eq!(before_record.access_count, 0);
        assert_eq!(before_record.last_accessed_at_unix_ms, 0);
        assert!(!before_record.forgotten);

        let read = store
            .read_entry("memory-lifecycle", None)
            .expect("read lifecycle memory")
            .expect("memory-lifecycle should exist");
        assert_eq!(read.access_count, 1);
        assert!(read.last_accessed_at_unix_ms > 0);

        let after_read = store
            .list_latest_records(None, usize::MAX)
            .expect("list after read touch");
        let after_read_record = after_read
            .iter()
            .find(|record| record.entry.memory_id == "memory-lifecycle")
            .expect("memory-lifecycle exists after read touch");
        assert_eq!(after_read_record.access_count, 1);
        assert!(after_read_record.last_accessed_at_unix_ms > 0);

        let result = store
            .search(
                "lifecycle metadata sample",
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
            .expect("search lifecycle memory");
        assert!(
            result
                .matches
                .iter()
                .any(|candidate| candidate.memory_id == "memory-lifecycle"),
            "search should return lifecycle memory"
        );

        let after_search = store
            .list_latest_records(None, usize::MAX)
            .expect("list after search touch");
        let after_search_record = after_search
            .iter()
            .find(|record| record.entry.memory_id == "memory-lifecycle")
            .expect("memory-lifecycle exists after search touch");
        assert!(
            after_search_record.access_count >= 2,
            "search touch should increment access count"
        );
        assert!(after_search_record.last_accessed_at_unix_ms > 0);
    }

    fn ingestion_scope() -> MemoryScope {
        MemoryScope {
            workspace_id: "workspace-ingestion".to_string(),
            channel_id: "channel-ingestion".to_string(),
            actor_id: "assistant".to_string(),
        }
    }

    fn ingestion_checkpoint_row_count(memory_root: &std::path::Path) -> usize {
        let sqlite_path = memory_root.join("entries.sqlite");
        if !sqlite_path.exists() {
            return 0;
        }
        let connection = rusqlite::Connection::open(&sqlite_path)
            .expect("open sqlite memory store for checkpoint assertions");
        let mut statement = connection
            .prepare("SELECT COUNT(*) FROM memory_ingestion_checkpoints")
            .expect("prepare checkpoint count statement");
        statement
            .query_row([], |row| row.get::<_, usize>(0))
            .expect("query checkpoint row count")
    }

    fn llm_ingestion_options(server: &MockServer) -> MemoryIngestionLlmOptions {
        MemoryIngestionLlmOptions {
            provider: "openai-compatible".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_base: server.url(""),
            api_key: "test-key".to_string(),
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn spec_2503_c01_watch_poll_skips_when_directory_unchanged() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("poll.txt"), "a\nb\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };
        let mut polling_state = MemoryIngestionWatchPollingState::default();

        let first = store
            .ingest_directory_watch_poll_once(&ingest_dir, &options, &mut polling_state)
            .expect("first poll should process changed directory");
        assert_eq!(first.chunks_ingested, 1);
        assert_eq!(first.processed_files, 1);

        let second = store
            .ingest_directory_watch_poll_once(&ingest_dir, &options, &mut polling_state)
            .expect("second poll should skip unchanged directory");
        assert_eq!(second.chunks_ingested, 0);
        assert_eq!(second.processed_files, 0);
        assert!(
            second
                .diagnostics
                .iter()
                .any(|line| line.contains("ingestion_watch_poll_no_changes")),
            "watch poll should emit explicit no-change diagnostic"
        );
    }

    #[test]
    fn spec_2503_c02_watch_poll_processes_on_directory_change() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("first.txt"), "a\nb\n").expect("write first ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };
        let mut polling_state = MemoryIngestionWatchPollingState::default();

        let first = store
            .ingest_directory_watch_poll_once(&ingest_dir, &options, &mut polling_state)
            .expect("first poll should process initial file");
        assert_eq!(first.chunks_ingested, 1);

        let second = store
            .ingest_directory_watch_poll_once(&ingest_dir, &options, &mut polling_state)
            .expect("second poll should skip unchanged directory");
        assert_eq!(second.chunks_ingested, 0);

        std::fs::write(ingest_dir.join("second.txt"), "c\nd\n").expect("write changed file");
        let third = store
            .ingest_directory_watch_poll_once(&ingest_dir, &options, &mut polling_state)
            .expect("third poll should process changed directory");
        assert!(
            third.chunks_ingested > 0,
            "changed directory should trigger ingestion"
        );
    }

    #[test]
    fn integration_spec_2503_c03_llm_chunk_processing_uses_memory_write_tool_calls() {
        let server = MockServer::start();
        let llm = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body_obj(&json!({
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "type": "function",
                                    "function": {
                                        "name": "memory_write",
                                        "arguments": "{\"summary\":\"LLM extracted summary\",\"facts\":[\"fact-a\",\"fact-b\"],\"tags\":[\"llm-tag\"],\"memory_type\":\"fact\",\"importance\":0.8}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(
            ingest_dir.join("llm.txt"),
            "chunk line one\nchunk line two\n",
        )
        .expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 4,
            delete_source_on_success: false,
        };
        let result = store
            .ingest_directory_worker_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &llm_ingestion_options(&server),
            )
            .expect("llm ingestion should succeed");

        llm.assert();
        assert_eq!(result.chunks_ingested, 1);

        let records = store
            .list_latest_records(None, usize::MAX)
            .expect("list ingested llm records");
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.entry.summary, "LLM extracted summary");
        assert_eq!(
            record.entry.facts,
            vec!["fact-a".to_string(), "fact-b".to_string()]
        );
        assert!(record.entry.tags.iter().any(|tag| tag == "llm-tag"));
    }

    #[test]
    fn integration_spec_2503_c04_llm_rerun_skips_durable_chunk_checkpoints() {
        let server = MockServer::start();
        let _llm = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body_obj(&json!({
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "type": "function",
                                    "function": {
                                        "name": "memory_write",
                                        "arguments": "{\"summary\":\"checkpointed chunk\",\"facts\":[\"fact-1\"]}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("rerun.txt"), "a\nb\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };

        let first = store
            .ingest_directory_worker_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &llm_ingestion_options(&server),
            )
            .expect("first llm worker run should succeed");
        assert_eq!(first.chunks_ingested, 1);

        let second = store
            .ingest_directory_worker_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &llm_ingestion_options(&server),
            )
            .expect("second llm worker run should succeed");
        assert_eq!(second.chunks_ingested, 0);
        assert_eq!(second.chunks_skipped_existing, 1);
        assert_eq!(
            ingestion_checkpoint_row_count(&memory_root),
            1,
            "durable checkpoint should prevent duplicate writes across reruns"
        );
    }

    #[test]
    fn regression_spec_2503_c05_llm_parse_failure_keeps_source_file_for_retry() {
        let server = MockServer::start();
        let llm = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body_obj(&json!({
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "type": "function",
                                    "function": {
                                        "name": "memory_write",
                                        "arguments": "{not-json"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("bad-llm.txt"), "a\nb\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: true,
        };
        let result = store
            .ingest_directory_worker_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &llm_ingestion_options(&server),
            )
            .expect("llm run should return diagnostics on parse failure");

        llm.assert();
        assert!(
            ingest_dir.join("bad-llm.txt").exists(),
            "llm parse failure should keep source file for retry"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|line| line.contains("ingestion_chunk_llm_parse_failed")),
            "diagnostics should include parse failure reason"
        );
        assert_eq!(result.chunks_ingested, 0);
    }

    #[test]
    fn integration_spec_2503_c06_llm_watch_poll_emits_no_change_diagnostic() {
        let server = MockServer::start();
        let llm = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body_obj(&json!({
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "type": "function",
                                    "function": {
                                        "name": "memory_write",
                                        "arguments": "{\"summary\":\"watch llm summary\",\"facts\":[\"fact\"]}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("watch.txt"), "a\nb\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };
        let mut polling_state = MemoryIngestionWatchPollingState::default();

        let first = store
            .ingest_directory_watch_poll_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &mut polling_state,
                &llm_ingestion_options(&server),
            )
            .expect("first llm watch poll should process file");
        assert_eq!(first.chunks_ingested, 1);
        llm.assert();

        let second = store
            .ingest_directory_watch_poll_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &mut polling_state,
                &llm_ingestion_options(&server),
            )
            .expect("second llm watch poll should short-circuit");
        assert_eq!(second.chunks_ingested, 0);
        assert!(
            second
                .diagnostics
                .iter()
                .any(|line| line.contains("ingestion_watch_poll_no_changes")),
            "llm watch poll should emit no-change diagnostic"
        );
    }

    #[test]
    fn regression_spec_2503_c07_llm_rejects_unsupported_provider() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("unsupported.txt"), "a\nb\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: true,
        };
        let llm_options = MemoryIngestionLlmOptions {
            provider: "local".to_string(),
            model: "local-model".to_string(),
            api_base: "http://127.0.0.1:9".to_string(),
            api_key: "unused".to_string(),
            timeout_ms: 1_000,
        };

        let result = store
            .ingest_directory_worker_once_with_llm_memory_save(&ingest_dir, &options, &llm_options)
            .expect("unsupported provider should return diagnostics");
        assert_eq!(result.chunks_ingested, 0);
        assert!(result.failed_files >= 1);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|line| line.contains("unsupported provider")),
            "diagnostics should mention unsupported llm provider"
        );
        assert!(
            ingest_dir.join("unsupported.txt").exists(),
            "failed llm extraction should keep source file for retry"
        );
    }

    #[test]
    fn regression_spec_2503_c08_llm_blank_memory_id_falls_back_to_deterministic_id() {
        let server = MockServer::start();
        let llm = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body_obj(&json!({
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "type": "function",
                                    "function": {
                                        "name": "memory_write",
                                        "arguments": "{\"memory_id\":\"   \",\"summary\":\"fallback memory id\",\"facts\":[\"fact\"]}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("blank-id.txt"), "line\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 4,
            delete_source_on_success: false,
        };
        let result = store
            .ingest_directory_worker_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &llm_ingestion_options(&server),
            )
            .expect("llm ingestion should succeed with fallback memory id");

        llm.assert();
        assert_eq!(result.chunks_ingested, 1);
        let records = store
            .list_latest_records(None, usize::MAX)
            .expect("list fallback-id records");
        assert_eq!(records.len(), 1);
        assert!(
            records[0].entry.memory_id.starts_with("ingest-"),
            "blank memory_id should fall back to deterministic ingestion id"
        );
    }

    #[test]
    fn regression_spec_2503_c09_llm_invalid_importance_keeps_source_file_for_retry() {
        let server = MockServer::start();
        let llm = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body_obj(&json!({
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "type": "function",
                                    "function": {
                                        "name": "memory_write",
                                        "arguments": "{\"summary\":\"bad importance\",\"facts\":[\"fact\"],\"importance\":2.0}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }));
        });

        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("bad-importance.txt"), "x\ny\n").expect("write ingest");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: true,
        };
        let result = store
            .ingest_directory_worker_once_with_llm_memory_save(
                &ingest_dir,
                &options,
                &llm_ingestion_options(&server),
            )
            .expect("llm run should return diagnostics for invalid importance");

        llm.assert();
        assert_eq!(result.chunks_ingested, 0);
        assert!(result.failed_files >= 1);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|line| line.contains("ingestion_chunk_llm_processing_failed")),
            "out-of-range importance should fail during llm parse, not downstream write"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|line| line.contains(MEMORY_INGESTION_LLM_PARSE_FAILURE_REASON)),
            "diagnostics should preserve llm parse failure reason"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|line| line.contains("importance must be within 0.0..=1.0")),
            "diagnostics should include invalid importance validation"
        );
        assert!(
            ingest_dir.join("bad-importance.txt").exists(),
            "invalid llm payload should keep source file for retry"
        );
    }

    #[test]
    fn spec_2497_c01_worker_entrypoint_executes_ingestion_and_returns_counters() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("worker.txt"), "a\nb\nc\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };
        let result = store
            .ingest_directory_worker_once(&ingest_dir, &options)
            .expect("worker ingestion should succeed");

        assert_eq!(result.discovered_files, 1);
        assert_eq!(result.supported_files, 1);
        assert_eq!(result.processed_files, 1);
        assert_eq!(result.chunks_discovered, 2);
        assert_eq!(result.chunks_ingested, 2);
        assert_eq!(result.chunks_skipped_existing, 0);
    }

    #[test]
    fn spec_2497_c02_checkpoint_key_uses_sha256_hex_digest() {
        let checkpoint =
            ingestion_chunk_checkpoint_key(std::path::Path::new("demo.md"), 1, "hello\nworld");
        let digest = checkpoint
            .strip_prefix("ingestion:chunk:")
            .expect("checkpoint prefix should be present");
        assert_eq!(
            digest,
            "4ef378c52f8551b93b0da4321267eff2c401e6108020f14050009795cb22c73a"
        );
        assert_eq!(digest.len(), 64, "digest should be full SHA-256 length");
        assert!(
            digest
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
            "digest should be lowercase hex"
        );
    }

    #[test]
    fn integration_spec_2497_c03_rerun_skips_chunks_from_durable_checkpoints() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("rerun.txt"), "1\n2\n3\n4\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };

        let first = store
            .ingest_directory_worker_once(&ingest_dir, &options)
            .expect("first worker run should succeed");
        assert_eq!(first.chunks_ingested, 2);
        assert_eq!(first.chunks_skipped_existing, 0);
        assert_eq!(ingestion_checkpoint_row_count(&memory_root), 2);

        let sqlite_path = memory_root.join("entries.sqlite");
        let connection =
            rusqlite::Connection::open(&sqlite_path).expect("open sqlite memory store for setup");
        connection
            .execute("DELETE FROM memory_records", [])
            .expect("clear memory records to force checkpoint-backed dedupe");
        assert!(
            store
                .list_latest_records(None, usize::MAX)
                .expect("list latest records after cleanup")
                .is_empty(),
            "test setup expects memory records to be absent before rerun"
        );

        let second = store
            .ingest_directory_worker_once(&ingest_dir, &options)
            .expect("second worker run should succeed");
        assert_eq!(second.chunks_ingested, 0);
        assert_eq!(second.chunks_skipped_existing, 2);
        assert_eq!(
            ingestion_checkpoint_row_count(&memory_root),
            2,
            "rerun should not duplicate durable checkpoint rows"
        );
    }

    #[test]
    fn regression_spec_2497_c04_chunk_write_failure_keeps_source_file_for_retry() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("bad.txt"), [0xFF_u8, 0xFE_u8, 0xFD_u8])
            .expect("write invalid utf8 file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: true,
        };

        let result = store
            .ingest_directory_worker_once(&ingest_dir, &options)
            .expect("worker ingestion should return diagnostics for failures");
        assert!(
            ingest_dir.join("bad.txt").exists(),
            "failed file should remain for retry"
        );
        assert!(
            result.failed_files >= 1,
            "failed file should be counted in diagnostics"
        );
        assert_eq!(
            ingestion_checkpoint_row_count(&memory_root),
            0,
            "failed file should not create durable chunk checkpoints"
        );
    }

    #[test]
    fn spec_2492_c01_ingestion_writes_deterministic_chunk_memories_for_supported_files() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(
            ingest_dir.join("alpha.txt"),
            "line-1\nline-2\nline-3\nline-4\nline-5\n",
        )
        .expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: true,
        };
        let result = store
            .ingest_directory_once(&ingest_dir, &options)
            .expect("ingestion should succeed");

        assert_eq!(result.discovered_files, 1);
        assert_eq!(result.supported_files, 1);
        assert_eq!(result.processed_files, 1);
        assert_eq!(result.deleted_files, 1);
        assert_eq!(result.chunks_discovered, 3);
        assert_eq!(result.chunks_ingested, 3);
        assert_eq!(result.chunks_skipped_existing, 0);
        assert!(
            !ingest_dir.join("alpha.txt").exists(),
            "source file should be deleted after full success"
        );

        let records = store
            .list_latest_records(
                Some(&MemoryScopeFilter {
                    workspace_id: Some("workspace-ingestion".to_string()),
                    channel_id: Some("channel-ingestion".to_string()),
                    actor_id: Some("assistant".to_string()),
                }),
                usize::MAX,
            )
            .expect("list ingested records");
        assert_eq!(records.len(), 3);
        for record in records {
            assert!(
                record
                    .entry
                    .source_event_key
                    .starts_with("ingestion:chunk:"),
                "source_event_key should include ingestion checkpoint prefix"
            );
        }
    }

    #[test]
    fn integration_spec_2492_c02_ingestion_rerun_skips_existing_chunk_checkpoints() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("rerun.md"), "a\nb\nc\nd\n").expect("write ingest file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: false,
        };
        let first = store
            .ingest_directory_once(&ingest_dir, &options)
            .expect("first ingestion should succeed");
        assert_eq!(first.chunks_ingested, 2);
        assert_eq!(first.chunks_skipped_existing, 0);
        assert!(ingest_dir.join("rerun.md").exists());

        let second = store
            .ingest_directory_once(&ingest_dir, &options)
            .expect("second ingestion should succeed");
        assert_eq!(second.chunks_ingested, 0);
        assert_eq!(second.chunks_skipped_existing, 2);

        let records = store
            .list_latest_records(None, usize::MAX)
            .expect("list records after rerun");
        assert_eq!(
            records.len(),
            2,
            "rerun should not create duplicate chunk records"
        );
    }

    #[test]
    fn regression_spec_2492_c03_ingestion_deletes_only_after_full_file_success() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("good.log"), "ok-1\nok-2\n").expect("write good file");
        std::fs::write(ingest_dir.join("bad.txt"), [0xFF_u8, 0xFE_u8, 0xFD_u8])
            .expect("write invalid utf8 file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 2,
            delete_source_on_success: true,
        };
        let result = store
            .ingest_directory_once(&ingest_dir, &options)
            .expect("ingestion run should complete with per-file diagnostics");

        assert!(
            !ingest_dir.join("good.log").exists(),
            "fully ingested file should be deleted"
        );
        assert!(
            ingest_dir.join("bad.txt").exists(),
            "failed file should remain for retry"
        );
        assert!(
            result.failed_files >= 1,
            "invalid UTF-8 source should be counted as failed file"
        );
    }

    #[test]
    fn regression_spec_2492_c04_ingestion_skips_unsupported_extensions_with_counters() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("ingest");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&ingest_dir).expect("create ingest dir");
        std::fs::write(ingest_dir.join("supported.toml"), "k = \"v\"\n").expect("write toml");
        std::fs::write(ingest_dir.join("skip.bin"), "noop").expect("write unsupported file");

        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 8,
            delete_source_on_success: false,
        };
        let result = store
            .ingest_directory_once(&ingest_dir, &options)
            .expect("ingestion should succeed");

        assert_eq!(result.discovered_files, 2);
        assert_eq!(result.supported_files, 1);
        assert_eq!(result.skipped_unsupported_files, 1);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|line| line.contains("ingestion_file_unsupported_extension")),
            "diagnostics should include unsupported extension reason"
        );
    }

    #[test]
    fn regression_spec_2492_c06_missing_ingest_directory_reports_diagnostic() {
        let temp = tempdir().expect("tempdir");
        let ingest_dir = temp.path().join("missing-ingest");
        let memory_root = temp.path().join("memory");
        let store = FileMemoryStore::new(&memory_root);
        let options = crate::runtime::MemoryIngestionOptions {
            scope: ingestion_scope(),
            chunk_line_count: 8,
            delete_source_on_success: true,
        };

        let result = store
            .ingest_directory_once(&ingest_dir, &options)
            .expect("missing ingestion directory should return diagnostics");
        assert_eq!(result.discovered_files, 0);
        assert_eq!(result.chunks_discovered, 0);
        assert_eq!(result.chunks_ingested, 0);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(
            result.diagnostics[0].contains("ingestion_directory_missing"),
            "missing directory should include explicit diagnostic"
        );
    }

    #[test]
    fn unit_chunk_text_by_lines_returns_empty_for_whitespace_only_input() {
        let chunks = chunk_text_by_lines("", 3);
        assert!(
            chunks.is_empty(),
            "empty content should not emit synthetic chunks"
        );
    }

    #[test]
    fn unit_ingestion_helpers_produce_stable_summary_ids_truncation_and_hash() {
        assert_eq!(
            sanitize_memory_id_component("Alpha beta_123!!"),
            "alpha-beta-123"
        );
        assert_eq!(truncate_chars("abcdef", 3), "abc");
        assert_eq!(truncate_chars("abcdef", 0), "");
        assert_eq!(fnv1a64_hex(b"abc"), "e71fa2190541574b");

        let summary = ingestion_chunk_summary(
            std::path::Path::new("/tmp/Alpha Note.md"),
            1,
            "Preview line\nsecond line",
        );
        assert_eq!(summary, "Ingested chunk 2 from Alpha Note.md: Preview line");

        let long_preview = "x".repeat(120);
        let truncated_summary =
            ingestion_chunk_summary(std::path::Path::new("long.txt"), 0, long_preview.as_str());
        assert!(
            truncated_summary.ends_with(&format!(": {}", "x".repeat(80))),
            "summary preview should be truncated to eighty characters"
        );

        let memory_id = ingestion_chunk_memory_id(
            std::path::Path::new("Alpha beta_123!!.md"),
            0,
            "ingestion:chunk:e71fa2190541574b",
        );
        assert!(
            memory_id.starts_with("ingest-alpha-beta-123-0001-e71fa2190541"),
            "memory id should include sanitized stem, 1-indexed chunk, and digest prefix"
        );

        let variant0 = ingestion_chunk_memory_id_with_variant(
            std::path::Path::new("Alpha beta_123!!.md"),
            0,
            "ingestion:chunk:e71fa2190541574b",
            0,
        );
        assert_eq!(variant0, memory_id);
        let variant1 = ingestion_chunk_memory_id_with_variant(
            std::path::Path::new("Alpha beta_123!!.md"),
            0,
            "ingestion:chunk:e71fa2190541574b",
            1,
        );
        assert!(variant1.ends_with("-02"));
        assert_ne!(variant1, memory_id);
    }
}
