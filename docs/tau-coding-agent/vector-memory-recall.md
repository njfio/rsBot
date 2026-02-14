# Vector Memory Recall (Issue #1188)

Tau now supports deterministic vector-based memory recall for truncated histories in `tau-agent-core`.

## What changed

- Added memory retrieval controls to `AgentConfig`:
  - `memory_retrieval_limit` (default `3`)
  - `memory_embedding_dimensions` (default `128`)
  - `memory_min_similarity` (default `0.55`)
  - `memory_max_chars_per_item` (default `180`)
- Added bounded recall injection in `Agent::request_messages()`:
  - when `max_context_messages` truncates history and recall is enabled, a system block is inserted with prefix `[Tau memory recall]`
  - recall is inserted after existing system prompts to preserve role ordering
- Added deterministic hashing-based embeddings and cosine similarity ranking:
  - `embed_text_vector(...)`
  - `retrieve_memory_matches(...)`
  - `cosine_similarity(...)`
  - `fnv1a_hash(...)`

## Safety and behavior

- Recall only considers historical `user` and `assistant` messages.
- Empty query/candidate messages are skipped.
- Setting `memory_retrieval_limit=0` disables recall.
- If no truncated history exists, no recall message is added.

## Tests added

- Unit:
  - vector retrieval prefers semantically related history
- Functional:
  - request shaping attaches recall when relevant truncated history exists
- Integration:
  - ranking keeps relevant history above unrelated entries
- Regression:
  - recall disabled cleanly when retrieval limit is zero

## Validation

- `cargo fmt --all`
- `cargo test -p tau-agent-core`
- `cargo test -p tau-runtime`
- `cargo test -p tau-coding-agent run_prompt_with_cancellation`
- `cargo check --workspace`
- `cargo clippy -p tau-agent-core -p tau-runtime -p tau-coding-agent --all-targets -- -D warnings`
