#!/usr/bin/env bash
set -euo pipefail

DEKU_VERSION="${DEKU_VERSION:-latest}"
DEKU_REPO="${DEKU_REPO:-yourorg/deku}"
DEKU_RELEASE_BASE_URL="${DEKU_RELEASE_BASE_URL:-}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${DEKU_CONFIG_DIR:-${HOME:-/root}/.deku}"
ANGIE_CONF_DIR="${ANGIE_CONF_DIR:-/etc/angie/conf.d/deku}"
ANGIE_BASE_CONF="${ANGIE_BASE_CONF:-/etc/angie/conf.d/deku-default.conf}"
SYSTEMD_UNIT_PATH="${SYSTEMD_UNIT_PATH:-/etc/systemd/system/deku.service}"
DEKU_GLOBAL_DOMAIN="${DEKU_GLOBAL_DOMAIN:-}"
DEKU_API_PORT="${DEKU_API_PORT:-2810}"
DEKU_SSH_PORT="${DEKU_SSH_PORT:-22}"
DEKU_DATA_DIR="${DEKU_DATA_DIR:-${CONFIG_DIR}}"

TMP_DIR="$(mktemp -d)"
CHECKSUMS_FILE="${TMP_DIR}/SHA256SUMS"
trap 'rm -rf "$TMP_DIR"' EXIT

log() {
  printf '==> %s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_root() {
  if [[ "$(id -u)" -ne 0 ]]; then
    fail "run this installer as root"
  fi
}

require_linux() {
  if [[ "$(uname -s)" != "Linux" ]]; then
    fail "this installer currently supports Linux only"
  fi
}

detect_arch() {
  case "$(uname -m)" in
    x86_64) echo "amd64" ;;
    aarch64|arm64) echo "arm64" ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
  esac
}

normalize_version() {
  if [[ "$DEKU_VERSION" == "latest" ]]; then
    echo "latest"
  elif [[ "$DEKU_VERSION" == v* ]]; then
    echo "$DEKU_VERSION"
  else
    echo "v$DEKU_VERSION"
  fi
}

release_base_url() {
  if [[ -n "$DEKU_RELEASE_BASE_URL" ]]; then
    echo "$DEKU_RELEASE_BASE_URL"
    return
  fi

  local version
  version="$(normalize_version)"

  if [[ "$version" == "latest" ]]; then
    echo "https://github.com/${DEKU_REPO}/releases/latest/download"
  else
    echo "https://github.com/${DEKU_REPO}/releases/download/${version}"
  fi
}

download_artifact() {
  local artifact="$1"
  local dest="$2"
  local base_url
  base_url="$(release_base_url)"

  log "Downloading ${artifact}"
  curl --fail --location --silent --show-error --retry 3 \
    "${base_url}/${artifact}" \
    -o "${dest}"
}

download_checksums() {
  local base_url
  base_url="$(release_base_url)"

  if curl --fail --location --silent --show-error --retry 3 \
    "${base_url}/SHA256SUMS" \
    -o "${CHECKSUMS_FILE}"; then
    return 0
  fi

  printf 'warning: SHA256SUMS not found for this release; continuing without checksum verification.\n' >&2
  return 1
}

verify_artifact() {
  local artifact="$1"
  local dest="$2"

  if [[ ! -f "${CHECKSUMS_FILE}" ]]; then
    return 0
  fi

  local expected actual
  expected="$(awk -v artifact="$artifact" '$2 == artifact { print $1 }' "${CHECKSUMS_FILE}")"
  if [[ -z "$expected" ]]; then
    fail "missing checksum entry for ${artifact}"
  fi

  actual="$(sha256sum "${dest}" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    fail "checksum verification failed for ${artifact}"
  fi
}

install_prerequisites() {
  if ! command -v apt-get >/dev/null 2>&1; then
    fail "apt-get is required; supported targets are Ubuntu 22.04+ and Debian 11+"
  fi

  export DEBIAN_FRONTEND=noninteractive

  log "Installing base packages"
  apt-get update -qq
  apt-get install -y ca-certificates curl gnupg tar
}

