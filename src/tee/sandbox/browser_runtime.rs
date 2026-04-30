use crate::tee::sandbox::{
    config::{MountConfig, MountType},
    error::SandboxError,
    nsjail::{NsjailSandbox, spawn_child_reaper},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, info, warn};

const SANDBOX_NODE_BINARY_ENV: &str = "CREDBRIDGE_SANDBOX_NODE_BINARY";
const NODE_PATH_ENV: &str = "NODE_PATH";
const LIGHTPANDA_BINARY_PATH_ENV: &str = "LIGHTPANDA_BINARY_PATH";
const LIGHTPANDA_DISABLE_TELEMETRY_ENV: &str = "LIGHTPANDA_DISABLE_TELEMETRY";

const NODE_BINARY_CANDIDATES: &[&str] = &["/usr/bin/node", "/usr/local/bin/node"];
const NODE_PATH_CANDIDATES: &[&str] = &["/opt/credbridge-browser-runtime/node_modules"];
const LIGHTPANDA_BINARY_PATH_CANDIDATES: &[&str] = &["/usr/local/bin/lightpanda"];
const DEV_NULL_PATH: &str = "/dev/null";
const BROWSER_RUNTIME_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const BROWSER_RUNTIME_KILL_TIMEOUT: Duration = Duration::from_secs(3);
const BROWSER_RUNTIME_RPC_TIMEOUT: Duration = Duration::from_secs(45);
const SYSTEM_RUNTIME_MOUNT_CANDIDATES: &[&str] = &[
    "/etc/resolv.conf",
    "/etc/hosts",
    "/etc/nsswitch.conf",
    "/etc/ssl/certs",
    "/etc/ssl/cert.pem",
];

#[derive(Clone)]
pub struct SandboxBrowserRuntime {
    inner: Arc<Mutex<BrowserRuntimeProcess>>,
}

struct BrowserRuntimeProcess {
    child: Option<Child>,
    stdin: ChildStdin,
    stderr: BufReader<ChildStderr>,
    stdout: BufReader<ChildStdout>,
}

impl SandboxBrowserRuntime {
    pub async fn launch(work_dir: PathBuf, sandbox: &NsjailSandbox) -> Result<Self, SandboxError> {
        let runtime_dir = work_dir.join("browser-runtime");
        let profile_dir = work_dir.join("browser-profile");
        let home_dir = work_dir.join("home");
        let cache_dir = work_dir.join("cache");
        let data_dir = work_dir.join("data");
        let tmp_dir = work_dir.join("tmp");

        for dir in [
            &runtime_dir,
            &profile_dir,
            &home_dir,
            &cache_dir,
            &data_dir,
            &tmp_dir,
        ] {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(SandboxError::Io)?;
            sandbox.assign_mapped_root_owner(dir)?;
        }

        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/tee/sandbox/scripts/sandbox_executor.cjs");
        let node_binary = resolve_node_binary()?;
        let node_path = resolve_node_path()?;
        let lightpanda_binary_path = resolve_lightpanda_binary_path()?;
        let extra_mounts = browser_runtime_mounts(
            &script_path,
            Path::new(&node_path),
            Path::new(&lightpanda_binary_path),
        );
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
        env.insert("TMPDIR".to_string(), tmp_dir.to_string_lossy().to_string());
        env.insert(
            "XDG_CACHE_HOME".to_string(),
            cache_dir.to_string_lossy().to_string(),
        );
        env.insert(
            "XDG_CONFIG_HOME".to_string(),
            data_dir.join("config").to_string_lossy().to_string(),
        );
        env.insert(
            "XDG_DATA_HOME".to_string(),
            data_dir.to_string_lossy().to_string(),
        );
        env.insert(NODE_PATH_ENV.to_string(), node_path);
        env.insert(
            LIGHTPANDA_BINARY_PATH_ENV.to_string(),
            lightpanda_binary_path,
        );
        env.insert(
            LIGHTPANDA_DISABLE_TELEMETRY_ENV.to_string(),
            env::var(LIGHTPANDA_DISABLE_TELEMETRY_ENV).unwrap_or_else(|_| "true".to_string()),
        );

        // Browser runtime uses a dedicated relaxed seccomp profile so node/lightpanda
        // can start without dropping all syscall filtering for the scoped process.
        let mut child = sandbox
            .spawn_scoped_process(
                vec![node_binary, script_path.to_string_lossy().to_string()],
                runtime_dir.clone(),
                env,
                true,
                extra_mounts,
            )
            .await?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| SandboxError::Process("browser runtime missing stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SandboxError::Process("browser runtime missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SandboxError::Process("browser runtime missing stderr".to_string()))?;

        let runtime = Self {
            inner: Arc::new(Mutex::new(BrowserRuntimeProcess {
                child: Some(child),
                stdin,
                stderr: BufReader::new(stderr),
                stdout: BufReader::new(stdout),
            })),
        };

        info!(
            sandbox_id = %sandbox.id,
            work_dir = %work_dir.display(),
            runtime_dir = %runtime_dir.display(),
            profile_dir = %profile_dir.display(),
            "launched browser runtime directories"
        );

        runtime
            .send(json!({
                "type": "init",
                "profileDir": profile_dir.to_string_lossy(),
            }))
            .await?;

        Ok(runtime)
    }

