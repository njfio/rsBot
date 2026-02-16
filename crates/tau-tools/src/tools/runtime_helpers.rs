use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) enum PathMode {
    Read,
    Write,
    Edit,
    Directory,
}

pub(super) fn resolve_and_validate_path(
    user_path: &str,
    policy: &ToolPolicy,
    mode: PathMode,
) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| format!("failed to resolve cwd: {error}"))?;
    let input = PathBuf::from(user_path);
    let absolute = if input.is_absolute() {
        input
    } else {
        cwd.join(input)
    };

    let canonical = canonicalize_best_effort(&absolute).map_err(|error| {
        format!(
            "failed to canonicalize path '{}': {error}",
            absolute.display()
        )
    })?;

    if !is_path_allowed(&canonical, policy)? {
        return Err(format!(
            "path '{}' is outside allowed roots",
            canonical.display()
        ));
    }

    if matches!(mode, PathMode::Read | PathMode::Edit | PathMode::Directory) && !absolute.exists() {
        return Err(format!("path '{}' does not exist", absolute.display()));
    }

    Ok(absolute)
}

pub(super) fn validate_file_target(
    path: &Path,
    mode: PathMode,
    enforce_regular_files: bool,
) -> Result<(), String> {
    if !enforce_regular_files {
        return Ok(());
    }

    if !path.exists() {
        return Ok(());
    }

    let symlink_meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect path '{}': {error}", path.display()))?;
    if symlink_meta.file_type().is_symlink() {
        return Err(format!(
            "path '{}' is a symbolic link, which is denied by policy",
            path.display()
        ));
    }

    if matches!(mode, PathMode::Read | PathMode::Edit) && !symlink_meta.file_type().is_file() {
        return Err(format!(
            "path '{}' must be a regular file for this operation",
            path.display()
        ));
    }

    if matches!(mode, PathMode::Write) && path.exists() && !symlink_meta.file_type().is_file() {
        return Err(format!(
            "path '{}' must be a regular file when overwriting existing content",
            path.display()
        ));
    }

    Ok(())
}

pub(super) fn validate_directory_target(
    path: &Path,
    enforce_regular_files: bool,
) -> Result<(), String> {
    let symlink_meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect path '{}': {error}", path.display()))?;
    if enforce_regular_files && symlink_meta.file_type().is_symlink() {
        return Err(format!(
            "path '{}' is a symbolic link, which is denied by policy",
            path.display()
        ));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect path '{}': {error}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "path '{}' must be a directory for this operation",
            path.display()
        ));
    }

    Ok(())
}

pub(super) fn is_path_allowed(path: &Path, policy: &ToolPolicy) -> Result<bool, String> {
    if policy.allowed_roots.is_empty() {
        return Ok(true);
    }

    for root in &policy.allowed_roots {
        let canonical_root = canonicalize_best_effort(root)
            .map_err(|error| format!("invalid allowed root '{}': {error}", root.display()))?;

        if path.starts_with(&canonical_root) {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(super) fn default_protected_paths(allowed_roots: &[PathBuf]) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut paths = BTreeSet::new();
    for root in allowed_roots {
        let absolute_root = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        for relative_path in DEFAULT_PROTECTED_RELATIVE_PATHS {
            let candidate = absolute_root.join(relative_path);
            paths.insert(normalize_policy_path(&candidate));
        }
    }
    paths.into_iter().collect()
}

pub(super) fn normalize_policy_path(path: &Path) -> PathBuf {
    canonicalize_best_effort(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn canonicalize_best_effort(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path);
    }

    let mut missing_suffix: Vec<OsString> = Vec::new();
    let mut cursor = path;

    while !cursor.exists() {
        if let Some(file_name) = cursor.file_name() {
            missing_suffix.push(file_name.to_os_string());
        }

        cursor = match cursor.parent() {
            Some(parent) => parent,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no existing ancestor for path",
                ));
            }
        };
    }

    let mut canonical = std::fs::canonicalize(cursor)?;
    for component in missing_suffix.iter().rev() {
        canonical.push(component);
    }

    Ok(canonical)
}

