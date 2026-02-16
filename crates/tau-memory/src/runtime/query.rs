use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

use anyhow::{bail, Result};

use super::{
    rank_text_candidates, rank_text_candidates_bm25, reciprocal_rank_fuse, record_search_text,
    resize_and_normalize_embedding, FileMemoryStore, MemoryScopeFilter, MemorySearchMatch,
    MemorySearchOptions, MemorySearchResult, MemoryTree, MemoryTreeNode, RankedTextCandidate,
    RankedTextMatch, RuntimeMemoryRecord, MEMORY_EMBEDDING_REASON_HASH_ONLY,
    MEMORY_EMBEDDING_REASON_PROVIDER_FAILED, MEMORY_RETRIEVAL_BACKEND_HYBRID_BM25_RRF,
    MEMORY_RETRIEVAL_BACKEND_VECTOR_ONLY, MEMORY_RETRIEVAL_REASON_HYBRID_ENABLED,
    MEMORY_RETRIEVAL_REASON_VECTOR_ONLY,
};

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
                vector_score: vector_scores.get(item.key.as_str()).copied(),
                lexical_score: lexical_scores.get(item.key.as_str()).copied(),
                fused_score: options
                    .enable_hybrid_retrieval
                    .then(|| fused_scores.get(item.key.as_str()).copied())
                    .flatten(),
                scope: record.scope.clone(),
                summary: record.entry.summary.clone(),
                tags: record.entry.tags.clone(),
                facts: record.entry.facts.clone(),
                source_event_key: record.entry.source_event_key.clone(),
                embedding_source: record.embedding_source.clone(),
                embedding_model: record.embedding_model.clone(),
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

    pub(super) fn load_latest_records(&self) -> Result<Vec<RuntimeMemoryRecord>> {
        let records = self.load_records_backend()?;
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
}
