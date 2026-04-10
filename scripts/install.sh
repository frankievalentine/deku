#!/usr/bin/env bash
set -euo pipefail

DEKU_VERSION="${DEKU_VERSION:-latest}"
DEKU_REPO="${DEKU_REPO:-frankievalentine/deku}"
DEKU_RELEASE_BASE_URL="${DEKU_RELEASE_BASE_URL:-}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${DEKU_CONFIG_DIR:-${HOME:-/root}/.deku}"
ANGIE_CONF_DIR="${ANGIE_CONF_DIR:-/etc/angie/conf.d/deku}"
ANGIE_BASE_CONF="${ANGIE_BASE_CONF:-/etc/angie/conf.d/deku-default.conf}"
SYSTEMD_UNIT_PATH="${SYSTEMD_UNIT_PATH:-/etc/systemd/system/deku.service}"
DEKU_GLOBAL_DOMAIN="${DEKU_GLOBAL_DOMAIN:-}"
DEKU_API_PORT="${DEKU_API_PORT:-2810}"
DEKU_SSH_PORT="${DEKU_SSH_PORT:-2222}"
DEKU_DATA_DIR="${DEKU_DATA_DIR:-${CONFIG_DIR}}"

TMP_DIR="$(mktemp -d)"
CHECKSUMS_FILE="${TMP_DIR}/SHA256SUMS"
RESOLVED_VERSION=""
SETUP_RAN=0
trap 'rm -rf "$TMP_DIR"' EXIT

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  COLOR_BOLD=$'\033[1m'
  COLOR_BLUE=$'\033[1;34m'
  COLOR_GREEN=$'\033[1;32m'
  COLOR_YELLOW=$'\033[1;33m'
  COLOR_RED=$'\033[1;31m'
  COLOR_RESET=$'\033[0m'
else
  COLOR_BOLD=""
  COLOR_BLUE=""
  COLOR_GREEN=""
  COLOR_YELLOW=""
  COLOR_RED=""
  COLOR_RESET=""
fi

log() {
  printf '%b==>%b %s\n' "$COLOR_BLUE" "$COLOR_RESET" "$*"
}

success() {
  printf '%b%s%b\n' "$COLOR_GREEN" "$*" "$COLOR_RESET"
}

important() {
  printf '%b%s%b\n' "$COLOR_RED" "$*" "$COLOR_RESET"
}

bold() {
  printf '%b%s%b' "$COLOR_BOLD" "$*" "$COLOR_RESET"
}

warn() {
  printf '%bwarning:%b %s\n' "$COLOR_YELLOW" "$COLOR_RESET" "$*" >&2
}

fail() {
  printf '%berror:%b %s\n' "$COLOR_RED" "$COLOR_RESET" "$*" >&2
  exit 1
}

quiet_run() {
  local output_file status
  output_file="$(mktemp "${TMP_DIR}/cmd.XXXXXX")"

  if "$@" >"${output_file}" 2>&1; then
    rm -f "${output_file}"
    return 0
  fi

  status=$?
  cat "${output_file}" >&2
  rm -f "${output_file}"
  fail "command failed (${status}): $*"
}

install_support_artifact() {
  local artifact="$1"
  local dest="$2"
  local mode="$3"
  local downloaded="${TMP_DIR}/${artifact}"

  download_artifact "$artifact" "$downloaded"
  verify_artifact "$artifact" "$downloaded"

  mkdir -p "$(dirname "$dest")"

  case "$artifact" in
    deku.service)
      sed "s|__DEKU_INSTALL_DIR__|${INSTALL_DIR}|g" "$downloaded" > "$dest"
      chmod "$mode" "$dest"
      ;;
    *)
      install -m "$mode" "$downloaded" "$dest"
      ;;
  esac
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

normalize_version_string() {
  local version="$1"

  if [[ -z "$version" ]]; then
    return 1
  fi

  if [[ "$version" == v* ]]; then
    echo "$version"
  else
    echo "v$version"
  fi
}

fetch_latest_release_version() {
  local payload tag

  payload="$(curl --fail --location --silent --show-error --retry 3 \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: deku-install/latest" \
    "https://api.github.com/repos/${DEKU_REPO}/releases/latest")" \
    || fail "failed to resolve the latest Deku release version"

  tag="$(printf '%s' "$payload" | tr -d '\n' | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
  [[ -n "$tag" ]] || fail "failed to parse the latest Deku release version"

  normalize_version_string "$tag"
}