    pub async fn navigate(&self, url: &str) -> Result<String, SandboxError> {
        let response = self
            .send(json!({
                "type": "execute",
                "operationType": "navigate",
                "parameters": { "url": url },
            }))
            .await?;

        response
            .get("data")
            .and_then(|value| value.get("final_url"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| SandboxError::Other("navigation_failed: missing final_url".to_string()))
    }

    pub async fn click(&self, selector: &str) -> Result<(), SandboxError> {
        self.send(json!({
            "type": "execute",
            "operationType": "click",
            "parameters": { "selector": selector },
        }))
        .await
        .map(|_| ())
    }

    pub async fn fill(&self, selector: &str, value: &str) -> Result<(), SandboxError> {
        self.send(json!({
            "type": "execute",
            "operationType": "fill",
            "parameters": { "selector": selector, "value": value },
        }))
        .await
        .map(|_| ())
    }

    pub async fn wait_for_selector(
        &self,
        selector: Option<&str>,
        timeout_ms: u64,
    ) -> Result<(), SandboxError> {
        let mut parameters = serde_json::Map::new();
        parameters.insert("timeout_ms".to_string(), Value::Number(timeout_ms.into()));
        if let Some(selector) = selector {
            parameters.insert("selector".to_string(), Value::String(selector.to_string()));
        }

        self.send(json!({
            "type": "execute",
            "operationType": "wait",
            "parameters": parameters,
        }))
        .await
        .map(|_| ())
    }

    pub async fn selector_text_metadata(&self, selector: &str) -> Result<Value, SandboxError> {
        self.send(json!({
            "type": "execute",
            "operationType": "selector_metadata",
            "parameters": { "selector": selector },
        }))
        .await
        .map(|response| response.get("data").cloned().unwrap_or(Value::Null))
    }

    pub async fn execute_script(
        &self,
        script: &str,
        bindings: &HashMap<String, String>,
    ) -> Result<Value, SandboxError> {
        let response = self
            .send(json!({
                "type": "execute",
                "operationType": "execute_script",
                "parameters": {
                    "script": script,
                    "bindings": bindings,
                },
            }))
            .await?;

        Ok(response
            .get("data")
            .and_then(|value| value.get("result"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    pub async fn bootstrap_page(
        &self,
        mode: &str,
        script_selectors: &[String],
        include_plain_scripts: bool,
        replay_lifecycle_events: bool,
        wait_selector: Option<&str>,
        wait_timeout_ms: u64,
    ) -> Result<Value, SandboxError> {
        let started_at = Instant::now();
        info!(
            mode,
            script_selector_count = script_selectors.len(),
            include_plain_scripts,
            replay_lifecycle_events,
            wait_selector = wait_selector.unwrap_or(""),
            wait_timeout_ms,
            "browser runtime bootstrap_page start"
        );
        let response = self
            .send(json!({
                "type": "execute",
                "operationType": "bootstrap_page",
                "parameters": {
                    "mode": mode,
                    "script_selectors": script_selectors,
                    "include_plain_scripts": include_plain_scripts,
                    "replay_lifecycle_events": replay_lifecycle_events,
                    "wait_selector": wait_selector,
                    "wait_timeout_ms": wait_timeout_ms,
                },
            }))
            .await?;

        let payload = response.get("data").cloned().unwrap_or(Value::Null);
        let diagnostics = payload
            .get("diagnostics")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let compatibility_injections = diagnostics
            .get("compatibility_injections")
            .cloned()
            .unwrap_or(Value::Null);
        let gtag_before_type = diagnostics
            .get("gtag_before_type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let gtag_after_type = diagnostics
            .get("gtag_after_type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let data_layer_initialized = diagnostics
            .get("data_layer_initialized")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let compatibility_applied = diagnostics
            .get("compatibility_applied")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        info!(
            compatibility_injections = %compatibility_injections,
            gtag_before_type,
            gtag_after_type,
            data_layer_initialized,
            compatibility_applied,
            "browser runtime bootstrap compatibility pass start"
        );
        info!(
            compatibility_injections = %compatibility_injections,
            gtag_before_type,
            gtag_after_type,
            data_layer_initialized,
            compatibility_applied,
            "browser runtime bootstrap compatibility pass completed"
        );
        info!(
            elapsed_ms = started_at.elapsed().as_millis(),
            discovered_scripts = diagnostics
                .get("discovered_scripts")
                .and_then(|value| value.as_u64())
                .unwrap_or(0),
            reinjected_scripts = diagnostics
                .get("reinjected_scripts")
                .and_then(|value| value.as_u64())
                .unwrap_or(0),
            compatibility_injections = %compatibility_injections,
            gtag_before_type,
            gtag_after_type,
            data_layer_initialized,
            compatibility_applied,
            ready_state_before_scan = diagnostics
                .get("ready_state_before_scan")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            ready_state_after_injection = diagnostics
                .get("ready_state_after_injection")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            "browser runtime bootstrap_page completed"
        );

        Ok(payload)
    }

    pub async fn execute_export(
        &self,
        selectors: &[String],
        mask_selectors: &[String],
    ) -> Result<Value, SandboxError> {
        self.send(json!({
            "type": "execute",
            "operationType": "export",
            "parameters": {
                "selectors": selectors,
                "maskSelectors": mask_selectors,
            },
        }))
        .await
        .map(|response| response.get("data").cloned().unwrap_or(Value::Null))
    }

    pub async fn current_url(&self) -> Result<String, SandboxError> {
        self.send(json!({
            "type": "execute",
            "operationType": "browser_state",
            "parameters": {},
        }))
        .await?
        .get("data")
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| SandboxError::Other("browser_state_failed: missing current url".to_string()))
    }

    pub async fn dom_export(
        &self,
        root_selector: &str,
        format: &str,
        include_text: bool,
        include_metadata: bool,
        sensitive_selectors: &[String],
    ) -> Result<Value, SandboxError> {
        self.send(json!({
            "type": "execute",
            "operationType": "dom_export",
            "parameters": {
                "root_selector": root_selector,
                "format": format,
                "include_text": include_text,
                "include_metadata": include_metadata,
                "maskSelectors": sensitive_selectors,
            },
        }))
        .await
        .map(|response| response.get("data").cloned().unwrap_or(Value::Null))
    }

    pub async fn close(&self) -> Result<(), SandboxError> {
        let close_result = match timeout(
            BROWSER_RUNTIME_CLOSE_TIMEOUT,
            self.send(json!({
                "type": "execute",
                "operationType": "close_runtime",
                "parameters": {},
            })),
        )
        .await
        {
            Ok(result) => result.map(|_| ()),
            Err(_) => Err(SandboxError::Timeout {
                operation: "close_runtime".to_string(),
            }),
        };

        let mut process = self.inner.lock().await;
        let Some(mut child) = process.child.take() else {
            return close_result;
        };

        let wait_result = timeout(BROWSER_RUNTIME_KILL_TIMEOUT, async {
            let _ = child.start_kill();
            child.wait().await.map_err(SandboxError::Io)
        })
        .await;

        match wait_result {
            Ok(Ok(_)) => close_result,
            Ok(Err(error)) => Err(error),
            Err(_) => Err(SandboxError::Timeout {
                operation: "kill_browser_runtime".to_string(),
            }),
        }
    }

    async fn send(&self, message: Value) -> Result<Value, SandboxError> {
        self.send_with_timeout(message, BROWSER_RUNTIME_RPC_TIMEOUT)
            .await
    }

    async fn send_with_timeout(
        &self,
        message: Value,
        timeout_duration: Duration,
    ) -> Result<Value, SandboxError> {
        let mut process = self.inner.lock().await;
        let message_type = message
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let operation_type = message
            .get("operationType")
            .and_then(Value::as_str)
            .unwrap_or("");
        let started_at = Instant::now();
        let serialized = serde_json::to_string(&message).map_err(|error| {
            SandboxError::Serialization(format!("failed to encode browser message: {error}"))
        })?;
        let request_label = browser_runtime_request_label(message_type, operation_type);
        let line = match timeout(
            timeout_duration,
            exchange_browser_runtime_message(&mut process, &serialized),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                let exit_detail = terminate_browser_runtime_process(
                    &mut process,
                    &format!("timed out during {request_label}"),
                )
                .await;
                warn!(
                    message_type,
                    operation_type,
                    timeout_ms = timeout_duration.as_millis(),
                    elapsed_ms = started_at.elapsed().as_millis(),
                    exit_detail = exit_detail.as_deref().unwrap_or(""),
                    "browser runtime request timed out"
                );
                return Err(SandboxError::Timeout {
                    operation: request_label,
                });
            }
        };
        debug!(
            message_type,
            operation_type,
            elapsed_ms = started_at.elapsed().as_millis(),
            response_len = line.len(),
            "browser runtime response line read"
        );
        if line.trim().is_empty() {
            let Some(mut child) = process.child.take() else {
                return Err(SandboxError::Process(
                    "browser runtime closed without response; child already reaped".to_string(),
                ));
            };
            let status = child.wait().await.map_err(SandboxError::Io)?;
            let mut stderr = String::new();
            let _ = process.stderr.read_to_string(&mut stderr).await;
            return Err(SandboxError::Process(browser_runtime_exit_detail(
                "closed without response",
                status,
                stderr.trim(),
            )));
        }

        let response: Value = serde_json::from_str(line.trim()).map_err(|error| {
            SandboxError::Serialization(format!("failed to decode browser response: {error}"))
        })?;

        if !response.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            warn!(
                message_type,
                operation_type,
                elapsed_ms = started_at.elapsed().as_millis(),
                error = response
                    .get("error")
                    .and_then(|value| value.as_str())
                    .unwrap_or("browser runtime error"),
                "browser runtime returned error response"
            );
            return Err(map_runtime_error(operation_type, &response));
        }

        debug!(
            message_type,
            operation_type,
            elapsed_ms = started_at.elapsed().as_millis(),
            "browser runtime returned ok response"
        );

        Ok(response)
    }
}

async fn exchange_browser_runtime_message(
    process: &mut BrowserRuntimeProcess,
    serialized: &str,
) -> Result<String, SandboxError> {
    if let Err(error) = process.stdin.write_all(serialized.as_bytes()).await {
        return Err(browser_runtime_io_error("write request", process, error).await);
    }
    if let Err(error) = process.stdin.write_all(b"\n").await {
        return Err(browser_runtime_io_error("write request terminator", process, error).await);
    }
    if let Err(error) = process.stdin.flush().await {
        return Err(browser_runtime_io_error("flush request", process, error).await);
    }

    let mut line = String::new();
    if let Err(error) = process.stdout.read_line(&mut line).await {
        return Err(browser_runtime_io_error("read response", process, error).await);
    }

    Ok(line)
}

fn map_runtime_error(operation_type: &str, response: &Value) -> SandboxError {
    let message = response
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("browser runtime error")
        .to_string();
    let error_code = response
        .get("error_code")
        .and_then(Value::as_str)
        .unwrap_or("");

    if error_code == "timeout" || browser_runtime_error_is_timeout(&message) {
        return SandboxError::Timeout {
            operation: browser_runtime_operation_label(operation_type),
        };
    }

    SandboxError::Other(message)
}

fn browser_runtime_request_label(message_type: &str, operation_type: &str) -> String {
    if !operation_type.trim().is_empty() {
        operation_type.to_string()
    } else if !message_type.trim().is_empty() {
        format!("browser_runtime_{}", message_type.trim())
    } else {
        "browser_runtime_operation".to_string()
    }
}

fn browser_runtime_operation_label(operation_type: &str) -> String {
    if operation_type.trim().is_empty() {
        "browser_runtime_operation".to_string()
    } else {
        operation_type.to_string()
    }
}

fn browser_runtime_error_is_timeout(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains(" timed out")
        || lower.starts_with("timeout ")
        || lower.contains(" timeout ")
        || lower.ends_with(" timeout")
}

async fn browser_runtime_io_error(
    action: &str,
    process: &mut BrowserRuntimeProcess,
    error: std::io::Error,
) -> SandboxError {
    if let Some(child) = process.child.as_mut()
        && let Ok(Some(status)) = child.try_wait()
    {
        let mut stderr = String::new();
        let _ = process.stderr.read_to_string(&mut stderr).await;
        return SandboxError::Process(browser_runtime_exit_detail(
            &format!("failed to {action}"),
            status,
            stderr.trim(),
        ));
    }

    SandboxError::Io(error)
}

async fn terminate_browser_runtime_process(
    process: &mut BrowserRuntimeProcess,
    context: &str,
) -> Option<String> {
    let mut child = process.child.take()?;
    let _ = child.start_kill();

    match timeout(BROWSER_RUNTIME_KILL_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => Some(browser_runtime_exit_detail(context, status, "")),
        Ok(Err(error)) => Some(format!(
            "browser runtime {context}: failed to wait for process exit: {error}"
        )),
        Err(_) => {
            spawn_child_reaper(child, "browser runtime timeout reaper".to_string(), true);
            Some(format!(
                "browser runtime {context}: process did not exit within {} ms",
                BROWSER_RUNTIME_KILL_TIMEOUT.as_millis()
            ))
        }
    }
}

impl Drop for BrowserRuntimeProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            spawn_child_reaper(child, "browser runtime".to_string(), true);
        }
    }
}

fn browser_runtime_exit_detail(
    context: &str,
    status: impl std::fmt::Display,
    stderr: &str,
) -> String {
    if stderr.is_empty() {
        format!("browser runtime {context} (status: {status})")
    } else {
        format!("browser runtime {context} (status: {status}): {stderr}")
    }
}

fn resolve_node_binary() -> Result<String, SandboxError> {
    resolve_executable_path(SANDBOX_NODE_BINARY_ENV, "node", NODE_BINARY_CANDIDATES)
}

fn resolve_node_path() -> Result<String, SandboxError> {
    resolve_existing_path(NODE_PATH_ENV, NODE_PATH_CANDIDATES)
}

fn resolve_lightpanda_binary_path() -> Result<String, SandboxError> {
    if let Ok(configured) = env::var(LIGHTPANDA_BINARY_PATH_ENV) {
        let trimmed = configured.trim();
        if !trimmed.is_empty() && trimmed != "0" {
            let configured_path = Path::new(trimmed);
            if is_executable_file(configured_path) {
                return Ok(trimmed.to_string());
            }
            return Err(SandboxError::Config(format!(
                "{LIGHTPANDA_BINARY_PATH_ENV} points to a non-executable path: {trimmed}"
            )));
        }
    }

    resolve_executable_path(
        LIGHTPANDA_BINARY_PATH_ENV,
        "lightpanda",
        LIGHTPANDA_BINARY_PATH_CANDIDATES,
    )
}

fn resolve_existing_path(env_key: &str, candidates: &[&str]) -> Result<String, SandboxError> {
    if let Ok(configured) = env::var(env_key) {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            let configured_path = Path::new(trimmed);
            if configured_path.exists() {
                return Ok(trimmed.to_string());
            }
            return Err(SandboxError::Config(format!(
                "{env_key} points to missing path: {trimmed}"
            )));
        }
    }