pub(super) fn required_string(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("missing required string argument '{key}'"))
}

pub(super) fn optional_usize(
    arguments: &Value,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(default);
    };
    let parsed = value
        .as_u64()
        .ok_or_else(|| format!("optional argument '{key}' must be an integer"))?
        as usize;
    if parsed == 0 {
        return Err(format!("optional argument '{key}' must be greater than 0"));
    }
    if parsed > max {
        return Err(format!("optional argument '{key}' exceeds maximum {max}"));
    }
    Ok(parsed)
}

pub(super) fn optional_u64(arguments: &Value, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let parsed = value
        .as_u64()
        .ok_or_else(|| format!("optional argument '{key}' must be an integer"))?;
    Ok(Some(parsed))
}

pub(super) fn optional_positive_u64(arguments: &Value, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let parsed = value
        .as_u64()
        .ok_or_else(|| format!("optional argument '{key}' must be an integer"))?;
    if parsed == 0 {
        return Err(format!("optional argument '{key}' must be greater than 0"));
    }
    Ok(Some(parsed))
}

pub(super) fn optional_positive_usize(
    arguments: &Value,
    key: &str,
) -> Result<Option<usize>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let parsed_u64 = value
        .as_u64()
        .ok_or_else(|| format!("optional argument '{key}' must be an integer"))?;
    if parsed_u64 == 0 {
        return Err(format!("optional argument '{key}' must be greater than 0"));
    }
    let parsed = usize::try_from(parsed_u64)
        .map_err(|_| format!("optional argument '{key}' exceeds host usize range"))?;
    Ok(Some(parsed))
}

pub(super) fn optional_basis_points(arguments: &Value, key: &str) -> Result<Option<u16>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let Some(parsed) = value.as_u64() else {
        return Err(format!("'{key}' must be an integer in range 0..=10000"));
    };
    if parsed > 10_000 {
        return Err(format!("'{key}' must be <= 10000"));
    }
    Ok(Some(parsed as u16))
}

pub(super) fn optional_string_array(
    arguments: &Value,
    key: &str,
    max_items: usize,
    max_chars_per_item: usize,
) -> Result<Vec<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(format!("'{key}' must be an array of strings"));
    };
    if items.len() > max_items {
        return Err(format!("'{key}' exceeds max length of {max_items} items"));
    }
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let Some(raw) = item.as_str() else {
            return Err(format!("'{key}' must be an array of strings"));
        };
        let normalized = raw.trim();
        if normalized.is_empty() {
            continue;
        }
        if normalized.chars().count() > max_chars_per_item {
            return Err(format!(
                "'{key}' entry exceeds max length of {max_chars_per_item} characters"
            ));
        }
        values.push(normalized.to_string());
    }
    Ok(values)
}

pub(super) fn optional_string(arguments: &Value, key: &str) -> Result<Option<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let Some(raw) = value.as_str() else {
        return Err(format!("optional argument '{key}' must be a string"));
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.to_string()))
}

pub(super) fn optional_string_array_unbounded(
    arguments: &Value,
    key: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(format!("'{key}' must be an array of strings"));
    };
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let Some(raw) = item.as_str() else {
            return Err(format!("'{key}' must be an array of strings"));
        };
        values.push(raw.to_string());
    }
    Ok(values)
}

pub(super) fn optional_string_map(
    arguments: &Value,
    key: &str,
) -> Result<BTreeMap<String, String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(BTreeMap::new());
    };
    let Some(object) = value.as_object() else {
        return Err(format!("optional argument '{key}' must be an object"));
    };
    let mut output = BTreeMap::new();
    for (name, entry) in object {
        let normalized = name.trim();
        if normalized.is_empty() {
            return Err(format!("optional argument '{key}' contains an empty key"));
        }
        let Some(raw_value) = entry.as_str() else {
            return Err(format!(
                "optional argument '{key}' value for '{}' must be a string",
                normalized
            ));
        };
        output.insert(normalized.to_string(), raw_value.to_string());
    }
    Ok(output)
}

