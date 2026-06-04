use crate::models::{CredentialCustomFunction, CredentialProvider};
use crate::tee::sandbox::error::SandboxError;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
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
struct RunnerInvocation {
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Clone)]
struct CustomFunctionExecutionRequest {
    function_name: String,
    runtime_dir: PathBuf,
    execution_dir: PathBuf,
    runner_path: PathBuf,
}

#[async_trait]
trait CustomFunctionRunner: Send + Sync {
    async fn execute(
        &self,
        request: &CustomFunctionExecutionRequest,
    ) -> Result<Output, SandboxError>;
}

#[derive(Debug, Default)]
struct DefaultCustomFunctionRunner;

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
    let runner = DefaultCustomFunctionRunner;
    execute_custom_function_with_runner(custom_functions, function_name, args, context, &runner)
        .await
}

async fn execute_custom_function_with_runner(
    custom_functions: &[CredentialCustomFunction],
    function_name: &str,
    args: &[Value],
    context: &FunctionExecutionContext,
    runner: &dyn CustomFunctionRunner,
) -> Result<String, SandboxError> {
    let function = custom_functions
        .iter()
        .find(|item| item.function_name == function_name)
        .ok_or_else(|| SandboxError::Other(format!("unknown custom function: {function_name}")))?;

    let imports = parse_imports_annotation(&function.function_body)?;
    ensure_safe_function_body(&function.function_body)?;
    let runtime_dir = ensure_runtime_directory(&imports).await?;
    let execution_dir = create_execution_directory()?;
    prepare_execution_directory(&runtime_dir, &execution_dir, &imports)?;

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

        let output = runner
            .execute(&CustomFunctionExecutionRequest {
                function_name: function_name.to_string(),
                runtime_dir: runtime_dir.clone(),
                execution_dir: execution_dir.clone(),
                runner_path: runner_path.clone(),
            })
            .await?;

        if !output.status.success() {
            let stderr = redact_sensitive_values(&String::from_utf8_lossy(&output.stderr), context);
            let reason = non_empty_runner_message(stderr.trim())
                .unwrap_or_else(|| format!("runner exited with status {}", output.status));
            return Err(SandboxError::Other(format!(
                "custom function {function_name} failed: {reason}",
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
    if let Err(error) = fs::remove_dir_all(execution_dir)
        && execution_dir.exists()
    {
        warn!(
            path = %execution_dir.display(),
            "failed to remove custom function execution directory: {error}"
        );
    }
}

#[async_trait]
impl CustomFunctionRunner for DefaultCustomFunctionRunner {
    async fn execute(
        &self,
        request: &CustomFunctionExecutionRequest,
    ) -> Result<Output, SandboxError> {
        let node_binary = resolve_binary("node")?;
        let invocation = build_runner_invocation(
            cfg!(target_os = "macos") && Path::new("/usr/bin/sandbox-exec").exists(),
            node_binary,
            &request.runtime_dir,
            &request.execution_dir,
            &request.runner_path,
        );
        let mut command = Command::new(&invocation.program);
        command
            .args(&invocation.args)
            .current_dir(&request.execution_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        timeout(FUNCTION_EXECUTION_TIMEOUT, command.output())
            .await
            .map_err(|_| SandboxError::Timeout {
                operation: format!("custom function {}", request.function_name),
            })?
            .map_err(|error| {
                SandboxError::Other(format!("failed to execute custom function runner: {error}"))
            })
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
    let runtime_root = runtime_cache_root();
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

fn runtime_cache_root() -> PathBuf {
    canonical_temp_dir()
        .join("credbridge-custom-functions")
        .join("cache")
}

fn execution_root() -> PathBuf {
    canonical_temp_dir()
        .join("credbridge-custom-functions")
        .join("run")
}

fn canonical_temp_dir() -> PathBuf {
    let temp_dir = std::env::temp_dir();
    fs::canonicalize(&temp_dir).unwrap_or(temp_dir)
}

fn create_execution_directory() -> Result<PathBuf, SandboxError> {
    let root = execution_root();
    fs::create_dir_all(&root).map_err(|error| {
        SandboxError::Other(format!("failed to create function execution root: {error}"))
    })?;
    let execution_dir = root.join(format!("run-{}", Uuid::now_v7()));
    fs::create_dir_all(&execution_dir).map_err(|error| {
        SandboxError::Other(format!(
            "failed to create function execution directory: {error}"
        ))
    })?;

    fs::canonicalize(&execution_dir).map_err(|error| {
        SandboxError::Other(format!(
            "failed to canonicalize function execution directory: {error}"
        ))
    })
}

fn prepare_execution_directory(
    runtime_dir: &Path,
    execution_dir: &Path,
    imports: &[FunctionImportSpec],
) -> Result<(), SandboxError> {
    ensure_package_manifest(execution_dir)?;
    if imports.is_empty() {
        return Ok(());
    }

    let source_node_modules = runtime_dir.join("node_modules");
    if !source_node_modules.exists() {
        return Ok(());
    }

    let target_node_modules = execution_dir.join("node_modules");
    if target_node_modules.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source_node_modules, &target_node_modules).map_err(
            |error| {
                SandboxError::Other(format!(
                    "failed to link execution dependencies into runtime directory: {error}"
                ))
            },
        )?;

        Ok(())
    }

    #[cfg(not(unix))]
    {
        copy_directory_recursive(&source_node_modules, &target_node_modules).map_err(|error| {
            SandboxError::Other(format!(
                "failed to copy execution dependencies into runtime directory: {error}"
            ))
        })?;
        Ok(())
    }
}

#[cfg(not(unix))]
fn copy_directory_recursive(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_directory_recursive(&source_path, &target_path)?;
            continue;
        }

        if file_type.is_file() {
            fs::copy(&source_path, &target_path)?;
            continue;
        }

        if file_type.is_symlink() {
            let resolved = fs::canonicalize(&source_path)?;
            if resolved.is_dir() {
                copy_directory_recursive(&resolved, &target_path)?;
            } else {
                fs::copy(&resolved, &target_path)?;
            }
        }
    }
    Ok(())
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

fn build_runner_invocation(
    use_macos_sandbox: bool,
    node_binary: PathBuf,
    runtime_dir: &Path,
    execution_dir: &Path,
    runner_path: &Path,
) -> RunnerInvocation {
    let runner_entrypoint = runner_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("runner.mjs")
        .to_string();
    let node_binary_display = node_binary.display().to_string();
    let mut node_args = vec![
        "--experimental-strip-types".to_string(),
        "--permission".to_string(),
    ];
    for path in permission_path_variants(runtime_dir) {
        node_args.push(format!("--allow-fs-read={path}"));
    }
    for path in permission_path_variants(execution_dir) {
        node_args.push(format!("--allow-fs-read={path}"));
        node_args.push(format!("--allow-fs-write={path}"));
    }
    node_args.push(runner_entrypoint);

    if use_macos_sandbox {
        let mut args = vec![
            "-p".to_string(),
            build_macos_seatbelt_profile(&node_binary, runtime_dir, execution_dir),
            node_binary_display,
        ];
        args.extend(node_args);
        RunnerInvocation {
            program: "/usr/bin/sandbox-exec".to_string(),
            args,
        }
    } else {
        RunnerInvocation {
            program: node_binary_display,
            args: node_args,
        }
    }
}

fn permission_path_variants(path: &Path) -> Vec<String> {
    let display = path.display().to_string();
    let mut variants = vec![display.clone()];

    #[cfg(windows)]
    {
        const VERBATIM_PREFIX: &str = r"\\?\";
        if let Some(stripped) = display.strip_prefix(VERBATIM_PREFIX) {
            variants.push(stripped.to_string());
        } else if path.is_absolute() {
            variants.push(format!("{VERBATIM_PREFIX}{display}"));
        }
    }

    variants.dedup();
    variants
}

fn build_macos_seatbelt_profile(
    node_binary: &Path,
    runtime_dir: &Path,
    execution_dir: &Path,
) -> String {
    let node_dir = node_binary
        .parent()
        .unwrap_or_else(|| Path::new("/usr/bin"));
    let execution_dir = execution_dir.display();
    let runtime_dir = runtime_dir.display();
    let node_dir = node_dir.display();

    format!(
        "(version 1) \
         (deny default) \
         (import \"system.sb\") \
         (allow process-exec) \
         (allow file-read* \
             (subpath \"{node_dir}\") \
             (subpath \"/System\") \
             (subpath \"/usr\") \
             (subpath \"/opt/homebrew\") \
             (subpath \"/Library\") \
             (subpath \"/private/var/select\") \
             (subpath \"/dev\") \
             (subpath \"{runtime_dir}\") \
             (subpath \"{execution_dir}\")) \
         (allow file-read-metadata) \
         (allow file-write* (subpath \"{execution_dir}\")) \
         (deny network*) \
         (deny process-fork)"
    )
}

fn resolve_binary(binary_name: &str) -> Result<PathBuf, SandboxError> {
    let path = std::env::var_os("PATH").ok_or_else(|| {
        SandboxError::Other(format!("failed to resolve {binary_name}: PATH is not set"))
    })?;

    #[cfg(windows)]
    let mut candidates = vec![binary_name.to_string()];
    #[cfg(not(windows))]
    let candidates = vec![binary_name.to_string()];
    #[cfg(windows)]
    if Path::new(binary_name).extension().is_none() {
        candidates.push(format!("{binary_name}.exe"));
        candidates.push(format!("{binary_name}.cmd"));
        candidates.push(format!("{binary_name}.bat"));
    }

    for dir in std::env::split_paths(&path) {
        for candidate in &candidates {
            let candidate_path = dir.join(candidate);
            if candidate_path.is_file() {
                return fs::canonicalize(&candidate_path).or(Ok(candidate_path));
            }
        }
    }

    Err(SandboxError::Other(format!(
        "failed to resolve {binary_name}: not found in PATH"
    )))
}

fn redact_sensitive_values(text: &str, context: &FunctionExecutionContext) -> String {
    let mut values = context
        .credential
        .values()
        .filter(|value| !value.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();

    let mut redacted = text.to_string();
    for sensitive in values {
        redacted = if sensitive.len() >= "[REDACTED]".len() {
            redacted.replace(&sensitive, "[REDACTED]")
        } else {
            replace_sensitive_with_boundaries(&redacted, &sensitive)
        };
    }

    redacted
}

fn replace_sensitive_with_boundaries(text: &str, sensitive: &str) -> String {
    if sensitive.is_empty() {
        return text.to_string();
    }

    let mut redacted = String::with_capacity(text.len());
    let mut last_end = 0usize;

    for (start, matched) in text.match_indices(sensitive) {
        let end = start + matched.len();
        if !is_sensitive_match_boundary(text, start, end) {
            continue;
        }

        redacted.push_str(&text[last_end..start]);
        redacted.push_str("[REDACTED]");
        last_end = end;
    }

    if last_end == 0 {
        return text.to_string();
    }

    redacted.push_str(&text[last_end..]);
    redacted
}

fn is_sensitive_match_boundary(text: &str, start: usize, end: usize) -> bool {
    let previous = text[..start].chars().next_back();
    let next = text[end..].chars().next();

    previous.is_none_or(is_sensitive_boundary_char) && next.is_none_or(is_sensitive_boundary_char)
}

fn is_sensitive_boundary_char(ch: char) -> bool {
    !(ch.is_ascii_alphanumeric() || ch == '_')
}

fn non_empty_runner_message(message: &str) -> Option<String> {
    if message.is_empty() {
        None
    } else {
        Some(message.to_string())
    }
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

const sealedEnv = new Proxy(Object.create(null), {{
  get() {{
    return undefined;
  }},
  set() {{
    return true;
  }},
  has() {{
    return false;
  }},
  ownKeys() {{
    return [];
  }},
  getOwnPropertyDescriptor() {{
    return undefined;
  }},
}});
process.env = sealedEnv;

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
    use super::{
        FunctionExecutionContext, build_macos_seatbelt_profile, build_runner_invocation,
        cleanup_execution_directory, execute_custom_function, parse_imports_annotation,
        validate_custom_functions,
    };
    use crate::models::{CredentialCustomFunction, CredentialProvider};
    use serde_json::{Map, json};
    use std::fs;
    use std::path::PathBuf;

    #[cfg(not(unix))]
    use super::{FunctionImportSpec, prepare_execution_directory};

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

    #[test]
    fn macos_runner_invocation_wraps_node_with_sandbox_exec_and_permissions() {
        let runtime_dir = PathBuf::from("/tmp/credbridge-custom-functions/cache/abc");
        let execution_dir = PathBuf::from("/tmp/credbridge-custom-functions/run/xyz");
        let runner_path = execution_dir.join("runner.mjs");
        let invocation = build_runner_invocation(
            true,
            PathBuf::from("/opt/homebrew/bin/node"),
            &runtime_dir,
            &execution_dir,
            &runner_path,
        );

        assert_eq!(invocation.program, "/usr/bin/sandbox-exec");
        assert!(invocation.args.iter().any(|arg| arg == "-p"));
        assert!(
            invocation
                .args
                .iter()
                .any(|arg| arg == "/opt/homebrew/bin/node")
        );
        assert!(invocation.args.iter().any(|arg| {
            arg == "--permission"
                || arg.contains("--allow-fs-read=/tmp/credbridge-custom-functions/cache/abc")
                || arg.contains("--allow-fs-read=/tmp/credbridge-custom-functions/run/xyz")
                || arg.contains("--allow-fs-write=/tmp/credbridge-custom-functions/run/xyz")
        }));
    }

    #[test]
    fn macos_seatbelt_profile_scopes_cache_and_execution_directories() {
        let profile = build_macos_seatbelt_profile(
            PathBuf::from("/opt/homebrew/bin/node").as_path(),
            PathBuf::from("/tmp/credbridge-custom-functions/cache/abc").as_path(),
            PathBuf::from("/tmp/credbridge-custom-functions/run/xyz").as_path(),
        );

        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(deny process-fork)"));
        assert!(profile.contains("(subpath \"/tmp/credbridge-custom-functions/cache/abc\")"));
        assert!(profile.contains("(subpath \"/tmp/credbridge-custom-functions/run/xyz\")"));
    }

    #[tokio::test]
    async fn execute_custom_function_redacts_credential_values_from_runtime_errors() {
        let custom_functions = vec![CredentialCustomFunction {
            function_name: "leak".to_string(),
            function_description: None,
            function_body:
                "export default function func({ credential }) { throw new Error(`leak:${credential.api_key}`); }"
                    .to_string(),
        }];
        let context = FunctionExecutionContext {
            credential: std::collections::HashMap::from([(
                "api_key".to_string(),
                "super-secret-value".to_string(),
            )]),
            provider: Some(CredentialProvider::Custom),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            query: String::new(),
            headers: Map::new(),
            body: Some(json!({"probe": true})),
        };

        let error = execute_custom_function(&custom_functions, "leak", &[], &context)
            .await
            .expect_err("runtime failure should bubble up");
        let message = error.to_string();

        assert!(message.contains("custom function leak failed"));
        assert!(!message.contains("super-secret-value"));
        assert!(
            message.contains("[REDACTED]"),
            "unexpected leak error: {message}"
        );
    }

    #[tokio::test]
    async fn execute_custom_function_blocks_process_env_during_module_import() {
        // SAFETY: test-only mutation, scoped to this test execution.
        unsafe {
            std::env::set_var("CB_ENV_IMPORT_PROBE", "host-secret-value");
        }
        let custom_functions = vec![CredentialCustomFunction {
            function_name: "env_probe".to_string(),
            function_description: None,
            function_body: r#"
const leak = process["en" + "v"]?.CB_ENV_IMPORT_PROBE;
export default function func() {
  return leak ?? "blocked";
}
"#
            .to_string(),
        }];
        let context = test_execution_context();

        let result = execute_custom_function(&custom_functions, "env_probe", &[], &context)
            .await
            .expect("process.env should be masked during module import");

        assert_eq!(result, "blocked");
        // SAFETY: test-only cleanup paired with set_var above.
        unsafe {
            std::env::remove_var("CB_ENV_IMPORT_PROBE");
        }
    }

    #[tokio::test]
    async fn execute_custom_function_blocks_obfuscated_network_access() {
        let custom_functions = vec![CredentialCustomFunction {
            function_name: "network_probe".to_string(),
            function_description: None,
            function_body: r#"
export default async function func() {
  return await globalThis["fetch"]("https://example.com").then(() => "unexpected");
}
"#
            .to_string(),
        }];
        let context = test_execution_context();

        let error = execute_custom_function(&custom_functions, "network_probe", &[], &context)
            .await
            .expect_err("network access should be denied");

        assert!(
            error
                .to_string()
                .contains("custom function network_probe failed")
        );
    }

    #[tokio::test]
    async fn execute_custom_function_blocks_subprocess_spawn() {
        let custom_functions = vec![CredentialCustomFunction {
            function_name: "spawn_probe".to_string(),
            function_description: None,
            function_body: r#"
export default function func() {
  const childProcess = process.getBuiltinModule("node:child_" + "process");
  return childProcess.execFileSync("/bin/echo", ["unexpected"], { encoding: "utf8" });
}
"#
            .to_string(),
        }];
        let context = test_execution_context();

        let error = execute_custom_function(&custom_functions, "spawn_probe", &[], &context)
            .await
            .expect_err("subprocess spawn should be denied");

        assert!(
            error
                .to_string()
                .contains("custom function spawn_probe failed")
        );
    }

    #[tokio::test]
    async fn execute_custom_function_blocks_out_of_scope_file_reads() {
        let custom_functions = vec![CredentialCustomFunction {
            function_name: "fs_probe".to_string(),
            function_description: None,
            function_body: r#"
export default function func() {
  const fsModule = process.getBuiltinModule("node:f" + "s");
  return fsModule.readFileSync("/etc/hosts", "utf8");
}
"#
            .to_string(),
        }];
        let context = test_execution_context();

        let error = execute_custom_function(&custom_functions, "fs_probe", &[], &context)
            .await
            .expect_err("out-of-scope file access should be denied");

        assert!(
            error
                .to_string()
                .contains("custom function fs_probe failed")
        );
    }

    fn test_execution_context() -> FunctionExecutionContext {
        FunctionExecutionContext {
            credential: std::collections::HashMap::from([(
                "api_key".to_string(),
                "super-secret-value".to_string(),
            )]),
            provider: Some(CredentialProvider::Custom),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            query: String::new(),
            headers: Map::new(),
            body: Some(json!({"probe": true})),
        }
    }

    #[cfg(not(unix))]
    #[test]
    fn prepare_execution_directory_copies_dependencies_on_non_unix() {
        let root = std::env::temp_dir().join(format!(
            "credbridge-function-runtime-test-{}",
            uuid::Uuid::now_v7()
        ));
        let runtime_dir = root.join("runtime");
        let execution_dir = root.join("execution");
        let source_package_dir = runtime_dir.join("node_modules").join("probe_pkg");
        let source_file = source_package_dir.join("index.js");

        fs::create_dir_all(&source_package_dir).expect("create runtime node_modules");
        fs::create_dir_all(&execution_dir).expect("create execution directory");
        fs::write(&source_file, "export default 'ok';").expect("write mock dependency file");

        let imports = vec![FunctionImportSpec {
            package: "probe_pkg".to_string(),
            version: "1.0.0".to_string(),
        }];

        prepare_execution_directory(&runtime_dir, &execution_dir, &imports)
            .expect("prepare execution directory should succeed");

        let copied_file = execution_dir
            .join("node_modules")
            .join("probe_pkg")
            .join("index.js");
        assert!(copied_file.exists(), "dependency file should be copied");

        let _ = fs::remove_dir_all(root);
    }
}