    for candidate in candidates {
        if Path::new(candidate).exists() {
            return Ok((*candidate).to_string());
        }
    }

    Err(SandboxError::Config(format!(
        "{env_key} is not configured and none of the default paths exist: {}",
        candidates.join(", ")
    )))
}

fn resolve_executable_path(
    env_key: &str,
    command_name: &str,
    candidates: &[&str],
) -> Result<String, SandboxError> {
    if let Ok(configured) = env::var(env_key) {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            let configured_path = Path::new(trimmed);
            if is_executable_file(configured_path) {
                return Ok(trimmed.to_string());
            }
            return Err(SandboxError::Config(format!(
                "{env_key} points to a non-executable path: {trimmed}"
            )));
        }
    }

    if let Some(path_value) = env::var_os("PATH") {
        for entry in env::split_paths(&path_value) {
            let candidate = entry.join(command_name);
            if is_executable_file(&candidate) {
                return Ok(candidate.to_string_lossy().to_string());
            }
        }
    }

    for candidate in candidates {
        let candidate_path = Path::new(candidate);
        if is_executable_file(candidate_path) {
            return Ok((*candidate).to_string());
        }
    }

    Err(SandboxError::Config(format!(
        "{command_name} executable is not available; set {env_key} or install it in one of: {}",
        candidates.join(", ")
    )))
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn browser_runtime_mounts(
    script_path: &Path,
    node_path: &Path,
    lightpanda_binary_path: &Path,
) -> Vec<MountConfig> {
    let mut mounts = Vec::new();

    if let Some(script_dir) = script_path.parent() {
        push_read_only_bind_mount(&mut mounts, script_dir);
    }
    push_read_only_bind_mount(&mut mounts, Path::new(DEV_NULL_PATH));
    push_read_only_bind_mount(&mut mounts, node_path);
    push_read_only_bind_mount(&mut mounts, lightpanda_binary_path);
    for candidate in SYSTEM_RUNTIME_MOUNT_CANDIDATES {
        push_existing_read_only_bind_mount(&mut mounts, Path::new(candidate));
    }

    mounts
}