pub(super) fn resolve_background_job_runtime(
    policy: &Arc<ToolPolicy>,
) -> Result<Arc<BackgroundJobRuntime>, String> {
    let state_dir = normalize_policy_path(policy.jobs_state_dir.as_path());
    let registry = BACKGROUND_JOB_RUNTIME_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match registry.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = guard.get(state_dir.as_path()) {
        return Ok(existing.clone());
    }

    let runtime = BackgroundJobRuntime::new(BackgroundJobRuntimeConfig {
        state_dir: state_dir.clone(),
        default_timeout_ms: policy.jobs_default_timeout_ms.max(1),
        max_timeout_ms: policy
            .jobs_max_timeout_ms
            .max(policy.jobs_default_timeout_ms.max(1)),
        worker_poll_ms: 100,
    })
    .map_err(|error| format!("failed to initialize background job runtime: {error}"))?;
    let runtime = Arc::new(runtime);
    guard.insert(state_dir, runtime.clone());
    Ok(runtime)
}

pub(super) fn background_job_record_payload(
    record: &tau_runtime::BackgroundJobRecord,
    include_output_paths: bool,
) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("job_id".to_string(), json!(record.job_id));
    payload.insert("command".to_string(), json!(record.command));
    payload.insert("args".to_string(), json!(record.args));
    payload.insert("cwd".to_string(), json!(record.cwd));
    payload.insert(
        "env_keys".to_string(),
        json!(record.env.keys().cloned().collect::<Vec<_>>()),
    );
    payload.insert("status".to_string(), json!(record.status.as_str()));
    payload.insert("reason_code".to_string(), json!(record.reason_code));
    payload.insert("created_unix_ms".to_string(), json!(record.created_unix_ms));
    payload.insert("updated_unix_ms".to_string(), json!(record.updated_unix_ms));
    payload.insert("started_unix_ms".to_string(), json!(record.started_unix_ms));
    payload.insert(
        "finished_unix_ms".to_string(),
        json!(record.finished_unix_ms),
    );
    payload.insert("exit_code".to_string(), json!(record.exit_code));
    payload.insert("error".to_string(), json!(record.error));
    payload.insert(
        "requested_timeout_ms".to_string(),
        json!(record.requested_timeout_ms),
    );
    payload.insert(
        "effective_timeout_ms".to_string(),
        json!(record.effective_timeout_ms),
    );
    payload.insert(
        "cancellation_requested".to_string(),
        json!(record.cancellation_requested),
    );
    if include_output_paths {
        payload.insert(
            "stdout_path".to_string(),
            json!(record.stdout_path.display().to_string()),
        );
        payload.insert(
            "stderr_path".to_string(),
            json!(record.stderr_path.display().to_string()),
        );
    }
    Value::Object(payload)
}

pub(super) fn background_job_health_payload(
    health: &tau_runtime::BackgroundJobHealthSnapshot,
) -> Value {
    json!({
        "updated_unix_ms": health.updated_unix_ms,
        "queue_depth": health.queue_depth,
        "running_jobs": health.running_jobs,
        "created_total": health.created_total,
        "started_total": health.started_total,
        "succeeded_total": health.succeeded_total,
        "failed_total": health.failed_total,
        "cancelled_total": health.cancelled_total,
        "last_job_id": health.last_job_id,
        "last_reason_code": health.last_reason_code,
        "reason_codes": health.reason_codes,
        "diagnostics": health.diagnostics,
    })
}

pub(super) fn read_output_preview(path: &Path, max_bytes: usize) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let max_bytes = max_bytes.max(1);
    let (window, truncated) = if bytes.len() > max_bytes {
        (&bytes[bytes.len() - max_bytes..], true)
    } else {
        (bytes.as_slice(), false)
    };
    let mut preview = String::from_utf8_lossy(window).to_string();
    if truncated {
        preview = format!("<output truncated>\n{preview}");
    }
    Ok(redact_secrets(preview.as_str()))
}

