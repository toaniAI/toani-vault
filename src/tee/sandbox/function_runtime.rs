use crate::models::{CredentialCustomFunction, CredentialProvider};
use crate::tee::sandbox::error::SandboxError;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command;
use tokio::time::{Duration, timeout};
use tracing::warn;
use uuid::Uuid;

const IMPORTS_PREFIX: &str = "/* @imports:";
const FUNCTION_INSTALL_TIMEOUT: Duration = Duration::from_secs(60);
const FUNCTION_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct FunctionImportSpec {
    package: String,
    version: String,
}

#[derive(Debug, Clone)]
pub struct FunctionExecutionContext {
    pub credential: HashMap<String, String>,
    pub provider: Option<CredentialProvider>,
    pub method: String,
    pub url: String,
    pub query: String,
    pub headers: serde_json::Map<String, Value>,
    pub body: Option<Value>,
}

pub fn validate_custom_functions(
    custom_functions: &[CredentialCustomFunction],
) -> Result<(), SandboxError> {
    let mut names = HashSet::new();
    for function in custom_functions {
        let name = function.function_name.trim();
        if name.is_empty() {
            return Err(SandboxError::Other(
                "custom_functions.function_name must not be empty".to_string(),
            ));
        }
        if !names.insert(name.to_string()) {
            return Err(SandboxError::Other(format!(
                "duplicate custom function name: {name}"
            )));
        }
        if function.function_body.trim().is_empty() {
            return Err(SandboxError::Other(format!(
                "custom function body is required: {name}"
            )));
        }

        parse_imports_annotation(&function.function_body)?;
        ensure_safe_function_body(&function.function_body)?;
    }
    Ok(())
}

pub async fn execute_custom_function(
    custom_functions: &[CredentialCustomFunction],
    function_name: &str,
    args: &[Value],
    context: &FunctionExecutionContext,
) -> Result<String, SandboxError> {
    let function = custom_functions
        .iter()
        .find(|item| item.function_name == function_name)
        .ok_or_else(|| SandboxError::Other(format!("unknown custom function: {function_name}")))?;

    let imports = parse_imports_annotation(&function.function_body)?;
    ensure_safe_function_body(&function.function_body)?;
    let runtime_dir = ensure_runtime_directory(&imports).await?;
    let execution_dir = runtime_dir.join(format!("run-{}", Uuid::now_v7()));
    fs::create_dir_all(&execution_dir).map_err(|error| {
        SandboxError::Other(format!(
            "failed to create function runtime directory: {error}"
        ))
    })?;

    let user_module_path = execution_dir.join("user.mts");
    let runner_path = execution_dir.join("runner.mjs");
    let result = async {
        fs::write(&user_module_path, function.function_body.as_bytes()).map_err(|error| {
            SandboxError::Other(format!("failed to write custom function source: {error}"))
        })?;
        fs::write(
            &runner_path,
            build_runner_script(
                context,
                args,
                user_module_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("user.mts"),
            )?,
        )
        .map_err(|error| {
            SandboxError::Other(format!("failed to write function runner: {error}"))
        })?;

        let output = timeout(
            FUNCTION_EXECUTION_TIMEOUT,
            Command::new("node")
                .arg("--experimental-strip-types")
                .arg("--permission")
                .arg(format!("--allow-fs-read={}", runtime_dir.display()))
                .arg(format!("--allow-fs-write={}", execution_dir.display()))
                .arg(runner_path.as_os_str())
                .current_dir(&execution_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SandboxError::Timeout {
            operation: format!("custom function {function_name}"),
        })?
        .map_err(|error| {
            SandboxError::Other(format!("failed to execute custom function: {error}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SandboxError::Other(format!(
                "custom function {function_name} failed: {}",
                stderr.trim()
            )));
        }

        String::from_utf8(output.stdout).map_err(|error| {
            SandboxError::Other(format!(
                "custom function output is not valid UTF-8: {error}"
            ))
        })
    }
    .await;

    cleanup_execution_directory(&execution_dir);
    result
}

fn cleanup_execution_directory(execution_dir: &Path) {
    if let Err(error) = fs::remove_dir_all(execution_dir) {
        if execution_dir.exists() {
            warn!(
                path = %execution_dir.display(),
                "failed to remove custom function execution directory: {error}"
            );
        }
    }
}

fn parse_imports_annotation(body: &str) -> Result<Vec<FunctionImportSpec>, SandboxError> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with("/*") {
        return Ok(Vec::new());
    }

    let Some(end_index) = trimmed.find("*/") else {
        return Err(SandboxError::Other(
            "custom function imports annotation must be terminated with */".to_string(),
        ));
    };

    let annotation = &trimmed[..end_index + 2];
    if !annotation.starts_with(IMPORTS_PREFIX) {
        return Ok(Vec::new());
    }

    let inner = annotation
        .trim_start_matches(IMPORTS_PREFIX)
        .trim_end_matches("*/")
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(',')
        .map(|item| {
            let spec = item.trim();
            let Some((package, version)) = spec.rsplit_once('@') else {
                return Err(SandboxError::Other(format!(
                    "invalid @imports package spec: {spec}"
                )));
            };
            let package = package.trim();
            let version = version.trim();
            if package.is_empty() || version.is_empty() {
                return Err(SandboxError::Other(format!(
                    "invalid @imports package spec: {spec}"
                )));
            }
            if !package_name_regex().is_match(package) {
                return Err(SandboxError::Other(format!(
                    "custom function imports require a registry package name: {package}"
                )));
            }
            if !exact_semver_regex().is_match(version) {
                return Err(SandboxError::Other(format!(
                    "custom function imports require an exact semver version: {spec}"
                )));
            }
            Ok(FunctionImportSpec {
                package: package.to_string(),
                version: version.to_string(),
            })
        })
        .collect()
}

