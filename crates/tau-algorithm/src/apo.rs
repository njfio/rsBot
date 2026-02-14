use crate::{Algorithm, AlgorithmContext, AlgorithmRunSummary, PromptExample};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use tau_ai::{ChatRequest, LlmClient, Message};
use tau_training_types::ResourcesUpdate;

const APO_NAME: &str = "apo";

/// Scores prompts for a given validation dataset.
#[async_trait]
pub trait PromptEvaluator: Send + Sync {
    async fn score_prompt(&self, prompt: &str, dataset: &[PromptExample]) -> Result<f64>;
}

/// Versioned prompt candidate tracked by APO.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionedPrompt {
    pub version: String,
    pub prompt: String,
    pub score: Option<f64>,
    pub parent_version: Option<String>,
    pub critique: Option<String>,
    pub round: usize,
}

/// APO runtime configuration.
#[derive(Debug, Clone)]
pub struct ApoConfig {
    pub rounds: usize,
    pub beam_width: usize,
    pub candidates_per_parent: usize,
    pub gradient_model: String,
    pub edit_model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl Default for ApoConfig {
    fn default() -> Self {
        Self {
            rounds: 3,
            beam_width: 2,
            candidates_per_parent: 1,
            gradient_model: "gpt-4o-mini".to_string(),
            edit_model: "gpt-4o-mini".to_string(),
            temperature: Some(0.0),
            max_tokens: Some(512),
        }
    }
}

/// Prompt templates used by APO's gradient/edit steps.
#[derive(Debug, Clone)]
pub struct ApoTemplates {
    pub gradient_system: String,
    pub gradient_user: String,
    pub edit_system: String,
    pub edit_user: String,
}

impl Default for ApoTemplates {
    fn default() -> Self {
        Self {
            gradient_system:
                "You critique system prompts for an agent. Return concise actionable guidance."
                    .to_string(),
            gradient_user: concat!(
                "Current system prompt:\n{{prompt}}\n\n",
                "Training examples (input => expected):\n{{examples}}\n\n",
                "Explain how to improve this prompt to increase task success."
            )
            .to_string(),
            edit_system:
                "You edit system prompts using critique text. Return only the revised prompt."
                    .to_string(),
            edit_user: concat!(
                "Original prompt:\n{{prompt}}\n\n",
                "Critique:\n{{critique}}\n\n",
                "Revised prompt:"
            )
            .to_string(),
        }
    }
}

impl ApoTemplates {
    fn render_gradient_user(&self, prompt: &str, examples: &[PromptExample]) -> String {
        let examples_rendered = render_examples(examples);
        self.gradient_user
            .replace("{{prompt}}", prompt)
            .replace("{{examples}}", &examples_rendered)
    }

    fn render_edit_user(&self, prompt: &str, critique: &str) -> String {
        self.edit_user
            .replace("{{prompt}}", prompt)
            .replace("{{critique}}", critique)
    }
}

/// APO algorithm implementation.
pub struct ApoAlgorithm {
    gradient_client: Arc<dyn LlmClient>,
    edit_client: Arc<dyn LlmClient>,
    evaluator: Arc<dyn PromptEvaluator>,
    config: ApoConfig,
    templates: ApoTemplates,
}

impl ApoAlgorithm {
    /// Creates APO with explicit clients and evaluator.
    pub fn new(
        gradient_client: Arc<dyn LlmClient>,
        edit_client: Arc<dyn LlmClient>,
        evaluator: Arc<dyn PromptEvaluator>,
        config: ApoConfig,
    ) -> Self {
        Self {
            gradient_client,
            edit_client,
            evaluator,
            config,
            templates: ApoTemplates::default(),
        }
    }

    /// Overrides gradient/edit prompt templates.
    pub fn with_templates(mut self, templates: ApoTemplates) -> Self {
        self.templates = templates;
        self
    }

    async fn request_gradient(&self, prompt: &str, examples: &[PromptExample]) -> Result<String> {
        let user = self.templates.render_gradient_user(prompt, examples);
        request_text_completion(
            self.gradient_client.as_ref(),
            &self.config.gradient_model,
            &self.templates.gradient_system,
            &user,
            self.config.temperature,
            self.config.max_tokens,
        )
        .await
    }

