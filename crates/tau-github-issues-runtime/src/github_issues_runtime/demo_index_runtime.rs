//! Demo-index execution and report rendering helpers for the GitHub Issues runtime.

use super::*;

impl GithubIssuesBridgeRuntime {
    async fn execute_demo_index_script(
        &self,
        args: &[String],
        include_binary: bool,
    ) -> Result<std::process::Output> {
        if !self.demo_index_script_path.exists() {
            bail!(
                "demo-index script not found at {}",
                self.demo_index_script_path.display()
            );
        }
        let mut command = tokio::process::Command::new(&self.demo_index_script_path);
        command.args(args);
        command.arg("--repo-root").arg(&self.demo_index_repo_root);
        if include_binary {
            command.arg("--binary").arg(&self.demo_index_binary_path);
            command.arg("--skip-build");
        }
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.output().await.with_context(|| {
            format!(
                "failed to execute demo-index script {}",
                self.demo_index_script_path.display()
            )
        })
    }

    pub(super) async fn render_demo_index_inventory(&self, issue_number: u64) -> Result<String> {
        let args = vec!["--list".to_string(), "--json".to_string()];
        let output = self.execute_demo_index_script(&args, false).await?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            bail!(
                "demo-index list failed with exit code {}: {}",
                output.status.code().unwrap_or(1),
                truncate_for_error(&stderr, 240)
            );
        }
        let inventory: DemoIndexScenarioInventory =
            serde_json::from_str(&stdout).with_context(|| {
                format!(
                    "failed to parse demo-index list json output: {}",
                    truncate_for_error(&stdout, 240)
                )
            })?;
        let mut lines = vec![format!(
            "Tau demo-index scenario inventory for issue #{}: {} scenario(s).",
            issue_number,
            inventory.scenarios.len()
        )];
        for scenario in inventory.scenarios {
            lines.push(format!("- `{}`: {}", scenario.id, scenario.description));
            lines.push(format!(
                "  wrapper: {} | command: {}",
                scenario.wrapper, scenario.command
            ));
            if let Some(marker) = scenario.expected_markers.first() {
                lines.push(format!("  expected_marker: {}", marker));
            }
            lines.push(format!("  troubleshooting: {}", scenario.troubleshooting));
        }
        lines.push(String::new());
        lines.push(self.render_issue_demo_index_reports(issue_number)?);
        Ok(lines.join("\n"))
    }

    pub(super) async fn execute_demo_index_run(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &DemoIndexRunCommand,
    ) -> Result<DemoIndexRunExecution> {
        let run_id = format!(
            "demo-index-{}-{}-{}",
            issue_number,
            current_unix_timestamp_ms(),
            shared_short_key_hash(event_key)
        );
        let report_dir = self.repository_state_dir.join("demo-index-reports");
        std::fs::create_dir_all(&report_dir)
            .with_context(|| format!("failed to create {}", report_dir.display()))?;
        let report_file = report_dir.join(format!("{}.json", run_id));
        let only = command.scenarios.join(",");
        let args = vec![
            "--json".to_string(),
            "--report-file".to_string(),
            report_file.display().to_string(),
            "--only".to_string(),
            only.clone(),
            "--timeout-seconds".to_string(),
            command.timeout_seconds.to_string(),
            "--fail-fast".to_string(),
        ];
        let output = self.execute_demo_index_script(&args, true).await?;
        let exit_code = output.status.code().unwrap_or(1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let report_payload = if report_file.exists() {
            std::fs::read_to_string(&report_file)
                .with_context(|| format!("failed to read {}", report_file.display()))?
        } else {
            stdout.clone()
        };
        let summary = serde_json::from_str::<DemoIndexRunReport>(&report_payload)
            .or_else(|_| serde_json::from_str::<DemoIndexRunReport>(&stdout))
            .ok();

        let command_line = format!(
            "{} --json --report-file {} --only {} --timeout-seconds {} --fail-fast --repo-root {} --binary {} --skip-build",
            self.demo_index_script_path.display(),
            report_file.display(),
            only,
            command.timeout_seconds,
            self.demo_index_repo_root.display(),
            self.demo_index_binary_path.display()
        );
        let channel_store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let retention_days =
            normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
        let report_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-demo-index-report",
            "private",
            retention_days,
            "json",
            &report_payload,
        )?;
        let log_payload = format!(
            "command: {command_line}\nexit_code: {exit_code}\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
        );
        let log_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-demo-index-log",
            "private",
            retention_days,
            "log",
            &log_payload,
        )?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "github".to_string(),
            payload: json!({
                "command": "demo-index-run",
                "run_id": run_id,
                "scenarios": command.scenarios.clone(),
                "timeout_seconds": command.timeout_seconds,
                "exit_code": exit_code,
                "report_artifact": {
                    "id": report_artifact.id,
                    "path": report_artifact.relative_path,
                    "checksum_sha256": report_artifact.checksum_sha256,
                    "bytes": report_artifact.bytes,
                    "expires_unix_ms": report_artifact.expires_unix_ms,
                },
                "log_artifact": {
                    "id": log_artifact.id,
                    "path": log_artifact.relative_path,
                    "checksum_sha256": log_artifact.checksum_sha256,
                    "bytes": log_artifact.bytes,
                    "expires_unix_ms": log_artifact.expires_unix_ms,
                },
            }),
        })?;
        Ok(DemoIndexRunExecution {
            run_id,
            command_line,
            exit_code,
            summary,
            report_artifact,
            log_artifact,
        })
    }

    pub(super) fn render_issue_demo_index_reports(&self, issue_number: u64) -> Result<String> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let loaded = store.load_artifact_records_tolerant()?;
        let now_unix_ms = current_unix_timestamp_ms();
        let mut reports = loaded
            .records
            .into_iter()
            .filter(|artifact| artifact.artifact_type == "github-issue-demo-index-report")
            .filter(|artifact| !is_shared_expired_at(artifact.expires_unix_ms, now_unix_ms))
            .collect::<Vec<_>>();
        reports.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut lines = vec![format!(
            "Tau demo-index latest report pointers for issue #{}: {}",
            issue_number,
            reports.len()
        )];
        if reports.is_empty() {
            lines.push("none".to_string());
        } else {
            for artifact in reports.iter().take(5) {
                lines.push(format!(
                    "- id `{}` run_id `{}` created_unix_ms `{}` bytes `{}` path `{}`",
                    artifact.id,
                    artifact.run_id,
                    artifact.created_unix_ms,
                    artifact.bytes,
                    artifact.relative_path,
                ));
            }
            if reports.len() > 5 {
                lines.push(format!(
                    "... {} additional reports omitted",
                    reports.len() - 5
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
}
