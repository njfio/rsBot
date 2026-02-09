use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct TransportHealthSnapshot {
    #[serde(default)]
    pub(crate) updated_unix_ms: u64,
    #[serde(default)]
    pub(crate) cycle_duration_ms: u64,
    #[serde(default)]
    pub(crate) queue_depth: usize,
    #[serde(default)]
    pub(crate) active_runs: usize,
    #[serde(default)]
    pub(crate) failure_streak: usize,
    #[serde(default)]
    pub(crate) last_cycle_discovered: usize,
    #[serde(default)]
    pub(crate) last_cycle_processed: usize,
    #[serde(default)]
    pub(crate) last_cycle_completed: usize,
    #[serde(default)]
    pub(crate) last_cycle_failed: usize,
    #[serde(default)]
    pub(crate) last_cycle_duplicates: usize,
}
