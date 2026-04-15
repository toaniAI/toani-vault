#!/bin/sh

set -eu

log() {
    echo "[runtime-preflight] $*" >&2
}

fail() {
    log "ERROR: $*"
    exit 1
}

find_existing_path() {
    for path in "$@"; do
        if [ -n "$path" ] && [ -e "$path" ]; then
            echo "$path"
            return 0
        fi
    done
    return 1
}

is_truthy() {
    value="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
    case "$value" in
        1|true|yes|on)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

require_subid_entry() {
    file_path="$1"
    account_name="$2"

    [ -f "$file_path" ] || fail "$file_path is missing"

    if ! awk -F: -v user="$account_name" '$1 == user { found = 1 } END { exit found ? 0 : 1 }' "$file_path"; then
        fail "$file_path does not contain a subordinate id range for current user '$account_name'"
    fi
}

ensure_nsjail_userns_prerequisites() {
    if is_truthy "${CREDBRIDGE_SKIP_NSJAIL_PREFLIGHT:-false}"; then
        log "skipping nsjail/userns preflight because CREDBRIDGE_SKIP_NSJAIL_PREFLIGHT is enabled"
        return 0
    fi

    current_uid="$(id -u)"
    current_gid="$(id -g)"
    current_user="$(id -un 2>/dev/null || true)"
    [ -n "$current_user" ] || fail "unable to determine current runtime user"

    log "current runtime user=$current_user uid=$current_uid gid=$current_gid"

    nsjail_binary="${NSJAIL_PATH:-$(command -v nsjail 2>/dev/null || true)}"
    [ -n "$nsjail_binary" ] || fail "nsjail executable not found; install nsjail or set NSJAIL_PATH"
    [ -x "$nsjail_binary" ] || fail "nsjail executable is not executable: $nsjail_binary"
    export NSJAIL_PATH="$nsjail_binary"
    log "effective NSJAIL_PATH=$NSJAIL_PATH"

    newuidmap_binary="$(command -v newuidmap 2>/dev/null || true)"
    [ -n "$newuidmap_binary" ] || fail "newuidmap executable not found; install uidmap"
    [ -x "$newuidmap_binary" ] || fail "newuidmap executable is not executable: $newuidmap_binary"
    [ -u "$newuidmap_binary" ] || fail "newuidmap is missing the setuid bit: $newuidmap_binary"

    newgidmap_binary="$(command -v newgidmap 2>/dev/null || true)"
    [ -n "$newgidmap_binary" ] || fail "newgidmap executable not found; install uidmap"
    [ -x "$newgidmap_binary" ] || fail "newgidmap executable is not executable: $newgidmap_binary"
    [ -u "$newgidmap_binary" ] || fail "newgidmap is missing the setuid bit: $newgidmap_binary"

    log "uidmap helpers verified: newuidmap=$newuidmap_binary newgidmap=$newgidmap_binary"

    require_subid_entry /etc/subuid "$current_user"
    require_subid_entry /etc/subgid "$current_user"
    log "subordinate id ranges found for current user '$current_user'"

    userns_clone_file="/proc/sys/kernel/unprivileged_userns_clone"
    if [ -r "$userns_clone_file" ]; then
        userns_clone_value="$(cat "$userns_clone_file")"
        [ "$userns_clone_value" = "1" ] || fail "$userns_clone_file is $userns_clone_value; expected 1"
        log "kernel.unprivileged_userns_clone=$userns_clone_value"
    else
        log "kernel user namespace toggle not readable at $userns_clone_file; continuing"
    fi

    [ -d /sys/fs/cgroup ] || fail "/sys/fs/cgroup is not mounted"
    log "/sys/fs/cgroup is mounted"

    if is_truthy "${CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST:-false}"; then
        log "skipping nsjail smoke test because CREDBRIDGE_SKIP_NSJAIL_SMOKE_TEST is enabled"
        return 0
    fi

    inside_uid="${CREDBRIDGE_NSJAIL_INSIDE_UID:-0}"
    outside_uid="${CREDBRIDGE_NSJAIL_OUTSIDE_UID:-100000}"
    uid_count="${CREDBRIDGE_NSJAIL_UID_COUNT:-1}"
    inside_gid="${CREDBRIDGE_NSJAIL_INSIDE_GID:-0}"
    outside_gid="${CREDBRIDGE_NSJAIL_OUTSIDE_GID:-100000}"
    gid_count="${CREDBRIDGE_NSJAIL_GID_COUNT:-1}"

    log "running nsjail smoke test with uid_mapping=${inside_uid}:${outside_uid}:${uid_count} gid_mapping=${inside_gid}:${outside_gid}:${gid_count}"

    smoke_output_file="$(mktemp)"
    if ! "$nsjail_binary" --mode o \
        --uid_mapping "${inside_uid}:${outside_uid}:${uid_count}" \
        --gid_mapping "${inside_gid}:${outside_gid}:${gid_count}" \
        -- /bin/sh -c 'id >/dev/null && echo NSJAIL_USERNS_OK' >"$smoke_output_file" 2>&1; then
        cat "$smoke_output_file" >&2
        rm -f "$smoke_output_file"
        fail "nsjail smoke test failed; user namespace mapping is not usable for current runtime user '$current_user'"
    fi

    smoke_output="$(cat "$smoke_output_file")"
    rm -f "$smoke_output_file"

    printf '%s\n' "$smoke_output" | grep -q "NSJAIL_USERNS_OK" || fail "nsjail smoke test did not produce success marker"
    log "nsjail smoke test passed"
}

