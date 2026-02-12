use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_ai::ModelRef;

use tau_core::write_text_atomic;

pub const MODEL_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const MODELS_LIST_USAGE: &str = "/models-list [query] [--provider <name>] [--tools <true|false>] [--multimodal <true|false>] [--reasoning <true|false>] [--limit <n>]";
pub const MODEL_SHOW_USAGE: &str = "/model-show <provider/model>";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCatalogEntry {
    pub provider: String,
    pub model: String,
    pub context_window_tokens: Option<u32>,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_multimodal: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCatalogFile {
    pub schema_version: u32,
    pub entries: Vec<ModelCatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelCatalogSource {
    BuiltIn,
    Cache { path: PathBuf },
    Remote { url: String, cache_path: PathBuf },
    CacheFallback { path: PathBuf, reason: String },
}

#[derive(Debug, Clone)]
pub struct ModelCatalog {
    entries: Vec<ModelCatalogEntry>,
    index: HashMap<String, usize>,
    source: ModelCatalogSource,
    cache_age: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelListArgs {
    pub query: Option<String>,
    pub provider: Option<String>,
    pub tools: Option<bool>,
    pub multimodal: Option<bool>,
    pub reasoning: Option<bool>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogLoadOptions {
    pub cache_path: PathBuf,
    pub refresh_url: Option<String>,
    pub offline: bool,
    pub stale_after_hours: u64,
    pub request_timeout_ms: u64,
}

impl Default for ModelListArgs {
    fn default() -> Self {
        Self {
            query: None,
            provider: None,
            tools: None,
            multimodal: None,
            reasoning: None,
            limit: 50,
        }
    }
}

impl ModelCatalog {
    pub fn built_in() -> Self {
        Self::from_file(
            built_in_model_catalog_file(),
            ModelCatalogSource::BuiltIn,
            None,
        )
        .expect("built-in model catalog should be valid")
    }

    pub fn entries(&self) -> &[ModelCatalogEntry] {
        &self.entries
    }

    pub fn source(&self) -> &ModelCatalogSource {
        &self.source
    }

    pub fn find(&self, provider: &str, model: &str) -> Option<&ModelCatalogEntry> {
        let key = normalize_model_key(provider, model);
        self.index
            .get(&key)
            .and_then(|index| self.entries.get(*index))
    }

    pub fn find_model_ref(&self, model_ref: &ModelRef) -> Option<&ModelCatalogEntry> {
        self.find(model_ref.provider.as_str(), &model_ref.model)
    }

    pub fn is_stale(&self, stale_after_hours: u64) -> bool {
        let threshold = Duration::from_secs(stale_after_hours.saturating_mul(60 * 60));
        self.cache_age.map(|age| age >= threshold).unwrap_or(false)
    }

    pub fn diagnostics_line(&self, stale_after_hours: u64) -> String {
        let source = match &self.source {
            ModelCatalogSource::BuiltIn => "built-in".to_string(),
            ModelCatalogSource::Cache { path } => format!("cache path={}", path.display()),
            ModelCatalogSource::Remote { url, cache_path } => {
                format!("remote url={} cache_path={}", url, cache_path.display())
            }
            ModelCatalogSource::CacheFallback { path, reason } => {
                format!(
                    "cache-fallback path={} reason={}",
                    path.display(),
                    flatten_whitespace(reason)
                )
            }
        };
        if let Some(cache_age) = self.cache_age {
            let cache_age_hours = cache_age.as_secs_f64() / 3600.0;
            let stale = self.is_stale(stale_after_hours);
            format!(
                "source={} entries={} cache_age_hours={cache_age_hours:.2} stale={stale}",
                source,
                self.entries.len(),
            )
        } else {
            format!("source={} entries={}", source, self.entries.len())
        }
    }

    pub fn from_file(
        mut file: ModelCatalogFile,
        source: ModelCatalogSource,
        cache_age: Option<Duration>,
    ) -> Result<Self> {
        validate_model_catalog_file(&file)?;

        for entry in &mut file.entries {
            entry.provider = entry.provider.trim().to_ascii_lowercase();
            entry.model = entry.model.trim().to_string();
        }

        let mut index = HashMap::new();
        for (entry_index, entry) in file.entries.iter().enumerate() {
            index.insert(
                normalize_model_key(&entry.provider, &entry.model),
                entry_index,
            );
        }
        Ok(Self {
            entries: file.entries,
            index,
            source,
            cache_age,
        })
    }
}

pub fn default_model_catalog_cache_path() -> PathBuf {
    PathBuf::from(".tau/models/catalog.json")
}

pub fn parse_model_catalog_payload(payload: &str) -> Result<ModelCatalogFile> {
    if let Ok(file) = serde_json::from_str::<ModelCatalogFile>(payload) {
        return Ok(file);
    }

    let entries = serde_json::from_str::<Vec<ModelCatalogEntry>>(payload)
        .context("failed to parse model catalog as object or entry array")?;
    Ok(ModelCatalogFile {
        schema_version: MODEL_CATALOG_SCHEMA_VERSION,
        entries,
    })
}

pub fn validate_model_catalog_file(file: &ModelCatalogFile) -> Result<()> {
    if file.schema_version != MODEL_CATALOG_SCHEMA_VERSION {
        bail!(
            "unsupported model catalog schema_version {} (expected {})",
            file.schema_version,
            MODEL_CATALOG_SCHEMA_VERSION
        );
    }

    let mut seen = HashMap::new();
    for entry in &file.entries {
        if entry.provider.trim().is_empty() {
            bail!("model catalog contains entry with empty provider");
        }
        if entry.model.trim().is_empty() {
            bail!("model catalog contains entry with empty model");
        }
        if matches!(entry.context_window_tokens, Some(0)) {
            bail!(
                "model catalog entry '{} / {}' has invalid context_window_tokens=0",
                entry.provider,
                entry.model
            );
        }
        if entry
            .input_cost_per_million
            .map(|value| value.is_sign_negative())
            .unwrap_or(false)
        {
            bail!(
                "model catalog entry '{} / {}' has negative input_cost_per_million",
                entry.provider,
                entry.model
            );
        }
        if entry
            .output_cost_per_million
            .map(|value| value.is_sign_negative())
            .unwrap_or(false)
        {
            bail!(
                "model catalog entry '{} / {}' has negative output_cost_per_million",
                entry.provider,
                entry.model
            );
        }

        let key = normalize_model_key(&entry.provider, &entry.model);
        if let Some(previous) = seen.insert(key.clone(), true) {
            let _ = previous;
            bail!("model catalog contains duplicate entry '{}'", key);
        }
    }

    Ok(())
}

pub async fn load_model_catalog_with_cache(
    options: &ModelCatalogLoadOptions,
) -> Result<ModelCatalog> {
    if let Some(url) = options.refresh_url.as_deref() {
        if !options.offline {
            match fetch_remote_catalog(url, options.request_timeout_ms).await {
                Ok(file) => {
                    write_model_catalog_cache(&options.cache_path, &file)?;
                    let cache_age = read_cache_age(&options.cache_path);
                    return ModelCatalog::from_file(
                        file,
                        ModelCatalogSource::Remote {
                            url: url.to_string(),
                            cache_path: options.cache_path.clone(),
                        },
                        cache_age,
                    );
                }
                Err(error) => {
                    if let Ok((cache_file, cache_age)) =
                        read_model_catalog_cache(&options.cache_path)
                    {
                        return ModelCatalog::from_file(
                            cache_file,
                            ModelCatalogSource::CacheFallback {
                                path: options.cache_path.clone(),
                                reason: format!("{error:#}"),
                            },
                            cache_age,
                        );
                    }
                }
            }
        }
    }

    if let Ok((cache_file, cache_age)) = read_model_catalog_cache(&options.cache_path) {
        return ModelCatalog::from_file(
            cache_file,
            ModelCatalogSource::Cache {
                path: options.cache_path.clone(),
            },
            cache_age,
        );
    }

    let _ = options.stale_after_hours;
    Ok(ModelCatalog::built_in())
}

pub fn ensure_model_supports_tools(catalog: &ModelCatalog, model_ref: &ModelRef) -> Result<()> {
    let Some(entry) = catalog.find_model_ref(model_ref) else {
        println!(
            "model catalog warning: missing entry for '{}/{}'; skipping capability guardrail",
            model_ref.provider.as_str(),
            model_ref.model
        );
        return Ok(());
    };

    if !entry.supports_tools {
        bail!(
            "model '{}' is marked as tool-incompatible in the model catalog; select a model with supports_tools=true",
            format_model_ref(model_ref)
        );
    }

    Ok(())
}

pub fn parse_models_list_args(input: &str) -> Result<ModelListArgs> {
    if input.trim().is_empty() {
        return Ok(ModelListArgs::default());
    }

    let tokens = shell_words::split(input).map_err(|error| anyhow!("invalid args: {error}"))?;
    let mut args = ModelListArgs::default();
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        match token {
            "--provider" => {
                let value = tokens
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("--provider requires a value"))?;
                args.provider = Some(value.trim().to_ascii_lowercase());
                index += 2;
            }
            "--tools" => {
                let value = tokens
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("--tools requires true or false"))?;
                args.tools = Some(parse_bool_arg("--tools", value)?);
                index += 2;
            }
            "--multimodal" => {
                let value = tokens
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("--multimodal requires true or false"))?;
                args.multimodal = Some(parse_bool_arg("--multimodal", value)?);
                index += 2;
            }
            "--reasoning" => {
                let value = tokens
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("--reasoning requires true or false"))?;
                args.reasoning = Some(parse_bool_arg("--reasoning", value)?);
                index += 2;
            }
            "--limit" => {
                let value = tokens
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("--limit requires a numeric value"))?;
                args.limit = value
                    .parse::<usize>()
                    .map_err(|_| anyhow!("invalid --limit value '{value}'"))?;
                index += 2;
            }
            token if token.starts_with('-') => {
                bail!("unknown flag '{token}'");
            }
            _ => {
                if let Some(existing_query) = args.query.as_mut() {
                    existing_query.push(' ');
                    existing_query.push_str(tokens[index].as_str());
                } else {
                    args.query = Some(tokens[index].clone());
                }
                index += 1;
            }
        }
    }

    Ok(args)
}

