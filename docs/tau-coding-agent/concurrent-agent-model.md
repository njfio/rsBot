# Concurrent Agent Model

## Context

`tau-agent-core::Agent` is stateful and historically `&mut self` driven. That made single-agent CLI flows straightforward, but it forced library users to orchestrate concurrency externally.

Wave 3 introduces a library-facing concurrency model that keeps current ergonomics while enabling safe parallel runs.

## Problem

We needed a model that allows concurrent agent execution without:

- sharing mutable message state across runs
- breaking existing `Agent::prompt` and `Agent::continue_turn` behavior
- forcing callers to redesign their current single-agent flow

## Options Considered

1. `Arc<Mutex<Agent>>` shared mutable agent
- Pros: simple surface
- Cons: serializes work through one lock, weak isolation, harder failure boundaries

2. Split immutable runtime and mutable session state
- Pros: strongest long-term architecture
- Cons: larger refactor and migration cost for this wave

3. Clone/fork from a configured base agent (selected)
- Pros: minimal migration, strong isolation, concurrent execution possible immediately
- Cons: each fork keeps independent message history (intentional tradeoff)

## Selected Design

### New API surface

- `Agent::fork(&self) -> Agent`
- `Agent::run_parallel_prompts<I, S>(&self, prompts: I, max_parallel_runs: usize) -> Vec<Result<Vec<Message>, AgentError>>`

where:

- each prompt run executes on an isolated fork
- tools, provider client, and event subscribers are inherited from the base agent
- result ordering is deterministic (matches input prompt order)
- failures are per-run and do not cancel sibling runs

### Safety and state boundaries

- Message history is isolated per fork.
- Tool implementations remain `Send + Sync` and shared through `Arc`, so no mutable aliasing is introduced in core runtime state.
- Join failures are converted into typed agent errors, keeping the API fail-closed.

## Behavioral notes

- This model is for concurrent *runs*, not shared mutable transcript collaboration.
- Callers that need a shared conversation should continue using a single `Agent` instance.

## Migration guidance

Single-agent code remains unchanged:

```rust
let mut agent = Agent::new(client, config);
let reply = agent.prompt("hello").await?;
```

Parallel fan-out code:

```rust
let agent = Agent::new(client, config);
let results = agent
    .run_parallel_prompts(vec!["task A", "task B", "task C"], 3)
    .await;
```

## Validation

Added tests cover:

- fork state isolation and inherited tool behavior
- concurrent parallel execution timing/ordering
- per-prompt failure isolation
- empty-input behavior
