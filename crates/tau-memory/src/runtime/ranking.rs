use std::collections::HashMap;

use anyhow::{anyhow, Result};

use super::{
    current_unix_timestamp_ms, FileMemoryStore, MemoryEmbeddingProviderConfig, RankedTextCandidate,
    RankedTextMatch, RuntimeMemoryRecord, MEMORY_EMBEDDING_REASON_HASH_ONLY,
    MEMORY_EMBEDDING_REASON_PROVIDER_FAILED, MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS,
    MEMORY_EMBEDDING_SOURCE_HASH, MEMORY_EMBEDDING_SOURCE_PROVIDER, MEMORY_RUNTIME_SCHEMA_VERSION,
};

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

/// Ranks text candidates using BM25 lexical scoring.
pub fn rank_text_candidates_bm25(
    query: &str,
    candidates: Vec<RankedTextCandidate>,
    limit: usize,
    k1: f32,
    b: f32,
    min_score: f32,
) -> Vec<RankedTextMatch> {
    if limit == 0 || candidates.is_empty() {
        return Vec::new();
    }

    let query_tokens = tokenize_text(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }

    let safe_k1 = k1.max(0.1);
    let safe_b = b.clamp(0.0, 1.0);
    let safe_min_score = min_score.max(0.0);

    let mut corpus_tokens = Vec::with_capacity(candidates.len());
    let mut doc_frequencies = HashMap::<String, usize>::new();
    let mut total_doc_len = 0usize;
    for candidate in &candidates {
        let tokens = tokenize_text(candidate.text.as_str());
        total_doc_len = total_doc_len.saturating_add(tokens.len());
        let unique_terms = tokens
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        for term in unique_terms {
            *doc_frequencies.entry(term).or_default() += 1;
        }
        corpus_tokens.push(tokens);
    }

    let doc_count = candidates.len() as f32;
    let average_doc_len = (total_doc_len as f32 / doc_count).max(1.0);
    let mut matches = Vec::new();
    for (candidate, tokens) in candidates.into_iter().zip(corpus_tokens.into_iter()) {
        if tokens.is_empty() {
            continue;
        }
        let mut term_frequencies = HashMap::<String, usize>::new();
        for token in tokens {
            *term_frequencies.entry(token).or_default() += 1;
        }

        let doc_len = term_frequencies.values().sum::<usize>() as f32;
        let mut score = 0.0f32;
        for term in &query_tokens {
            let term_frequency = *term_frequencies.get(term.as_str()).unwrap_or(&0) as f32;
            if term_frequency <= 0.0 {
                continue;
            }
            let doc_frequency = *doc_frequencies.get(term.as_str()).unwrap_or(&0) as f32;
            if doc_frequency <= 0.0 {
                continue;
            }
            let idf = (((doc_count - doc_frequency + 0.5) / (doc_frequency + 0.5)) + 1.0).ln();
            let normalization = safe_k1 * (1.0 - safe_b + safe_b * (doc_len / average_doc_len));
            let denominator = (term_frequency + normalization).max(f32::EPSILON);
            score += idf * ((term_frequency * (safe_k1 + 1.0)) / denominator);
        }

        if score >= safe_min_score {
            matches.push(RankedTextMatch {
                key: candidate.key,
                text: candidate.text,
                score,
            });
        }
    }

    matches.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.key.cmp(&right.key))
    });
    matches.truncate(limit);
    matches
}

