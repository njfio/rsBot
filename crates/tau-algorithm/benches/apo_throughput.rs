use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
use tau_algorithm::{
    Algorithm, AlgorithmContext, ApoAlgorithm, ApoConfig, PromptEvaluator, PromptExample,
};
use tau_training_store::{InMemoryTrainingStore, TrainingStore};

#[derive(Clone)]
struct ConstantClient {
    value: String,
}

#[async_trait]
impl LlmClient for ConstantClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        Ok(ChatResponse {
            message: Message::assistant_text(self.value.clone()),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })
    }
}

struct LengthEvaluator;

#[async_trait]
impl PromptEvaluator for LengthEvaluator {
    async fn score_prompt(&self, prompt: &str, _dataset: &[PromptExample]) -> anyhow::Result<f64> {
        Ok(prompt.len() as f64)
    }
}

fn bench_apo_throughput(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    c.bench_function("apo_run_two_rounds", |bench| {
        bench.iter(|| {
            runtime.block_on(async {
                let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
                let algorithm = ApoAlgorithm::new(
                    Arc::new(ConstantClient {
                        value: "critique".to_string(),
                    }),
                    Arc::new(ConstantClient {
                        value: "better prompt".to_string(),
                    }),
                    Arc::new(LengthEvaluator),
                    ApoConfig {
                        rounds: 2,
                        beam_width: 2,
                        candidates_per_parent: 2,
                        ..ApoConfig::default()
                    },
                );

                let _summary = algorithm
                    .run(AlgorithmContext::new(
                        store,
                        "seed prompt",
                        vec![PromptExample::new("input", "expected")],
                        vec![PromptExample::new("input", "expected")],
                    ))
                    .await
                    .expect("apo run");
            });
        })
    });
}

criterion_group!(benches, bench_apo_throughput);
criterion_main!(benches);