fn ensure_safe_function_body(body: &str) -> Result<(), SandboxError> {
    let forbidden_patterns = [
        "process.env",
        "require(",
        "import(",
        "child_process",
        "node:child_process",
        "worker_threads",
        "node:worker_threads",
        "node:fs",
        "node:fs/promises",
        " from 'fs'",
        " from \"fs\"",
        " from 'http'",
        " from \"http\"",
        " from 'https'",
        " from \"https\"",
        " from 'net'",
        " from \"net\"",
        " from 'tls'",
        " from \"tls\"",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
    ];

    if let Some(pattern) = forbidden_patterns
        .iter()
        .find(|pattern| body.contains(**pattern))
    {
        return Err(SandboxError::Other(format!(
            "custom function body contains forbidden capability: {pattern}"
        )));
    }

    if !body.contains("export default") {
        return Err(SandboxError::Other(
            "custom function body must export a default function".to_string(),
        ));
    }

    Ok(())
}

async fn ensure_runtime_directory(imports: &[FunctionImportSpec]) -> Result<PathBuf, SandboxError> {
    let runtime_root = std::env::temp_dir().join("credbridge-custom-functions");
    fs::create_dir_all(&runtime_root).map_err(|error| {
        SandboxError::Other(format!("failed to create function cache root: {error}"))
    })?;

    let mut hasher = Sha256::new();
    for import in imports {
        hasher.update(import.package.as_bytes());
        hasher.update(b"@");
        hasher.update(import.version.as_bytes());
        hasher.update(b";");
    }
    let key = format!("{:x}", hasher.finalize());
    let runtime_dir = runtime_root.join(key);
    fs::create_dir_all(&runtime_dir).map_err(|error| {
        SandboxError::Other(format!(
            "failed to create function cache directory: {error}"
        ))
    })?;

    ensure_package_manifest(&runtime_dir)?;
    if !imports.is_empty() && !dependencies_ready(&runtime_dir, imports) {
        install_runtime_dependencies(&runtime_dir, imports).await?;
    }

    Ok(runtime_dir)
}

fn ensure_package_manifest(runtime_dir: &Path) -> Result<(), SandboxError> {
    let package_json = runtime_dir.join("package.json");
    if package_json.exists() {
        return Ok(());
    }

    fs::write(
        package_json,
        r#"{
  "name": "credbridge-custom-function-runtime",
  "private": true,
  "type": "module"
}
"#,
    )
    .map_err(|error| SandboxError::Other(format!("failed to write package.json: {error}")))
}

fn dependencies_ready(runtime_dir: &Path, imports: &[FunctionImportSpec]) -> bool {
    imports.iter().all(|import| {
        runtime_dir
            .join("node_modules")
            .join(import.package.as_str())
            .exists()
    })
}