pub(super) fn memory_scope_filter_from_arguments(arguments: &Value) -> Option<MemoryScopeFilter> {
    let workspace_id = arguments
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let channel_id = arguments
        .get("channel_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let actor_id = arguments
        .get("actor_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if workspace_id.is_none() && channel_id.is_none() && actor_id.is_none() {
        None
    } else {
        Some(MemoryScopeFilter {
            workspace_id,
            channel_id,
            actor_id,
        })
    }
}

pub(super) fn generate_memory_id() -> String {
    let counter = MEMORY_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("memory-{}-{counter}", current_unix_timestamp_ms())
}

pub(super) fn parse_http_method(arguments: &Value) -> Result<Method, String> {
    let method = arguments
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .trim()
        .to_ascii_uppercase();
    match method.as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "DELETE" => Ok(Method::DELETE),
        _ => Err(format!(
            "unsupported HTTP method '{method}'; supported values: GET, POST, PUT, DELETE"
        )),
    }
}

pub(super) fn parse_http_headers(arguments: &Value) -> Result<Vec<(HeaderName, String)>, String> {
    let Some(headers_value) = arguments.get("headers") else {
        return Ok(Vec::new());
    };
    let header_map = headers_value
        .as_object()
        .ok_or_else(|| "optional argument 'headers' must be an object".to_string())?;
    let mut headers = Vec::with_capacity(header_map.len());
    for (name, raw_value) in header_map {
        let value = raw_value
            .as_str()
            .ok_or_else(|| format!("header '{name}' must be a string value"))?;
        if value.contains('\r') || value.contains('\n') {
            return Err(format!("header '{name}' must not include CR/LF characters"));
        }
        let parsed_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| format!("invalid header name '{name}': {error}"))?;
        HeaderValue::from_str(value)
            .map_err(|error| format!("invalid header value for '{name}': {error}"))?;
        headers.push((parsed_name, value.to_string()));
    }
    Ok(headers)
}

pub(super) fn classify_http_status(status: StatusCode) -> (&'static str, bool) {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ("http_rate_limited", true);
    }
    if status.is_server_error() {
        return ("http_status_server_error", true);
    }
    if status.is_client_error() {
        return ("http_status_client_error", false);
    }
    ("http_status_unexpected", true)
}

pub(super) fn resolve_sandbox_spec(
    policy: &ToolPolicy,
    shell: &str,
    command: &str,
    cwd: &Path,
) -> Result<BashSandboxSpec, String> {
    let bwrap_available = cfg!(target_os = "linux") && command_available("bwrap");
    let docker_available = policy.os_sandbox_docker_enabled && command_available("docker");
    let spec = if !policy.os_sandbox_command.is_empty() {
        build_spec_from_command_template(&policy.os_sandbox_command, shell, command, cwd)?
    } else {
        match policy.os_sandbox_mode {
            OsSandboxMode::Off => BashSandboxSpec {
                program: shell.to_string(),
                args: vec!["-lc".to_string(), command.to_string()],
                sandboxed: false,
                backend: "none".to_string(),
            },
            OsSandboxMode::Auto => {
                if let Some(spec) = auto_sandbox_spec(policy, shell, command, cwd) {
                    spec
                } else {
                    BashSandboxSpec {
                        program: shell.to_string(),
                        args: vec!["-lc".to_string(), command.to_string()],
                        sandboxed: false,
                        backend: "none".to_string(),
                    }
                }
            }
            OsSandboxMode::Force => {
                if let Some(spec) = auto_sandbox_spec(policy, shell, command, cwd) {
                    spec
                } else {
                    if policy.os_sandbox_docker_enabled && !docker_available && !bwrap_available {
                        return Err(SANDBOX_DOCKER_UNAVAILABLE_ERROR.to_string());
                    }
                    return Err(SANDBOX_FORCE_UNAVAILABLE_ERROR.to_string());
                }
            }
        }
    };

    if matches!(policy.os_sandbox_policy_mode, OsSandboxPolicyMode::Required) && !spec.sandboxed {
        if policy.os_sandbox_docker_enabled
            && matches!(
                policy.os_sandbox_mode,
                OsSandboxMode::Auto | OsSandboxMode::Force
            )
            && !docker_available
            && !bwrap_available
        {
            return Err(SANDBOX_DOCKER_UNAVAILABLE_ERROR.to_string());
        }
        return Err(SANDBOX_REQUIRED_UNAVAILABLE_ERROR.to_string());
    }

    Ok(spec)
}