resolve_requested_version() {
  if [[ -n "$RESOLVED_VERSION" ]]; then
    echo "$RESOLVED_VERSION"
    return
  fi

  if [[ "$DEKU_VERSION" == "latest" ]]; then
    RESOLVED_VERSION="$(fetch_latest_release_version)"
  else
    RESOLVED_VERSION="$(normalize_version)"
  fi

  echo "$RESOLVED_VERSION"
}

release_base_url() {
  if [[ -n "$DEKU_RELEASE_BASE_URL" ]]; then
    echo "$DEKU_RELEASE_BASE_URL"
    return
  fi

  local version
  version="$(resolve_requested_version)"
  echo "https://github.com/${DEKU_REPO}/releases/download/${version}"
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

  if curl --fail --location --silent --retry 3 \
    "${base_url}/SHA256SUMS" \
    -o "${CHECKSUMS_FILE}"; then
    return 0
  fi

  warn "SHA256SUMS not found for this release; continuing without checksum verification."
  return 1
}

download_dashboard_bundle() {
  log "Installing dashboard assets"
  download_artifact "deku-dashboard.tar.gz" "${TMP_DIR}/deku-dashboard.tar.gz"
  verify_artifact "deku-dashboard.tar.gz" "${TMP_DIR}/deku-dashboard.tar.gz"
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

  log "Installing system packages"
  quiet_run apt-get update -qq
  quiet_run apt-get install -y -qq ca-certificates curl gnupg tar
}

angie_repo_line() {
  local distro="$1"
  local version_id="$2"
  local codename="$3"

  if [[ -z "$distro" || -z "$version_id" || -z "$codename" ]]; then
    fail "could not determine distro/version/codename for Angie repository"
  fi

  printf '%s\n' \
    "deb [signed-by=/usr/share/keyrings/angie-signing.gpg] https://download.angie.software/angie/${distro}/${version_id} ${codename} main"
}

install_angie() {
  local distro version_id codename repo_line

  if [[ ! -r /etc/os-release ]]; then
    fail "/etc/os-release is required to install Angie"
  fi

  # shellcheck disable=SC1091
  source /etc/os-release
  distro="${ID:-}"
  version_id="${VERSION_ID:-}"
  codename="${VERSION_CODENAME:-}"
  repo_line="$(angie_repo_line "$distro" "$version_id" "$codename")"

  log "Installing Angie"
  curl --fail --location --silent --show-error https://angie.software/keys/angie-signing.gpg \
    -o "${TMP_DIR}/angie-signing.gpg"
  quiet_run gpg --dearmor --yes --batch \
    -o /usr/share/keyrings/angie-signing.gpg \
    "${TMP_DIR}/angie-signing.gpg"
  printf '%s\n' "$repo_line" > /etc/apt/sources.list.d/angie.list

  quiet_run apt-get update -qq
  quiet_run apt-get install -y -qq angie

  mkdir -p "$ANGIE_CONF_DIR"
  install_support_artifact "angie-deku.conf" "$ANGIE_BASE_CONF" 0644
}

install_binaries() {
  local arch="$1"

  log "Installing Deku binaries"
  mkdir -p "$INSTALL_DIR"
  download_artifact "dekud-linux-${arch}" "${TMP_DIR}/dekud"
  download_artifact "deku-linux-${arch}" "${TMP_DIR}/deku"
  verify_artifact "dekud-linux-${arch}" "${TMP_DIR}/dekud"
  verify_artifact "deku-linux-${arch}" "${TMP_DIR}/deku"

  install -m 0755 "${TMP_DIR}/dekud" "${INSTALL_DIR}/dekud"
  install -m 0755 "${TMP_DIR}/deku" "${INSTALL_DIR}/deku"
}

write_systemd_unit() {
  log "Installing systemd unit"
  install_support_artifact "deku.service" "$SYSTEMD_UNIT_PATH" 0644
}

can_prompt_setup() {
  [[ -z "${DEKU_INSTALL_FORCE_DEFAULTS:-}" ]] || return 1

  # A readable /dev/tty path is not enough; the current shell must actually have
  # a controlling terminal so `deku setup` can prompt successfully.
  ( : </dev/tty >/dev/tty ) >/dev/null 2>&1
}

