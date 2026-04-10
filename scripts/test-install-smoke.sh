#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d /tmp/deku-smoke.XXXXXX)"
DIST_RELEASE_DIR="${DEKU_INSTALL_SMOKE_RELEASE_DIR:-}"
trap 'cleanup' EXIT

cleanup() {
  if [[ -n "${DEKU_SYSTEMCTL_STATE_DIR:-}" && -f "${DEKU_SYSTEMCTL_STATE_DIR}/deku.pid" ]]; then
    kill "$(cat "${DEKU_SYSTEMCTL_STATE_DIR}/deku.pid")" >/dev/null 2>&1 || true
  fi
  rm -rf "$TEST_ROOT"
}

dump_output_on_error() {
  local status="$?"
  if [[ "$status" -eq 0 ]]; then
    return
  fi

  for output_file in "${first_output_file:-}" "${second_output_file:-}"; do
    if [[ -n "$output_file" && -f "$output_file" ]]; then
      printf '\n---- %s ----\n' "$(basename "$output_file")" >&2
      cat "$output_file" >&2
    fi
  done

  return "$status"
}

trap 'dump_output_on_error' ERR

checksum_file() {
  local file="$1"

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
    return
  fi

  shasum -a 256 "$file" | awk '{print $1}'
}

write_release_fixture() {
  local fixture_dir="$1"
  local version="$2"

  mkdir -p "$fixture_dir/dashboard"
  printf '<html><body>%s</body></html>\n' "$version" > "${fixture_dir}/dashboard/index.html"
  tar -czf "${fixture_dir}/deku-dashboard.tar.gz" -C "${fixture_dir}/dashboard" .

  cat > "${fixture_dir}/deku-linux-amd64" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --version|-v|version)
    printf '%s\n' "__DEKU_FIXTURE_VERSION__"
    ;;
  setup)
    shift

    data_dir="${DEKU_CONFIG_DIR}"
    api_port="2810"
    ssh_port="2222"
    angie_conf_dir="/etc/angie/conf.d/deku"
    global_domain=""
    installer_mode="0"
    token_output=""

    while [[ $# -gt 0 ]]; do
      case "$1" in
        --data-dir)
          data_dir="$2"
          shift 2
          ;;
        --api-port)
          api_port="$2"
          shift 2
          ;;
        --ssh-port)
          ssh_port="$2"
          shift 2
          ;;
        --angie-conf-dir)
          angie_conf_dir="$2"
          shift 2
          ;;
        --global-domain)
          global_domain="$2"
          shift 2
          ;;
        --installer)
          installer_mode="1"
          shift
          ;;
        --token-output)
          token_output="$2"
          shift 2
          ;;
        --defaults|--no-systemd)
          shift
          ;;
        *)
          echo "unexpected arg: $1" >&2
          exit 1
          ;;
      esac
    done

    mkdir -p "${DEKU_CONFIG_DIR}" "${data_dir}"
    {
      printf 'data_dir = "%s"\n' "$data_dir"
      printf 'api_port = %s\n' "$api_port"
      printf 'ssh_port = %s\n' "$ssh_port"
      printf 'angie_conf_dir = "%s"\n' "$angie_conf_dir"
      if [[ -n "$global_domain" ]]; then
        printf 'global_domain = "%s"\n' "$global_domain"
      fi
      printf '[dashboard_auth]\n'
      printf 'token_hash = "$argon2id$fixture"\n'
      printf 'created_at = 2026-04-03T00:00:00Z\n'
    } > "${DEKU_CONFIG_DIR}/config.toml"
    count_file="${DEKU_SETUP_COUNT_FILE:?}"
    count=0
    if [[ -f "$count_file" ]]; then
      count="$(cat "$count_file")"
    fi
    printf '%s' "$((count + 1))" > "$count_file"
    if [[ -n "$token_output" ]]; then
      printf 'dku_SETUPTOKEN123456789\n' > "$token_output"
    fi
    dashboard_host="${DEKU_DASHBOARD_HOST:-203.0.113.10}"
    if [[ "$installer_mode" == "1" ]]; then
      exit 0
    fi
    cat <<OUT