pub(super) fn build_spec_from_command_template(
    template: &[String],
    shell: &str,
    command: &str,
    cwd: &Path,
) -> Result<BashSandboxSpec, String> {
    let Some(program_template) = template.first() else {
        return Err("sandbox command template is empty".to_string());
    };
    let mut args = Vec::new();
    let mut has_shell = false;
    let mut has_command = false;
    for token in &template[1..] {
        if token == "{shell}" {
            has_shell = true;
            args.push(shell.to_string());
            continue;
        }
        if token == "{command}" {
            has_command = true;
            args.push(command.to_string());
            continue;
        }
        args.push(token.replace("{cwd}", &cwd.display().to_string()));
    }

    let program = program_template.replace("{cwd}", &cwd.display().to_string());
    if !has_shell {
        args.push(shell.to_string());
    }
    if !has_command {
        args.push("-lc".to_string());
        args.push(command.to_string());
    }

    Ok(BashSandboxSpec {
        program,
        args,
        sandboxed: true,
        backend: "template".to_string(),
    })
}

pub(super) fn auto_sandbox_spec(
    policy: &ToolPolicy,
    shell: &str,
    command: &str,
    cwd: &Path,
) -> Option<BashSandboxSpec> {
    #[cfg(not(target_os = "linux"))]
    let _ = shell;

    #[cfg(target_os = "linux")]
    {
        if command_available("bwrap") {
            let mut args = vec![
                "--die-with-parent".to_string(),
                "--new-session".to_string(),
                "--unshare-all".to_string(),
                "--proc".to_string(),
                "/proc".to_string(),
                "--dev".to_string(),
                "/dev".to_string(),
                "--tmpfs".to_string(),
                "/tmp".to_string(),
            ];
            for mount in ["/usr", "/bin", "/lib", "/lib64"] {
                if Path::new(mount).exists() {
                    args.extend_from_slice(&[
                        "--ro-bind".to_string(),
                        mount.to_string(),
                        mount.to_string(),
                    ]);
                }
            }
            args.extend_from_slice(&[
                "--bind".to_string(),
                cwd.display().to_string(),
                cwd.display().to_string(),
                "--chdir".to_string(),
                cwd.display().to_string(),
                shell.to_string(),
                "-lc".to_string(),
                command.to_string(),
            ]);
            return Some(BashSandboxSpec {
                program: "bwrap".to_string(),
                args,
                sandboxed: true,
                backend: "bwrap".to_string(),
            });
        }
    }

    if policy.os_sandbox_docker_enabled && command_available("docker") {
        return Some(build_docker_sandbox_spec(policy, command, cwd));
    }

    None
}

pub(super) fn command_available(command: &str) -> bool {
    let path = std::env::var_os("PATH");
    let Some(path) = path else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(command);
        if candidate.exists() {
            return true;
        }
    }
    false
}

