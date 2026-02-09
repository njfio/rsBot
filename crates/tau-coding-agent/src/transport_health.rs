use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransportHealthState {
    Healthy,
    Degraded,
    Failing,
}

impl TransportHealthState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Failing => "failing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransportHealthClassification {
    pub(crate) state: TransportHealthState,
    pub(crate) reason: String,
    pub(crate) recommendation: &'static str,
}

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
    pub(crate) fn classify(&self) -> TransportHealthClassification {
        if self.failure_streak >= 3 {
            return TransportHealthClassification {
                state: TransportHealthState::Failing,
                reason: format!(
                    "transport failures are sustained (failure_streak={})",
                    self.failure_streak
                ),
                recommendation:
                    "check bridge credentials/connectivity and restart the transport worker",
            };
        }

        if self.failure_streak > 0 || self.last_cycle_failed > 0 {
            return TransportHealthClassification {
                state: TransportHealthState::Degraded,
                reason: format!(
                    "recent transport failures observed (failure_streak={}, last_cycle_failed={})",
                    self.failure_streak, self.last_cycle_failed
                ),
                recommendation: "inspect bridge logs and watch the next poll cycle",
            };
        }

        TransportHealthClassification {
            state: TransportHealthState::Healthy,
            reason: "no recent transport failures observed".to_string(),
            recommendation: "no immediate action required",
        }
    }

    pub(crate) fn health_detail_lines(&self) -> Vec<String> {
        let classification = self.classify();
        let mut lines = vec![
            format!("transport_health_reason: {}", classification.reason),
            format!(
                "transport_health_recommendation: {}",
                classification.recommendation
            ),
        ];
        lines.extend(self.status_lines());
        lines
    }

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
    use super::{TransportHealthSnapshot, TransportHealthState};

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

    #[test]
    fn unit_transport_health_classify_reports_degraded_and_failing_states() {
        let degraded = TransportHealthSnapshot {
            failure_streak: 1,
            last_cycle_failed: 1,
            ..TransportHealthSnapshot::default()
        };
        let degraded_classification = degraded.classify();
        assert_eq!(
            degraded_classification.state,
            TransportHealthState::Degraded
        );
        assert!(degraded_classification.reason.contains("failure_streak=1"));

        let failing = TransportHealthSnapshot {
            failure_streak: 3,
            ..TransportHealthSnapshot::default()
        };
        let failing_classification = failing.classify();
        assert_eq!(failing_classification.state, TransportHealthState::Failing);
        assert!(failing_classification.reason.contains("failure_streak=3"));
    }

    #[test]
    fn regression_transport_health_detail_lines_include_reason_and_recommendation() {
        let lines = TransportHealthSnapshot::default().health_detail_lines();
        assert!(lines[0].contains("transport_health_reason:"));
        assert!(lines[1].contains("transport_health_recommendation:"));
        assert!(lines
            .iter()
            .any(|line| line == "transport_failure_streak: 0"));
    }
}