normalize_json_bool() {
    value="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"

    case "$value" in
        1|true|yes|on)
            echo "true"
            ;;
        0|false|no|off)
            echo "false"
            ;;
        *)
            fail "invalid boolean value for SGX/DCAP config: $1"
            ;;
    esac
}

library_exists() {
    env_path="$1"
    shift

    if [ -n "$env_path" ]; then
        configured_path="$(printenv "$env_path" 2>/dev/null || true)"
        if [ -n "$configured_path" ]; then
            [ -e "$configured_path" ] && return 0
            fail "configured library path $env_path=$configured_path does not exist"
        fi
    fi

    for candidate in "$@"; do
        if [ -e "$candidate" ]; then
            return 0
        fi
    done

    if command -v ldconfig >/dev/null 2>&1; then
        for candidate in "$@"; do
            base="$(basename "$candidate")"
            if ldconfig -p 2>/dev/null | grep -q "$base"; then
                return 0
            fi
        done
    fi

    return 1
}

resolve_browser_runtime_path() {
    env_name="$1"
    shift

    configured_path="$(printenv "$env_name" 2>/dev/null || true)"
    if [ -n "$configured_path" ] && [ "$configured_path" != "0" ]; then
        [ -e "$configured_path" ] && {
            echo "$configured_path"
            return 0
        }
        fail "configured browser runtime path $env_name=$configured_path does not exist"
    fi

    find_existing_path "$@" || return 1
}

resolve_browser_runtime_executable() {
    env_name="$1"
    shift

    configured_path="$(printenv "$env_name" 2>/dev/null || true)"
    if [ -n "$configured_path" ] && [ "$configured_path" != "0" ]; then
        [ -x "$configured_path" ] && {
            echo "$configured_path"
            return 0
        }
        fail "configured browser runtime executable $env_name=$configured_path does not exist or is not executable"
    fi

    for candidate in "$@"; do
        if [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

ensure_browser_runtime_prerequisites() {
    sandbox_node_binary="${CREDBRIDGE_SANDBOX_NODE_BINARY:-$(command -v node 2>/dev/null || true)}"
    [ -n "$sandbox_node_binary" ] || fail "node executable not found; install nodejs or set CREDBRIDGE_SANDBOX_NODE_BINARY"
    [ -x "$sandbox_node_binary" ] || fail "sandbox node binary is not executable: $sandbox_node_binary"
    export CREDBRIDGE_SANDBOX_NODE_BINARY="$sandbox_node_binary"
    log "effective sandbox node binary=$CREDBRIDGE_SANDBOX_NODE_BINARY"

    if [ -z "${NODE_PATH:-}" ]; then
        NODE_PATH="$(resolve_browser_runtime_path NODE_PATH \
            /opt/credbridge-browser-runtime/node_modules || true)"
        [ -n "${NODE_PATH:-}" ] || fail "NODE_PATH is not set and default browser runtime modules path is missing"
        export NODE_PATH
    fi
    log "effective NODE_PATH=$NODE_PATH"

    LIGHTPANDA_BINARY_PATH="$(resolve_browser_runtime_executable LIGHTPANDA_BINARY_PATH \
        /usr/local/bin/lightpanda || true)"
    [ -n "${LIGHTPANDA_BINARY_PATH:-}" ] || fail "LIGHTPANDA_BINARY_PATH is not set and no executable Lightpanda binary was found"
    export LIGHTPANDA_BINARY_PATH
    export LIGHTPANDA_DISABLE_TELEMETRY="${LIGHTPANDA_DISABLE_TELEMETRY:-true}"
    log "effective LIGHTPANDA_BINARY_PATH=$LIGHTPANDA_BINARY_PATH"
    log "effective LIGHTPANDA_DISABLE_TELEMETRY=$LIGHTPANDA_DISABLE_TELEMETRY"

    lightpanda_version="$("$LIGHTPANDA_BINARY_PATH" --version 2>&1 || true)"
    [ -n "$lightpanda_version" ] || fail "lightpanda --version produced no output"
    log "lightpanda version=$lightpanda_version"

    puppeteer_entry="$("$CREDBRIDGE_SANDBOX_NODE_BINARY" -e 'process.stdout.write(require.resolve("puppeteer-core"))' 2>/dev/null || true)"
    [ -n "$puppeteer_entry" ] || fail "puppeteer-core package is not resolvable with current NODE_PATH=$NODE_PATH"
    [ -f "$puppeteer_entry" ] || fail "puppeteer-core resolved to a missing file: $puppeteer_entry"
    log "puppeteer-core entrypoint=$puppeteer_entry"

    "$CREDBRIDGE_SANDBOX_NODE_BINARY" <<'NODE'
const http = require('http');
const net = require('net');
const { spawn } = require('child_process');
const puppeteer = require('puppeteer-core');

function listen(server, host = '127.0.0.1', port = 0) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, host, () => resolve(server.address().port));
  });
}

