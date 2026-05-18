#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"

SERVICE_USER="${AGENTICS_DGX_SERVICE_USER:-agentics}"
SERVICE_GROUP="${AGENTICS_DGX_SERVICE_GROUP:-agentics}"
CONFIG_ROOT="${AGENTICS_DGX_CONFIG_ROOT:-/etc/agentics}"
RELEASE_ROOT="${AGENTICS_DGX_RELEASE_ROOT:-/opt/agentics}"
STATE_ROOT="${AGENTICS_DGX_STATE_ROOT:-/srv/agentics}"
TEST_STATE_ROOT="${AGENTICS_DGX_TEST_STATE_ROOT:-/srv/agentics-test}"
SYSTEMD_ROOT="${AGENTICS_DGX_SYSTEMD_ROOT:-/etc/systemd/system}"
DOCKER_HOST_URI="${AGENTICS_DOCKER_HOST:-unix:///run/agentics/docker.sock}"
SERVICES=(
  agentics-web.service
  agentics-worker.service
  agentics-api.service
  agentics-docker.service
)

usage() {
  cat <<'EOF'
Usage:
  scripts/ops/manage-dgx-spark-profile.sh install [--skip-storage]
  scripts/ops/manage-dgx-spark-profile.sh start
  scripts/ops/manage-dgx-spark-profile.sh stop
  scripts/ops/manage-dgx-spark-profile.sh uninstall [--purge-data]

Environment overrides:
  AGENTICS_DGX_SERVICE_USER       default: agentics
  AGENTICS_DGX_SERVICE_GROUP      default: agentics
  AGENTICS_DGX_CONFIG_ROOT        default: /etc/agentics
  AGENTICS_DGX_RELEASE_ROOT       default: /opt/agentics
  AGENTICS_DGX_STATE_ROOT         default: /srv/agentics
  AGENTICS_DGX_TEST_STATE_ROOT    default: /srv/agentics-test
  AGENTICS_DGX_SYSTEMD_ROOT       default: /etc/systemd/system
EOF
}

require_linux() {
  if [ "$(uname -s)" != "Linux" ]; then
    printf 'DGX Spark profile management is Linux-only. Detected: %s\n' "$(uname -s)" >&2
    exit 2
  fi
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    printf 'DGX Spark profile management must run as root. Use sudo.\n' >&2
    exit 2
  fi
}

log() {
  printf '[agentics-dgx-profile] %s\n' "$*"
}

systemctl_if_available() {
  if command -v systemctl >/dev/null 2>&1; then
    systemctl "$@"
  fi
}

create_service_identity() {
  if ! getent group "$SERVICE_GROUP" >/dev/null; then
    groupadd --system "$SERVICE_GROUP"
  fi
  if ! getent passwd "$SERVICE_USER" >/dev/null; then
    useradd --system --gid "$SERVICE_GROUP" --home-dir "$STATE_ROOT" --shell /usr/sbin/nologin "$SERVICE_USER"
  fi
}

install_profile() {
  local skip_storage=0
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --skip-storage) skip_storage=1 ;;
      -h|--help) usage; exit 0 ;;
      *) printf 'unknown install argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
    shift
  done

  create_service_identity
  install -d -m 0750 -o root -g "$SERVICE_GROUP" "$CONFIG_ROOT"
  install -d -m 0755 "$SYSTEMD_ROOT"
  install -m 0644 "${REPO_ROOT}/deploy/dgx-spark/dockerd-agentics.json" "${CONFIG_ROOT}/dockerd-agentics.json"
  if [ ! -f "${CONFIG_ROOT}/agentics.env" ]; then
    install -m 0640 -o root -g "$SERVICE_GROUP" "${REPO_ROOT}/deploy/dgx-spark/agentics.env.example" "${CONFIG_ROOT}/agentics.env"
  fi
  install -m 0644 "${REPO_ROOT}/deploy/dgx-spark/"*.service "$SYSTEMD_ROOT/"

  if [ "$skip_storage" -eq 0 ]; then
    AGENTICS_DGX_CONFIRM=prepare-storage \
      AGENTICS_DGX_STATE_ROOT="$STATE_ROOT" \
      AGENTICS_DGX_SERVICE_USER="$SERVICE_USER" \
      AGENTICS_DGX_SERVICE_GROUP="$SERVICE_GROUP" \
      AGENTICS_DGX_PERSIST_FSTAB="${AGENTICS_DGX_PERSIST_FSTAB:-1}" \
      "${REPO_ROOT}/scripts/ops/prepare-dgx-spark-storage.sh"
  fi

  systemctl_if_available daemon-reload
  log "installed DGX profile files"
  log "edit ${CONFIG_ROOT}/agentics.env before starting services"
}

start_profile() {
  systemctl_if_available daemon-reload
  systemctl_if_available enable --now agentics-docker.service
  systemctl_if_available start agentics-api.service
  systemctl_if_available start agentics-worker.service
  systemctl_if_available start agentics-web.service
  log "started DGX profile services"
}

