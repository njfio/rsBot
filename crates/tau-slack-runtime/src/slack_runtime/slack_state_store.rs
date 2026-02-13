use std::{
    collections::HashSet,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{write_text_atomic, TransportHealthSnapshot, SLACK_STATE_SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackBridgeState {
    schema_version: u32,
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for SlackBridgeState {
    fn default() -> Self {
        Self {
            schema_version: SLACK_STATE_SCHEMA_VERSION,
            processed_event_keys: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub(super) struct SlackBridgeStateStore {
    path: PathBuf,
    cap: usize,
    state: SlackBridgeState,
    processed_index: HashSet<String>,
}

impl SlackBridgeStateStore {
    pub(super) fn load(path: PathBuf, cap: usize) -> Result<Self> {
        let mut state = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read state file {}", path.display()))?;
            serde_json::from_str::<SlackBridgeState>(&raw).with_context(|| {
                format!("failed to parse slack bridge state file {}", path.display())
            })?
        } else {
            SlackBridgeState::default()
        };

        if state.schema_version != SLACK_STATE_SCHEMA_VERSION {
            bail!(
                "unsupported slack bridge state schema: expected {}, found {}",
                SLACK_STATE_SCHEMA_VERSION,
                state.schema_version
            );
        }

        let cap = cap.max(1);
        if state.processed_event_keys.len() > cap {
            let keep_from = state.processed_event_keys.len() - cap;
            state.processed_event_keys = state.processed_event_keys[keep_from..].to_vec();
        }

        let processed_index = state
            .processed_event_keys
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        Ok(Self {
            path,
            cap,
            state,
            processed_index,
        })
    }

    pub(super) fn contains(&self, key: &str) -> bool {
        self.processed_index.contains(key)
    }

    pub(super) fn mark_processed(&mut self, key: &str) -> bool {
        if self.processed_index.contains(key) {
            return false;
        }
        self.state.processed_event_keys.push(key.to_string());
        self.processed_index.insert(key.to_string());
        while self.state.processed_event_keys.len() > self.cap {
            let removed = self.state.processed_event_keys.remove(0);
            self.processed_index.remove(&removed);
        }
        true
    }

    pub(super) fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    pub(super) fn update_transport_health(&mut self, value: TransportHealthSnapshot) -> bool {
        if self.state.health == value {
            return false;
        }
        self.state.health = value;
        true
    }

    pub(super) fn save(&self) -> Result<()> {
        let mut payload =
            serde_json::to_string_pretty(&self.state).context("failed to serialize state")?;
        payload.push('\n');
        write_text_atomic(&self.path, &payload)
            .with_context(|| format!("failed to write state file {}", self.path.display()))?;
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct JsonlEventLog {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
}

impl JsonlEventLog {
    pub(super) fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub(super) fn append(&self, value: &Value) -> Result<()> {
        let line = serde_json::to_string(value).context("failed to encode log event")?;
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("event log mutex is poisoned"))?;
        writeln!(file, "{line}")
            .with_context(|| format!("failed to append to {}", self.path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush {}", self.path.display()))?;
        Ok(())
    }
}