pub(super) fn build_docker_sandbox_spec(
    policy: &ToolPolicy,
    command: &str,
    cwd: &Path,
) -> BashSandboxSpec {
    let image = policy.os_sandbox_docker_image.trim();
    let cwd_display = cwd.display().to_string();
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "--init".to_string(),
        "--network".to_string(),
        os_sandbox_docker_network_name(policy.os_sandbox_docker_network).to_string(),
        "--pids-limit".to_string(),
        policy.os_sandbox_docker_pids_limit.to_string(),
        "--memory".to_string(),
        format!("{}m", policy.os_sandbox_docker_memory_mb),
        "--cpus".to_string(),
        format!("{:.3}", policy.os_sandbox_docker_cpu_limit),
        "--security-opt".to_string(),
        "no-new-privileges".to_string(),
        "--cap-drop".to_string(),
        "ALL".to_string(),
        "--tmpfs".to_string(),
        format!(
            "/tmp:rw,nosuid,nodev,noexec,size={}m",
            DOCKER_SANDBOX_TMPFS_SIZE_MB
        ),
        "--volume".to_string(),
        format!("{cwd_display}:{cwd_display}:rw"),
        "--workdir".to_string(),
        cwd_display,
    ];
    if policy.os_sandbox_docker_read_only_rootfs {
        args.push("--read-only".to_string());
    }
    for env_name in &policy.os_sandbox_docker_env_allowlist {
        if let Ok(value) = std::env::var(env_name) {
            args.push("--env".to_string());
            args.push(format!("{env_name}={value}"));
        }
    }
    args.push(image.to_string());
    args.push("sh".to_string());
    args.push("-lc".to_string());
    args.push(command.to_string());
    BashSandboxSpec {
        program: "docker".to_string(),
        args,
        sandboxed: true,
        backend: "docker".to_string(),
    }
}

pub(super) fn leading_executable(command: &str) -> Option<String> {
    let tokens = shell_words::split(command).ok()?;
    for token in tokens {
        if is_shell_assignment(&token) {
            continue;
        }

        return Some(
            Path::new(&token)
                .file_name()
                .map(|file_name| file_name.to_string_lossy().to_string())
                .unwrap_or(token),
        );
    }
    None
}

pub(super) fn is_shell_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn is_command_allowed(executable: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|entry| {
        if let Some(prefix) = entry.strip_suffix('*') {
            executable.starts_with(prefix)
        } else {
            executable == entry
        }
    })
}

pub(super) fn bash_profile_name(profile: BashCommandProfile) -> &'static str {
    match profile {
        BashCommandProfile::Permissive => "permissive",
        BashCommandProfile::Balanced => "balanced",
        BashCommandProfile::Strict => "strict",
    }
}

pub(super) fn os_sandbox_mode_name(mode: OsSandboxMode) -> &'static str {
    match mode {
        OsSandboxMode::Off => "off",
        OsSandboxMode::Auto => "auto",
        OsSandboxMode::Force => "force",
    }
}

/// Return stable string label for OS sandbox policy mode.
pub fn os_sandbox_policy_mode_name(mode: OsSandboxPolicyMode) -> &'static str {
    match mode {
        OsSandboxPolicyMode::BestEffort => "best-effort",
        OsSandboxPolicyMode::Required => "required",
    }
}

/// Return stable string label for OS sandbox docker network mode.
pub fn os_sandbox_docker_network_name(mode: OsSandboxDockerNetwork) -> &'static str {
    match mode {
        OsSandboxDockerNetwork::None => "none",
        OsSandboxDockerNetwork::Bridge => "bridge",
        OsSandboxDockerNetwork::Host => "host",
    }
}

pub(super) fn truncate_bytes(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }

    if limit == 0 {
        return "<output truncated>".to_string();
    }

    let mut end = limit.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }

    let mut output = value[..end].to_string();
    output.push_str("\n<output truncated>");
    output
}

pub(super) fn redact_secrets(text: &str) -> String {
    static DETECTOR: OnceLock<DefaultLeakDetector> = OnceLock::new();
    let detector = DETECTOR.get_or_init(DefaultLeakDetector::new);
    let mut redacted = detector.scan(text, "[REDACTED]").redacted_text;

    for (name, value) in std::env::vars() {
        let upper = name.to_ascii_uppercase();
        let is_sensitive = upper.ends_with("_KEY")
            || upper.ends_with("_TOKEN")
            || upper.ends_with("_SECRET")
            || upper.ends_with("_PASSWORD");
        if !is_sensitive || value.trim().len() < 6 {
            continue;
        }

        redacted = redacted.replace(&value, "[REDACTED]");
    }

    redacted
}