run_setup() {
  if [[ -f "${CONFIG_DIR}/config.toml" ]]; then
    SETUP_RAN=0
    log "Config already exists; keeping current settings"
    return
  fi

  local cmd=(
    "${INSTALL_DIR}/deku" setup
    --no-systemd
    --installer
    --token-output "${TMP_DIR}/dashboard-token"
  )

  if [[ "${DEKU_DATA_DIR}" != "${CONFIG_DIR}" ]]; then
    cmd+=(--data-dir "${DEKU_DATA_DIR}")
  fi

  if [[ "${DEKU_API_PORT}" != "2810" ]]; then
    cmd+=(--api-port "${DEKU_API_PORT}")
  fi

  if [[ "${DEKU_SSH_PORT}" != "2222" ]]; then
    cmd+=(--ssh-port "${DEKU_SSH_PORT}")
  fi

  if [[ "${ANGIE_CONF_DIR}" != "/etc/angie/conf.d/deku" ]]; then
    cmd+=(--angie-conf-dir "${ANGIE_CONF_DIR}")
  fi

  if [[ -n "${DEKU_GLOBAL_DOMAIN}" ]]; then
    cmd+=(--global-domain "${DEKU_GLOBAL_DOMAIN}")
  fi

  if can_prompt_setup; then
    SETUP_RAN=1
    log "Launching deku setup"
    "${cmd[@]}" </dev/tty
  else
    SETUP_RAN=1
    log "Running deku setup with defaults"
    warn "No interactive terminal detected; run \`deku setup\` later to customize settings."
    "${cmd[@]}" --defaults
  fi
}