pub(super) fn reciprocal_rank_fuse(
    vector_ranked: &[RankedTextMatch],
    lexical_ranked: &[RankedTextMatch],
    limit: usize,
    rrf_k: usize,
    vector_weight: f32,
    lexical_weight: f32,
) -> Vec<RankedTextMatch> {
    if limit == 0 {
        return Vec::new();
    }
    let safe_rrf_k = rrf_k.max(1) as f32;
    let safe_vector_weight = vector_weight.max(0.0);
    let safe_lexical_weight = lexical_weight.max(0.0);

    let mut scores = HashMap::<String, f32>::new();
    let mut texts = HashMap::<String, String>::new();
    for (rank, candidate) in vector_ranked.iter().enumerate() {
        let contribution = safe_vector_weight / (safe_rrf_k + rank as f32 + 1.0);
        *scores.entry(candidate.key.clone()).or_default() += contribution;
        texts
            .entry(candidate.key.clone())
            .or_insert_with(|| candidate.text.clone());
    }
    for (rank, candidate) in lexical_ranked.iter().enumerate() {
        let contribution = safe_lexical_weight / (safe_rrf_k + rank as f32 + 1.0);
        *scores.entry(candidate.key.clone()).or_default() += contribution;
        texts
            .entry(candidate.key.clone())
            .or_insert_with(|| candidate.text.clone());
    }

    let mut fused = scores
        .into_iter()
        .filter_map(|(key, score)| {
            texts.get(key.as_str()).map(|text| RankedTextMatch {
                key,
                text: text.clone(),
                score,
            })
        })
        .collect::<Vec<_>>();
    fused.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.key.cmp(&right.key))
    });
    fused.truncate(limit);
    fused
}

/// Converts text to a normalized fixed-size vector using FNV-1a token hashing.
pub fn embed_text_vector(text: &str, dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(1);
    let mut vector = vec![0.0f32; dimensions];
    for token in tokenize_text(text) {
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

pub(super) fn embed_text_vectors_via_provider(
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

pub(super) fn resize_and_normalize_embedding(values: &[f32], dimensions: usize) -> Vec<f32> {
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

fn tokenize_text(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>()
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

pub(super) fn record_search_text(record: &RuntimeMemoryRecord) -> String {
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

pub(super) fn record_search_text_for_entry(entry: &super::MemoryEntry) -> String {
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

impl FileMemoryStore {
    pub(super) fn compute_embedding(
        &self,
        text: &str,
        dimensions: usize,
        prefer_provider: bool,
    ) -> super::ComputedEmbedding {
        if prefer_provider {
            if let Some(config) = &self.embedding_provider {
                let provider = config.provider.trim().to_ascii_lowercase();
                if provider == "openai" || provider == "openai-compatible" {
                    if let Ok(vectors) =
                        embed_text_vectors_via_provider(&[text.to_string()], dimensions, config)
                    {
                        if let Some(first) = vectors.first() {
                            return super::ComputedEmbedding {
                                vector: first.clone(),
                                backend: MEMORY_EMBEDDING_SOURCE_PROVIDER.to_string(),
                                model: Some(config.model.clone()),
                                reason_code: MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS.to_string(),
                            };
                        }
                    }
                    return super::ComputedEmbedding {
                        vector: embed_text_vector(text, dimensions),
                        backend: MEMORY_EMBEDDING_SOURCE_HASH.to_string(),
                        model: None,
                        reason_code: MEMORY_EMBEDDING_REASON_PROVIDER_FAILED.to_string(),
                    };
                }
            }
        }

        super::ComputedEmbedding {
            vector: embed_text_vector(text, dimensions),
            backend: MEMORY_EMBEDDING_SOURCE_HASH.to_string(),
            model: None,
            reason_code: MEMORY_EMBEDDING_REASON_HASH_ONLY.to_string(),
        }
    }

    pub(super) fn migrate_records_to_provider_embeddings(
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
            .map_err(|error| anyhow!(error))?;

        let mut migrated = 0usize;
        for (record, vector) in to_migrate.into_iter().zip(vectors.into_iter()) {
            let migrated_record = RuntimeMemoryRecord {
                schema_version: MEMORY_RUNTIME_SCHEMA_VERSION,
                updated_unix_ms: current_unix_timestamp_ms(),
                scope: record.scope,
                entry: record.entry,
                memory_type: record.memory_type,
                importance: record.importance,
                embedding_source: MEMORY_EMBEDDING_SOURCE_PROVIDER.to_string(),
                embedding_model: Some(config.model.clone()),
                embedding_vector: vector,
                embedding_reason_code: MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS.to_string(),
                last_accessed_at_unix_ms: record.last_accessed_at_unix_ms,
                access_count: record.access_count,
                forgotten: record.forgotten,
                relations: record.relations,
            };
            self.append_record_backend(&migrated_record)?;
            migrated = migrated.saturating_add(1);
        }

        Ok(migrated)
    }
}
