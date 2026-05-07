#!/bin/sh

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
DRONE_FILE="$ROOT_DIR/.drone.yml"
APP_DOCKERFILE="$ROOT_DIR/Dockerfile"
RUNTIME_DOCKERFILE="$ROOT_DIR/docker/base/runtime.Dockerfile"
RUNTIME_PREFLIGHT="$ROOT_DIR/docker/scripts/runtime-preflight.sh"
BROWSER_RUNTIME_RS="$ROOT_DIR/src/tee/sandbox/browser_runtime.rs"
NSJAIL_RS="$ROOT_DIR/src/tee/sandbox/nsjail.rs"
SANDBOX_CONFIG_RS="$ROOT_DIR/src/tee/sandbox/config.rs"
SANDBOX_POOL_RS="$ROOT_DIR/src/tee/sandbox/pool.rs"
SANDBOX_EXECUTOR="$ROOT_DIR/src/tee/sandbox/scripts/sandbox_executor.cjs"

fail() {
    echo "[verify-nsjail-runtime-contract] ERROR: $*" >&2
    exit 1
}

require_file() {
    [ -f "$1" ] || fail "required file missing: $1"
}

require_grep() {
    pattern="$1"
    file_path="$2"
    grep -Fq -- "$pattern" "$file_path" || fail "expected pattern not found in $file_path: $pattern"
}

require_file "$RUNTIME_DOCKERFILE"
require_file "$RUNTIME_PREFLIGHT"
require_file "$DRONE_FILE"
require_file "$APP_DOCKERFILE"
require_file "$BROWSER_RUNTIME_RS"
require_file "$NSJAIL_RS"
require_file "$SANDBOX_CONFIG_RS"
require_file "$SANDBOX_POOL_RS"
require_file "$SANDBOX_EXECUTOR"

sh -n "$RUNTIME_PREFLIGHT"
sh -n "$0"

require_grep "ghcr.io/example-org/credbridge-runtime-sandbox:latest" "$DRONE_FILE"
require_grep "docker/base/runtime.Dockerfile" "$DRONE_FILE"
require_grep "docker/scripts/verify-nsjail-runtime-contract.sh" "$DRONE_FILE"
require_grep "RUNTIME_BASE_IMAGE=ghcr.io/example-org/credbridge-runtime-sandbox:latest" "$APP_DOCKERFILE"
require_grep "COPY --from=builder /app/src/tee/sandbox/scripts /app/src/tee/sandbox/scripts" "$APP_DOCKERFILE"
require_grep "COPY docker/scripts/runtime-preflight.sh /app/runtime-preflight.sh" "$APP_DOCKERFILE"
require_grep "uidmap \\" "$RUNTIME_DOCKERFILE"
require_grep "root:100000:65536" "$RUNTIME_DOCKERFILE"
require_grep "appuser:100000:65536" "$RUNTIME_DOCKERFILE"
require_grep "test -u /usr/bin/newuidmap" "$RUNTIME_DOCKERFILE"
require_grep "test -u /usr/bin/newgidmap" "$RUNTIME_DOCKERFILE"
require_grep "apt-get install -y --no-install-recommends nodejs" "$RUNTIME_DOCKERFILE"
require_grep "ENV NODE_PATH=/opt/credbridge-browser-runtime/node_modules" "$RUNTIME_DOCKERFILE"
require_grep "ENV LIGHTPANDA_BINARY_PATH=/usr/local/bin/lightpanda" "$RUNTIME_DOCKERFILE"
require_grep "ENV LIGHTPANDA_DISABLE_TELEMETRY=true" "$RUNTIME_DOCKERFILE"
require_grep '@lightpanda/browser' "$RUNTIME_DOCKERFILE"
require_grep 'puppeteer-core' "$RUNTIME_DOCKERFILE"
require_grep 'install -m 0755 /root/.cache/lightpanda-node/lightpanda "$LIGHTPANDA_BINARY_PATH"' "$RUNTIME_DOCKERFILE"
require_grep '"$LIGHTPANDA_BINARY_PATH" version' "$RUNTIME_DOCKERFILE"

