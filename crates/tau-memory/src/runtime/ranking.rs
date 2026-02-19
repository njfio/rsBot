use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use super::{
    current_unix_timestamp_ms, FileMemoryStore, MemoryEmbeddingProviderConfig, RankedTextCandidate,
    RankedTextMatch, RuntimeMemoryRecord, MEMORY_EMBEDDING_REASON_HASH_ONLY,
    MEMORY_EMBEDDING_REASON_PROVIDER_FAILED, MEMORY_EMBEDDING_REASON_PROVIDER_SUCCESS,
    MEMORY_EMBEDDING_SOURCE_HASH, MEMORY_EMBEDDING_SOURCE_PROVIDER,
    MEMORY_EMBEDDING_SOURCE_PROVIDER_LOCAL_FASTEMBED, MEMORY_RUNTIME_SCHEMA_VERSION,
};

const LOCAL_EMBEDDING_MODEL_HASH_ONLY: &str = "local-hash";
const LOCAL_EMBEDDING_CACHE_DIR_ENV: &str = "TAU_MEMORY_LOCAL_EMBEDDING_CACHE_DIR";

std::thread_local! {
    static LOCAL_FASTEMBED_MODEL_CACHE: RefCell<HashMap<String, TextEmbedding>> = RefCell::new(HashMap::new());
}

trait LocalEmbeddingProvider {
    fn embed(
        &self,
        inputs: &[String],
        dimensions: usize,
        model: &str,
    ) -> Result<Vec<Vec<f32>>, String>;
}

#[derive(Debug, Clone, Copy, Default)]
struct FastEmbedLocalEmbeddingProvider;

impl LocalEmbeddingProvider for FastEmbedLocalEmbeddingProvider {
    fn embed(
        &self,
        inputs: &[String],
        dimensions: usize,
        model: &str,
    ) -> Result<Vec<Vec<f32>>, String> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        #[cfg(test)]
        if let Some(mode) = LOCAL_EMBEDDING_TEST_MODE.with(|slot| *slot.borrow()) {
            return match mode {
                LocalEmbeddingTestMode::Success => Ok(inputs
                    .iter()
                    .map(|input| deterministic_local_test_embedding(input.as_str(), dimensions))
                    .collect::<Vec<_>>()),
                LocalEmbeddingTestMode::Failure => {
                    Err("local embedding backend unavailable (test override)".to_string())
                }
            };
        }

        let normalized_model = model.trim();
        if normalized_model.is_empty() {
            return Err("local embedding model must not be empty".to_string());
        }
        let embedding_model = parse_local_embedding_model(normalized_model)?;

        LOCAL_FASTEMBED_MODEL_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if !cache.contains_key(normalized_model) {
                let embedder =
                    TextEmbedding::try_new(build_local_text_init_options(embedding_model))
                        .map_err(|error| {
                            format!("local embedding model initialization failed: {error}")
                        })?;
                cache.insert(normalized_model.to_string(), embedder);
            }

            let embedder = cache.get_mut(normalized_model).ok_or_else(|| {
                format!("failed to access cached local embedding model '{normalized_model}'")
            })?;
            let vectors = embedder
                .embed(inputs, None)
                .map_err(|error| format!("local embedding inference failed: {error}"))?;

            Ok(vectors
                .into_iter()
                .map(|vector| resize_and_normalize_embedding(vector.as_slice(), dimensions))
                .collect::<Vec<_>>())
        })
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalEmbeddingTestMode {
    Success,
    Failure,
}

#[cfg(test)]
std::thread_local! {
    static LOCAL_EMBEDDING_TEST_MODE: RefCell<Option<LocalEmbeddingTestMode>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn with_local_embedding_test_mode<T>(
    mode: LocalEmbeddingTestMode,
    operation: impl FnOnce() -> T,
) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("local embedding test override lock");
    let previous_mode = LOCAL_EMBEDDING_TEST_MODE.with(|slot| slot.replace(Some(mode)));
    let outcome = operation();
    LOCAL_EMBEDDING_TEST_MODE.with(|slot| {
        slot.replace(previous_mode);
    });
    outcome
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

fn embed_text_vectors_via_local_provider(
    inputs: &[String],
    dimensions: usize,
    config: &MemoryEmbeddingProviderConfig,
    provider: &dyn LocalEmbeddingProvider,
) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    provider.embed(inputs, dimensions, config.model.as_str())
}

