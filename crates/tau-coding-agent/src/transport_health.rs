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

impl TransportHealthSnapshot {
    pub(crate) fn status_lines(&self) -> Vec<String> {
        vec![
            format!("transport_updated_unix_ms: {}", self.updated_unix_ms),
            format!("transport_cycle_duration_ms: {}", self.cycle_duration_ms),
            format!("transport_queue_depth: {}", self.queue_depth),
            format!("transport_active_runs: {}", self.active_runs),
            format!("transport_failure_streak: {}", self.failure_streak),
            format!(
                "transport_last_cycle_discovered: {}",
                self.last_cycle_discovered
            ),
            format!(
                "transport_last_cycle_processed: {}",
                self.last_cycle_processed
            ),
            format!(
                "transport_last_cycle_completed: {}",
                self.last_cycle_completed
            ),
            format!("transport_last_cycle_failed: {}", self.last_cycle_failed),
            format!(
                "transport_last_cycle_duplicates: {}",
                self.last_cycle_duplicates
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::TransportHealthSnapshot;

    #[test]
    fn unit_transport_health_status_lines_are_deterministic() {
        let snapshot = TransportHealthSnapshot {
            updated_unix_ms: 100,
            cycle_duration_ms: 25,
            queue_depth: 3,
            active_runs: 1,
            failure_streak: 2,
            last_cycle_discovered: 10,
            last_cycle_processed: 8,
            last_cycle_completed: 7,
            last_cycle_failed: 1,
            last_cycle_duplicates: 2,
        };
        let lines = snapshot.status_lines();
        assert_eq!(lines[0], "transport_updated_unix_ms: 100");
        assert_eq!(lines[3], "transport_active_runs: 1");
        assert_eq!(lines[4], "transport_failure_streak: 2");
        assert_eq!(lines[9], "transport_last_cycle_duplicates: 2");
    }

    #[test]
    fn regression_transport_health_status_lines_render_default_zero_values() {
        let snapshot = TransportHealthSnapshot::default();
        let lines = snapshot.status_lines();
        assert_eq!(lines[0], "transport_updated_unix_ms: 0");
        assert_eq!(lines[4], "transport_failure_streak: 0");
        assert_eq!(lines[9], "transport_last_cycle_duplicates: 0");
    }
}