pub fn render_models_list(catalog: &ModelCatalog, args: &ModelListArgs) -> String {
    let mut rows = catalog
        .entries()
        .iter()
        .filter(|entry| model_entry_matches_filters(entry, args))
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.model.cmp(&right.model))
    });

    let total_matches = rows.len();
    if args.limit > 0 && rows.len() > args.limit {
        rows.truncate(args.limit);
    }

    let mut lines = vec![format!(
        "models list: source={} total_matches={} shown={}",
        summarize_source(catalog.source()),
        total_matches,
        rows.len()
    )];
    if rows.is_empty() {
        lines.push("models list: no matches".to_string());
        return lines.join("\n");
    }

    for entry in rows {
        lines.push(format!(
            "model: {}/{} tools={} multimodal={} reasoning={} context_window_tokens={} input_cost_per_million={} output_cost_per_million={}",
            entry.provider,
            entry.model,
            entry.supports_tools,
            entry.supports_multimodal,
            entry.supports_reasoning,
            entry
                .context_window_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            format_cost(entry.input_cost_per_million),
            format_cost(entry.output_cost_per_million),
        ));
    }

    lines.join("\n")
}

pub fn render_model_show(catalog: &ModelCatalog, raw_model: &str) -> Result<String> {
    let parsed = ModelRef::parse(raw_model)
        .map_err(|error| anyhow!("invalid model reference '{}': {error}", raw_model))?;

    let Some(entry) = catalog.find_model_ref(&parsed) else {
        return Ok(format!(
            "model show: not found: {}\nrun /models-list to inspect available catalog entries",
            format_model_ref(&parsed)
        ));
    };

    let mut lines = Vec::new();
    lines.push(format!("model show: {}/{}", entry.provider, entry.model));
    lines.push(format!("source={}", summarize_source(catalog.source())));
    lines.push(format!(
        "context_window_tokens={}",
        entry
            .context_window_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    lines.push(format!("supports_tools={}", entry.supports_tools));
    lines.push(format!("supports_multimodal={}", entry.supports_multimodal));
    lines.push(format!("supports_reasoning={}", entry.supports_reasoning));
    lines.push(format!(
        "input_cost_per_million={}",
        format_cost(entry.input_cost_per_million)
    ));
    lines.push(format!(
        "output_cost_per_million={}",
        format_cost(entry.output_cost_per_million)
    ));
    Ok(lines.join("\n"))
}

fn model_entry_matches_filters(entry: &ModelCatalogEntry, args: &ModelListArgs) -> bool {
    if let Some(provider) = args.provider.as_deref() {
        if !entry.provider.eq_ignore_ascii_case(provider) {
            return false;
        }
    }

    if let Some(tools) = args.tools {
        if entry.supports_tools != tools {
            return false;
        }
    }

    if let Some(multimodal) = args.multimodal {
        if entry.supports_multimodal != multimodal {
            return false;
        }
    }

    if let Some(reasoning) = args.reasoning {
        if entry.supports_reasoning != reasoning {
            return false;
        }
    }

    if let Some(query) = args.query.as_deref() {
        let normalized_query = query.to_ascii_lowercase();
        let haystack = format!("{}/{}", entry.provider, entry.model).to_ascii_lowercase();
        if !haystack.contains(&normalized_query) {
            return false;
        }
    }

    true
}

fn parse_bool_arg(flag: &str, raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => bail!("{flag} expects true or false, got '{raw}'"),
    }
}

fn normalize_model_key(provider: &str, model: &str) -> String {
    format!(
        "{}/{}",
        provider.trim().to_ascii_lowercase(),
        model.trim().to_ascii_lowercase()
    )
}

fn format_cost(value: Option<f64>) -> String {
    value
        .map(|cost| format!("{cost:.6}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_model_ref(model_ref: &ModelRef) -> String {
    format!("{}/{}", model_ref.provider.as_str(), model_ref.model)
}

fn summarize_source(source: &ModelCatalogSource) -> String {
    match source {
        ModelCatalogSource::BuiltIn => "built-in".to_string(),
        ModelCatalogSource::Cache { path } => format!("cache:{}", path.display()),
        ModelCatalogSource::Remote { url, .. } => format!("remote:{url}"),
        ModelCatalogSource::CacheFallback { path, .. } => {
            format!("cache-fallback:{}", path.display())
        }
    }
}

async fn fetch_remote_catalog(url: &str, request_timeout_ms: u64) -> Result<ModelCatalogFile> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(request_timeout_ms))
        .build()
        .context("failed to build model catalog HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to fetch model catalog URL '{url}'"))?;
    if !response.status().is_success() {
        bail!(
            "model catalog fetch failed for '{}' with status {}",
            url,
            response.status()
        );
    }
    let payload = response
        .text()
        .await
        .with_context(|| format!("failed to read model catalog response from '{url}'"))?;
    parse_model_catalog_payload(&payload)
}