fn build_local_text_init_options(model: EmbeddingModel) -> TextInitOptions {
    let mut options = TextInitOptions::new(model).with_show_download_progress(false);
    if let Some(cache_dir) = std::env::var_os(LOCAL_EMBEDDING_CACHE_DIR_ENV) {
        options = options.with_cache_dir(PathBuf::from(cache_dir));
    }
    options
}

fn parse_local_embedding_model(model: &str) -> Result<EmbeddingModel, String> {
    model.parse::<EmbeddingModel>().or_else(|error| {
        let alias = if model.eq_ignore_ascii_case("BAAI/bge-small-en-v1.5") {
            Some(EmbeddingModel::BGESmallENV15)
        } else if model.eq_ignore_ascii_case("BAAI/bge-base-en-v1.5") {
            Some(EmbeddingModel::BGEBaseENV15)
        } else if model.eq_ignore_ascii_case("BAAI/bge-large-en-v1.5") {
            Some(EmbeddingModel::BGELargeENV15)
        } else {
            None
        };
        alias.ok_or_else(|| format!("unsupported local embedding model '{model}': {error}"))
    })
}

#[cfg(test)]
fn clear_local_fastembed_model_cache() {
    LOCAL_FASTEMBED_MODEL_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

#[cfg(test)]
fn deterministic_local_test_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(1);
    let mut vector = vec![0.0f32; dimensions];
    for (index, byte) in text.as_bytes().iter().enumerate() {
        let bucket = index % dimensions;
        vector[bucket] += (*byte as f32 + 1.0) / 256.0;
    }
    resize_and_normalize_embedding(&vector, dimensions)
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
                if provider == "local" {
                    let model = config.model.trim();
                    if model.eq_ignore_ascii_case(LOCAL_EMBEDDING_MODEL_HASH_ONLY) {
                        return super::ComputedEmbedding {
                            vector: embed_text_vector(text, dimensions),
                            backend: MEMORY_EMBEDDING_SOURCE_HASH.to_string(),
                            model: None,
                            reason_code: MEMORY_EMBEDDING_REASON_HASH_ONLY.to_string(),
                        };
                    }
                    if let Ok(vectors) = embed_text_vectors_via_local_provider(
                        &[text.to_string()],
                        dimensions,
                        config,
                        &FastEmbedLocalEmbeddingProvider,
                    ) {
                        if let Some(first) = vectors.first() {
                            return super::ComputedEmbedding {
                                vector: first.clone(),
                                backend: MEMORY_EMBEDDING_SOURCE_PROVIDER_LOCAL_FASTEMBED
                                    .to_string(),
                                model: Some(model.to_string()),
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

#[cfg(test)]
mod tests {
    use super::{
        clear_local_fastembed_model_cache, parse_local_embedding_model,
        FastEmbedLocalEmbeddingProvider, LocalEmbeddingProvider, LOCAL_EMBEDDING_CACHE_DIR_ENV,
    };
    use fastembed::EmbeddingModel;
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
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn local_embedding_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn regression_spec_2553_c05_local_provider_cache_miss_reports_initialization_error() {
        let _guard = local_embedding_env_lock()
            .lock()
            .expect("local embedding env lock");
        clear_local_fastembed_model_cache();

        let temp = tempdir().expect("tempdir");
        let cache_file = temp.path().join("cache-file");
        std::fs::write(&cache_file, "cache roots must be directories").expect("create cache file");
        let _cache_env = ScopedEnvVar::set(
            LOCAL_EMBEDDING_CACHE_DIR_ENV,
            cache_file.to_string_lossy().as_ref(),
        );

        let provider = FastEmbedLocalEmbeddingProvider;
        let error = provider
            .embed(
                &[String::from(
                    "local provider initialization should fail with invalid cache path",
                )],
                8,
                "BAAI/bge-small-en-v1.5",
            )
            .expect_err("invalid cache path should fail local provider initialization");
        assert!(
            error.contains("local embedding model initialization failed"),
            "expected initialization failure error, got: {error}"
        );
    }

    #[test]
    fn unit_spec_2553_c06_local_embedding_model_alias_parser_maps_and_rejects() {
        assert_eq!(
            parse_local_embedding_model("BAAI/bge-large-en-v1.5")
                .expect("BAAI alias should map to fastembed model"),
            EmbeddingModel::BGELargeENV15
        );

        let error = parse_local_embedding_model("unsupported/local-model")
            .expect_err("unsupported models should return a parse error");
        assert!(
            error.contains("unsupported local embedding model"),
            "unexpected alias parser error: {error}"
        );
    }
}
