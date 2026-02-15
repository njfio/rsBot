//! Wasmtime-backed sandbox runtime for extension and generated tool execution.
//!
//! Provides a deny-by-default capability contract with memory/fuel/timeout
//! enforcement and structured reason-code diagnostics.

use std::{
    fmt,
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use wasmparser::{Parser, Payload};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const WASM_PAGE_SIZE_BYTES: u64 = 65_536;
const WASM_MEMORY_EXPORT_NAME: &str = "memory";
const WASM_ALLOC_EXPORT_NAME: &str = "tau_extension_alloc";
const WASM_INVOKE_EXPORT_NAME: &str = "tau_extension_invoke";

/// Default fuel budget for wasm sandbox execution.
pub const WASM_SANDBOX_FUEL_LIMIT_DEFAULT: u64 = 2_000_000;
/// Default memory ceiling for wasm sandbox execution.
pub const WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT: u64 = 32 * 1024 * 1024;
/// Default timeout budget for wasm sandbox execution.
pub const WASM_SANDBOX_TIMEOUT_MS_DEFAULT: u64 = 5_000;
/// Default response byte ceiling for wasm sandbox execution.
pub const WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT: usize = 256_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Enumerates supported filesystem capability modes for wasm sandbox execution.
pub enum WasmSandboxFilesystemMode {
    Deny,
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Enumerates supported network capability modes for wasm sandbox execution.
pub enum WasmSandboxNetworkMode {
    Deny,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Capability profile requested for a sandboxed wasm invocation.
pub struct WasmSandboxCapabilityProfile {
    pub filesystem_mode: WasmSandboxFilesystemMode,
    pub network_mode: WasmSandboxNetworkMode,
    pub env_allowlist: Vec<String>,
}

impl Default for WasmSandboxCapabilityProfile {
    fn default() -> Self {
        Self {
            filesystem_mode: WasmSandboxFilesystemMode::Deny,
            network_mode: WasmSandboxNetworkMode::Deny,
            env_allowlist: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Runtime limits applied to a sandboxed wasm invocation.
pub struct WasmSandboxLimits {
    pub fuel_limit: u64,
    pub memory_limit_bytes: u64,
    pub timeout_ms: u64,
    pub max_response_bytes: usize,
}

impl Default for WasmSandboxLimits {
    fn default() -> Self {
        Self {
            fuel_limit: WASM_SANDBOX_FUEL_LIMIT_DEFAULT,
            memory_limit_bytes: WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT,
            timeout_ms: WASM_SANDBOX_TIMEOUT_MS_DEFAULT,
            max_response_bytes: WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Execution request for the wasm sandbox runtime.
pub struct WasmSandboxExecutionRequest {
    pub module_path: PathBuf,
    pub request_json: String,
    pub limits: WasmSandboxLimits,
    pub capabilities: WasmSandboxCapabilityProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Successful execution report for a sandboxed wasm invocation.
pub struct WasmSandboxExecutionReport {
    pub response_json: String,
    pub fuel_consumed: u64,
    pub reason_codes: Vec<String>,
    pub diagnostics: Vec<String>,
    pub limits: WasmSandboxLimits,
    pub capabilities: WasmSandboxCapabilityProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Structured execution failure for sandboxed wasm invocations.
pub struct WasmSandboxError {
    pub reason_code: String,
    pub message: String,
    pub diagnostics: Vec<String>,
}

impl WasmSandboxError {
    fn new(reason_code: &str, message: impl Into<String>) -> Self {
        Self {
            reason_code: reason_code.to_string(),
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    fn with_diagnostics(
        reason_code: &str,
        message: impl Into<String>,
        diagnostics: Vec<String>,
    ) -> Self {
        Self {
            reason_code: reason_code.to_string(),
            message: message.into(),
            diagnostics,
        }
    }
}

impl fmt::Display for WasmSandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.diagnostics.is_empty() {
            write!(f, "{} ({})", self.message, self.reason_code)
        } else {
            write!(
                f,
                "{} ({}) diagnostics={}",
                self.message,
                self.reason_code,
                self.diagnostics.join("; ")
            )
        }
    }
}

impl std::error::Error for WasmSandboxError {}

#[derive(Debug)]
struct WasmStoreState {
    limits: StoreLimits,
}

/// Executes a wasm module inside a fuel- and memory-bounded wasmtime sandbox.
pub async fn execute_wasm_sandbox(
    request: WasmSandboxExecutionRequest,
) -> Result<WasmSandboxExecutionReport, WasmSandboxError> {
    let join = tokio::task::spawn_blocking(move || execute_wasm_sandbox_sync(request));
    join.await.map_err(|error| {
        WasmSandboxError::new(
            "wasm_execution_join_error",
            format!("failed to join wasm sandbox task: {error}"),
        )
    })?
}

/// Executes a wasm module in a blocking context with timeout enforcement.
pub fn execute_wasm_sandbox_sync(
    request: WasmSandboxExecutionRequest,
) -> Result<WasmSandboxExecutionReport, WasmSandboxError> {
    validate_capability_profile(&request.capabilities)?;
    validate_limits(&request.limits)?;

    let timeout_ms = request.limits.timeout_ms.max(1);
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("tau-wasm-sandbox".to_string())
        .spawn(move || {
            let _ = sender.send(execute_wasm_sandbox_blocking(request));
        })
        .map_err(|error| {
            WasmSandboxError::new(
                "wasm_execution_spawn_failed",
                format!("failed to spawn wasm sandbox worker: {error}"),
            )
        })?;

    match receiver.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(WasmSandboxError::new(
            "wasm_execution_timeout",
            format!("wasm sandbox execution timed out after {} ms", timeout_ms),
        )),
        Err(RecvTimeoutError::Disconnected) => Err(WasmSandboxError::new(
            "wasm_execution_join_error",
            "wasm sandbox worker terminated before returning a result",
        )),
    }
}

fn validate_capability_profile(
    capabilities: &WasmSandboxCapabilityProfile,
) -> Result<(), WasmSandboxError> {
    match capabilities.filesystem_mode {
        WasmSandboxFilesystemMode::Deny => {}
        WasmSandboxFilesystemMode::ReadOnly | WasmSandboxFilesystemMode::ReadWrite => {
            return Err(WasmSandboxError::new(
                "wasm_capability_filesystem_unsupported",
                "filesystem capabilities are not available in this wasm sandbox runtime",
            ));
        }
    }

    if matches!(capabilities.network_mode, WasmSandboxNetworkMode::Allow) {
        return Err(WasmSandboxError::new(
            "wasm_capability_network_unsupported",
            "network capabilities are not available in this wasm sandbox runtime",
        ));
    }

    if !capabilities.env_allowlist.is_empty() {
        return Err(WasmSandboxError::new(
            "wasm_capability_env_unsupported",
            "environment allowlist is not available in this wasm sandbox runtime",
        ));
    }

    Ok(())
}

fn validate_limits(limits: &WasmSandboxLimits) -> Result<(), WasmSandboxError> {
    if limits.fuel_limit == 0 {
        return Err(WasmSandboxError::new(
            "wasm_limit_invalid_fuel",
            "wasm fuel limit must be greater than 0",
        ));
    }
    if limits.memory_limit_bytes == 0 {
        return Err(WasmSandboxError::new(
            "wasm_limit_invalid_memory",
            "wasm memory limit must be greater than 0",
        ));
    }
    if limits.timeout_ms == 0 {
        return Err(WasmSandboxError::new(
            "wasm_limit_invalid_timeout",
            "wasm timeout must be greater than 0",
        ));
    }
    if limits.max_response_bytes == 0 {
        return Err(WasmSandboxError::new(
            "wasm_limit_invalid_response_size",
            "wasm max response bytes must be greater than 0",
        ));
    }
    Ok(())
}

fn execute_wasm_sandbox_blocking(
    request: WasmSandboxExecutionRequest,
) -> Result<WasmSandboxExecutionReport, WasmSandboxError> {
    if !request.module_path.exists() {
        return Err(WasmSandboxError::new(
            "wasm_module_missing",
            format!(
                "wasm module does not exist: {}",
                request.module_path.display()
            ),
        ));
    }
    if !request.module_path.is_file() {
        return Err(WasmSandboxError::new(
            "wasm_module_not_file",
            format!(
                "wasm module path is not a file: {}",
                request.module_path.display()
            ),
        ));
    }

    let module_bytes = std::fs::read(&request.module_path).map_err(|error| {
        WasmSandboxError::new(
            "wasm_module_read_failed",
            format!(
                "failed to read wasm module '{}': {error}",
                request.module_path.display()
            ),
        )
    })?;
    validate_wasm_module_for_limits(&module_bytes, request.limits.memory_limit_bytes)?;

    let mut config = Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config).map_err(|error| {
        WasmSandboxError::new(
            "wasm_engine_init_failed",
            format!("failed to initialize wasm engine: {error}"),
        )
    })?;
    let module = Module::new(&engine, &module_bytes).map_err(|error| {
        WasmSandboxError::new(
            "wasm_module_compile_failed",
            format!(
                "failed to compile wasm module '{}': {error}",
                request.module_path.display()
            ),
        )
    })?;

    let mut store = Store::new(
        &engine,
        WasmStoreState {
            limits: StoreLimitsBuilder::new()
                .memory_size(request.limits.memory_limit_bytes as usize)
                .build(),
        },
    );
    store.limiter(|state| &mut state.limits);
    store.set_fuel(request.limits.fuel_limit).map_err(|error| {
        WasmSandboxError::new(
            "wasm_fuel_config_failed",
            format!("failed to configure wasm fuel limit: {error}"),
        )
    })?;

    let linker = Linker::<WasmStoreState>::new(&engine);
    let instance = linker.instantiate(&mut store, &module).map_err(|error| {
        WasmSandboxError::new(
            "wasm_instance_init_failed",
            format!("failed to instantiate wasm module: {error}"),
        )
    })?;

    let memory = instance
        .get_memory(&mut store, WASM_MEMORY_EXPORT_NAME)
        .ok_or_else(|| {
            WasmSandboxError::new(
                "wasm_export_missing_memory",
                format!(
                    "wasm module missing required memory export '{}'",
                    WASM_MEMORY_EXPORT_NAME
                ),
            )
        })?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, WASM_ALLOC_EXPORT_NAME)
        .map_err(|error| {
            WasmSandboxError::new(
                "wasm_export_missing_alloc",
                format!(
                    "wasm module missing required alloc export '{}': {error}",
                    WASM_ALLOC_EXPORT_NAME
                ),
            )
        })?;
    let invoke = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, WASM_INVOKE_EXPORT_NAME)
        .map_err(|error| {
            WasmSandboxError::new(
                "wasm_export_missing_invoke",
                format!(
                    "wasm module missing required invoke export '{}': {error}",
                    WASM_INVOKE_EXPORT_NAME
                ),
            )
        })?;

    let request_bytes = request.request_json.as_bytes();
    let request_len: i32 = request_bytes.len().try_into().map_err(|_| {
        WasmSandboxError::new(
            "wasm_request_too_large",
            "request payload exceeds wasm i32 length boundary",
        )
    })?;
    let request_ptr = alloc.call(&mut store, request_len).map_err(|error| {
        WasmSandboxError::new(
            "wasm_alloc_failed",
            format!("wasm alloc export failed while reserving request buffer: {error}"),
        )
    })?;
    if request_ptr < 0 {
        return Err(WasmSandboxError::new(
            "wasm_alloc_invalid_pointer",
            "wasm alloc export returned a negative pointer",
        ));
    }
    let request_ptr: usize = request_ptr as usize;
    validate_memory_range(&memory, &store, request_ptr, request_bytes.len()).map_err(|error| {
        WasmSandboxError::new(
            "wasm_request_range_invalid",
            format!("request buffer outside wasm memory bounds: {error}"),
        )
    })?;
    memory
        .write(&mut store, request_ptr, request_bytes)
        .map_err(|error| {
            WasmSandboxError::new(
                "wasm_request_write_failed",
                format!("failed to write request buffer into wasm memory: {error}"),
            )
        })?;

    let packed = invoke
        .call(&mut store, (request_ptr as i32, request_len))
        .map_err(|error| {
            let remaining_fuel = store.get_fuel().unwrap_or_default();
            WasmSandboxError::with_diagnostics(
                "wasm_execution_trap",
                format!("wasm invoke export trapped: {error}"),
                vec![format!(
                    "fuel_consumed={}",
                    request.limits.fuel_limit.saturating_sub(remaining_fuel)
                )],
            )
        })?;
    let packed = packed as u64;
    let response_ptr = (packed >> 32) as usize;
    let response_len = (packed & 0xFFFF_FFFF) as usize;
    if response_len > request.limits.max_response_bytes {
        return Err(WasmSandboxError::new(
            "wasm_response_too_large",
            format!(
                "wasm response length {} exceeds limit {}",
                response_len, request.limits.max_response_bytes
            ),
        ));
    }
    validate_memory_range(&memory, &store, response_ptr, response_len).map_err(|error| {
        WasmSandboxError::new(
            "wasm_response_range_invalid",
            format!("response buffer outside wasm memory bounds: {error}"),
        )
    })?;
    let mut response_bytes = vec![0u8; response_len];
    memory
        .read(&store, response_ptr, &mut response_bytes)
        .map_err(|error| {
            WasmSandboxError::new(
                "wasm_response_read_failed",
                format!("failed to read response bytes from wasm memory: {error}"),
            )
        })?;
    let response_json = String::from_utf8(response_bytes).map_err(|error| {
        WasmSandboxError::new(
            "wasm_response_not_utf8",
            format!("wasm response is not valid UTF-8: {error}"),
        )
    })?;
    if response_json.trim().is_empty() {
        return Err(WasmSandboxError::new(
            "wasm_response_empty",
            "wasm response payload is empty",
        ));
    }
    let remaining_fuel = store.get_fuel().unwrap_or_default();
    let fuel_consumed = request.limits.fuel_limit.saturating_sub(remaining_fuel);
    Ok(WasmSandboxExecutionReport {
        response_json,
        fuel_consumed,
        reason_codes: vec!["wasm_execution_succeeded".to_string()],
        diagnostics: vec![
            format!(
                "module={} fuel_consumed={} memory_limit_bytes={}",
                request.module_path.display(),
                fuel_consumed,
                request.limits.memory_limit_bytes
            ),
            "capabilities=deny-filesystem,deny-network,deny-env".to_string(),
        ],
        limits: request.limits,
        capabilities: request.capabilities,
    })
}

fn validate_wasm_module_for_limits(
    module_bytes: &[u8],
    memory_limit_bytes: u64,
) -> Result<(), WasmSandboxError> {
    let mut diagnostics = Vec::new();
    for payload in Parser::new(0).parse_all(module_bytes) {
        let payload = payload.map_err(|error| {
            WasmSandboxError::new(
                "wasm_module_parse_failed",
                format!("failed to parse wasm module bytes: {error}"),
            )
        })?;
        if let Payload::MemorySection(section) = payload {
            for memory in section {
                let memory = memory.map_err(|error| {
                    WasmSandboxError::new(
                        "wasm_module_parse_failed",
                        format!("failed to parse wasm memory section: {error}"),
                    )
                })?;
                let min_bytes = memory.initial.saturating_mul(WASM_PAGE_SIZE_BYTES);
                diagnostics.push(format!(
                    "memory.initial_pages={} memory.initial_bytes={}",
                    memory.initial, min_bytes
                ));
                if min_bytes > memory_limit_bytes {
                    return Err(WasmSandboxError::with_diagnostics(
                        "wasm_module_memory_declared_exceeds_limit",
                        format!(
                            "wasm module declares minimum memory {} bytes above limit {} bytes",
                            min_bytes, memory_limit_bytes
                        ),
                        diagnostics,
                    ));
                }
                if let Some(max_pages) = memory.maximum {
                    let max_bytes = max_pages.saturating_mul(WASM_PAGE_SIZE_BYTES);
                    diagnostics.push(format!(
                        "memory.maximum_pages={} memory.maximum_bytes={}",
                        max_pages, max_bytes
                    ));
                    if max_bytes > memory_limit_bytes {
                        return Err(WasmSandboxError::with_diagnostics(
                            "wasm_module_memory_declared_exceeds_limit",
                            format!(
                                "wasm module declares maximum memory {} bytes above limit {} bytes",
                                max_bytes, memory_limit_bytes
                            ),
                            diagnostics,
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_memory_range(
    memory: &wasmtime::Memory,
    store: &Store<WasmStoreState>,
    offset: usize,
    len: usize,
) -> Result<(), String> {
    let memory_size = memory.data_size(store);
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "memory range overflow".to_string())?;
    if end > memory_size {
        return Err(format!(
            "offset={} len={} end={} exceeds memory_size={}",
            offset, len, end, memory_size
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        execute_wasm_sandbox, WasmSandboxCapabilityProfile, WasmSandboxError,
        WasmSandboxExecutionRequest, WasmSandboxFilesystemMode, WasmSandboxLimits,
    };
    use tempfile::tempdir;

    fn write_wasm(path: &std::path::Path, wat_source: &str) {
        let bytes = wat::parse_str(wat_source).expect("parse wat");
        std::fs::write(path, bytes).expect("write wasm file");
    }

    fn ok_module_wat() -> &'static str {
        r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (data (i32.const 0) "{\"content\":{\"status\":\"ok\",\"message\":\"done\"},\"is_error\":false}")
  (func (export "tau_extension_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    global.get $heap
    local.set $ptr
    global.get $heap
    local.get $len
    i32.add
    global.set $heap
    local.get $ptr)
  (func (export "tau_extension_invoke") (param i32 i32) (result i64)
    i64.const 61)
)"#
    }

    #[test]
    fn unit_wasm_sandbox_capability_profile_default_denies_all() {
        let profile = WasmSandboxCapabilityProfile::default();
        assert_eq!(profile.filesystem_mode, WasmSandboxFilesystemMode::Deny);
        assert!(profile.env_allowlist.is_empty());
    }

    #[tokio::test]
    async fn functional_execute_wasm_sandbox_returns_response_payload() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("ok.wasm");
        write_wasm(&module_path, ok_module_wat());

        let report = execute_wasm_sandbox(WasmSandboxExecutionRequest {
            module_path,
            request_json: "{\"hook\":\"tool-call\"}".to_string(),
            limits: WasmSandboxLimits::default(),
            capabilities: WasmSandboxCapabilityProfile::default(),
        })
        .await
        .expect("wasm sandbox execution should succeed");

        assert!(report.response_json.contains("\"status\":\"ok\""));
        assert!(report.fuel_consumed > 0);
        assert!(report
            .reason_codes
            .iter()
            .any(|reason| reason == "wasm_execution_succeeded"));
    }

    #[tokio::test]
    async fn regression_execute_wasm_sandbox_rejects_declared_memory_above_limit() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("large-memory.wasm");
        write_wasm(
            &module_path,
            r#"(module
  (memory (export "memory") 2)
  (func (export "tau_extension_alloc") (param i32) (result i32) i32.const 0)
  (func (export "tau_extension_invoke") (param i32 i32) (result i64) i64.const 0)
)"#,
        );

        let error = execute_wasm_sandbox(WasmSandboxExecutionRequest {
            module_path,
            request_json: "{}".to_string(),
            limits: WasmSandboxLimits {
                memory_limit_bytes: 65_536,
                ..WasmSandboxLimits::default()
            },
            capabilities: WasmSandboxCapabilityProfile::default(),
        })
        .await
        .expect_err("declared memory over limit should fail closed");

        assert_eq!(
            error.reason_code,
            "wasm_module_memory_declared_exceeds_limit"
        );
    }

    #[tokio::test]
    async fn regression_execute_wasm_sandbox_rejects_unsupported_filesystem_capability() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("ok.wasm");
        write_wasm(&module_path, ok_module_wat());

        let error = execute_wasm_sandbox(WasmSandboxExecutionRequest {
            module_path,
            request_json: "{}".to_string(),
            limits: WasmSandboxLimits::default(),
            capabilities: WasmSandboxCapabilityProfile {
                filesystem_mode: WasmSandboxFilesystemMode::ReadOnly,
                ..WasmSandboxCapabilityProfile::default()
            },
        })
        .await
        .expect_err("unsupported filesystem capability should fail closed");

        assert_eq!(error.reason_code, "wasm_capability_filesystem_unsupported");
    }

    #[tokio::test]
    async fn regression_execute_wasm_sandbox_rejects_invalid_wasm_bytes() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("invalid.wasm");
        std::fs::write(&module_path, b"not-wasm").expect("write invalid bytes");

        let error = execute_wasm_sandbox(WasmSandboxExecutionRequest {
            module_path,
            request_json: "{}".to_string(),
            limits: WasmSandboxLimits::default(),
            capabilities: WasmSandboxCapabilityProfile::default(),
        })
        .await
        .expect_err("invalid module bytes should fail closed");

        assert!(matches!(
            error,
            WasmSandboxError {
                reason_code,
                ..
            } if reason_code == "wasm_module_parse_failed" || reason_code == "wasm_module_compile_failed"
        ));
    }
}