require_grep "ensure_nsjail_userns_prerequisites" "$RUNTIME_PREFLIGHT"
require_grep 'pod_ip_base_url="${owner_scheme}://${POD_IP}:${owner_port}"' "$RUNTIME_PREFLIGHT"
require_grep "source=explicit_env" "$RUNTIME_PREFLIGHT"
require_grep "source=pod_ip" "$RUNTIME_PREFLIGHT"
require_grep "source=headless_dns_fallback" "$RUNTIME_PREFLIGHT"
require_grep "overrides POD_IP-derived owner URL" "$RUNTIME_PREFLIGHT"
require_grep "require_subid_entry /etc/subuid" "$RUNTIME_PREFLIGHT"
require_grep "require_subid_entry /etc/subgid" "$RUNTIME_PREFLIGHT"
require_grep "CREDBRIDGE_SKIP_NSJAIL_PREFLIGHT" "$RUNTIME_PREFLIGHT"
require_grep "NSJAIL_USERNS_OK" "$RUNTIME_PREFLIGHT"
require_grep "--bindmount_ro /dev/null:/dev/null" "$RUNTIME_PREFLIGHT"
require_grep "--bindmount_ro /bin:/bin" "$RUNTIME_PREFLIGHT"
require_grep "--bindmount_ro /lib:/lib" "$RUNTIME_PREFLIGHT"
require_grep "--bindmount_ro /lib64:/lib64" "$RUNTIME_PREFLIGHT"
require_grep "--bindmount_ro /usr:/usr" "$RUNTIME_PREFLIGHT"
require_grep "ensure_browser_runtime_prerequisites" "$RUNTIME_PREFLIGHT"
require_grep "CREDBRIDGE_SANDBOX_NODE_BINARY" "$RUNTIME_PREFLIGHT"
require_grep "NODE_PATH" "$RUNTIME_PREFLIGHT"
require_grep "LIGHTPANDA_BINARY_PATH" "$RUNTIME_PREFLIGHT"
require_grep 'require.resolve("puppeteer-core")' "$RUNTIME_PREFLIGHT"
require_grep "LIGHTPANDA_OK" "$RUNTIME_PREFLIGHT"
require_grep "puppeteer.connect" "$RUNTIME_PREFLIGHT"
require_grep "'serve'," "$RUNTIME_PREFLIGHT"

require_grep 'const NODE_PATH_CANDIDATES: &[&str] = &["/opt/credbridge-browser-runtime/node_modules"];' "$BROWSER_RUNTIME_RS"
require_grep 'const LIGHTPANDA_BINARY_PATH_CANDIDATES: &[&str] = &["/usr/local/bin/lightpanda"];' "$BROWSER_RUNTIME_RS"
require_grep 'env.insert(NODE_PATH_ENV.to_string(), node_path);' "$BROWSER_RUNTIME_RS"
require_grep 'LIGHTPANDA_DISABLE_TELEMETRY_ENV.to_string()' "$BROWSER_RUNTIME_RS"
require_grep "browser_runtime_mounts(" "$BROWSER_RUNTIME_RS"
require_grep "push_read_only_bind_mount(&mut mounts, node_path);" "$BROWSER_RUNTIME_RS"
require_grep "push_read_only_bind_mount(&mut mounts, lightpanda_binary_path);" "$BROWSER_RUNTIME_RS"
require_grep "spawn_scoped_process(" "$BROWSER_RUNTIME_RS"

require_grep "disable_seccomp_for_browser_runtime: bool" "$SANDBOX_CONFIG_RS"
require_grep "BROWSER_RUNTIME_RELAXED_SYSCALLS" "$SANDBOX_CONFIG_RS"
require_grep "generate_browser_runtime_seccomp_bpf" "$SANDBOX_CONFIG_RS"
require_grep "execve" "$SANDBOX_CONFIG_RS"
require_grep "clone" "$SANDBOX_CONFIG_RS"

require_grep "disable_seccomp_for_browser_runtime: true" "$SANDBOX_POOL_RS"
require_grep "disable_seccomp_for_browser_runtime" "$NSJAIL_RS"
require_grep "for mount in extra_mounts" "$NSJAIL_RS"
require_grep "for (key, value) in env" "$NSJAIL_RS"

require_grep "const puppeteer = require('puppeteer-core');" "$SANDBOX_EXECUTOR"
require_grep "LIGHTPANDA_BINARY_PATH is required" "$SANDBOX_EXECUTOR"
require_grep "'serve'" "$SANDBOX_EXECUTOR"
require_grep "'--host', '127.0.0.1'" "$SANDBOX_EXECUTOR"
require_grep "'--port', String(port)" "$SANDBOX_EXECUTOR"
require_grep "puppeteer.connect" "$SANDBOX_EXECUTOR"
require_grep "browser.createBrowserContext()" "$SANDBOX_EXECUTOR"
require_grep "currentPage.evaluate(" "$SANDBOX_EXECUTOR"
require_grep "await stopLightpanda();" "$SANDBOX_EXECUTOR"
require_grep "child.kill('SIGKILL')" "$SANDBOX_EXECUTOR"

if grep -Fq "browser.pages()" "$SANDBOX_EXECUTOR"; then
    fail "sandbox executor must not reuse Lightpanda's startup page via browser.pages()"
fi

echo "[verify-nsjail-runtime-contract] ok"
