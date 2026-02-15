//! SQLite-backed `TrainingStore` implementation with durable persistence.

use crate::{
    Attempt, AttemptStatus, DequeuedRollout, ResourcesUpdate, Rollout, RolloutQuery, RolloutStatus,
    StoreResult, TrainingSpan, TrainingStore, TrainingStoreError, WorkerState,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tau_training_types::{RolloutConfig, RolloutMode};
use tokio::sync::Notify;

/// Persistent SQLite store backend used for training workflows.
#[derive(Debug)]
pub struct SqliteTrainingStore {
    db_path: PathBuf,
    notify: Notify,
}

impl SqliteTrainingStore {
    /// Creates a SQLite-backed store at `path`, creating schema if needed.
    pub fn new(path: impl AsRef<Path>) -> StoreResult<Self> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut store = Self {
            db_path,
            notify: Notify::new(),
        };
        let connection = store.open_connection()?;
        store.initialize_schema(&connection)?;
        Ok(store)
    }

    fn open_connection(&self) -> StoreResult<Connection> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            "#,
        )?;
        Ok(connection)
    }

    fn initialize_schema(&mut self, connection: &Connection) -> StoreResult<()> {
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS rollouts (
                rollout_id TEXT PRIMARY KEY,
                input_json TEXT NOT NULL,
                start_time TEXT NULL,
                end_time TEXT NULL,
                mode TEXT NULL,
                status TEXT NOT NULL,
                config_json TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                assigned_worker_id TEXT NULL,
                attempt_count INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS rollout_queue (
                queue_id INTEGER PRIMARY KEY AUTOINCREMENT,
                rollout_id TEXT NOT NULL UNIQUE,
                FOREIGN KEY(rollout_id) REFERENCES rollouts(rollout_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS attempts (
                attempt_id TEXT PRIMARY KEY,
                rollout_id TEXT NOT NULL,
                sequence_id INTEGER NOT NULL,
                worker_id TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                last_heartbeat_at TEXT NOT NULL,
                ended_at TEXT NULL,
                error_message TEXT NULL,
                FOREIGN KEY(rollout_id) REFERENCES rollouts(rollout_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_attempts_rollout ON attempts (rollout_id, sequence_id);

            CREATE TABLE IF NOT EXISTS spans (
                span_row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                rollout_id TEXT NOT NULL,
                attempt_id TEXT NOT NULL,
                sequence_id INTEGER NOT NULL,
                span_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_spans_rollout_attempt
                ON spans (rollout_id, attempt_id, sequence_id);

            CREATE TABLE IF NOT EXISTS resources (
                resources_id TEXT PRIMARY KEY,
                version INTEGER NOT NULL UNIQUE,
                resources_json TEXT NOT NULL,
                created_time TEXT NOT NULL,
                is_latest INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_resources_latest ON resources (is_latest, version);

            CREATE TABLE IF NOT EXISTS workers (
                worker_id TEXT PRIMARY KEY,
                registered_at TEXT NOT NULL,
                last_heartbeat_at TEXT NOT NULL,
                active_rollout_id TEXT NULL,
                active_attempt_id TEXT NULL
            );
            "#,
        )?;
        Ok(())
    }
}

#[async_trait]
impl TrainingStore for SqliteTrainingStore {
    async fn enqueue_rollout(&self, mut rollout: Rollout) -> StoreResult<()> {
        rollout.status = RolloutStatus::Queuing;
        rollout.assigned_worker_id = None;
        rollout.start_time = None;
        rollout.end_time = None;
        rollout.attempt_count = 0;

        let mut connection = self.open_connection()?;
        let transaction = connection.transaction()?;

        let exists = transaction
            .query_row(
                "SELECT 1 FROM rollouts WHERE rollout_id = ?1",
                params![rollout.rollout_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(TrainingStoreError::RolloutAlreadyExists(rollout.rollout_id));
        }

        transaction.execute(
            r#"
            INSERT INTO rollouts (
                rollout_id, input_json, start_time, end_time, mode, status,
                config_json, metadata_json, assigned_worker_id, attempt_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                rollout.rollout_id,
                serialize_json(&rollout.input)?,
                option_timestamp_to_db(rollout.start_time),
                option_timestamp_to_db(rollout.end_time),
                rollout.mode.map(rollout_mode_to_db),
                rollout_status_to_db(rollout.status),
                serialize_json(&rollout.config)?,
                serialize_json(&rollout.metadata)?,
                rollout.assigned_worker_id,
                i64::from(rollout.attempt_count),
            ],
        )?;
        transaction.execute(
            "INSERT INTO rollout_queue (rollout_id) VALUES (?1)",
            params![rollout.rollout_id],
        )?;
        transaction.commit()?;

        self.notify.notify_waiters();
        Ok(())
    }

    async fn dequeue_rollout(&self, worker_id: &str) -> StoreResult<Option<DequeuedRollout>> {
        let mut connection = self.open_connection()?;

        loop {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let queued: Option<(i64, String)> = transaction
                .query_row(
                    "SELECT queue_id, rollout_id FROM rollout_queue ORDER BY queue_id LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            let Some((queue_id, rollout_id)) = queued else {
                transaction.commit()?;
                return Ok(None);
            };

            let rollout_row: Option<Rollout> = transaction
                .query_row(
                    r#"
                    SELECT
                        rollout_id, input_json, start_time, end_time, mode, status, config_json,
                        metadata_json, assigned_worker_id, attempt_count
                    FROM rollouts
                    WHERE rollout_id = ?1
                    "#,
                    params![rollout_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, Option<String>>(8)?,
                            row.get::<_, i64>(9)?,
                        ))
                    },
                )
                .optional()?
                .map(
                    |(
                        rollout_id,
                        input_json,
                        start_time,
                        end_time,
                        mode,
                        status,
                        config_json,
                        metadata_json,
                        assigned_worker_id,
                        attempt_count,
                    )|
                     -> StoreResult<Rollout> {
                        Ok(Rollout {
                            rollout_id,
                            input: deserialize_json(&input_json)?,
                            start_time: option_timestamp_from_db(start_time)?,
                            end_time: option_timestamp_from_db(end_time)?,
                            mode: option_rollout_mode_from_db(mode)?,
                            status: rollout_status_from_db(&status)?,
                            config: deserialize_json(&config_json)?,
                            metadata: deserialize_json(&metadata_json)?,
                            assigned_worker_id,
                            attempt_count: i64_to_u32("attempt_count", attempt_count)?,
                        })
                    },
                )
                .transpose()?;

            let Some(mut rollout) = rollout_row else {
                transaction.execute(
                    "DELETE FROM rollout_queue WHERE queue_id = ?1",
                    params![queue_id],
                )?;
                transaction.commit()?;
                continue;
            };

            if !matches!(
                rollout.status,
                RolloutStatus::Queuing | RolloutStatus::Requeuing
            ) {
                transaction.execute(
                    "DELETE FROM rollout_queue WHERE queue_id = ?1",
                    params![queue_id],
                )?;
                transaction.commit()?;
                continue;
            }

            let from = rollout.status;
            let to = RolloutStatus::Running;
            if !from.can_transition_to(to) {
                return Err(TrainingStoreError::InvalidRolloutTransition { from, to });
            }

            let now = Utc::now();
            let start_time = rollout.start_time.unwrap_or(now);
            let next_sequence = rollout.attempt_count.saturating_add(1);
            let attempt_id = format!("{}:attempt-{}", rollout.rollout_id, next_sequence);

            transaction.execute(
                r#"
                UPDATE rollouts
                SET status = ?1, assigned_worker_id = ?2, start_time = ?3, attempt_count = ?4
                WHERE rollout_id = ?5
                "#,
                params![
                    rollout_status_to_db(to),
                    worker_id,
                    timestamp_to_db(start_time),
                    i64::from(next_sequence),
                    rollout.rollout_id
                ],
            )?;

            let attempt = Attempt {
                attempt_id: attempt_id.clone(),
                rollout_id: rollout.rollout_id.clone(),
                sequence_id: next_sequence,
                worker_id: worker_id.to_string(),
                status: AttemptStatus::Running,
                started_at: now,
                last_heartbeat_at: now,
                ended_at: None,
                error_message: None,
            };

            transaction.execute(
                r#"
                INSERT INTO attempts (
                    attempt_id, rollout_id, sequence_id, worker_id, status, started_at,
                    last_heartbeat_at, ended_at, error_message
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    attempt.attempt_id,
                    attempt.rollout_id,
                    i64::from(attempt.sequence_id),
                    attempt.worker_id,
                    attempt_status_to_db(attempt.status),
                    timestamp_to_db(attempt.started_at),
                    timestamp_to_db(attempt.last_heartbeat_at),
                    option_timestamp_to_db(attempt.ended_at),
                    attempt.error_message,
                ],
            )?;

            let now_db = timestamp_to_db(now);
            transaction.execute(
                r#"
                INSERT INTO workers (
                    worker_id, registered_at, last_heartbeat_at, active_rollout_id, active_attempt_id
                ) VALUES (?1, ?2, ?2, ?3, ?4)
                ON CONFLICT(worker_id) DO UPDATE SET
                    last_heartbeat_at = excluded.last_heartbeat_at,
                    active_rollout_id = excluded.active_rollout_id,
                    active_attempt_id = excluded.active_attempt_id
                "#,
                params![worker_id, now_db, rollout.rollout_id, attempt_id],
            )?;

            transaction.execute(
                "DELETE FROM rollout_queue WHERE queue_id = ?1",
                params![queue_id],
            )?;
            transaction.commit()?;

            rollout.status = to;
            rollout.start_time = Some(start_time);
            rollout.attempt_count = next_sequence;
            rollout.assigned_worker_id = Some(worker_id.to_string());

            self.notify.notify_waiters();
            return Ok(Some(DequeuedRollout { rollout, attempt }));
        }
    }

    async fn update_rollout_status(
        &self,
        rollout_id: &str,
        status: RolloutStatus,
    ) -> StoreResult<()> {
        let connection = self.open_connection()?;
        let current_status: Option<String> = connection
            .query_row(
                "SELECT status FROM rollouts WHERE rollout_id = ?1",
                params![rollout_id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(current_status) = current_status else {
            return Err(TrainingStoreError::RolloutNotFound(rollout_id.to_string()));
        };
        let from = rollout_status_from_db(&current_status)?;
        if !from.can_transition_to(status) {
            return Err(TrainingStoreError::InvalidRolloutTransition { from, to: status });
        }

        if status.is_terminal() {
            connection.execute(
                "UPDATE rollouts SET status = ?1, end_time = ?2 WHERE rollout_id = ?3",
                params![
                    rollout_status_to_db(status),
                    timestamp_to_db(Utc::now()),
                    rollout_id
                ],
            )?;
        } else {
            connection.execute(
                "UPDATE rollouts SET status = ?1 WHERE rollout_id = ?2",
                params![rollout_status_to_db(status), rollout_id],
            )?;
        }

        self.notify.notify_waiters();
        Ok(())
    }

    async fn cancel_rollout(&self, rollout_id: &str) -> StoreResult<()> {
        self.update_rollout_status(rollout_id, RolloutStatus::Cancelled)
            .await
    }

    async fn add_span(&self, span: TrainingSpan) -> StoreResult<()> {
        let connection = self.open_connection()?;
        connection.execute(
            r#"
            INSERT INTO spans (rollout_id, attempt_id, sequence_id, span_json)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                span.rollout_id,
                span.attempt_id,
                i64::try_from(span.sequence_id).map_err(|_| {
                    TrainingStoreError::InvalidPersistedValue {
                        field: "sequence_id",
                        value: span.sequence_id.to_string(),
                    }
                })?,
                serialize_json(&span)?
            ],
        )?;
        self.notify.notify_waiters();
        Ok(())
    }

    async fn query_spans(
        &self,
        rollout_id: &str,
        attempt_id: Option<&str>,
    ) -> StoreResult<Vec<TrainingSpan>> {
        let connection = self.open_connection()?;
        let sql = if attempt_id.is_some() {
            r#"
                SELECT span_json FROM spans
                WHERE rollout_id = ?1 AND attempt_id = ?2
                ORDER BY sequence_id ASC, span_row_id ASC
                "#
        } else {
            r#"
                SELECT span_json FROM spans
                WHERE rollout_id = ?1
                ORDER BY sequence_id ASC, span_row_id ASC
                "#
        };

        let mut statement = connection.prepare(sql)?;
        let mut rows = if let Some(attempt_id) = attempt_id {
            statement.query(params![rollout_id, attempt_id])?
        } else {
            statement.query(params![rollout_id])?
        };

        let mut spans = Vec::new();
        while let Some(row) = rows.next()? {
            let span_json: String = row.get(0)?;
            spans.push(deserialize_json(&span_json)?);
        }
        Ok(spans)
    }

    async fn get_next_span_sequence_id(
        &self,
        rollout_id: &str,
        attempt_id: &str,
    ) -> StoreResult<u64> {
        let connection = self.open_connection()?;
        let max_sequence: Option<i64> = connection.query_row(
            "SELECT MAX(sequence_id) FROM spans WHERE rollout_id = ?1 AND attempt_id = ?2",
            params![rollout_id, attempt_id],
            |row| row.get(0),
        )?;

        let next = max_sequence.unwrap_or(0) + 1;
        u64::try_from(next).map_err(|_| TrainingStoreError::InvalidPersistedValue {
            field: "sequence_id",
            value: next.to_string(),
        })
    }

    async fn update_resources(
        &self,
        resources: HashMap<String, Value>,
    ) -> StoreResult<ResourcesUpdate> {
        let mut connection = self.open_connection()?;
        let transaction = connection.transaction()?;

        transaction.execute("UPDATE resources SET is_latest = 0 WHERE is_latest = 1", [])?;
        let version: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM resources",
            [],
            |row| row.get(0),
        )?;
        let resources_id = format!("resources-{version}");
        let created_time = Utc::now();
        transaction.execute(
            r#"
            INSERT INTO resources (resources_id, version, resources_json, created_time, is_latest)
            VALUES (?1, ?2, ?3, ?4, 1)
            "#,
            params![
                resources_id,
                version,
                serialize_json(&resources)?,
                timestamp_to_db(created_time)
            ],
        )?;
        transaction.commit()?;

        self.notify.notify_waiters();
        Ok(ResourcesUpdate {
            resources_id,
            version: u64::try_from(version).map_err(|_| {
                TrainingStoreError::InvalidPersistedValue {
                    field: "version",
                    value: version.to_string(),
                }
            })?,
            resources,
            created_time,
            is_latest: true,
        })
    }

    async fn get_latest_resources(&self) -> StoreResult<Option<ResourcesUpdate>> {
        let connection = self.open_connection()?;
        let row: Option<(String, i64, String, String, i64)> = connection
            .query_row(
                r#"
                SELECT resources_id, version, resources_json, created_time, is_latest
                FROM resources
                WHERE is_latest = 1
                ORDER BY version DESC
                LIMIT 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(resources_id, version, resources_json, created_time, is_latest)| -> StoreResult<_> {
                Ok(ResourcesUpdate {
                    resources_id,
                    version: i64_to_u64("version", version)?,
                    resources: deserialize_json(&resources_json)?,
                    created_time: timestamp_from_db(&created_time)?,
                    is_latest: is_latest != 0,
                })
            },
        )
        .transpose()
    }

    async fn get_resources_by_id(
        &self,
        resources_id: &str,
    ) -> StoreResult<Option<ResourcesUpdate>> {
        let connection = self.open_connection()?;
        let row: Option<(String, i64, String, String, i64)> = connection
            .query_row(
                r#"
                SELECT resources_id, version, resources_json, created_time, is_latest
                FROM resources
                WHERE resources_id = ?1
                "#,
                params![resources_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(resources_id, version, resources_json, created_time, is_latest)| -> StoreResult<_> {
                Ok(ResourcesUpdate {
                    resources_id,
                    version: i64_to_u64("version", version)?,
                    resources: deserialize_json(&resources_json)?,
                    created_time: timestamp_from_db(&created_time)?,
                    is_latest: is_latest != 0,
                })
            },
        )
        .transpose()
    }

    async fn query_rollouts(&self, query: RolloutQuery) -> StoreResult<Vec<Rollout>> {
        let connection = self.open_connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT
                rollout_id, input_json, start_time, end_time, mode, status, config_json,
                metadata_json, assigned_worker_id, attempt_count
            FROM rollouts
            ORDER BY rollout_id ASC
            "#,
        )?;
        let mut rows = statement.query([])?;

        let mut rollouts = Vec::new();
        while let Some(row) = rows.next()? {
            let row_rollout = Rollout {
                rollout_id: row.get(0)?,
                input: deserialize_json::<Value>(&row.get::<_, String>(1)?)?,
                start_time: option_timestamp_from_db(row.get(2)?)?,
                end_time: option_timestamp_from_db(row.get(3)?)?,
                mode: option_rollout_mode_from_db(row.get(4)?)?,
                status: rollout_status_from_db(&row.get::<_, String>(5)?)?,
                config: deserialize_json::<RolloutConfig>(&row.get::<_, String>(6)?)?,
                metadata: deserialize_json::<HashMap<String, Value>>(&row.get::<_, String>(7)?)?,
                assigned_worker_id: row.get(8)?,
                attempt_count: i64_to_u32("attempt_count", row.get(9)?)?,
            };

            let status_match = query
                .statuses
                .as_ref()
                .is_none_or(|statuses| statuses.contains(&row_rollout.status));
            let mode_match = query.mode.is_none_or(|mode| row_rollout.mode == Some(mode));
            let id_match = query
                .ids
                .as_ref()
                .is_none_or(|ids| ids.iter().any(|item| item == &row_rollout.rollout_id));

            if status_match && mode_match && id_match {
                rollouts.push(row_rollout);
            }
        }

        let start = query.offset.min(rollouts.len());
        let mut sliced = rollouts.split_off(start);
        if let Some(limit) = query.limit {
            sliced.truncate(limit);
        }
        Ok(sliced)
    }

    async fn wait_for_rollouts(
        &self,
        statuses: &[RolloutStatus],
        timeout: Duration,
    ) -> StoreResult<Vec<Rollout>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }

        let deadline = Instant::now() + timeout;
        loop {
            let matches = self
                .query_rollouts(RolloutQuery {
                    statuses: Some(statuses.to_vec()),
                    ..RolloutQuery::default()
                })
                .await?;
            if !matches.is_empty() {
                return Ok(matches);
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(Vec::new());
            }

            let remaining = deadline.saturating_duration_since(now);
            if tokio::time::timeout(remaining, self.notify.notified())
                .await
                .is_err()
            {
                return Ok(Vec::new());
            }
        }
    }

    async fn register_worker(&self, worker_id: &str) -> StoreResult<WorkerState> {
        let connection = self.open_connection()?;
        let existing: Option<(String, String, String, Option<String>, Option<String>)> = connection
            .query_row(
                r#"
                SELECT worker_id, registered_at, last_heartbeat_at, active_rollout_id, active_attempt_id
                FROM workers
                WHERE worker_id = ?1
                "#,
                params![worker_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;

        let now = Utc::now();
        let now_db = timestamp_to_db(now);
        let worker = if let Some((
            worker_id,
            registered_at,
            _last,
            active_rollout_id,
            active_attempt_id,
        )) = existing
        {
            connection.execute(
                "UPDATE workers SET last_heartbeat_at = ?1 WHERE worker_id = ?2",
                params![now_db, worker_id],
            )?;
            WorkerState {
                worker_id,
                registered_at: timestamp_from_db(&registered_at)?,
                last_heartbeat_at: now,
                active_rollout_id,
                active_attempt_id,
            }
        } else {
            connection.execute(
                r#"
                INSERT INTO workers (
                    worker_id, registered_at, last_heartbeat_at, active_rollout_id, active_attempt_id
                ) VALUES (?1, ?2, ?2, NULL, NULL)
                "#,
                params![worker_id, now_db],
            )?;
            WorkerState {
                worker_id: worker_id.to_string(),
                registered_at: now,
                last_heartbeat_at: now,
                active_rollout_id: None,
                active_attempt_id: None,
            }
        };

        self.notify.notify_waiters();
        Ok(worker)
    }

    async fn update_worker_heartbeat(
        &self,
        worker_id: &str,
        active_rollout_id: Option<String>,
        active_attempt_id: Option<String>,
    ) -> StoreResult<()> {
        let connection = self.open_connection()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM workers WHERE worker_id = ?1",
                params![worker_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(TrainingStoreError::WorkerNotFound(worker_id.to_string()));
        }

        connection.execute(
            r#"
            UPDATE workers
            SET last_heartbeat_at = ?1, active_rollout_id = ?2, active_attempt_id = ?3
            WHERE worker_id = ?4
            "#,
            params![
                timestamp_to_db(Utc::now()),
                active_rollout_id,
                active_attempt_id,
                worker_id
            ],
        )?;
        self.notify.notify_waiters();
        Ok(())
    }

    async fn reassign_timed_out_rollouts(
        &self,
        heartbeat_timeout: Duration,
    ) -> StoreResult<Vec<String>> {
        let mut connection = self.open_connection()?;
        let transaction = connection.transaction()?;
        let now = Utc::now();
        let mut timed_out = Vec::new();

        let mut statement = transaction.prepare(
            r#"
            SELECT w.worker_id, w.active_rollout_id, w.active_attempt_id, a.status, a.last_heartbeat_at
            FROM workers w
            LEFT JOIN attempts a ON a.attempt_id = w.active_attempt_id
            WHERE w.active_rollout_id IS NOT NULL AND w.active_attempt_id IS NOT NULL
            "#,
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let worker_id: String = row.get(0)?;
            let rollout_id: Option<String> = row.get(1)?;
            let attempt_id: Option<String> = row.get(2)?;
            let attempt_status: Option<String> = row.get(3)?;
            let last_heartbeat_at: Option<String> = row.get(4)?;
            let (Some(rollout_id), Some(attempt_id), Some(attempt_status), Some(last_heartbeat_at)) =
                (rollout_id, attempt_id, attempt_status, last_heartbeat_at)
            else {
                continue;
            };
            if attempt_status_from_db(&attempt_status)? != AttemptStatus::Running {
                continue;
            }
            let last_heartbeat = timestamp_from_db(&last_heartbeat_at)?;
            let elapsed = now
                .signed_duration_since(last_heartbeat)
                .to_std()
                .unwrap_or_default();
            if elapsed <= heartbeat_timeout {
                continue;
            }
            timed_out.push((worker_id, rollout_id, attempt_id));
        }
        drop(rows);
        drop(statement);

        let mut requeued_rollouts = Vec::new();
        for (worker_id, rollout_id, attempt_id) in timed_out {
            transaction.execute(
                r#"
                UPDATE attempts
                SET status = ?1, last_heartbeat_at = ?2, ended_at = ?3, error_message = ?4
                WHERE attempt_id = ?5 AND status = ?6
                "#,
                params![
                    attempt_status_to_db(AttemptStatus::Timeout),
                    timestamp_to_db(now),
                    timestamp_to_db(now),
                    "worker heartbeat timeout",
                    attempt_id,
                    attempt_status_to_db(AttemptStatus::Running)
                ],
            )?;

            let rollout_status_text: Option<String> = transaction
                .query_row(
                    "SELECT status FROM rollouts WHERE rollout_id = ?1",
                    params![rollout_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(rollout_status_text) = rollout_status_text {
                let rollout_status = rollout_status_from_db(&rollout_status_text)?;
                if rollout_status.can_transition_to(RolloutStatus::Requeuing) {
                    transaction.execute(
                        "UPDATE rollouts SET status = ?1, assigned_worker_id = NULL WHERE rollout_id = ?2",
                        params![rollout_status_to_db(RolloutStatus::Requeuing), rollout_id],
                    )?;
                    transaction.execute(
                        "INSERT OR IGNORE INTO rollout_queue (rollout_id) VALUES (?1)",
                        params![rollout_id],
                    )?;
                    if !requeued_rollouts.contains(&rollout_id) {
                        requeued_rollouts.push(rollout_id.clone());
                    }
                }
            }

            transaction.execute(
                "UPDATE workers SET last_heartbeat_at = ?1, active_rollout_id = NULL, active_attempt_id = NULL WHERE worker_id = ?2",
                params![timestamp_to_db(now), worker_id],
            )?;
        }

        transaction.commit()?;
        if !requeued_rollouts.is_empty() {
            self.notify.notify_waiters();
        }
        Ok(requeued_rollouts)
    }

    async fn query_workers(&self) -> StoreResult<Vec<WorkerState>> {
        let connection = self.open_connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT worker_id, registered_at, last_heartbeat_at, active_rollout_id, active_attempt_id
            FROM workers
            ORDER BY worker_id ASC
            "#,
        )?;
        let mut rows = statement.query([])?;
        let mut workers = Vec::new();
        while let Some(row) = rows.next()? {
            workers.push(WorkerState {
                worker_id: row.get(0)?,
                registered_at: timestamp_from_db(&row.get::<_, String>(1)?)?,
                last_heartbeat_at: timestamp_from_db(&row.get::<_, String>(2)?)?,
                active_rollout_id: row.get(3)?,
                active_attempt_id: row.get(4)?,
            });
        }
        Ok(workers)
    }

    async fn update_attempt_status(
        &self,
        attempt_id: &str,
        status: AttemptStatus,
        error_message: Option<String>,
    ) -> StoreResult<()> {
        let connection = self.open_connection()?;
        let attempt_row: Option<(
            String,
            String,
            i64,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        )> = connection
            .query_row(
                r#"
                    SELECT
                        attempt_id, rollout_id, sequence_id, worker_id, status, started_at,
                        last_heartbeat_at, ended_at, error_message
                    FROM attempts
                    WHERE attempt_id = ?1
                    "#,
                params![attempt_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            attempt_id,
            _rollout_id,
            _sequence_id,
            _worker_id,
            status_text,
            _started_at,
            _last_heartbeat_at,
            _ended_at,
            _error_message,
        )) = attempt_row
        else {
            return Err(TrainingStoreError::AttemptNotFound(attempt_id.to_string()));
        };
        let from = attempt_status_from_db(&status_text)?;
        if !from.can_transition_to(status) {
            return Err(TrainingStoreError::InvalidAttemptTransition { from, to: status });
        }

        if status.is_terminal() {
            connection.execute(
                r#"
                UPDATE attempts
                SET status = ?1, last_heartbeat_at = ?2, ended_at = ?3, error_message = ?4
                WHERE attempt_id = ?5
                "#,
                params![
                    attempt_status_to_db(status),
                    timestamp_to_db(Utc::now()),
                    timestamp_to_db(Utc::now()),
                    error_message,
                    attempt_id
                ],
            )?;
        } else {
            connection.execute(
                r#"
                UPDATE attempts
                SET status = ?1, last_heartbeat_at = ?2
                WHERE attempt_id = ?3
                "#,
                params![
                    attempt_status_to_db(status),
                    timestamp_to_db(Utc::now()),
                    attempt_id
                ],
            )?;
        }

        self.notify.notify_waiters();
        Ok(())
    }

    async fn get_attempt(&self, attempt_id: &str) -> StoreResult<Option<Attempt>> {
        let connection = self.open_connection()?;
        let row: Option<(
            String,
            String,
            i64,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        )> = connection
            .query_row(
                r#"
                    SELECT
                        attempt_id, rollout_id, sequence_id, worker_id, status, started_at,
                        last_heartbeat_at, ended_at, error_message
                    FROM attempts
                    WHERE attempt_id = ?1
                    "#,
                params![attempt_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(
                attempt_id,
                rollout_id,
                sequence_id,
                worker_id,
                status,
                started_at,
                last_heartbeat_at,
                ended_at,
                error_message,
            )|
             -> StoreResult<Attempt> {
                Ok(Attempt {
                    attempt_id,
                    rollout_id,
                    sequence_id: i64_to_u32("sequence_id", sequence_id)?,
                    worker_id,
                    status: attempt_status_from_db(&status)?,
                    started_at: timestamp_from_db(&started_at)?,
                    last_heartbeat_at: timestamp_from_db(&last_heartbeat_at)?,
                    ended_at: option_timestamp_from_db(ended_at)?,
                    error_message,
                })
            },
        )
        .transpose()
    }
}

fn serialize_json<T: Serialize>(value: &T) -> StoreResult<String> {
    serde_json::to_string(value).map_err(TrainingStoreError::from)
}

fn deserialize_json<T: DeserializeOwned>(value: &str) -> StoreResult<T> {
    serde_json::from_str(value).map_err(TrainingStoreError::from)
}

fn timestamp_to_db(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn option_timestamp_to_db(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(timestamp_to_db)
}

fn timestamp_from_db(value: &str) -> StoreResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn option_timestamp_from_db(value: Option<String>) -> StoreResult<Option<DateTime<Utc>>> {
    value.as_deref().map(timestamp_from_db).transpose()
}

fn rollout_status_to_db(status: RolloutStatus) -> &'static str {
    match status {
        RolloutStatus::Queuing => "queuing",
        RolloutStatus::Preparing => "preparing",
        RolloutStatus::Running => "running",
        RolloutStatus::Failed => "failed",
        RolloutStatus::Succeeded => "succeeded",
        RolloutStatus::Cancelled => "cancelled",
        RolloutStatus::Requeuing => "requeuing",
    }
}

fn rollout_status_from_db(value: &str) -> StoreResult<RolloutStatus> {
    match value {
        "queuing" => Ok(RolloutStatus::Queuing),
        "preparing" => Ok(RolloutStatus::Preparing),
        "running" => Ok(RolloutStatus::Running),
        "failed" => Ok(RolloutStatus::Failed),
        "succeeded" => Ok(RolloutStatus::Succeeded),
        "cancelled" => Ok(RolloutStatus::Cancelled),
        "requeuing" => Ok(RolloutStatus::Requeuing),
        _ => Err(TrainingStoreError::InvalidPersistedValue {
            field: "rollout_status",
            value: value.to_string(),
        }),
    }
}

fn attempt_status_to_db(status: AttemptStatus) -> &'static str {
    match status {
        AttemptStatus::Preparing => "preparing",
        AttemptStatus::Running => "running",
        AttemptStatus::Failed => "failed",
        AttemptStatus::Succeeded => "succeeded",
        AttemptStatus::Unresponsive => "unresponsive",
        AttemptStatus::Timeout => "timeout",
    }
}

fn attempt_status_from_db(value: &str) -> StoreResult<AttemptStatus> {
    match value {
        "preparing" => Ok(AttemptStatus::Preparing),
        "running" => Ok(AttemptStatus::Running),
        "failed" => Ok(AttemptStatus::Failed),
        "succeeded" => Ok(AttemptStatus::Succeeded),
        "unresponsive" => Ok(AttemptStatus::Unresponsive),
        "timeout" => Ok(AttemptStatus::Timeout),
        _ => Err(TrainingStoreError::InvalidPersistedValue {
            field: "attempt_status",
            value: value.to_string(),
        }),
    }
}

fn rollout_mode_to_db(mode: RolloutMode) -> &'static str {
    match mode {
        RolloutMode::Train => "train",
        RolloutMode::Val => "val",
        RolloutMode::Test => "test",
    }
}

fn option_rollout_mode_from_db(value: Option<String>) -> StoreResult<Option<RolloutMode>> {
    value
        .as_deref()
        .map(|item| match item {
            "train" => Ok(RolloutMode::Train),
            "val" => Ok(RolloutMode::Val),
            "test" => Ok(RolloutMode::Test),
            _ => Err(TrainingStoreError::InvalidPersistedValue {
                field: "rollout_mode",
                value: item.to_string(),
            }),
        })
        .transpose()
}

fn i64_to_u32(field: &'static str, value: i64) -> StoreResult<u32> {
    u32::try_from(value).map_err(|_| TrainingStoreError::InvalidPersistedValue {
        field,
        value: value.to_string(),
    })
}

fn i64_to_u64(field: &'static str, value: i64) -> StoreResult<u64> {
    u64::try_from(value).map_err(|_| TrainingStoreError::InvalidPersistedValue {
        field,
        value: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::SqliteTrainingStore;
    use crate::{AttemptStatus, Rollout, RolloutQuery, RolloutStatus, TrainingSpan, TrainingStore};
    use serde_json::json;
    use std::collections::HashMap;
    use std::time::Duration;
    use tempfile::tempdir;

    #[tokio::test]
    async fn persists_rollout_state_across_reopen() {
        let temp = tempdir().expect("create tempdir");
        let db_path = temp.path().join("training.sqlite");

        {
            let store = SqliteTrainingStore::new(&db_path).expect("create sqlite store");
            store
                .enqueue_rollout(Rollout::new("r-1", json!({ "prompt": "hello" }), None))
                .await
                .expect("enqueue rollout");

            let dequeued = store
                .dequeue_rollout("worker-1")
                .await
                .expect("dequeue rollout")
                .expect("dequeued item");

            let span = TrainingSpan::new(
                dequeued.rollout.rollout_id,
                dequeued.attempt.attempt_id,
                1,
                "trace-1",
                "span-1",
                None,
                "runner.execute",
            );
            store.add_span(span).await.expect("add span");

            store
                .update_resources(HashMap::from([(
                    String::from("system_prompt"),
                    json!("You are concise."),
                )]))
                .await
                .expect("update resources");

            store
                .update_rollout_status("r-1", RolloutStatus::Succeeded)
                .await
                .expect("mark rollout succeeded");
        }

        let reopened = SqliteTrainingStore::new(&db_path).expect("reopen sqlite store");
        let succeeded = reopened
            .query_rollouts(RolloutQuery {
                statuses: Some(vec![RolloutStatus::Succeeded]),
                ..RolloutQuery::default()
            })
            .await
            .expect("query rollouts");
        assert_eq!(succeeded.len(), 1);

        let spans = reopened
            .query_spans("r-1", None)
            .await
            .expect("query spans");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "runner.execute");

        let resources = reopened
            .get_latest_resources()
            .await
            .expect("load resources")
            .expect("latest resources");
        assert_eq!(resources.version, 1);

        let workers = reopened.query_workers().await.expect("query workers");
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].worker_id, "worker-1");
    }

    #[tokio::test]
    async fn reassigns_timed_out_worker_and_requeues_rollout() {
        let temp = tempdir().expect("create tempdir");
        let db_path = temp.path().join("training.sqlite");
        let store = SqliteTrainingStore::new(&db_path).expect("create sqlite store");
        store
            .enqueue_rollout(Rollout::new(
                "r-chaos-1",
                json!({ "prompt": "hello" }),
                None,
            ))
            .await
            .expect("enqueue rollout");

        let first = store
            .dequeue_rollout("worker-1")
            .await
            .expect("dequeue first")
            .expect("first attempt");
        tokio::time::sleep(Duration::from_millis(30)).await;

        let requeued = store
            .reassign_timed_out_rollouts(Duration::from_millis(5))
            .await
            .expect("reassign timed out");
        assert_eq!(requeued, vec!["r-chaos-1".to_string()]);

        let first_attempt = store
            .get_attempt(&first.attempt.attempt_id)
            .await
            .expect("get first attempt")
            .expect("first attempt exists");
        assert_eq!(first_attempt.status, AttemptStatus::Timeout);

        let second = store
            .dequeue_rollout("worker-2")
            .await
            .expect("dequeue second")
            .expect("second attempt");
        assert_eq!(second.rollout.rollout_id, "r-chaos-1");
        assert_eq!(second.attempt.sequence_id, 2);
    }
}