async fn install_runtime_dependencies(
    runtime_dir: &Path,
    imports: &[FunctionImportSpec],
) -> Result<(), SandboxError> {
    let packages = imports
        .iter()
        .map(|import| format!("{}@{}", import.package, import.version))
        .collect::<Vec<_>>();

    let output = timeout(
        FUNCTION_INSTALL_TIMEOUT,
        Command::new("npm")
            .arg("install")
            .arg("--no-package-lock")
            .arg("--no-audit")
            .arg("--ignore-scripts")
            .arg("--silent")
            .args(&packages)
            .current_dir(runtime_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| SandboxError::Timeout {
        operation: "custom function dependency install".to_string(),
    })?
    .map_err(|error| {
        SandboxError::Other(format!(
            "failed to install custom function dependencies: {error}"
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SandboxError::Other(format!(
            "npm install failed for custom function dependencies: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

fn build_runner_script(
    context: &FunctionExecutionContext,
    args: &[Value],
    user_module_file: &str,
) -> Result<String, SandboxError> {
    let input_json = serde_json::to_string(&serde_json::json!({
        "args": args,
        "credential": context.credential,
        "provider": context.provider.map(|provider| provider.as_str().to_string()),
        "method": context.method,
        "url": context.url,
        "query": context.query,
        "headers": context.headers,
        "body": context.body,
    }))
    .map_err(|error| {
        SandboxError::Serialization(format!("serialize function input failed: {error}"))
    })?;

    Ok(format!(
        r#"import process from "node:process";

const input = {input_json};
const credential = input.credential ?? {{}};
const deny = (message) => () => {{
  throw new Error(message);
}};

Object.defineProperty(process, "env", {{
  configurable: false,
  enumerable: false,
  get() {{
    throw new Error("process.env is disabled");
  }},
  set() {{
    throw new Error("process.env is disabled");
  }},
}});

globalThis.fetch = deny("network access is disabled");
globalThis.XMLHttpRequest = class {{
  constructor() {{
    throw new Error("network access is disabled");
  }}
}};
globalThis.WebSocket = class {{
  constructor() {{
    throw new Error("network access is disabled");
  }}
}};

Object.assign(globalThis, credential, {{
  credential,
  provider: input.provider ?? null,
  args: input.args ?? [],
  method: input.method,
  url: input.url,
  query: input.query,
  headers: input.headers ?? {{}},
  body: input.body ?? null,
}});

const userModule = await import("./{user_module_file}");
const userFunction = userModule.default;

if (typeof userFunction !== "function") {{
  throw new Error("custom function module must export a default function");
}}

const result = await userFunction({{
  args: input.args ?? [],
  credential,
  provider: input.provider ?? null,
  method: input.method,
  url: input.url,
  query: input.query,
  headers: input.headers ?? {{}},
  body: input.body ?? null,
}});

if (typeof result !== "string") {{
  throw new Error("custom function must return a string");
}}

process.stdout.write(result);
"#,
    ))
}

fn package_name_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$")
            .expect("package name regex should compile")
    })
}

fn exact_semver_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$").expect("semver regex should compile")
    })
}

#[cfg(test)]
mod tests {
    use super::{cleanup_execution_directory, parse_imports_annotation, validate_custom_functions};
    use crate::models::CredentialCustomFunction;
    use std::fs;

    #[test]
    fn parses_imports_annotation() {
        let imports = parse_imports_annotation(
            "/* @imports: crypto-js@4.2.0, qs@6.13.0 */\nexport default function func() { return \"ok\"; }",
        )
        .expect("imports should parse");
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].package, "crypto-js");
        assert_eq!(imports[0].version, "4.2.0");
        assert_eq!(imports[1].package, "qs");
        assert_eq!(imports[1].version, "6.13.0");
    }

    #[test]
    fn rejects_non_registry_import_specs() {
        let error = parse_imports_annotation(
            "/* @imports: npm:crypto-js@4.2.0 */\nexport default function func() { return \"ok\"; }",
        )
        .expect_err("npm alias imports should be rejected");
        assert!(error.to_string().contains("registry package name"));
    }

    #[test]
    fn rejects_non_exact_semver_import_versions() {
        let error = parse_imports_annotation(
            "/* @imports: crypto-js@latest */\nexport default function func() { return \"ok\"; }",
        )
        .expect_err("non exact versions should be rejected");
        assert!(error.to_string().contains("exact semver"));
    }

    #[test]
    fn rejects_duplicate_function_names() {
        let error = validate_custom_functions(&[
            CredentialCustomFunction {
                function_name: "dup".to_string(),
                function_description: None,
                function_body: "export default function func() { return \"a\"; }".to_string(),
            },
            CredentialCustomFunction {
                function_name: "dup".to_string(),
                function_description: None,
                function_body: "export default function func() { return \"b\"; }".to_string(),
            },
        ])
        .expect_err("duplicate names should be rejected");
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn cleanup_execution_directory_removes_run_directory() {
        let execution_dir =
            std::env::temp_dir().join(format!("credbridge-test-run-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&execution_dir).expect("execution dir should be created");
        fs::write(execution_dir.join("runner.mjs"), "console.log('ok');")
            .expect("runner file should be created");

        cleanup_execution_directory(&execution_dir);

        assert!(
            !execution_dir.exists(),
            "execution directory should be removed after cleanup"
        );
    }
}