function waitForPort(port, host = '127.0.0.1', timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = net.createConnection({ host, port }, () => {
        socket.destroy();
        resolve();
      });
      socket.once('error', () => {
        socket.destroy();
        if (Date.now() >= deadline) {
          reject(new Error(`Lightpanda CDP server did not open port ${port}`));
          return;
        }
        setTimeout(attempt, 100);
      });
    };
    attempt();
  });
}

(async () => {
  const app = http.createServer((req, res) => {
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    res.end('<!doctype html><title>credbridge runtime smoke</title><h1 id="ready">LIGHTPANDA_OK</h1>');
  });
  const appPort = await listen(app);
  const cdpProbe = net.createServer();
  const cdpPort = await listen(cdpProbe);
  await new Promise(resolve => cdpProbe.close(resolve));

  const lightpanda = spawn(process.env.LIGHTPANDA_BINARY_PATH, [
    'serve',
    '--host',
    '127.0.0.1',
    '--port',
    String(cdpPort),
  ], {
    env: {
      ...process.env,
      LIGHTPANDA_DISABLE_TELEMETRY: process.env.LIGHTPANDA_DISABLE_TELEMETRY || 'true',
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  let stderr = '';
  lightpanda.stderr.on('data', chunk => { stderr += chunk.toString(); });

  let browser;
  try {
    await waitForPort(cdpPort);
    browser = await puppeteer.connect({ browserWSEndpoint: `ws://127.0.0.1:${cdpPort}` });
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${appPort}/`, { waitUntil: 'domcontentloaded', timeout: 5000 });
    const marker = await page.$eval('#ready', element => element.textContent);
    if (marker !== 'LIGHTPANDA_OK') {
      throw new Error(`unexpected smoke marker: ${marker}`);
    }
  } finally {
    if (browser) {
      await browser.disconnect();
    }
    lightpanda.kill('SIGTERM');
    app.close();
  }

  console.error('[runtime-preflight] lightpanda puppeteer-core smoke test passed');
})().catch(error => {
  console.error(error && error.stack ? error.stack : String(error));
  process.exit(1);
});
NODE
}

write_qcnl_config() {
    config_path="${SGX_QCNL_CONFIG_PATH:-/etc/sgx_default_qcnl.conf}"
    pccs_url="${DCAP_PCCS_URL:-}"
    use_secure_cert="${DCAP_USE_SECURE_CERT:-true}"
    collateral_service="${DCAP_COLLATERAL_SERVICE:-}"
    pccs_api_version="${DCAP_PCCS_API_VERSION:-}"

    if [ -z "$pccs_url" ]; then
        if [ -f "$config_path" ]; then
            log "DCAP_PCCS_URL not set, keeping existing QCNL config at $config_path"
            return 0
        fi
        fail "DCAP_PCCS_URL is required when no QCNL config exists at $config_path"
    fi

    use_secure_cert="$(normalize_json_bool "$use_secure_cert")"

    cat > "$config_path" <<EOF
{
  "pccs_url": "$pccs_url",
  "use_secure_cert": $use_secure_cert$(if [ -n "$collateral_service" ]; then printf ',\n  "collateral_service": "%s"' "$collateral_service"; fi)$(if [ -n "$pccs_api_version" ]; then printf ',\n  "pccs_api_version": "%s"' "$pccs_api_version"; fi)
}
EOF

    log "wrote SGX QCNL config to $config_path"
}

ensure_browser_runtime_prerequisites
ensure_nsjail_userns_prerequisites

TEE_MODE_VALUE="${TEE_MODE:-simulation}"

if [ "$TEE_MODE_VALUE" != "hardware" ]; then
    exec "$@"
fi

log "TEE_MODE=hardware detected, running SGX/DCAP preflight checks"

if [ -n "${TEE_ENCLAVE_PATH:-}" ]; then
    [ -f "$TEE_ENCLAVE_PATH" ] || fail "TEE_ENCLAVE_PATH does not point to a file: $TEE_ENCLAVE_PATH"
elif [ -f /app/credbridge_enclave.signed.so ]; then
    export TEE_ENCLAVE_PATH=/app/credbridge_enclave.signed.so
else
    fail "TEE_ENCLAVE_PATH is not set and /app/credbridge_enclave.signed.so is missing"
fi

log "effective TEE_ENCLAVE_PATH=$TEE_ENCLAVE_PATH"

SGX_DEVICE_PATH="$(find_existing_path \
    /dev/sgx_enclave \
    /dev/sgx/enclave \
    /dev/sgx \
    /dev/isgx || true)"
[ -n "$SGX_DEVICE_PATH" ] || fail "no SGX device found; mount /dev/sgx_enclave or equivalent into the container"
log "found SGX device at $SGX_DEVICE_PATH"

SGX_PROVISION_PATH="$(find_existing_path \
    /dev/sgx_provision \
    /dev/sgx/provision || true)"
AESM_SOCKET_PATH="${SGX_AESM_SOCKET_PATH:-/var/run/aesmd/aesm.socket}"
AESM_SOCKET_PRESENT=""
[ -S "$AESM_SOCKET_PATH" ] && AESM_SOCKET_PRESENT="$AESM_SOCKET_PATH"

if [ -z "$SGX_PROVISION_PATH" ] && [ -z "$AESM_SOCKET_PRESENT" ]; then
    fail "neither SGX provisioning device nor AESM socket is available; mount /dev/sgx_provision or $AESM_SOCKET_PATH"
fi

[ -n "$SGX_PROVISION_PATH" ] && log "found SGX provisioning device at $SGX_PROVISION_PATH"
[ -n "$AESM_SOCKET_PRESENT" ] && log "found AESM socket at $AESM_SOCKET_PRESENT"

library_exists \
    TEE_SGX_DCAP_QL_LIB_PATH \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_ql.so.1 \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_ql.so \
    /usr/lib/libsgx_dcap_ql.so.1 \
    /usr/lib/libsgx_dcap_ql.so \
    /lib/x86_64-linux-gnu/libsgx_dcap_ql.so.1 \
    /lib/x86_64-linux-gnu/libsgx_dcap_ql.so \
    || fail "Intel DCAP quote library is missing; install libsgx-dcap-ql"
log "Intel DCAP quote library is available"

library_exists \
    TEE_SGX_DCAP_QV_LIB_PATH \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so.1 \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so \
    /usr/lib/libsgx_dcap_quoteverify.so.1 \
    /usr/lib/libsgx_dcap_quoteverify.so \
    /lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so.1 \
    /lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so \
    || fail "Intel DCAP quote verification library is missing; install libsgx-dcap-quote-verify"
log "Intel DCAP quote verification library is available"

library_exists \
    "" \
    /usr/lib/x86_64-linux-gnu/libdcap_quoteprov.so.1 \
    /usr/lib/x86_64-linux-gnu/libdcap_quoteprov.so \
    /usr/lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so.1 \
    /usr/lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so \
    /usr/lib/libdcap_quoteprov.so.1 \
    /usr/lib/libdcap_quoteprov.so \
    /usr/lib/libsgx_default_qcnl_wrapper.so.1 \
    /usr/lib/libsgx_default_qcnl_wrapper.so \
    /lib/x86_64-linux-gnu/libdcap_quoteprov.so.1 \
    /lib/x86_64-linux-gnu/libdcap_quoteprov.so \
    /lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so.1 \
    /lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so \
    || fail "Intel DCAP default QPL library is missing; install libsgx-dcap-default-qpl"
log "Intel DCAP default QPL library is available"

write_qcnl_config

exec "$@"
