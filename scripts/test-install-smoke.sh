#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d /tmp/deku-smoke.XXXXXX)"
trap 'cleanup' EXIT

cleanup() {
  if [[ -n "${DEKU_SYSTEMCTL_STATE_DIR:-}" && -f "${DEKU_SYSTEMCTL_STATE_DIR}/deku.pid" ]]; then
    kill "$(cat "${DEKU_SYSTEMCTL_STATE_DIR}/deku.pid")" >/dev/null 2>&1 || true
  fi
  rm -rf "$TEST_ROOT"
}

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
if [[ "${1:-}" != "setup" ]]; then
  echo "unsupported command" >&2
  exit 1
fi
shift

data_dir=""
api_port=""
ssh_port=""
angie_conf_dir=""
global_domain=""

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
} > "${DEKU_CONFIG_DIR}/config.toml"
count_file="${DEKU_SETUP_COUNT_FILE:?}"
count=0
if [[ -f "$count_file" ]]; then
  count="$(cat "$count_file")"
fi
printf '%s' "$((count + 1))" > "$count_file"
EOF
  chmod 0755 "${fixture_dir}/deku-linux-amd64"

  cat > "${fixture_dir}/dekud-linux-amd64" <<EOF
#!/usr/bin/env bash
echo "${version}"
EOF
  chmod 0755 "${fixture_dir}/dekud-linux-amd64"
  cp "${fixture_dir}/deku-linux-amd64" "${fixture_dir}/deku-linux-arm64"
  cp "${fixture_dir}/dekud-linux-amd64" "${fixture_dir}/dekud-linux-arm64"

  : > "${fixture_dir}/SHA256SUMS"
  for artifact in \
    deku-dashboard.tar.gz \
    deku-linux-amd64 \
    deku-linux-arm64 \
    dekud-linux-amd64 \
    dekud-linux-arm64
  do
    printf '%s  %s\n' "$(checksum_file "${fixture_dir}/${artifact}")" "$artifact" >> "${fixture_dir}/SHA256SUMS"
  done
}

mkdir -p "${TEST_ROOT}/fixtures/v1" "${TEST_ROOT}/fixtures/v2" "${TEST_ROOT}/mockbin" "${TEST_ROOT}/state"
write_release_fixture "${TEST_ROOT}/fixtures/v1" "dekud-v1"
write_release_fixture "${TEST_ROOT}/fixtures/v2" "dekud-v2"

cat > "${TEST_ROOT}/mockbin/systemctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state_dir="${DEKU_SYSTEMCTL_STATE_DIR:?}"
command="${1:-}"
shift || true
if [[ "${1:-}" == "--quiet" ]]; then
  shift
fi
service="${1:-}"

mkdir -p "$state_dir"

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
export DEKU_REPO="local/deku"

mkdir -p "$(dirname "$SYSTEMD_UNIT_PATH")"

source "${ROOT_DIR}/scripts/install.sh"

require_root() { :; }
require_linux() { :; }
install_prerequisites() { :; }
install_angie() {
  mkdir -p "$ANGIE_CONF_DIR" "$(dirname "$ANGIE_BASE_CONF")"
  cat > "$ANGIE_BASE_CONF" <<'EOF'
server { listen 80 default_server; return 404; }
EOF
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
socket_exists() {
  local socket_path="$1"
  [[ -e "$socket_path" ]]
}

CURRENT_RELEASE_DIR="${TEST_ROOT}/fixtures/v1"
main

[[ -x "${INSTALL_DIR}/deku" ]]
[[ -x "${INSTALL_DIR}/dekud" ]]
grep -q 'dekud-v1' "${INSTALL_DIR}/dekud"
grep -q 'dekud-v1' "${TEST_ROOT}/config/dashboard/index.html"
[[ "$(cat "${DEKU_SETUP_COUNT_FILE}")" == "1" ]]
cp "${DEKU_CONFIG_DIR}/config.toml" "${TEST_ROOT}/config.first"

CURRENT_RELEASE_DIR="${TEST_ROOT}/fixtures/v2"
main

grep -q 'dekud-v2' "${INSTALL_DIR}/dekud"
grep -q 'dekud-v2' "${TEST_ROOT}/config/dashboard/index.html"
cmp -s "${DEKU_CONFIG_DIR}/config.toml" "${TEST_ROOT}/config.first"
[[ "$(cat "${DEKU_SETUP_COUNT_FILE}")" == "1" ]]

echo "install smoke passed"