fn write_model_catalog_cache(path: &Path, file: &ModelCatalogFile) -> Result<()> {
    let payload =
        serde_json::to_string_pretty(file).context("failed to serialize model catalog")?;
    write_text_atomic(path, &payload)
        .with_context(|| format!("failed to persist model catalog cache {}", path.display()))
}

fn read_model_catalog_cache(path: &Path) -> Result<(ModelCatalogFile, Option<Duration>)> {
    let payload = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read model catalog cache {}", path.display()))?;
    let file = parse_model_catalog_payload(&payload)?;
    validate_model_catalog_file(&file)?;
    Ok((file, read_cache_age(path)))
}

fn read_cache_age(path: &Path) -> Option<Duration> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let now = SystemTime::now();
    now.duration_since(modified).ok()
}

fn flatten_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn built_in_model_catalog_file() -> ModelCatalogFile {
    ModelCatalogFile {
        schema_version: MODEL_CATALOG_SCHEMA_VERSION,
        entries: vec![
            ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: Some(0.150000),
                output_cost_per_million: Some(0.600000),
            },
            ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: Some(2.500000),
                output_cost_per_million: Some(10.000000),
            },
            ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "openai/gpt-4o-mini".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: Some(0.150000),
                output_cost_per_million: Some(0.600000),
            },
            ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "llama-3.3-70b".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: false,
                supports_reasoning: true,
                input_cost_per_million: None,
                output_cost_per_million: None,
            },
            ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "grok-4".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: None,
                output_cost_per_million: None,
            },
            ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "mistral-large-latest".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: false,
                supports_reasoning: true,
                input_cost_per_million: None,
                output_cost_per_million: None,
            },
            ModelCatalogEntry {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4".to_string(),
                context_window_tokens: Some(200_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: Some(3.000000),
                output_cost_per_million: Some(15.000000),
            },
            ModelCatalogEntry {
                provider: "anthropic".to_string(),
                model: "claude-3-5-haiku-latest".to_string(),
                context_window_tokens: Some(200_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: false,
                input_cost_per_million: Some(0.800000),
                output_cost_per_million: Some(4.000000),
            },
            ModelCatalogEntry {
                provider: "google".to_string(),
                model: "gemini-2.0-flash".to_string(),
                context_window_tokens: Some(1_048_576),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: Some(0.100000),
                output_cost_per_million: Some(0.400000),
            },
            ModelCatalogEntry {
                provider: "google".to_string(),
                model: "gemini-2.5-pro".to_string(),
                context_window_tokens: Some(1_048_576),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: Some(1.250000),
                output_cost_per_million: Some(10.000000),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use tempfile::tempdir;

    #[test]
    fn unit_validate_model_catalog_rejects_duplicate_entries() {
        let file = ModelCatalogFile {
            schema_version: MODEL_CATALOG_SCHEMA_VERSION,
            entries: vec![
                ModelCatalogEntry {
                    provider: "openai".to_string(),
                    model: "gpt-4o-mini".to_string(),
                    context_window_tokens: Some(128_000),
                    supports_tools: true,
                    supports_multimodal: true,
                    supports_reasoning: true,
                    input_cost_per_million: None,
                    output_cost_per_million: None,
                },
                ModelCatalogEntry {
                    provider: "OPENAI".to_string(),
                    model: "gpt-4o-mini".to_string(),
                    context_window_tokens: Some(128_000),
                    supports_tools: true,
                    supports_multimodal: true,
                    supports_reasoning: true,
                    input_cost_per_million: None,
                    output_cost_per_million: None,
                },
            ],
        };

        let error = validate_model_catalog_file(&file).expect_err("duplicate model should fail");
        assert!(error.to_string().contains("duplicate entry"));
    }

    #[test]
    fn unit_model_catalog_lookup_matches_case_insensitive_keys() {
        let catalog = ModelCatalog::built_in();
        let entry = catalog
            .find("OPENAI", "GPT-4O-MINI")
            .expect("lookup should be case-insensitive");
        assert_eq!(entry.provider, "openai");
        assert_eq!(entry.model, "gpt-4o-mini");
    }

    #[test]
    fn functional_parse_models_list_args_supports_filters_and_limit() {
        let args = parse_models_list_args(
            "gpt --provider openai --tools true --multimodal false --reasoning true --limit 3",
        )
        .expect("args should parse");
        assert_eq!(args.query.as_deref(), Some("gpt"));
        assert_eq!(args.provider.as_deref(), Some("openai"));
        assert_eq!(args.tools, Some(true));
        assert_eq!(args.multimodal, Some(false));
        assert_eq!(args.reasoning, Some(true));
        assert_eq!(args.limit, 3);
    }

    #[test]
    fn functional_render_model_show_displays_capabilities() {
        let catalog = ModelCatalog::built_in();
        let output = render_model_show(&catalog, "openai/gpt-4o-mini").expect("render show");
        assert!(output.contains("model show: openai/gpt-4o-mini"));
        assert!(output.contains("supports_tools=true"));
        assert!(output.contains("supports_multimodal=true"));
        assert!(output.contains("supports_reasoning=true"));
    }

    #[tokio::test]
    async fn integration_model_catalog_remote_refresh_writes_cache_and_supports_offline_reuse() {
        let temp = tempdir().expect("tempdir");
        let cache_path = temp.path().join("catalog.json");
        let server = MockServer::start();
        let refresh = server.mock(|when, then| {
            when.method(GET).path("/models.json");
            then.status(200).json_body(serde_json::json!({
                "schema_version": 1,
                "entries": [{
                    "provider": "openai",
                    "model": "gpt-4o-mini",
                    "context_window_tokens": 128000,
                    "supports_tools": true,
                    "supports_multimodal": true,
                    "supports_reasoning": true,
                    "input_cost_per_million": 0.15,
                    "output_cost_per_million": 0.6
                }]
            }));
        });

        let remote_options = ModelCatalogLoadOptions {
            cache_path: cache_path.clone(),
            refresh_url: Some(format!("{}/models.json", server.base_url())),
            offline: false,
            stale_after_hours: 24,
            request_timeout_ms: 5_000,
        };
        let refreshed = load_model_catalog_with_cache(&remote_options)
            .await
            .expect("remote refresh should succeed");
        refresh.assert_calls(1);
        assert!(matches!(
            refreshed.source(),
            ModelCatalogSource::Remote { .. }
        ));
        assert!(cache_path.exists(), "cache file should be written");

        let offline_options = ModelCatalogLoadOptions {
            cache_path,
            refresh_url: None,
            offline: true,
            stale_after_hours: 24,
            request_timeout_ms: 5_000,
        };
        let offline = load_model_catalog_with_cache(&offline_options)
            .await
            .expect("offline cache load should succeed");
        assert!(matches!(offline.source(), ModelCatalogSource::Cache { .. }));
        assert!(
            offline.find("openai", "gpt-4o-mini").is_some(),
            "cached model entry should be available offline"
        );
    }

    #[tokio::test]
    async fn regression_model_catalog_remote_failure_falls_back_to_cache() {
        let temp = tempdir().expect("tempdir");
        let cache_path = temp.path().join("catalog.json");
        let cached = ModelCatalogFile {
            schema_version: MODEL_CATALOG_SCHEMA_VERSION,
            entries: vec![ModelCatalogEntry {
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                context_window_tokens: Some(128_000),
                supports_tools: true,
                supports_multimodal: true,
                supports_reasoning: true,
                input_cost_per_million: None,
                output_cost_per_million: None,
            }],
        };
        write_model_catalog_cache(&cache_path, &cached).expect("write cache");

        let options = ModelCatalogLoadOptions {
            cache_path: cache_path.clone(),
            refresh_url: Some("http://127.0.0.1:1/unreachable".to_string()),
            offline: false,
            stale_after_hours: 24,
            request_timeout_ms: 200,
        };

        let catalog = load_model_catalog_with_cache(&options)
            .await
            .expect("cache fallback should succeed");
        match catalog.source() {
            ModelCatalogSource::CacheFallback { path, reason } => {
                assert_eq!(path, &cache_path);
                assert!(
                    reason.contains("failed to fetch model catalog URL")
                        || reason.contains("error sending request"),
                    "unexpected fallback reason: {reason}"
                );
            }
            other => panic!("expected cache fallback source, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn regression_model_catalog_corrupt_cache_falls_back_to_builtin_and_is_explicit() {
        let temp = tempdir().expect("tempdir");
        let cache_path = temp.path().join("catalog.json");
        std::fs::write(&cache_path, "{ not-json").expect("write corrupt cache");

        let options = ModelCatalogLoadOptions {
            cache_path,
            refresh_url: None,
            offline: true,
            stale_after_hours: 24,
            request_timeout_ms: 5_000,
        };
        let catalog = load_model_catalog_with_cache(&options)
            .await
            .expect("builtin fallback should succeed");
        assert!(matches!(catalog.source(), ModelCatalogSource::BuiltIn));
        assert!(catalog.entries().len() >= 5);
    }

    #[test]
    fn regression_model_catalog_diagnostics_reports_stale_cache_age() {
        let catalog = ModelCatalog {
            entries: vec![],
            index: HashMap::new(),
            source: ModelCatalogSource::Cache {
                path: PathBuf::from(".tau/models/catalog.json"),
            },
            cache_age: Some(Duration::from_secs(60)),
        };
        let line = catalog.diagnostics_line(0);
        assert!(line.contains("stale=true"));
        assert!(line.contains("cache_age_hours="));
    }
}
