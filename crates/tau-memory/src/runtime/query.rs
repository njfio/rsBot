use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

use anyhow::{bail, Result};

use super::{
    importance_rank_multiplier, rank_text_candidates, rank_text_candidates_bm25,
    reciprocal_rank_fuse, record_search_text, resize_and_normalize_embedding, FileMemoryStore,
    MemoryScopeFilter, MemorySearchMatch, MemorySearchOptions, MemorySearchResult, MemoryTree,
    MemoryTreeNode, RankedTextCandidate, RankedTextMatch, RuntimeMemoryRecord,
    MEMORY_EMBEDDING_REASON_HASH_ONLY, MEMORY_EMBEDDING_REASON_PROVIDER_FAILED,
    MEMORY_RETRIEVAL_BACKEND_HYBRID_BM25_RRF, MEMORY_RETRIEVAL_BACKEND_VECTOR_ONLY,
    MEMORY_RETRIEVAL_REASON_HYBRID_ENABLED, MEMORY_RETRIEVAL_REASON_VECTOR_ONLY,
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
    use crate::runtime::{FileMemoryStore, MemoryRelation, MemorySearchOptions, MemoryType};
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
}
