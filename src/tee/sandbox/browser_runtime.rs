use crate::tee::sandbox::{error::SandboxError, nsjail::NsjailSandbox};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

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
        }

        let script_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/scripts/sandbox_executor.cjs");
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
        env.insert("PLAYWRIGHT_BROWSERS_PATH".to_string(), "0".to_string());

        let mut child = sandbox
            .spawn_scoped_process(
                vec![
                    "node".to_string(),
                    script_path.to_string_lossy().to_string(),
                ],
                runtime_dir,
                env,
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

    pub async fn screenshot(
        &self,
        sensitive_selectors: &[String],
    ) -> Result<Vec<u8>, SandboxError> {
        let response = self
            .send(json!({
                "type": "execute",
                "operationType": "screenshot",
                "parameters": {
                    "maskSelectors": sensitive_selectors,
                },
            }))
            .await?;

        let encoded = response
            .get("screenshot_base64")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SandboxError::Other("screenshot_failed: missing image payload".to_string())
            })?;

        STANDARD.decode(encoded).map_err(|error| {
            SandboxError::Serialization(format!("invalid screenshot payload: {error}"))
        })
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
        process
            .stdin
            .write_all(serialized.as_bytes())
            .await
            .map_err(SandboxError::Io)?;
        process
            .stdin
            .write_all(b"\n")
            .await
            .map_err(SandboxError::Io)?;
        process.stdin.flush().await.map_err(SandboxError::Io)?;

        let mut line = String::new();
        process
            .stdout
            .read_line(&mut line)
            .await
            .map_err(SandboxError::Io)?;
        if line.trim().is_empty() {
            let status = process.child.wait().await.map_err(SandboxError::Io)?;
            let mut stderr = String::new();
            let _ = process.stderr.read_to_string(&mut stderr).await;
            let stderr = stderr.trim();
            let detail = if stderr.is_empty() {
                format!("browser runtime closed without response (status: {status})")
            } else {
                format!("browser runtime closed without response (status: {status}): {stderr}")
            };
            return Err(SandboxError::Process(detail));
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
