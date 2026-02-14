use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TRANSPORT_REPLAY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `TransportReplayKind` values.
pub enum TransportReplayKind {
    GithubIssues,
    SlackSocket,
    EventScheduler,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `TransportReplayFault` values.
pub enum TransportReplayFault {
    Duplicate,
    OutOfOrder,
    TransientFailure,
    StoreCorruption,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `TransportReplayFixtureEvent` used across Tau components.
pub struct TransportReplayFixtureEvent {
    pub sequence: u64,
    pub event_key: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub faults: Vec<TransportReplayFault>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `TransportReplayFixture` used across Tau components.
pub struct TransportReplayFixture {
    pub schema_version: u32,
    pub name: String,
    pub transport: TransportReplayKind,
    #[serde(default)]
    pub description: String,
    pub events: Vec<TransportReplayFixtureEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ReplayDispatchKind` values.
pub enum ReplayDispatchKind {
    Delivery,
    TransientProbe,
    CorruptionMarker,
}

#[derive(Debug, Clone, PartialEq)]
/// Public struct `ReplayDispatchEvent` used across Tau components.
pub struct ReplayDispatchEvent {
    pub sequence: u64,
    pub event_key: String,
    pub channel: String,
    pub kind: String,
    pub payload: Value,
    pub duplicate_delivery: bool,
    pub dispatch_kind: ReplayDispatchKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ReplayStep` values.
pub enum ReplayStep {
    Applied,
    DuplicateIgnored,
    TransientFailure,
    Recovered,
    Skipped,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `TransportReplaySummary` used across Tau components.
pub struct TransportReplaySummary {
    pub discovered_events: usize,
    pub dispatched_events: usize,
    pub applied_events: usize,
    pub duplicate_events: usize,
    pub transient_failures: usize,
    pub recovered_events: usize,
    pub skipped_events: usize,
    pub corruption_markers: usize,
    pub repair_actions: usize,
}

/// Trait contract for `TransportReplayDriver` behavior.
pub trait TransportReplayDriver {
    fn apply(&mut self, dispatch: &ReplayDispatchEvent) -> Result<ReplayStep>;

