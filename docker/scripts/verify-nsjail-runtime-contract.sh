#!/bin/sh

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
RUNTIME_DOCKERFILE="$ROOT_DIR/docker/base/runtime.Dockerfile"
RUNTIME_PREFLIGHT="$ROOT_DIR/docker/scripts/runtime-preflight.sh"

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
    grep -Fq "$pattern" "$file_path" || fail "expected pattern not found in $file_path: $pattern"
}

require_file "$RUNTIME_DOCKERFILE"
require_file "$RUNTIME_PREFLIGHT"

sh -n "$RUNTIME_PREFLIGHT"
sh -n "$0"

require_grep "uidmap \\" "$RUNTIME_DOCKERFILE"
require_grep "root:100000:65536" "$RUNTIME_DOCKERFILE"
require_grep "appuser:100000:65536" "$RUNTIME_DOCKERFILE"
require_grep "test -u /usr/bin/newuidmap" "$RUNTIME_DOCKERFILE"
require_grep "test -u /usr/bin/newgidmap" "$RUNTIME_DOCKERFILE"
require_grep "ENV LIGHTPANDA_BINARY_PATH=/usr/local/bin/lightpanda" "$RUNTIME_DOCKERFILE"
require_grep "ENV LIGHTPANDA_DISABLE_TELEMETRY=true" "$RUNTIME_DOCKERFILE"
require_grep '@lightpanda/browser' "$RUNTIME_DOCKERFILE"
require_grep 'puppeteer-core' "$RUNTIME_DOCKERFILE"
require_grep 'install -m 0755 /root/.cache/lightpanda-node/lightpanda "$LIGHTPANDA_BINARY_PATH"' "$RUNTIME_DOCKERFILE"

require_grep "ensure_nsjail_userns_prerequisites" "$RUNTIME_PREFLIGHT"
require_grep "require_subid_entry /etc/subuid" "$RUNTIME_PREFLIGHT"
require_grep "require_subid_entry /etc/subgid" "$RUNTIME_PREFLIGHT"
require_grep "CREDBRIDGE_SKIP_NSJAIL_PREFLIGHT" "$RUNTIME_PREFLIGHT"
require_grep "NSJAIL_USERNS_OK" "$RUNTIME_PREFLIGHT"
require_grep "LIGHTPANDA_BINARY_PATH" "$RUNTIME_PREFLIGHT"
require_grep 'require.resolve("puppeteer-core")' "$RUNTIME_PREFLIGHT"

echo "[verify-nsjail-runtime-contract] ok"