detect_dashboard_host() {
  if [[ -n "${DEKU_DASHBOARD_HOST:-}" ]]; then
    printf '%s\n' "${DEKU_DASHBOARD_HOST}"
    return
  fi

  local host_ip=""

  if command -v ip >/dev/null 2>&1; then
    host_ip="$(ip -4 route get 192.0.2.1 2>/dev/null | awk '{for (i = 1; i <= NF; i++) if ($i == "src") { print $(i + 1); exit }}')"
  fi

  if [[ -z "$host_ip" ]] && command -v hostname >/dev/null 2>&1; then
    host_ip="$(hostname -I 2>/dev/null | awk '{for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+(\.[0-9]+){3}$/ && $i !~ /^127\./) { print $i; exit }}')"
  fi

  if [[ -n "$host_ip" ]]; then
    printf '%s\n' "$host_ip"
    return
  fi

  printf '127.0.0.1\n'
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

configured_ssh_port() {
  local config_path="${CONFIG_DIR}/config.toml"

  if [[ ! -f "$config_path" ]]; then
    fail "expected config at ${CONFIG_DIR}/config.toml after setup"
  fi

  local ssh_port
  ssh_port="$(sed -n 's/^ssh_port = \([0-9][0-9]*\)$/\1/p' "$config_path" | head -n 1)"

  if [[ -z "$ssh_port" ]]; then
    fail "could not read ssh_port from ${config_path}"
  fi

  printf '%s\n' "$ssh_port"
}

normalize_installed_version() {
  local version="$1"

  version="${version##* }"
  version="${version//$'\r'/}"
  version="${version//$'\n'/}"

  [[ -n "$version" ]] || return 1
  normalize_version_string "$version"
}

installed_version() {
  local deku_bin="${INSTALL_DIR}/deku"
  local version=""

  [[ -x "$deku_bin" ]] || return 1

  if version="$("$deku_bin" version 2>/dev/null | head -n 1)"; then
    :
  elif version="$("$deku_bin" --version 2>/dev/null | head -n 1)"; then
    :
  else
    return 1
  fi

  normalize_installed_version "$version"
}

maybe_skip_reinstall() {
  local target_version="$1"
  local current_version=""

  if [[ "$DEKU_VERSION" != "latest" ]]; then
    return 1
  fi

  if [[ ! -x "${INSTALL_DIR}/deku" ]] \
    || [[ ! -f "${SYSTEMD_UNIT_PATH}" ]] \
    || [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
    return 1
  fi

  if current_version="$(installed_version 2>/dev/null)" \
    && [[ -n "$current_version" ]] \
    && [[ "$current_version" == "$target_version" ]]; then
    success "Deku is already installed and at the latest version."
    return 0
  fi

  return 1
}

install_dashboard_assets() {
  local data_dir="$1"
  local dashboard_dir="${data_dir}/dashboard"
  local staging_dir="${TMP_DIR}/dashboard"

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
  quiet_run systemctl daemon-reload

  log "Starting services"
  quiet_run systemctl enable angie
  if systemctl is-active --quiet angie; then
    quiet_run systemctl reload angie || quiet_run systemctl restart angie
  else
    quiet_run systemctl start angie
  fi

  quiet_run systemctl enable deku
  if systemctl is-active --quiet deku; then
    quiet_run systemctl restart deku
  else
    quiet_run systemctl start deku
  fi
}

socket_exists() {
  local socket_path="$1"
  [[ -S "$socket_path" ]]
}

verify_installation() {
  local data_dir="$1"
  local api_port="$2"
  local ssh_port="$3"
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
    if [[ "$ssh_port" == "22" ]]; then
      warn "Deku is configured to bind its embedded SSH deploy server to port 22."
      warn "If OpenSSH already owns port 22, dekud will stay offline until you move Deku to another port such as 2222."
      warn "Update ${CONFIG_DIR}/config.toml, set ssh_port = 2222, then restart the deku service."
    fi
    fail "Deku API did not become healthy at ${health_url}"
  fi

  if ! socket_exists "$socket_path"; then
    fail "expected Deku unix socket at ${socket_path}"
  fi
}

warn_if_missing_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    warn "docker is not installed; dekud will start but deploys will fail until Docker Engine is installed."
  fi
}

print_setup_summary() {
  local api_port="$1"
  local dashboard_token="$2"
  local dashboard_host="$3"
  printf '\n'
  success "Deku successfully installed."
  printf '\n'

  if [[ -n "$dashboard_token" ]]; then
    important "Save this dashboard token now. It will only be shown once."
    important "The token is stored hashed at rest and cannot be recovered later."
    printf '\n'
    printf 'Dashboard token: %s\n' "$dashboard_token"
  fi

  printf 'Dashboard URL: %s\n' "$(bold "http://${dashboard_host}:${api_port}")"
  printf 'Local URL:     %s\n' "$(bold "http://127.0.0.1:${api_port}")"
  printf 'SSH tunnel:    ssh -L %s:127.0.0.1:%s root@%s\n' "$api_port" "$api_port" "$dashboard_host"
}

print_get_started() {
  printf 'Get started:\n'
  printf '  deku apps create my-app\n'
  printf '  deku deploy run my-app --path /absolute/path/to/app\n'
}

main() {
  require_linux
  require_root
  RESOLVED_VERSION=""
  SETUP_RAN=0
  rm -f "${TMP_DIR}/dashboard-token"

  local arch
  arch="$(detect_arch)"
  local resolved_version
  resolved_version="$(resolve_requested_version)"
  if maybe_skip_reinstall "$resolved_version"; then
    return 0
  fi

  log "Installing Deku ${resolved_version} for linux/${arch}"
  install_prerequisites
  download_checksums || true
  install_angie
  install_binaries "$arch"
  write_systemd_unit
  download_dashboard_bundle
  run_setup

  local data_dir
  data_dir="$(configured_data_dir)"
  local api_port
  api_port="$(configured_api_port)"
  local ssh_port
  ssh_port="$(configured_ssh_port)"
  install_dashboard_assets "$data_dir"

  enable_services
  verify_installation "$data_dir" "$api_port" "$ssh_port"
  warn_if_missing_docker

  local dashboard_host
  dashboard_host="$(detect_dashboard_host)"
  local dashboard_token=""
  if [[ -f "${TMP_DIR}/dashboard-token" ]]; then
    dashboard_token="$(tr -d '\r\n' < "${TMP_DIR}/dashboard-token")"
  fi

  if [[ "$SETUP_RAN" == "1" ]]; then
    print_setup_summary "$api_port" "$dashboard_token" "$dashboard_host"
  else
    printf '\n'
    success "Deku successfully installed."
    printf '\n'
    printf 'Dashboard URL: %s\n' "$(bold "http://${dashboard_host}:${api_port}")"
    printf 'Local URL:     %s\n' "$(bold "http://127.0.0.1:${api_port}")"
    printf 'SSH tunnel:    ssh -L %s:127.0.0.1:%s root@%s\n' "$api_port" "$api_port" "$dashboard_host"
  fi
  printf '\n'
  print_get_started
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