fn push_existing_read_only_bind_mount(mounts: &mut Vec<MountConfig>, path: &Path) {
    if path.exists() {
        push_read_only_bind_mount(mounts, path);
    }
}

fn push_read_only_bind_mount(mounts: &mut Vec<MountConfig>, path: &Path) {
    if mounts
        .iter()
        .any(|mount| mount.src == path && mount.dst == path)
    {
        return;
    }

    mounts.push(MountConfig {
        src: path.to_path_buf(),
        dst: path.to_path_buf(),
        mount_type: MountType::Bind,
        read_only: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    async fn spawn_test_runtime(command: &str) -> SandboxBrowserRuntime {
        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("test runtime should spawn");
        let stdin = child
            .stdin
            .take()
            .expect("test runtime should expose stdin");
        let stdout = child
            .stdout
            .take()
            .expect("test runtime should expose stdout");
        let stderr = child
            .stderr
            .take()
            .expect("test runtime should expose stderr");

        SandboxBrowserRuntime {
            inner: Arc::new(Mutex::new(BrowserRuntimeProcess {
                child: Some(child),
                stdin,
                stderr: BufReader::new(stderr),
                stdout: BufReader::new(stdout),
            })),
        }
    }

    #[test]
    fn test_browser_runtime_mounts_include_runtime_paths() {
        let mounts = browser_runtime_mounts(
            Path::new("/app/src/tee/sandbox/scripts/sandbox_executor.cjs"),
            Path::new("/opt/credbridge-browser-runtime/node_modules"),
            Path::new("/usr/local/bin/lightpanda"),
        );

        assert!(mounts.iter().any(|mount| {
            mount.src == Path::new("/app/src/tee/sandbox/scripts")
                && mount.dst == Path::new("/app/src/tee/sandbox/scripts")
                && mount.read_only
        }));
        assert!(mounts.iter().any(|mount| {
            mount.src == Path::new("/dev/null")
                && mount.dst == Path::new("/dev/null")
                && mount.read_only
        }));
        assert!(mounts.iter().any(|mount| {
            mount.src == Path::new("/opt/credbridge-browser-runtime/node_modules")
                && mount.dst == Path::new("/opt/credbridge-browser-runtime/node_modules")
                && mount.read_only
        }));
        assert!(mounts.iter().any(|mount| {
            mount.src == Path::new("/usr/local/bin/lightpanda")
                && mount.dst == Path::new("/usr/local/bin/lightpanda")
                && mount.read_only
        }));
    }

    #[test]
    fn test_browser_runtime_mount_candidates_include_network_runtime_files() {
        assert!(SYSTEM_RUNTIME_MOUNT_CANDIDATES.contains(&"/etc/resolv.conf"));
        assert!(SYSTEM_RUNTIME_MOUNT_CANDIDATES.contains(&"/etc/hosts"));
        assert!(SYSTEM_RUNTIME_MOUNT_CANDIDATES.contains(&"/etc/nsswitch.conf"));
        assert!(SYSTEM_RUNTIME_MOUNT_CANDIDATES.contains(&"/etc/ssl/certs"));
    }

    #[test]
    fn test_browser_runtime_mounts_include_existing_network_runtime_files() {
        let mounts = browser_runtime_mounts(
            Path::new("/app/src/tee/sandbox/scripts/sandbox_executor.cjs"),
            Path::new("/opt/credbridge-browser-runtime/node_modules"),
            Path::new("/usr/local/bin/lightpanda"),
        );

        for candidate in SYSTEM_RUNTIME_MOUNT_CANDIDATES {
            let path = Path::new(candidate);
            if path.exists() {
                assert!(
                    mounts
                        .iter()
                        .any(|mount| { mount.src == path && mount.dst == path && mount.read_only }),
                    "existing browser runtime system path should be mounted read-only: {candidate}"
                );
            }
        }
    }

    #[test]
    fn test_browser_runtime_exit_detail_includes_stderr_when_present() {
        let detail = browser_runtime_exit_detail(
            "failed to read response",
            "exit status: 1",
            "nsjail: bad uid map",
        );

        assert_eq!(
            detail,
            "browser runtime failed to read response (status: exit status: 1): nsjail: bad uid map"
        );
    }

    #[test]
    fn test_map_runtime_error_classifies_timeout_response() {
        let error = map_runtime_error(
            "navigate",
            &json!({
                "ok": false,
                "error": "Navigation timeout of 30000 ms exceeded",
                "error_code": "timeout",
            }),
        );

        assert!(matches!(
            error,
            SandboxError::Timeout { operation } if operation == "navigate"
        ));
    }

    #[test]
    fn test_map_runtime_error_preserves_non_timeout_message() {
        let error = map_runtime_error(
            "click",
            &json!({
                "ok": false,
                "error": "selector_not_found: #missing",
                "error_code": "runtime_error",
            }),
        );

        assert!(matches!(
            error,
            SandboxError::Other(message) if message == "selector_not_found: #missing"
        ));
    }

    #[test]
    fn test_browser_runtime_request_label_uses_message_type_when_operation_is_empty() {
        assert_eq!(
            browser_runtime_request_label("init", ""),
            "browser_runtime_init"
        );
        assert_eq!(
            browser_runtime_request_label("", ""),
            "browser_runtime_operation"
        );
    }

    #[tokio::test]
    async fn test_send_timeout_kills_runtime_process_and_returns_timeout() {
        let runtime = spawn_test_runtime("while IFS= read -r _line; do sleep 60; done").await;

        let error = runtime
            .send_with_timeout(
                json!({
                    "type": "init",
                    "profileDir": "/tmp/test-profile",
                }),
                Duration::from_millis(50),
            )
            .await
            .expect_err("timed out runtime request should fail");

        assert!(matches!(
            error,
            SandboxError::Timeout { operation } if operation == "browser_runtime_init"
        ));
        assert!(
            runtime.inner.lock().await.child.is_none(),
            "timed out runtime process should be reaped so the session can rebuild a fresh runtime"
        );
    }
}