Dashboard URL: http://${dashboard_host}:${api_port}
Local URL:     http://127.0.0.1:${api_port}

Save this dashboard token now. It will only be shown once.
The token is stored hashed at rest and cannot be recovered later.

Dashboard token: dku_SETUPTOKEN123456789
Reset command:   deku dashboard reset-token
Status:          saved to config; restart \`dekud\` if it is not already running

Remote access:  make TCP port ${api_port} reachable from your browser, or use an SSH tunnel
SSH tunnel:     ssh -L ${api_port}:127.0.0.1:${api_port} root@${dashboard_host}
UFW allow:      sudo ufw allow ${api_port}/tcp
UFW restrict:   sudo ufw allow from YOUR_PUBLIC_IP to any port ${api_port} proto tcp
OUT
    ;;
  dashboard)
    api_port="$(sed -n 's/^api_port = \([0-9][0-9]*\)$/\1/p' "${DEKU_CONFIG_DIR}/config.toml" | head -n 1)"
    dashboard_host="${DEKU_DASHBOARD_HOST:-203.0.113.10}"
    printf 'Dashboard URL: http://%s:%s\n' "$dashboard_host" "$api_port"
    printf 'Local URL:     http://127.0.0.1:%s\n' "$api_port"
    printf 'Config path:   %s/config.toml\n' "${DEKU_CONFIG_DIR}"
    printf 'Token status:  configured (stored hashed at rest)\n'
    printf 'Reset token:   deku dashboard reset-token\n'
    printf '\nRemote access:  make TCP port %s reachable from your browser, or use an SSH tunnel\n' "$api_port"
    printf 'SSH tunnel:     ssh -L %s:127.0.0.1:%s root@%s\n' "$api_port" "$api_port" "$dashboard_host"
    printf 'UFW allow:      sudo ufw allow %s/tcp\n' "$api_port"
    printf 'UFW restrict:   sudo ufw allow from YOUR_PUBLIC_IP to any port %s proto tcp\n' "$api_port"
    printf '\nDashboard tokens are only shown when first created or reset.\n'
    ;;
  *)
    echo "unsupported command" >&2
    exit 1
    ;;
esac
EOF
  sed -i.bak "s/__DEKU_FIXTURE_VERSION__/${version}/g" "${fixture_dir}/deku-linux-amd64"
  rm -f "${fixture_dir}/deku-linux-amd64.bak"
  chmod 0755 "${fixture_dir}/deku-linux-amd64"

  cat > "${fixture_dir}/dekud-linux-amd64" <<EOF
#!/usr/bin/env bash
echo "${version}"
EOF
  chmod 0755 "${fixture_dir}/dekud-linux-amd64"
  cp "${fixture_dir}/deku-linux-amd64" "${fixture_dir}/deku-linux-arm64"
  cp "${fixture_dir}/dekud-linux-amd64" "${fixture_dir}/dekud-linux-arm64"

  cat > "${fixture_dir}/deku.service" <<'EOF'
[Unit]
Description=Deku PaaS Daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart=__DEKU_INSTALL_DIR__/dekud
Restart=always
RestartSec=5
User=root
Environment=HOME=/root
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

  cat > "${fixture_dir}/angie-deku.conf" <<'EOF'
server {
    listen 80 default_server;
    return 404;
}
EOF

  : > "${fixture_dir}/SHA256SUMS"
  for artifact in \
    deku-dashboard.tar.gz \
    deku-linux-amd64 \
    deku-linux-arm64 \
    dekud-linux-amd64 \
    dekud-linux-arm64 \
    deku.service \
    angie-deku.conf
  do
    printf '%s  %s\n' "$(checksum_file "${fixture_dir}/${artifact}")" "$artifact" >> "${fixture_dir}/SHA256SUMS"
  done
}

if [[ -z "$DIST_RELEASE_DIR" ]]; then
  mkdir -p "${TEST_ROOT}/fixtures/v1" "${TEST_ROOT}/fixtures/v2"
  write_release_fixture "${TEST_ROOT}/fixtures/v1" "v1.0.0"
  write_release_fixture "${TEST_ROOT}/fixtures/v2" "v2.0.0"
fi

mkdir -p "${TEST_ROOT}/mockbin" "${TEST_ROOT}/state"

cat > "${TEST_ROOT}/mockbin/systemctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state_dir="${DEKU_SYSTEMCTL_STATE_DIR:?}"
log_file="${DEKU_SYSTEMCTL_LOG_FILE:-}"
command="${1:-}"
shift || true
if [[ "${1:-}" == "--quiet" ]]; then
  shift
fi
service="${1:-}"

mkdir -p "$state_dir"
if [[ -n "$log_file" ]]; then
  printf '%s %s\n' "$command" "$service" >> "$log_file"
fi

is_active() {
  [[ -f "${state_dir}/${1}.active" ]]
}

start_deku() {
  mkdir -p "$state_dir"
  if [[ -f "${state_dir}/deku.pid" ]]; then
    kill "$(cat "${state_dir}/deku.pid")" >/dev/null 2>&1 || true
    rm -f "${state_dir}/deku.pid"
  fi
  data_dir="$(sed -n 's/^data_dir = "\(.*\)"$/\1/p' "$DEKU_CONFIG_DIR/config.toml" | head -n 1)"
  mkdir -p "$data_dir"
  : > "${data_dir}/deku.sock"
  (while true; do sleep 60; done) &
  echo $! > "${state_dir}/deku.pid"
  touch "${state_dir}/deku.active"
}

case "$command" in
  daemon-reload)
    exit 0
    ;;
  enable)
    touch "${state_dir}/${service}.enabled"
    exit 0
    ;;
  is-active)
    if is_active "$service"; then
      exit 0
    fi
    exit 3
    ;;
  start|restart)
    if [[ "$service" == "deku" ]]; then
      start_deku
    else
      touch "${state_dir}/${service}.active"
    fi
    exit 0
    ;;
  reload)
    touch "${state_dir}/${service}.reloaded"
    exit 0
    ;;
  *)
    echo "unsupported systemctl command: $*" >&2
    exit 1
    ;;
esac
EOF
chmod 0755 "${TEST_ROOT}/mockbin/systemctl"

cat > "${TEST_ROOT}/mockbin/angie" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-t" ]]; then
  exit 0
fi
echo "unsupported angie command: $*" >&2
exit 1
EOF
chmod 0755 "${TEST_ROOT}/mockbin/angie"

cat > "${TEST_ROOT}/mockbin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
target="${@: -1}"
if [[ "$target" == http://127.0.0.1:*"/healthz" ]]; then
  printf '{"status":"ok","service":"dekud"}'
  exit 0
fi
echo "unexpected curl target: $target" >&2
exit 1
EOF
chmod 0755 "${TEST_ROOT}/mockbin/curl"

cat > "${TEST_ROOT}/mockbin/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod 0755 "${TEST_ROOT}/mockbin/sleep"

export PATH="${TEST_ROOT}/mockbin:${PATH}"
export DEKU_CONFIG_DIR="${TEST_ROOT}/config"
export INSTALL_DIR="${TEST_ROOT}/install/bin"
export ANGIE_CONF_DIR="${TEST_ROOT}/angie/conf.d/deku"
export ANGIE_BASE_CONF="${TEST_ROOT}/angie/conf.d/deku-default.conf"
export SYSTEMD_UNIT_PATH="${TEST_ROOT}/systemd/deku.service"
export DEKU_SETUP_COUNT_FILE="${TEST_ROOT}/state/setup-count"
export DEKU_SYSTEMCTL_STATE_DIR="${TEST_ROOT}/state/systemctl"
export DEKU_SYSTEMCTL_LOG_FILE="${TEST_ROOT}/state/systemctl.log"
export DEKU_REPO="local/deku"
export DEKU_DASHBOARD_HOST="203.0.113.10"
export DEKU_INSTALL_FORCE_DEFAULTS="1"

mkdir -p "$(dirname "$SYSTEMD_UNIT_PATH")"

source "${ROOT_DIR}/scripts/install.sh"

[[ "$(angie_repo_line debian 13 trixie)" == \
  "deb [signed-by=/usr/share/keyrings/angie-signing.gpg] https://download.angie.software/angie/debian/13 trixie main" ]]
[[ "$(angie_repo_line ubuntu 24.04 noble)" == \
  "deb [signed-by=/usr/share/keyrings/angie-signing.gpg] https://download.angie.software/angie/ubuntu/24.04 noble main" ]]

require_root() { :; }
require_linux() { :; }
install_prerequisites() { :; }
install_angie() {
  mkdir -p "$ANGIE_CONF_DIR" "$(dirname "$ANGIE_BASE_CONF")"
  install_support_artifact "angie-deku.conf" "$ANGIE_BASE_CONF" 0644
}
download_artifact() {
  local artifact="$1"
  local dest="$2"
  cp "${CURRENT_RELEASE_DIR}/${artifact}" "$dest"
}
download_checksums() {
  cp "${CURRENT_RELEASE_DIR}/SHA256SUMS" "$CHECKSUMS_FILE"
}
verify_artifact() {
  local artifact="$1"
  local dest="$2"
  local expected actual
  expected="$(awk -v artifact="$artifact" '$2 == artifact { print $1 }' "${CHECKSUMS_FILE}")"
  actual="$(checksum_file "$dest")"
  [[ -n "$expected" ]]
  [[ "$actual" == "$expected" ]]
}
resolve_dist_release_version() {
  local arch artifact version

  arch="$(detect_arch)"
  artifact="${CURRENT_RELEASE_DIR}/deku-linux-${arch}"
  [[ -x "$artifact" ]] || fail "missing executable release artifact for ${arch}: ${artifact}"

  version="$("$artifact" version | head -n 1)"
  [[ -n "$version" ]] || fail "unable to read version from ${artifact}"

  normalize_version_string "$version"
}
resolve_requested_version() {
  case "${CURRENT_RELEASE_DIR}" in
    *"/fixtures/v1") echo "v1.0.0" ;;
    *"/fixtures/v2") echo "v2.0.0" ;;
    *)
      if [[ -n "${DEKU_INSTALL_SMOKE_RESOLVED_VERSION:-}" ]]; then
        echo "${DEKU_INSTALL_SMOKE_RESOLVED_VERSION}"
      else
        resolve_dist_release_version
      fi
      ;;
  esac
}
socket_exists() {
  local socket_path="$1"
  [[ -e "$socket_path" ]]
}

CURRENT_RELEASE_DIR="${TEST_ROOT}/fixtures/v1"
if [[ -n "$DIST_RELEASE_DIR" ]]; then
  CURRENT_RELEASE_DIR="$DIST_RELEASE_DIR"
fi
first_output_file="${TEST_ROOT}/first-install.out"
main >"${first_output_file}" 2>&1
first_output="$(cat "${first_output_file}")"
printf '%s\n' "$first_output"

[[ -x "${INSTALL_DIR}/deku" ]]
[[ -x "${INSTALL_DIR}/dekud" ]]
if [[ -z "$DIST_RELEASE_DIR" ]]; then
  grep -q 'v1.0.0' "${INSTALL_DIR}/dekud"
  grep -q 'v1.0.0' "${TEST_ROOT}/config/dashboard/index.html"
else
  [[ -f "${TEST_ROOT}/config/dashboard/index.html" ]]
fi
grep -q 'ssh_port = 2222' "${DEKU_CONFIG_DIR}/config.toml"
grep -q "${INSTALL_DIR}/dekud" "${SYSTEMD_UNIT_PATH}"
if [[ -f "${DEKU_SETUP_COUNT_FILE}" ]]; then
  [[ "$(cat "${DEKU_SETUP_COUNT_FILE}")" == "1" ]]
fi
[[ "$first_output" == *"Dashboard token:"* ]]
[[ "$first_output" == *"Dashboard URL: http://203.0.113.10:2810"* ]]
[[ "$first_output" == *"Local URL:     http://127.0.0.1:2810"* ]]
[[ "$first_output" == *"SSH tunnel:    ssh -L 2810:127.0.0.1:2810 root@203.0.113.10"* ]]
[[ "$first_output" == *"deku deploy run my-app --path /absolute/path/to/app"* ]]
[[ "$first_output" == *"Deku successfully installed."* ]]
[[ "$first_output" != *"Start \`dekud\`"* ]]
[[ "$first_output" != *"Config file:"* ]]
[[ "$first_output" != *"Dashboard assets:"* ]]
[[ "$first_output" != *"Reset command:"* ]]
[[ "$first_output" != *"Token reset:"* ]]
[[ "$first_output" != *"Setup complete."* ]]
cp "${DEKU_CONFIG_DIR}/config.toml" "${TEST_ROOT}/config.first"

CURRENT_RELEASE_DIR="${TEST_ROOT}/fixtures/v2"
if [[ -n "$DIST_RELEASE_DIR" ]]; then
  CURRENT_RELEASE_DIR="$DIST_RELEASE_DIR"
fi
second_output_file="${TEST_ROOT}/second-install.out"
main >"${second_output_file}" 2>&1
second_output="$(cat "${second_output_file}")"
printf '%s\n' "$second_output"

if [[ -z "$DIST_RELEASE_DIR" ]]; then
  grep -q 'v2.0.0' "${INSTALL_DIR}/dekud"
  grep -q 'v2.0.0' "${TEST_ROOT}/config/dashboard/index.html"
else
  [[ -f "${TEST_ROOT}/config/dashboard/index.html" ]]
fi
grep -q "${INSTALL_DIR}/dekud" "${SYSTEMD_UNIT_PATH}"
cmp -s "${DEKU_CONFIG_DIR}/config.toml" "${TEST_ROOT}/config.first"
if [[ -f "${DEKU_SETUP_COUNT_FILE}" ]]; then
  [[ "$(cat "${DEKU_SETUP_COUNT_FILE}")" == "1" ]]
fi
[[ "$second_output" != *"Dashboard token:"* ]]
[[ "$second_output" == *"Config already exists; keeping current settings"* ]]
[[ "$second_output" == *"Dashboard URL: http://203.0.113.10:2810"* ]]
[[ "$second_output" == *"Local URL:     http://127.0.0.1:2810"* ]]
[[ "$second_output" == *"SSH tunnel:    ssh -L 2810:127.0.0.1:2810 root@203.0.113.10"* ]]
[[ "$second_output" != *"Dashboard assets:"* ]]
[[ "$second_output" != *"Setup complete."* ]]
[[ "$second_output" != *"Reset command:"* ]]
[[ "$second_output" != *"Token reset:"* ]]
[[ "$second_output" == *"Deku successfully installed."* ]]

third_restart_count="$(wc -l < "${DEKU_SYSTEMCTL_LOG_FILE}")"
third_installed_version="$("${INSTALL_DIR}/deku" version)"
third_target_version="$(resolve_requested_version)"
printf 'Installed version: %s\n' "$third_installed_version"
printf 'Target version:    %s\n' "$third_target_version"

[[ "$third_installed_version" == "$third_target_version" ]] \
  || fail "installed version did not match the resolved target version"
[[ "$(wc -l < "${DEKU_SYSTEMCTL_LOG_FILE}")" == "$third_restart_count" ]] \
  || fail "same-version verification should not invoke additional systemctl commands"
if [[ -f "${DEKU_SETUP_COUNT_FILE}" ]]; then
  [[ "$(cat "${DEKU_SETUP_COUNT_FILE}")" == "1" ]] \
    || fail "same-version verification should not rerun setup"
fi

echo "install smoke passed"