    fn repair_after_corruption(&mut self, _dispatch: &ReplayDispatchEvent) -> Result<()> {
        Ok(())
    }
}

pub fn parse_transport_replay_fixture(raw: &str) -> Result<TransportReplayFixture> {
    let fixture = serde_json::from_str::<TransportReplayFixture>(raw)
        .context("failed to parse transport replay fixture")?;
    validate_transport_replay_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_transport_replay_fixture(path: &Path) -> Result<TransportReplayFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_transport_replay_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn validate_transport_replay_fixture(fixture: &TransportReplayFixture) -> Result<()> {
    if fixture.schema_version != TRANSPORT_REPLAY_SCHEMA_VERSION {
        bail!(
            "unsupported transport replay schema version {} (expected {})",
            fixture.schema_version,
            TRANSPORT_REPLAY_SCHEMA_VERSION
        );
    }
    if fixture.name.trim().is_empty() {
        bail!("fixture name cannot be empty");
    }
    if fixture.events.is_empty() {
        bail!("fixture must include at least one event");
    }

    let mut sequences = HashSet::new();
    for event in &fixture.events {
        if !sequences.insert(event.sequence) {
            bail!("fixture contains duplicate sequence {}", event.sequence);
        }
        if event.event_key.trim().is_empty() {
            bail!(
                "fixture event sequence {} has empty event_key",
                event.sequence
            );
        }
        let mut seen_faults = HashSet::new();
        for fault in &event.faults {
            if !seen_faults.insert(*fault) {
                bail!(
                    "fixture event sequence {} contains duplicate fault {:?}",
                    event.sequence,
                    fault
                );
            }
        }
    }

    Ok(())
}

pub fn materialize_transport_replay(fixture: &TransportReplayFixture) -> Vec<ReplayDispatchEvent> {
    let mut ordered = fixture.events.clone();
    ordered.sort_by_key(|event| event.sequence);

    let mut dispatches = Vec::new();
    for event in ordered {
        let has_transient = event
            .faults
            .contains(&TransportReplayFault::TransientFailure);
        let has_out_of_order = event.faults.contains(&TransportReplayFault::OutOfOrder);
        let has_duplicate = event.faults.contains(&TransportReplayFault::Duplicate);
        let has_corruption = event
            .faults
            .contains(&TransportReplayFault::StoreCorruption);

        if has_transient {
            dispatches.push(ReplayDispatchEvent {
                sequence: event.sequence,
                event_key: event.event_key.clone(),
                channel: event.channel.clone(),
                kind: event.kind.clone(),
                payload: event.payload.clone(),
                duplicate_delivery: false,
                dispatch_kind: ReplayDispatchKind::TransientProbe,
            });
        }

        dispatches.push(ReplayDispatchEvent {
            sequence: event.sequence,
            event_key: event.event_key.clone(),
            channel: event.channel.clone(),
            kind: event.kind.clone(),
            payload: event.payload.clone(),
            duplicate_delivery: false,
            dispatch_kind: ReplayDispatchKind::Delivery,
        });

        if has_out_of_order {
            let current_index = dispatches.len() - 1;
            if let Some(previous_index) = (0..current_index).rev().find(|index| {
                dispatches[*index].dispatch_kind != ReplayDispatchKind::CorruptionMarker
            }) {
                dispatches.swap(current_index, previous_index);
            }
        }

        if has_duplicate {
            dispatches.push(ReplayDispatchEvent {
                sequence: event.sequence,
                event_key: event.event_key.clone(),
                channel: event.channel.clone(),
                kind: event.kind.clone(),
                payload: event.payload.clone(),
                duplicate_delivery: true,
                dispatch_kind: ReplayDispatchKind::Delivery,
            });
        }

        if has_corruption {
            dispatches.push(ReplayDispatchEvent {
                sequence: event.sequence,
                event_key: event.event_key,
                channel: event.channel,
                kind: event.kind,
                payload: Value::Null,
                duplicate_delivery: false,
                dispatch_kind: ReplayDispatchKind::CorruptionMarker,
            });
        }
    }

    dispatches
}

pub fn run_transport_replay<D: TransportReplayDriver>(
    fixture: &TransportReplayFixture,
    driver: &mut D,
) -> Result<TransportReplaySummary> {
    validate_transport_replay_fixture(fixture)?;
    let dispatches = materialize_transport_replay(fixture);

    let mut summary = TransportReplaySummary {
        discovered_events: fixture.events.len(),
        ..TransportReplaySummary::default()
    };

    for dispatch in &dispatches {
        match dispatch.dispatch_kind {
            ReplayDispatchKind::CorruptionMarker => {
                summary.corruption_markers = summary.corruption_markers.saturating_add(1);
                driver.repair_after_corruption(dispatch)?;
                summary.repair_actions = summary.repair_actions.saturating_add(1);
            }
            ReplayDispatchKind::Delivery | ReplayDispatchKind::TransientProbe => {
                summary.dispatched_events = summary.dispatched_events.saturating_add(1);
                match driver.apply(dispatch)? {
                    ReplayStep::Applied => {
                        summary.applied_events = summary.applied_events.saturating_add(1);
                    }
                    ReplayStep::DuplicateIgnored => {
                        summary.duplicate_events = summary.duplicate_events.saturating_add(1);
                    }
                    ReplayStep::TransientFailure => {
                        summary.transient_failures = summary.transient_failures.saturating_add(1);
                    }
                    ReplayStep::Recovered => {
                        summary.recovered_events = summary.recovered_events.saturating_add(1);
                    }
                    ReplayStep::Skipped => {
                        summary.skipped_events = summary.skipped_events.saturating_add(1);
                    }
                }
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};
    use std::path::{Path, PathBuf};

    use super::{
        load_transport_replay_fixture, materialize_transport_replay,
        parse_transport_replay_fixture, run_transport_replay, ReplayDispatchEvent,
        ReplayDispatchKind, ReplayStep, TransportReplayDriver,
    };
    use anyhow::Result;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("transport-replay")
            .join(name)
    }

    struct IdempotentReplayDriver {
        processed: HashSet<String>,
        transient_failed: HashSet<String>,
        channel_applies: BTreeMap<String, usize>,
        corruption_repairs: usize,
        transport_name: &'static str,
    }

    impl IdempotentReplayDriver {
        fn new(transport_name: &'static str) -> Self {
            Self {
                processed: HashSet::new(),
                transient_failed: HashSet::new(),
                channel_applies: BTreeMap::new(),
                corruption_repairs: 0,
                transport_name,
            }
        }
    }

    impl TransportReplayDriver for IdempotentReplayDriver {
        fn apply(&mut self, dispatch: &ReplayDispatchEvent) -> Result<ReplayStep> {
            if dispatch.dispatch_kind == ReplayDispatchKind::TransientProbe {
                self.transient_failed.insert(dispatch.event_key.clone());
                return Ok(ReplayStep::TransientFailure);
            }

            if !self.processed.insert(dispatch.event_key.clone()) {
                return Ok(ReplayStep::DuplicateIgnored);
            }

            if self.transient_failed.remove(&dispatch.event_key) {
                let count = self
                    .channel_applies
                    .entry(dispatch.channel.clone())
                    .or_default();
                *count = count.saturating_add(1);
                return Ok(ReplayStep::Recovered);
            }

            if dispatch.kind == "noop" {
                return Ok(ReplayStep::Skipped);
            }

            let count = self
                .channel_applies
                .entry(dispatch.channel.clone())
                .or_default();
            *count = count.saturating_add(1);
            Ok(ReplayStep::Applied)
        }

        fn repair_after_corruption(&mut self, _dispatch: &ReplayDispatchEvent) -> Result<()> {
            self.corruption_repairs = self.corruption_repairs.saturating_add(1);
            Ok(())
        }
    }

    #[test]
    fn unit_transport_conformance_parse_rejects_invalid_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "bad",
  "transport": "github_issues",
  "events": [{"sequence":1,"event_key":"x"}]
}"#;
        let error = parse_transport_replay_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported transport replay schema version"));
    }

    #[test]
    fn unit_transport_conformance_materialize_injects_fault_dispatches() {
        let fixture =
            load_transport_replay_fixture(&fixture_path("slack-transient-duplicate.json"))
                .expect("fixture loads");
        let dispatches = materialize_transport_replay(&fixture);

        assert!(dispatches
            .iter()
            .any(|event| event.dispatch_kind == ReplayDispatchKind::TransientProbe));
        assert!(dispatches.iter().any(|event| event.duplicate_delivery));
    }

    #[test]
    fn functional_transport_conformance_runner_summarizes_categories() {
        let fixture = load_transport_replay_fixture(&fixture_path("events-corruption-repair.json"))
            .expect("fixture loads");
        let mut driver = IdempotentReplayDriver::new("events");
        let summary = run_transport_replay(&fixture, &mut driver).expect("replay succeeds");

        assert!(summary.discovered_events >= 3);
        assert!(summary.dispatched_events >= 3);
        assert!(summary.transient_failures >= 1);
        assert!(summary.recovered_events >= 1);
        assert!(summary.corruption_markers >= 1);
        assert_eq!(summary.corruption_markers, summary.repair_actions);
        assert!(driver.corruption_repairs >= 1);
    }

    #[test]
    fn integration_transport_conformance_github_replay_fixture_is_deterministic() {
        let fixture =
            load_transport_replay_fixture(&fixture_path("github-duplicate-out-of-order.json"))
                .expect("fixture loads");

        let mut first_driver = IdempotentReplayDriver::new("github");
        let first = run_transport_replay(&fixture, &mut first_driver).expect("first replay");

        let mut second_driver = IdempotentReplayDriver::new("github");
        let second = run_transport_replay(&fixture, &mut second_driver).expect("second replay");

        assert_eq!(first, second);
        assert_eq!(first_driver.channel_applies, second_driver.channel_applies);
        assert!(first.applied_events >= 2);
        assert!(first.duplicate_events >= 1);
    }

    #[test]
    fn integration_transport_conformance_slack_replay_fixture_is_deterministic() {
        let fixture =
            load_transport_replay_fixture(&fixture_path("slack-transient-duplicate.json"))
                .expect("fixture loads");

        let mut driver = IdempotentReplayDriver::new("slack");
        let summary = run_transport_replay(&fixture, &mut driver).expect("replay succeeds");

        assert_eq!(driver.transport_name, "slack");
        assert!(summary.applied_events >= 1);
        assert!(summary.duplicate_events >= 1);
        assert!(summary.transient_failures >= 1);
        assert!(summary.recovered_events >= 1);
    }

    #[test]
    fn integration_transport_conformance_events_replay_fixture_handles_repair() {
        let fixture = load_transport_replay_fixture(&fixture_path("events-corruption-repair.json"))
            .expect("fixture loads");

        let mut driver = IdempotentReplayDriver::new("events");
        let summary = run_transport_replay(&fixture, &mut driver).expect("replay succeeds");

        assert!(summary.corruption_markers >= 1);
        assert_eq!(summary.corruption_markers, driver.corruption_repairs);
        assert!(summary.applied_events + summary.recovered_events >= 2);
    }

    #[test]
    fn regression_transport_conformance_duplicate_and_out_of_order_stay_idempotent() {
        let fixture =
            load_transport_replay_fixture(&fixture_path("github-duplicate-out-of-order.json"))
                .expect("fixture loads");
        let mut driver = IdempotentReplayDriver::new("github");

        let _first = run_transport_replay(&fixture, &mut driver).expect("first replay");
        let second = run_transport_replay(&fixture, &mut driver).expect("second replay");

        assert_eq!(second.applied_events, 0);
        assert!(second.duplicate_events >= 2);
    }
}