install_angie() {
  local distro codename repo_line

  if [[ ! -r /etc/os-release ]]; then
    fail "/etc/os-release is required to install Angie"
  fi

  # shellcheck disable=SC1091
  source /etc/os-release
  distro="${ID:-}"
  codename="${VERSION_CODENAME:-}"

  if [[ -z "$distro" || -z "$codename" ]]; then
    fail "could not determine distro/codename for Angie repository"
  fi

  repo_line="deb [signed-by=/usr/share/keyrings/angie-signing.gpg] https://download.angie.software/angie/${distro}/${codename} free main"

  log "Installing Angie"
  curl --fail --location --silent --show-error https://angie.software/keys/angie-signing.gpg \
    | gpg --dearmor -o /usr/share/keyrings/angie-signing.gpg
  printf '%s\n' "$repo_line" > /etc/apt/sources.list.d/angie.list

  apt-get update -qq
  apt-get install -y angie

  mkdir -p "$ANGIE_CONF_DIR"
  cat > "$ANGIE_BASE_CONF" <<'EOF'
# Base Angie configuration for Deku
# This file is managed by the Deku installer.

server {
    listen 80 default_server;
    listen [::]:80 default_server;
    server_name _;

    location / {
        return 404;
    }
}
EOF
}

install_binaries() {
  local arch="$1"

  mkdir -p "$INSTALL_DIR"
  download_artifact "dekud-linux-${arch}" "${TMP_DIR}/dekud"
  download_artifact "deku-linux-${arch}" "${TMP_DIR}/deku"
  verify_artifact "dekud-linux-${arch}" "${TMP_DIR}/dekud"
  verify_artifact "deku-linux-${arch}" "${TMP_DIR}/deku"

  install -m 0755 "${TMP_DIR}/dekud" "${INSTALL_DIR}/dekud"
  install -m 0755 "${TMP_DIR}/deku" "${INSTALL_DIR}/deku"
}

write_systemd_unit() {
  log "Writing systemd unit"
  cat > "$SYSTEMD_UNIT_PATH" <<EOF
[Unit]
Description=Deku PaaS Daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/dekud
Restart=always
RestartSec=5
User=root
Environment=HOME=/root
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF
}

run_setup() {
  if [[ -f "${CONFIG_DIR}/config.toml" ]]; then
    log "Config already exists at ${CONFIG_DIR}/config.toml; skipping setup"
    return
  fi

  log "Running deku setup"
  local cmd=(
    "${INSTALL_DIR}/deku" setup
    --no-systemd
    --defaults
    --data-dir "${DEKU_DATA_DIR}"
    --api-port "${DEKU_API_PORT}"
    --ssh-port "${DEKU_SSH_PORT}"
    --angie-conf-dir "${ANGIE_CONF_DIR}"
  )

  if [[ -n "${DEKU_GLOBAL_DOMAIN}" ]]; then
    cmd+=(--global-domain "${DEKU_GLOBAL_DOMAIN}")
  fi

  "${cmd[@]}"
}

configured_data_dir() {
  local config_path="${CONFIG_DIR}/config.toml"

  if [[ ! -f "$config_path" ]]; then
    fail "expected config at ${config_path} after setup"
  fi

  local data_dir
  data_dir="$(sed -n 's/^data_dir = "\(.*\)"$/\1/p' "$config_path" | head -n 1)"

  if [[ -z "$data_dir" ]]; then
    fail "could not read data_dir from ${config_path}"
  fi

  printf '%s\n' "$data_dir"
}

configured_api_port() {
  local config_path="${CONFIG_DIR}/config.toml"

  if [[ ! -f "$config_path" ]]; then
    fail "expected config at ${config_path} after setup"
  fi

  local api_port
  api_port="$(sed -n 's/^api_port = \([0-9][0-9]*\)$/\1/p' "$config_path" | head -n 1)"

  if [[ -z "$api_port" ]]; then
    fail "could not read api_port from ${config_path}"
  fi

  printf '%s\n' "$api_port"
}

