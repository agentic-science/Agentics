#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Linux" ]; then
  printf 'DGX Spark storage preparation is Linux-only. Detected: %s\n' "$(uname -s)" >&2
  exit 2
fi

if [ "${AGENTICS_DGX_CONFIRM:-}" != "prepare-storage" ]; then
  cat >&2 <<'EOF'
Refusing to prepare DGX storage without explicit confirmation.

Set AGENTICS_DGX_CONFIRM=prepare-storage and run as an operator with privileges
to create directories, format loopback XFS images, and mount filesystems.
EOF
  exit 2
fi

STATE_ROOT="${AGENTICS_DGX_STATE_ROOT:-/srv/agentics}"
LOOP_IMAGE_ROOT="${AGENTICS_DGX_LOOP_IMAGE_ROOT:-${STATE_ROOT}/loop-images}"
DOCKER_DATA_ROOT="${AGENTICS_DGX_DOCKER_DATA_ROOT:-${STATE_ROOT}/docker-data-root}"
DOCKER_LOOP_IMAGE="${AGENTICS_DGX_DOCKER_LOOP_IMAGE:-${LOOP_IMAGE_ROOT}/docker-data-root.xfs}"
PHASE_MOUNT_ROOT="${AGENTICS_DGX_PHASE_MOUNT_ROOT:-${STATE_ROOT}/phase-mounts}"
DOCKER_LOOP_SIZE="${AGENTICS_DGX_DOCKER_LOOP_SIZE:-200G}"
PHASE_LOOP_SIZE="${AGENTICS_DGX_PHASE_LOOP_SIZE:-20G}"
SERVICE_USER="${AGENTICS_DGX_SERVICE_USER:-agentics}"
SERVICE_GROUP="${AGENTICS_DGX_SERVICE_GROUP:-agentics}"
PHASES="${AGENTICS_DGX_PHASES:-solution-setup solution-build solution-run scorer-prepare scorer-score}"
PERSIST_FSTAB="${AGENTICS_DGX_PERSIST_FSTAB:-0}"

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'missing required command: %s\n' "$1" >&2
    exit 1
  }
}

ensure_xfs_image() {
  local image_path="$1"
  local size="$2"
  if [ ! -f "$image_path" ]; then
    truncate -s "$size" "$image_path"
    mkfs.xfs -f "$image_path"
  fi
}

ensure_mount() {
  local image_path="$1"
  local mount_path="$2"
  mkdir -p "$mount_path"
  if findmnt --mountpoint "$mount_path" >/dev/null 2>&1; then
    return
  fi
  mount -o loop,prjquota "$image_path" "$mount_path"
}

ensure_fstab_entry() {
  local image_path="$1"
  local mount_path="$2"

  if [ "$PERSIST_FSTAB" != "1" ]; then
    return
  fi

  if awk -v target="$mount_path" '$1 !~ /^#/ && $2 == target { found = 1 } END { exit found ? 0 : 1 }' /etc/fstab; then
    return
  fi

  printf '%s %s xfs loop,prjquota,nofail 0 0\n' "$image_path" "$mount_path" >>/etc/fstab
}

require_command findmnt
require_command mkfs.xfs
require_command mount
require_command truncate
if [ "$PERSIST_FSTAB" = "1" ]; then
  require_command awk
fi

install -d -m 0755 "$STATE_ROOT" "$LOOP_IMAGE_ROOT" "$DOCKER_DATA_ROOT" "$PHASE_MOUNT_ROOT"
install -d -m 0755 "${STATE_ROOT}/storage" "${STATE_ROOT}/challenges" "${STATE_ROOT}/runtime"

ensure_xfs_image "$DOCKER_LOOP_IMAGE" "$DOCKER_LOOP_SIZE"
ensure_mount "$DOCKER_LOOP_IMAGE" "$DOCKER_DATA_ROOT"
ensure_fstab_entry "$DOCKER_LOOP_IMAGE" "$DOCKER_DATA_ROOT"

for phase in $PHASES; do
  image_path="${LOOP_IMAGE_ROOT}/phase-${phase}.xfs"
  mount_path="${PHASE_MOUNT_ROOT}/${phase}"
  ensure_xfs_image "$image_path" "$PHASE_LOOP_SIZE"
  ensure_mount "$image_path" "$mount_path"
  ensure_fstab_entry "$image_path" "$mount_path"
done

if getent passwd "$SERVICE_USER" >/dev/null 2>&1 && getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
  chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${STATE_ROOT}/storage" "${STATE_ROOT}/challenges" "${STATE_ROOT}/runtime" "$PHASE_MOUNT_ROOT"
fi

cat <<EOF
DGX storage preparation completed.

Docker data root: $DOCKER_DATA_ROOT
Phase mount root: $PHASE_MOUNT_ROOT
EOF

if [ "$PERSIST_FSTAB" = "1" ]; then
  printf 'Idempotent /etc/fstab entries are present for the loopback mounts.\n'
else
  printf 'Set AGENTICS_DGX_PERSIST_FSTAB=1 to write idempotent /etc/fstab entries.\n'
fi