stop_profile() {
  stop_application_services
  systemctl_if_available stop agentics-docker.service 2>/dev/null || true
  log "stopped DGX profile services"
}

stop_application_services() {
  for service in "${SERVICES[@]}"; do
    if [ "$service" = "agentics-docker.service" ]; then
      continue
    fi
    systemctl_if_available stop "$service" 2>/dev/null || true
  done
}

remove_agentics_docker_containers() {
  if ! command -v docker >/dev/null 2>&1; then
    return
  fi
  if [ ! -S /run/agentics/docker.sock ]; then
    return
  fi
  local containers
  containers="$(docker --host "$DOCKER_HOST_URI" ps -aq 2>/dev/null || true)"
  if [ -n "$containers" ]; then
    # shellcheck disable=SC2086
    docker --host "$DOCKER_HOST_URI" rm -f $containers >/dev/null 2>&1 || true
  fi
}

backup_and_remove_fstab_entries() {
  if [ ! -f /etc/fstab ]; then
    return
  fi
  if ! grep -qE "${STATE_ROOT}/loop-images|${TEST_STATE_ROOT}/loop-images" /etc/fstab; then
    return
  fi
  local backup="/etc/fstab.agentics-dgx-profile-backup.$(date +%Y%m%d%H%M%S)"
  cp /etc/fstab "$backup"
  sed -i "\#${STATE_ROOT}/loop-images/#d; \#${TEST_STATE_ROOT}/loop-images/#d" /etc/fstab
  log "removed DGX quota fstab entries; backup: ${backup}"
}

remove_project_entries() {
  for file in /etc/projects /etc/projid; do
    if [ -f "$file" ] && grep -qE "${STATE_ROOT}|${TEST_STATE_ROOT}" "$file"; then
      cp "$file" "${file}.agentics-dgx-profile-backup.$(date +%Y%m%d%H%M%S)"
      sed -i "\#${STATE_ROOT}#d; \#${TEST_STATE_ROOT}#d" "$file"
    fi
  done
}

unmount_tree() {
  local root="$1"
  local targets
  if [ ! -e "$root" ]; then
    return
  fi
  targets="$(findmnt -R "$root" -n -o TARGET 2>/dev/null || true)"
  if [ -z "$targets" ]; then
    return
  fi
  printf '%s\n' "$targets" | sort -r | while IFS= read -r target; do
    if [ -n "$target" ]; then
      umount "$target" 2>/dev/null || umount -l "$target" 2>/dev/null || true
    fi
  done
}

remove_quota_storage() {
  unmount_tree "$TEST_STATE_ROOT"
  unmount_tree "$STATE_ROOT"
  rm -rf \
    "${STATE_ROOT}/loop-images" \
    "${STATE_ROOT}/docker-data-root" \
    "${STATE_ROOT}/phase-mounts" \
    "$TEST_STATE_ROOT"
}

remove_systemd_units() {
  for service in "${SERVICES[@]}"; do
    systemctl_if_available disable "$service" >/dev/null 2>&1 || true
    rm -f "${SYSTEMD_ROOT}/${service}"
  done
  systemctl_if_available daemon-reload
  systemctl_if_available reset-failed
}

remove_service_identity() {
  if getent passwd "$SERVICE_USER" >/dev/null; then
    userdel "$SERVICE_USER" 2>/dev/null || true
  fi
  if getent group "$SERVICE_GROUP" >/dev/null; then
    groupdel "$SERVICE_GROUP" 2>/dev/null || true
  fi
}

uninstall_profile() {
  local purge_data=0
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --purge-data) purge_data=1 ;;
      -h|--help) usage; exit 0 ;;
      *) printf 'unknown uninstall argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
    shift
  done

  stop_application_services
  remove_agentics_docker_containers
  systemctl_if_available stop agentics-docker.service 2>/dev/null || true
  backup_and_remove_fstab_entries
  remove_project_entries
  remove_quota_storage
  remove_systemd_units
  rm -rf /run/agentics

  if [ "$purge_data" -eq 1 ]; then
    rm -rf "$CONFIG_ROOT" "$RELEASE_ROOT" "$STATE_ROOT" "$TEST_STATE_ROOT"
    remove_service_identity
    log "uninstalled DGX profile and purged data"
  else
    log "uninstalled DGX profile services and quota storage; preserved ${CONFIG_ROOT}, ${RELEASE_ROOT}, and durable data under ${STATE_ROOT}"
  fi
}

main() {
  require_linux
  require_root
  local command="${1:-}"
  if [ -z "$command" ]; then
    usage
    exit 2
  fi
  shift
  case "$command" in
    install) install_profile "$@" ;;
    start) start_profile "$@" ;;
    stop) stop_profile "$@" ;;
    uninstall) uninstall_profile "$@" ;;
    -h|--help) usage ;;
    *) printf 'unknown command: %s\n' "$command" >&2; usage >&2; exit 2 ;;
  esac
}

main "$@"