    async fn request_edit(&self, prompt: &str, critique: &str) -> Result<String> {
        let user = self.templates.render_edit_user(prompt, critique);
        request_text_completion(
            self.edit_client.as_ref(),
            &self.config.edit_model,
            &self.templates.edit_system,
            &user,
            self.config.temperature,
            self.config.max_tokens,
        )
        .await
    }

    async fn persist_best_prompt(
        &self,
        ctx: &AlgorithmContext,
        best: &VersionedPrompt,
    ) -> Result<ResourcesUpdate> {
        let mut resources = HashMap::new();
        resources.insert(
            "system_prompt".to_string(),
            Value::String(best.prompt.clone()),
        );
        resources.insert(
            "system_prompt_version".to_string(),
            Value::String(best.version.clone()),
        );
        resources.insert("algorithm".to_string(), Value::String(APO_NAME.to_string()));
        resources.insert("round".to_string(), json!(best.round));
        if let Some(score) = best.score {
            resources.insert("score".to_string(), json!(score));
        }

        ctx.store
            .update_resources(resources)
            .await
            .context("failed to persist APO resource update")
    }
}

#[async_trait]
impl Algorithm for ApoAlgorithm {
    async fn run(&self, ctx: AlgorithmContext) -> Result<AlgorithmRunSummary> {
        let beam_width = self.config.beam_width.max(1);
        let candidates_per_parent = self.config.candidates_per_parent.max(1);

        let baseline_score = self
            .evaluator
            .score_prompt(&ctx.seed_prompt, &ctx.validation_examples)
            .await
            .context("failed to score seed prompt")?;

        let mut beam = vec![VersionedPrompt {
            version: "v0".to_string(),
            prompt: ctx.seed_prompt.clone(),
            score: Some(baseline_score),
            parent_version: None,
            critique: None,
            round: 0,
        }];
        let mut beam_history = beam.clone();
        let mut resource_updates = vec![self.persist_best_prompt(&ctx, &beam[0]).await?];
        let mut rounds_completed = 0;

        for round in 1..=self.config.rounds {
            let parents = take_top_by_score(&beam, beam_width);
            let mut candidates = Vec::new();

            for (parent_idx, parent) in parents.into_iter().enumerate() {
                for candidate_idx in 0..candidates_per_parent {
                    let critique = self
                        .request_gradient(&parent.prompt, &ctx.train_examples)
                        .await
                        .with_context(|| {
                            format!(
                                "failed to generate critique for parent {} in round {}",
                                parent.version, round
                            )
                        })?;
                    let edited_prompt = self
                        .request_edit(&parent.prompt, &critique)
                        .await
                        .with_context(|| {
                            format!(
                                "failed to edit prompt for parent {} in round {}",
                                parent.version, round
                            )
                        })?;
                    let score = self
                        .evaluator
                        .score_prompt(&edited_prompt, &ctx.validation_examples)
                        .await
                        .with_context(|| {
                            format!(
                                "failed to score edited prompt for parent {} in round {}",
                                parent.version, round
                            )
                        })?;

                    candidates.push(VersionedPrompt {
                        version: format!("r{round}-p{}-c{}", parent_idx + 1, candidate_idx + 1),
                        prompt: edited_prompt,
                        score: Some(score),
                        parent_version: Some(parent.version.clone()),
                        critique: Some(critique),
                        round,
                    });
                }
            }

            if candidates.is_empty() {
                break;
            }

            candidates.sort_by(compare_prompt_score_desc);
            beam = candidates.into_iter().take(beam_width).collect();
            beam_history.extend(beam.clone());

            let Some(best) = beam.first() else {
                break;
            };
            resource_updates.push(self.persist_best_prompt(&ctx, best).await?);
            rounds_completed = round;
        }

        let best_prompt = beam_history
            .iter()
            .cloned()
            .max_by(compare_prompt_score_asc);

        Ok(AlgorithmRunSummary {
            algorithm_name: APO_NAME.to_string(),
            rounds_completed,
            best_prompt,
            resource_updates,
            beam_history,
        })
    }
}

fn compare_prompt_score_desc(left: &VersionedPrompt, right: &VersionedPrompt) -> Ordering {
    let left_score = prompt_score(left);
    let right_score = prompt_score(right);

    right_score
        .partial_cmp(&left_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.version.cmp(&right.version))
}

fn compare_prompt_score_asc(left: &VersionedPrompt, right: &VersionedPrompt) -> Ordering {
    let left_score = prompt_score(left);
    let right_score = prompt_score(right);

    left_score
        .partial_cmp(&right_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.version.cmp(&left.version))
}

fn prompt_score(prompt: &VersionedPrompt) -> f64 {
    prompt.score.unwrap_or(f64::NEG_INFINITY)
}

fn take_top_by_score(prompts: &[VersionedPrompt], width: usize) -> Vec<VersionedPrompt> {
    let mut sorted = prompts.to_vec();
    sorted.sort_by(compare_prompt_score_desc);
    sorted.into_iter().take(width).collect()
}

fn render_examples(examples: &[PromptExample]) -> String {
    if examples.is_empty() {
        return "(no examples)".to_string();
    }

    examples
        .iter()
        .enumerate()
        .map(|(index, sample)| format!("{}. {} => {}", index + 1, sample.input, sample.expected))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn request_text_completion(
    client: &dyn LlmClient,
    model: &str,
    system: &str,
    user: &str,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<String> {
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![Message::system(system), Message::user(user)],
        tools: Vec::new(),
        tool_choice: None,
        json_mode: false,
        max_tokens,
        temperature,
    };

    let response = client.complete(request).await?;
    let text = response.message.text_content().trim().to_string();
    if text.is_empty() {
        bail!("llm response text was empty");
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{
        compare_prompt_score_desc, request_text_completion, take_top_by_score, ApoAlgorithm,
        ApoConfig, PromptEvaluator, VersionedPrompt,
    };
    use crate::{Algorithm, AlgorithmContext, PromptExample};
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::json;
    use std::cmp::Ordering;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tau_trainer::{Trainer, TrainerConfig};
    use tau_training_runner::{RolloutExecutionOutcome, RolloutExecutor};
    use tau_training_store::{InMemoryTrainingStore, TrainingStore};
    use tau_training_tracer::TrainingTracer;
    use tau_training_types::{ResourcesUpdate, Reward};

    #[derive(Clone)]
    struct ScriptedClient {
        outputs: Arc<Mutex<VecDeque<String>>>,
    }

    impl ScriptedClient {
        fn new(lines: Vec<&str>) -> Self {
            Self {
                outputs: Arc::new(Mutex::new(
                    lines
                        .into_iter()
                        .map(ToString::to_string)
                        .collect::<VecDeque<_>>(),
                )),
            }
        }
    }

    #[async_trait]
    impl LlmClient for ScriptedClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            let mut outputs = self.outputs.lock().expect("scripted client mutex poisoned");
            let text = outputs
                .pop_front()
                .unwrap_or_else(|| "fallback prompt".to_string());
            Ok(ChatResponse {
                message: Message::assistant_text(text),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct KeywordEvaluator;

    #[async_trait]
    impl PromptEvaluator for KeywordEvaluator {
        async fn score_prompt(&self, prompt: &str, _dataset: &[PromptExample]) -> Result<f64> {
            let score = if prompt.contains("best") {
                0.95
            } else if prompt.contains("better") {
                0.7
            } else {
                0.2
            };
            Ok(score)
        }
    }

    struct ResourceAwareExecutor {
        expected_prompt: String,
    }

    #[async_trait]
    impl RolloutExecutor for ResourceAwareExecutor {
        async fn execute(
            &self,
            _rollout: &tau_training_types::Rollout,
            resources: Option<&ResourcesUpdate>,
            _tracer: Arc<TrainingTracer>,
        ) -> Result<RolloutExecutionOutcome> {
            let active_prompt = resources
                .and_then(|update| update.resources.get("system_prompt"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();

            let reward = if active_prompt == self.expected_prompt {
                1.0
            } else {
                0.0
            };

            Ok(RolloutExecutionOutcome {
                output: json!({ "active_prompt": active_prompt }),
                rewards: vec![Reward::new("resource_prompt_match", reward)],
            })
        }
    }

    #[test]
    fn selects_top_prompts_by_score() {
        let prompts = vec![
            VersionedPrompt {
                version: "a".to_string(),
                prompt: "A".to_string(),
                score: Some(0.2),
                parent_version: None,
                critique: None,
                round: 0,
            },
            VersionedPrompt {
                version: "b".to_string(),
                prompt: "B".to_string(),
                score: Some(0.8),
                parent_version: None,
                critique: None,
                round: 0,
            },
            VersionedPrompt {
                version: "c".to_string(),
                prompt: "C".to_string(),
                score: Some(0.6),
                parent_version: None,
                critique: None,
                round: 0,
            },
        ];

        let top = take_top_by_score(&prompts, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].version, "b");
        assert_eq!(top[1].version, "c");
        assert_eq!(compare_prompt_score_desc(&top[0], &top[1]), Ordering::Less);
    }

    #[tokio::test]
    async fn apo_improves_prompt_over_rounds_and_persists_resources() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        let algorithm = ApoAlgorithm::new(
            Arc::new(ScriptedClient::new(vec![
                "Critique #1",
                "Critique #2",
                "Critique #3",
            ])),
            Arc::new(ScriptedClient::new(vec![
                "better prompt",
                "best prompt",
                "best prompt v2",
            ])),
            Arc::new(KeywordEvaluator),
            ApoConfig {
                rounds: 3,
                beam_width: 1,
                candidates_per_parent: 1,
                ..ApoConfig::default()
            },
        );

        let summary = algorithm
            .run(AlgorithmContext::new(
                store.clone(),
                "base prompt",
                vec![PromptExample::new("2+2", "4")],
                vec![PromptExample::new("2+2", "4")],
            ))
            .await
            .expect("apo run");

        let best = summary.best_prompt.expect("best prompt");
        assert!(best.score.expect("best score") > 0.2);
        assert!(best.prompt.contains("best"));
        assert_eq!(summary.rounds_completed, 3);
        assert_eq!(summary.resource_updates.len(), 4);

        let latest = store
            .get_latest_resources()
            .await
            .expect("latest resources")
            .expect("resources present");
        let latest_prompt = latest
            .resources
            .get("system_prompt")
            .and_then(serde_json::Value::as_str)
            .expect("system prompt in resources");
        assert!(latest_prompt.contains("best"));
    }

    #[tokio::test]
    async fn integration_apo_updates_resources_consumed_by_trainer_runner() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        let algorithm = ApoAlgorithm::new(
            Arc::new(ScriptedClient::new(vec![
                "Improve structure",
                "Refine wording",
            ])),
            Arc::new(ScriptedClient::new(vec!["better prompt", "best prompt"])),
            Arc::new(KeywordEvaluator),
            ApoConfig {
                rounds: 2,
                beam_width: 1,
                candidates_per_parent: 1,
                ..ApoConfig::default()
            },
        );

        let summary = algorithm
            .run(AlgorithmContext::new(
                store.clone(),
                "base prompt",
                vec![PromptExample::new("x", "y")],
                vec![PromptExample::new("x", "y")],
            ))
            .await
            .expect("apo run");
        let best_prompt = summary.best_prompt.expect("best prompt").prompt;

        let trainer = Trainer::new(
            store,
            TrainerConfig {
                worker_count: 1,
                completion_timeout: std::time::Duration::from_secs(10),
                poll_interval: std::time::Duration::from_millis(20),
                heartbeat_interval: std::time::Duration::from_millis(30),
                completion_poll_interval: std::time::Duration::from_millis(20),
            },
        );

        let training_summary = trainer
            .fit(
                Arc::new(ResourceAwareExecutor {
                    expected_prompt: best_prompt,
                }),
                Some(vec![json!({ "prompt": "task" })]),
                Option::<Vec<serde_json::Value>>::None,
            )
            .await
            .expect("trainer fit");

        assert_eq!(training_summary.succeeded, 1);
    }

    #[tokio::test]
    async fn rejects_empty_llm_output() {
        let client = ScriptedClient::new(vec!["   "]);
        let error = request_text_completion(
            &client,
            "gpt-4o-mini",
            "system",
            "user",
            Some(0.0),
            Some(16),
        )
        .await
        .expect_err("empty output should fail");

        assert!(error.to_string().contains("empty"));
    }
}
