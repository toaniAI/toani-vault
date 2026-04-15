use crate::tee::sandbox::{
    config::{MountConfig, MountType},
    error::SandboxError,
    nsjail::NsjailSandbox,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

const SANDBOX_NODE_BINARY_ENV: &str = "CREDBRIDGE_SANDBOX_NODE_BINARY";
const NODE_PATH_ENV: &str = "NODE_PATH";
const LIGHTPANDA_BINARY_PATH_ENV: &str = "LIGHTPANDA_BINARY_PATH";
const LIGHTPANDA_DISABLE_TELEMETRY_ENV: &str = "LIGHTPANDA_DISABLE_TELEMETRY";

const NODE_BINARY_CANDIDATES: &[&str] = &["/usr/bin/node", "/usr/local/bin/node"];
const NODE_PATH_CANDIDATES: &[&str] = &["/opt/credbridge-browser-runtime/node_modules"];
const LIGHTPANDA_BINARY_PATH_CANDIDATES: &[&str] = &["/usr/local/bin/lightpanda"];
const DEV_NULL_PATH: &str = "/dev/null";

#[derive(Clone)]
pub struct SandboxBrowserRuntime {
    inner: Arc<Mutex<BrowserRuntimeProcess>>,
}

struct BrowserRuntimeProcess {
    child: Child,
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
                runtime_dir,
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
                child,
                stdin,
                stderr: BufReader::new(stderr),
                stdout: BufReader::new(stdout),
            })),
        };

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

    pub async fn close(&self) {
        let _ = self
            .send(json!({
                "type": "execute",
                "operationType": "close_runtime",
                "parameters": {},
            }))
            .await;

        let mut process = self.inner.lock().await;
        let _ = process.child.start_kill();
        let _ = process.child.wait().await;
    }

    async fn send(&self, message: Value) -> Result<Value, SandboxError> {
        let mut process = self.inner.lock().await;
        let serialized = serde_json::to_string(&message).map_err(|error| {
            SandboxError::Serialization(format!("failed to encode browser message: {error}"))
        })?;
        if let Err(error) = process.stdin.write_all(serialized.as_bytes()).await {
            return Err(browser_runtime_io_error("write request", &mut process, error).await);
        }
        if let Err(error) = process.stdin.write_all(b"\n").await {
            return Err(
                browser_runtime_io_error("write request terminator", &mut process, error).await,
            );
        }
        if let Err(error) = process.stdin.flush().await {
            return Err(browser_runtime_io_error("flush request", &mut process, error).await);
        }

        let mut line = String::new();
        if let Err(error) = process.stdout.read_line(&mut line).await {
            return Err(browser_runtime_io_error("read response", &mut process, error).await);
        }
        if line.trim().is_empty() {
            let status = process.child.wait().await.map_err(SandboxError::Io)?;
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
            return Err(SandboxError::Other(
                response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("browser runtime error")
                    .to_string(),
            ));
        }

        Ok(response)
    }
}

async fn browser_runtime_io_error(
    action: &str,
    process: &mut BrowserRuntimeProcess,
    error: std::io::Error,
) -> SandboxError {
    if let Ok(Some(status)) = process.child.try_wait() {
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

    mounts
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
}