install_dashboard_assets() {
  local data_dir="$1"
  local dashboard_dir="${data_dir}/dashboard"
  local staging_dir="${TMP_DIR}/dashboard"

  download_artifact "deku-dashboard.tar.gz" "${TMP_DIR}/deku-dashboard.tar.gz"
  verify_artifact "deku-dashboard.tar.gz" "${TMP_DIR}/deku-dashboard.tar.gz"
  mkdir -p "$staging_dir"
  tar -xzf "${TMP_DIR}/deku-dashboard.tar.gz" -C "$staging_dir"
  rm -rf "${dashboard_dir}.previous"
  if [[ -d "$dashboard_dir" ]]; then
    mv "$dashboard_dir" "${dashboard_dir}.previous"
  fi
  if ! mv "$staging_dir" "$dashboard_dir"; then
    rm -rf "$staging_dir"
    if [[ -d "${dashboard_dir}.previous" ]]; then
      mv "${dashboard_dir}.previous" "$dashboard_dir"
    fi
    fail "failed to update dashboard assets"
  fi
  rm -rf "${dashboard_dir}.previous"
}

enable_services() {
  log "Reloading service manager"
  systemctl daemon-reload

  log "Ensuring Angie is enabled"
  systemctl enable angie >/dev/null
  if systemctl is-active --quiet angie; then
    systemctl reload angie || systemctl restart angie
  else
    systemctl start angie
  fi

  log "Ensuring Deku is enabled"
  systemctl enable deku >/dev/null
  if systemctl is-active --quiet deku; then
    systemctl restart deku
  else
    systemctl start deku
  fi
}

socket_exists() {
  local socket_path="$1"
  [[ -S "$socket_path" ]]
}

verify_installation() {
  local data_dir="$1"
  local api_port="$2"
  local socket_path="${data_dir}/deku.sock"
  local health_url="http://127.0.0.1:${api_port}/healthz"

  log "Validating Angie configuration"
  angie -t -q || fail "angie config validation failed after install"

  log "Verifying systemd services"
  local service
  local attempt
  for service in angie deku; do
    for attempt in $(seq 1 15); do
      if systemctl is-active --quiet "$service"; then
        break
      fi
      sleep 1
    done
    systemctl is-active --quiet "$service" || fail "${service} service is not active"
  done

  log "Waiting for Deku API health check"
  local response=""
  for attempt in $(seq 1 30); do
    if response="$(curl --fail --silent --show-error "$health_url" 2>/dev/null)" \
      && [[ "$response" == *'"status":"ok"'* ]]; then
      break
    fi
    sleep 1
  done

  if [[ "$response" != *'"status":"ok"'* ]]; then
    fail "Deku API did not become healthy at ${health_url}"
  fi

  if ! socket_exists "$socket_path"; then
    fail "expected Deku unix socket at ${socket_path}"
  fi
}

warn_if_missing_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    printf 'warning: docker is not installed; dekud will start but deploys will fail until Docker Engine is installed.\n' >&2
  fi
}

main() {
  require_linux
  require_root

  if [[ "$DEKU_REPO" == "yourorg/deku" && -z "$DEKU_RELEASE_BASE_URL" ]]; then
    fail "set DEKU_REPO to your GitHub repo (for example owner/deku) or set DEKU_RELEASE_BASE_URL"
  fi

  local arch
  arch="$(detect_arch)"

  log "Installing Deku ${DEKU_VERSION} for linux/${arch}"
  install_prerequisites
  download_checksums || true
  install_angie
  install_binaries "$arch"
  write_systemd_unit
  run_setup

  local data_dir
  data_dir="$(configured_data_dir)"
  local api_port
  api_port="$(configured_api_port)"
  install_dashboard_assets "$data_dir"

  enable_services
  verify_installation "$data_dir" "$api_port"
  warn_if_missing_docker

  log "Install complete"
  printf 'Deku config: %s/config.toml\n' "$CONFIG_DIR"
  printf 'Dashboard assets: %s/dashboard\n' "$data_dir"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
