use super::*;

impl GithubIssuesBridgeRuntime {
    /// Build artifact index summary for one issue including latest record pointers.
    pub(super) fn issue_artifact_summary(&self, issue_number: u64) -> Result<IssueArtifactSummary> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let loaded = store.load_artifact_records_tolerant()?;
        let now_unix_ms = current_unix_timestamp_ms();
        let mut records = loaded.records;
        let active_records = records
            .iter()
            .filter(|record| !is_shared_expired_at(record.expires_unix_ms, now_unix_ms))
            .count();
        records.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        let latest = records.first();
        Ok(IssueArtifactSummary {
            total_records: records.len(),
            active_records,
            latest_artifact_id: latest.map(|record| record.id.clone()),
            latest_artifact_run_id: latest.map(|record| record.run_id.clone()),
            latest_artifact_created_unix_ms: latest.map(|record| record.created_unix_ms),
            invalid_index_lines: loaded.invalid_lines,
        })
    }

    pub(super) fn issue_chat_continuity_summary(
        &self,
        issue_number: u64,
    ) -> Result<IssueChatContinuitySummary> {
        let session_path = shared_session_path_for_issue(&self.repository_state_dir, issue_number);
        let store = SessionStore::load(&session_path)?;
        let head_id = store.head_id();
        let lineage = store.lineage_entries(head_id)?;
        let lineage_jsonl = store.export_lineage_jsonl(head_id)?;
        let digest = shared_sha256_hex(lineage_jsonl.as_bytes());
        let oldest_entry_id = lineage.first().map(|entry| entry.id);
        let newest_entry_id = lineage.last().map(|entry| entry.id);
        let newest_entry_role = lineage
            .last()
            .map(|entry| session_message_role(&entry.message));
        Ok(IssueChatContinuitySummary {
            entries: lineage.len(),
            head_id,
            oldest_entry_id,
            newest_entry_id,
            newest_entry_role,
            lineage_digest_sha256: digest,
            artifacts: self.issue_artifact_summary(issue_number)?,
        })
    }

    /// Render operational status lines for one issue run lifecycle and chat continuity.
    pub(super) fn render_issue_status(&self, issue_number: u64) -> String {
        let active = self.active_runs.get(&issue_number);
        let latest = self.latest_runs.get(&issue_number);
        let state = if active.is_some() { "running" } else { "idle" };
        let mut lines = vec![format!("Tau status for issue #{issue_number}: {state}")];
        if let Some(active) = active {
            lines.push(format!("active_run_id: {}", active.run_id));
            lines.push(format!("active_event_key: {}", active.event_key));
            lines.push(format!(
                "active_elapsed_ms: {}",
                active.started.elapsed().as_millis()
            ));
            lines.push(format!(
                "active_started_unix_ms: {}",
                active.started_unix_ms
            ));
            lines.push(format!(
                "cancellation_requested: {}",
                if *active.cancel_tx.borrow() {
                    "true"
                } else {
                    "false"
                }
            ));
        } else {
            lines.push("active_run_id: none".to_string());
        }

        if let Some(latest) = latest {
            lines.push(format!("latest_run_id: {}", latest.run_id));
            lines.push(format!("latest_event_key: {}", latest.event_key));
            lines.push(format!("latest_status: {}", latest.status));
            lines.push(format!(
                "latest_started_unix_ms: {}",
                latest.started_unix_ms
            ));
            lines.push(format!(
                "latest_completed_unix_ms: {}",
                latest.completed_unix_ms
            ));
            lines.push(format!("latest_duration_ms: {}", latest.duration_ms));
        } else {
            lines.push("latest_run_id: none".to_string());
        }
        lines.extend(self.state_store.transport_health().status_lines());

        if let Some(session) = self.state_store.issue_session(issue_number) {
            lines.push(format!("chat_session_id: {}", session.session_id));
            lines.push(format!(
                "chat_last_comment_id: {}",
                session
                    .last_comment_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
            lines.push(format!(
                "chat_last_run_id: {}",
                session.last_run_id.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_active_run_id: {}",
                session.active_run_id.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_event_key: {}",
                session.last_event_key.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_event_kind: {}",
                session.last_event_kind.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_actor_login: {}",
                session.last_actor_login.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_reason_code: {}",
                session.last_reason_code.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_processed_unix_ms: {}",
                session
                    .last_processed_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
            lines.push(format!(
                "chat_total_processed_events: {}",
                session.total_processed_events
            ));
            lines.push(format!(
                "chat_total_duplicate_events: {}",
                session.total_duplicate_events
            ));
            lines.push(format!(
                "chat_total_failed_events: {}",
                session.total_failed_events
            ));
            lines.push(format!(
                "chat_total_denied_events: {}",
                session.total_denied_events
            ));
            lines.push(format!(
                "chat_total_runs_started: {}",
                session.total_runs_started
            ));
            lines.push(format!(
                "chat_total_runs_completed: {}",
                session.total_runs_completed
            ));
            lines.push(format!(
                "chat_total_runs_failed: {}",
                session.total_runs_failed
            ));
        } else {
            lines.push("chat_session_id: none".to_string());
        }
        match self.issue_chat_continuity_summary(issue_number) {
            Ok(summary) => {
                lines.push(format!("chat_entries: {}", summary.entries));
                lines.push(format!(
                    "chat_head_id: {}",
                    summary
                        .head_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "chat_oldest_entry_id: {}",
                    summary
                        .oldest_entry_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "chat_newest_entry_id: {}",
                    summary
                        .newest_entry_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "chat_newest_entry_role: {}",
                    summary.newest_entry_role.as_deref().unwrap_or("none")
                ));
                lines.push(format!(
                    "chat_lineage_digest_sha256: {}",
                    summary.lineage_digest_sha256
                ));
                lines.push(format!(
                    "artifacts_active: {}",
                    summary.artifacts.active_records
                ));
                lines.push(format!(
                    "artifacts_total: {}",
                    summary.artifacts.total_records
                ));
                lines.push(format!(
                    "artifacts_latest_id: {}",
                    summary
                        .artifacts
                        .latest_artifact_id
                        .as_deref()
                        .unwrap_or("none")
                ));
                lines.push(format!(
                    "artifacts_latest_run_id: {}",
                    summary
                        .artifacts
                        .latest_artifact_run_id
                        .as_deref()
                        .unwrap_or("none")
                ));
                lines.push(format!(
                    "artifacts_latest_created_unix_ms: {}",
                    summary
                        .artifacts
                        .latest_artifact_created_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "artifacts_index_invalid_lines: {}",
                    summary.artifacts.invalid_index_lines
                ));
            }
            Err(error) => lines.push(format!(
                "chat_summary_error: {}",
                truncate_for_error(&error.to_string(), 240)
            )),
        }
        lines.join("\n")
    }

    pub(super) fn render_issue_health(&self, issue_number: u64) -> String {
        let active = self.active_runs.get(&issue_number);
        let runtime_state = if active.is_some() { "running" } else { "idle" };
        let health = self.state_store.transport_health();
        let classification = health.classify();
        let mut lines = vec![format!(
            "Tau health for issue #{}: {}",
            issue_number,
            classification.state.as_str()
        )];
        lines.push(format!("runtime_state: {runtime_state}"));
        if let Some(active) = active {
            lines.push(format!("active_run_id: {}", active.run_id));
            lines.push(format!("active_event_key: {}", active.event_key));
            lines.push(format!(
                "active_elapsed_ms: {}",
                active.started.elapsed().as_millis()
            ));
        } else {
            lines.push("active_run_id: none".to_string());
        }
        lines.extend(health.health_detail_lines());
        lines.join("\n")
    }

    pub(super) fn render_issue_artifacts(
        &self,
        issue_number: u64,
        run_id_filter: Option<&str>,
    ) -> Result<String> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let loaded = store.load_artifact_records_tolerant()?;
        let mut active = store.list_active_artifacts(current_unix_timestamp_ms())?;
        if let Some(run_id_filter) = run_id_filter {
            active.retain(|artifact| artifact.run_id == run_id_filter);
        }
        active.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut lines = vec![if let Some(run_id_filter) = run_id_filter {
            format!(
                "Tau artifacts for issue #{} run_id `{}`: active={}",
                issue_number,
                run_id_filter,
                active.len()
            )
        } else {
            format!(
                "Tau artifacts for issue #{}: active={}",
                issue_number,
                active.len()
            )
        }];
        if active.is_empty() {
            if let Some(run_id_filter) = run_id_filter {
                lines.push(format!("none for run_id `{}`", run_id_filter));
            } else {
                lines.push("none".to_string());
            }
        } else {
            let max_rows = 10_usize;
            for artifact in active.iter().take(max_rows) {
                lines.push(format!(
                    "- id `{}` type `{}` bytes `{}` visibility `{}` created_unix_ms `{}` expires_unix_ms `{}` checksum `{}` path `{}`",
                    artifact.id,
                    artifact.artifact_type,
                    artifact.bytes,
                    artifact.visibility,
                    artifact.created_unix_ms,
                    artifact
                        .expires_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    artifact.checksum_sha256,
                    artifact.relative_path,
                ));
            }
            if active.len() > max_rows {
                lines.push(format!(
                    "... {} additional artifacts omitted",
                    active.len() - max_rows
                ));
            }
        }
        if loaded.invalid_lines > 0 {
            lines.push(format!(
                "index_invalid_lines: {} (ignored)",
                loaded.invalid_lines
            ));
        }
        Ok(lines.join("\n"))
    }

    pub(super) fn render_issue_artifact_purge(&self, issue_number: u64) -> Result<String> {
        let now_unix_ms = current_unix_timestamp_ms();
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let purge = store.purge_expired_artifacts(now_unix_ms)?;
        let active = store.list_active_artifacts(now_unix_ms)?;
        Ok(format!(
            "Tau artifact purge for issue #{}: expired_removed={} invalid_removed={} attachment_expired_removed={} attachment_invalid_removed={} active_remaining={}",
            issue_number,
            purge.expired_removed,
            purge.invalid_removed,
            purge.attachment_expired_removed,
            purge.attachment_invalid_removed,
            active.len()
        ))
    }

    pub(super) fn render_issue_artifact_show(
        &self,
        issue_number: u64,
        artifact_id: &str,
    ) -> Result<String> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let loaded = store.load_artifact_records_tolerant()?;
        let now_unix_ms = current_unix_timestamp_ms();
        let artifact = loaded
            .records
            .iter()
            .find(|record| record.id == artifact_id);
        let mut lines = Vec::new();
        match artifact {
            Some(record) => {
                let expired = record
                    .expires_unix_ms
                    .map(|expires_unix_ms| expires_unix_ms <= now_unix_ms)
                    .unwrap_or(false);
                lines.push(format!(
                    "Tau artifact for issue #{} id `{}`: state={}",
                    issue_number,
                    artifact_id,
                    if expired { "expired" } else { "active" }
                ));
                lines.push(format!("run_id: {}", record.run_id));
                lines.push(format!("artifact_type: {}", record.artifact_type));
                lines.push(format!("visibility: {}", record.visibility));
                lines.push(format!("bytes: {}", record.bytes));
                lines.push(format!("created_unix_ms: {}", record.created_unix_ms));
                lines.push(format!(
                    "expires_unix_ms: {}",
                    record
                        .expires_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!("checksum: {}", record.checksum_sha256));
                lines.push(format!("path: {}", record.relative_path));
                if expired {
                    lines.push(
                        "artifact is expired and may be removed by `/tau artifacts purge`."
                            .to_string(),
                    );
                }
            }
            None => lines.push(format!(
                "Tau artifact for issue #{} id `{}`: not found",
                issue_number, artifact_id
            )),
        }
        if loaded.invalid_lines > 0 {
            lines.push(format!(
                "index_invalid_lines: {} (ignored)",
                loaded.invalid_lines
            ));
        }
        Ok(lines.join("\n"))
    }

    /// Execute GitHub issue auth subcommand and format response lines.
    pub(super) fn execute_issue_auth_command(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &TauIssueAuthCommand,
    ) -> Result<IssueAuthExecution> {
        let command_name = match command.kind {
            TauIssueAuthCommandKind::Status => "status",
            TauIssueAuthCommandKind::Matrix => "matrix",
        };
        let command_key = match command.kind {
            TauIssueAuthCommandKind::Status => "auth-status",
            TauIssueAuthCommandKind::Matrix => "auth-matrix",
        };
        let run_id = format!(
            "{}-{}-{}-{}",
            command_key,
            issue_number,
            current_unix_timestamp_ms(),
            shared_short_key_hash(event_key)
        );
        let report_payload = execute_auth_command(&self.config.auth_command_config, &command.args);
        let json_args = ensure_shared_auth_json_flag(&command.args);
        let report_payload_json =
            execute_auth_command(&self.config.auth_command_config, &json_args);
        let summary_kind = match command.kind {
            TauIssueAuthCommandKind::Status => IssueAuthSummaryKind::Status,
            TauIssueAuthCommandKind::Matrix => IssueAuthSummaryKind::Matrix,
        };
        let summary_line = build_shared_issue_auth_summary_line(summary_kind, &report_payload_json);
        let channel_store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let retention_days =
            normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
        let report_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-auth-report",
            "private",
            retention_days,
            "txt",
            &report_payload,
        )?;
        let json_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-auth-json",
            "private",
            retention_days,
            "json",
            &report_payload_json,
        )?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "github".to_string(),
            payload: json!({
                "command": command_key,
                "run_id": run_id,
                "args": command.args,
                "json_args": json_args,
                "subscription_strict": self.config.auth_command_config.provider_subscription_strict,
                "summary": summary_line,
                "report_artifact": {
                    "id": report_artifact.id,
                    "path": report_artifact.relative_path,
                    "checksum_sha256": report_artifact.checksum_sha256,
                    "bytes": report_artifact.bytes,
                    "expires_unix_ms": report_artifact.expires_unix_ms,
                },
                "json_artifact": {
                    "id": json_artifact.id,
                    "path": json_artifact.relative_path,
                    "checksum_sha256": json_artifact.checksum_sha256,
                    "bytes": json_artifact.bytes,
                    "expires_unix_ms": json_artifact.expires_unix_ms,
                },
            }),
        })?;
        Ok(IssueAuthExecution {
            run_id,
            command_name,
            summary_line,
            subscription_strict: self.config.auth_command_config.provider_subscription_strict,
            report_artifact,
            json_artifact,
        })
    }

    pub(super) fn render_issue_auth_posture_lines(&self) -> Vec<String> {
        vec![
            format!(
                "provider_mode: openai={} anthropic={} google={}",
                self.config.auth_command_config.openai_auth_mode.as_str(),
                self.config.auth_command_config.anthropic_auth_mode.as_str(),
                self.config.auth_command_config.google_auth_mode.as_str()
            ),
            format!(
                "login_backend_enabled: openai_codex={} anthropic_claude={} google_gemini={}",
                self.config.auth_command_config.openai_codex_backend,
                self.config.auth_command_config.anthropic_claude_backend,
                self.config.auth_command_config.google_gemini_backend
            ),
        ]
    }

    pub(super) fn execute_issue_doctor_command(
        &self,
        issue_number: u64,
        event_key: &str,
        command: IssueDoctorCommand,
    ) -> Result<IssueDoctorExecution> {
        let run_id = format!(
            "doctor-{}-{}-{}",
            issue_number,
            current_unix_timestamp_ms(),
            shared_short_key_hash(event_key)
        );
        let checks = run_doctor_checks_with_options(
            &self.config.doctor_config,
            DoctorCheckOptions {
                online: command.online,
            },
        );
        let pass = checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Pass)
            .count();
        let warn = checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Warn)
            .count();
        let fail = checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Fail)
            .count();
        let highlighted = checks
            .iter()
            .filter(|check| check.status != DoctorStatus::Pass)
            .take(5)
            .map(|check| {
                format!(
                    "key={} status={} code={}",
                    check.key,
                    doctor_status_label(check.status),
                    check.code
                )
            })
            .collect::<Vec<_>>();
        let report_payload = render_doctor_report(&checks);
        let report_payload_json = render_doctor_report_json(&checks);
        let channel_store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let retention_days =
            normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
        let report_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-doctor-report",
            "private",
            retention_days,
            "txt",
            &report_payload,
        )?;
        let json_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-doctor-json",
            "private",
            retention_days,
            "json",
            &report_payload_json,
        )?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "github".to_string(),
            payload: json!({
                "command": "doctor",
                "run_id": run_id,
                "online": command.online,
                "summary": {
                    "checks": checks.len(),
                    "pass": pass,
                    "warn": warn,
                    "fail": fail,
                },
                "report_artifact": {
                    "id": report_artifact.id,
                    "path": report_artifact.relative_path,
                    "checksum_sha256": report_artifact.checksum_sha256,
                    "bytes": report_artifact.bytes,
                    "expires_unix_ms": report_artifact.expires_unix_ms,
                },
                "json_artifact": {
                    "id": json_artifact.id,
                    "path": json_artifact.relative_path,
                    "checksum_sha256": json_artifact.checksum_sha256,
                    "bytes": json_artifact.bytes,
                    "expires_unix_ms": json_artifact.expires_unix_ms,
                },
            }),
        })?;
        Ok(IssueDoctorExecution {
            run_id,
            checks: checks.len(),
            pass,
            warn,
            fail,
            highlighted,
            report_artifact,
            json_artifact,
        })
    }

    pub(super) async fn post_issue_command_comment(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &str,
        status: &str,
        message: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let normalized_status = normalize_issue_command_status(status).to_string();
        let reason_code = issue_command_reason_code(command, &normalized_status);
        let mut content = if message.trim().is_empty() {
            "Tau command response.".to_string()
        } else {
            message.trim().to_string()
        };
        let mut overflow_artifact: Option<ChannelArtifactRecord> = None;
        let mut body = render_issue_command_comment(
            event_key,
            command,
            &normalized_status,
            &reason_code,
            &content,
        );
        if body.chars().count() > GITHUB_COMMENT_MAX_CHARS {
            let channel_store = ChannelStore::open(
                &self.repository_state_dir.join("channel-store"),
                "github",
                &format!("issue-{issue_number}"),
            )?;
            let run_id = format!(
                "command-overflow-{}-{}-{}",
                issue_number,
                current_unix_timestamp_ms(),
                shared_short_key_hash(event_key)
            );
            let retention_days =
                normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
            let artifact = channel_store.write_text_artifact(
                &run_id,
                "github-issue-command-overflow",
                "private",
                retention_days,
                "txt",
                &content,
            )?;
            let overflow_suffix = format!(
                "output_truncated: true\n{}",
                render_shared_issue_artifact_pointer_line(
                    "overflow_artifact",
                    &artifact.id,
                    &artifact.relative_path,
                    artifact.bytes,
                )
            );
            let mut excerpt_len = content.chars().count();
            loop {
                let excerpt = split_at_char_index(&content, excerpt_len).0;
                content = if excerpt.trim().is_empty() {
                    overflow_suffix.clone()
                } else {
                    format!("{}\n\n{}", excerpt.trim_end(), overflow_suffix)
                };
                body = render_issue_command_comment(
                    event_key,
                    command,
                    &normalized_status,
                    &reason_code,
                    &content,
                );
                if body.chars().count() <= GITHUB_COMMENT_MAX_CHARS || excerpt_len == 0 {
                    break;
                }
                let overflow = body.chars().count() - GITHUB_COMMENT_MAX_CHARS;
                excerpt_len = excerpt_len.saturating_sub(overflow.saturating_add(8));
            }
            overflow_artifact = Some(artifact);
        }
        let posted = self
            .github_client
            .create_issue_comment(issue_number, &body)
            .await?;
        self.outbound_log.append(&json!({
            "timestamp_unix_ms": current_unix_timestamp_ms(),
            "repo": self.repo.as_slug(),
            "event_key": event_key,
            "issue_number": issue_number,
            "command": command,
            "status": normalized_status,
            "reason_code": reason_code,
            "posted_comment_id": posted.id,
            "posted_comment_url": posted.html_url,
            "overflow_artifact": overflow_artifact.as_ref().map(|artifact| json!({
                "id": artifact.id,
                "path": artifact.relative_path,
                "bytes": artifact.bytes,
                "checksum_sha256": artifact.checksum_sha256,
            })),
        }))?;
        Ok(posted)
    }

    pub(super) fn append_channel_log(
        &self,
        event: &GithubBridgeEvent,
        direction: &str,
        payload: Value,
    ) -> Result<()> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{}", event.issue_number),
        )?;
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: direction.to_string(),
            event_key: Some(event.key.clone()),
            source: "github".to_string(),
            payload,
        })
    }
}
