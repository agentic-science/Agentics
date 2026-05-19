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
PHASE_SLOT_CLASSES_MB="${AGENTICS_DGX_PHASE_SLOT_CLASSES_MB:-${AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB:-64 256 1024 4096}}"
PHASE_SLOTS_PER_CLASS="${AGENTICS_DGX_PHASE_SLOTS_PER_CLASS:-4}"
PHASE_PROJECT_ID_BASE="${AGENTICS_DGX_PHASE_PROJECT_ID_BASE:-100000}"
PHASE_SLOT_INODES_PER_MB="${AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB:-256}"
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

ensure_positive_integer() {
  local value="$1"
  local label="$2"
  case "$value" in
    ''|*[!0-9]*)
      printf '%s must be a positive integer, got: %s\n' "$label" "$value" >&2
      exit 2
      ;;
  esac
  if [ "$value" -le 0 ]; then
    printf '%s must be greater than zero, got: %s\n' "$label" "$value" >&2
    exit 2
  fi
}

phase_slot_classes() {
  printf '%s\n' "$PHASE_SLOT_CLASSES_MB" | tr ',' ' '
}

ensure_quota_slot() {
  local mount_path="$1"
  local phase="$2"
  local class_mb="$3"
  local slot_index="$4"
  local project_id="$5"
  local slot_name
  local slot_class_path
  local slot_path
  local inode_hard_limit

  ensure_positive_integer "$class_mb" "phase slot class"
  ensure_positive_integer "$PHASE_SLOT_INODES_PER_MB" "AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB"
  slot_name="$(printf 'slot-%03d' "$slot_index")"
  slot_class_path="${mount_path}/slots/${class_mb}mb"
  slot_path="${slot_class_path}/${slot_name}"
  inode_hard_limit=$((class_mb * PHASE_SLOT_INODES_PER_MB))
  install -d -m 0755 "$slot_class_path" "$slot_path"
  xfs_quota -x -c "project -s -p ${slot_path} ${project_id}" "$mount_path"
  xfs_quota -x -c "limit -p bhard=${class_mb}m ihard=${inode_hard_limit} ${project_id}" "$mount_path"
  cat >"${slot_path}/.agentics-slot.json" <<EOF
{"phase":"${phase}","slot_class_mb":${class_mb},"slot_index":${slot_index},"project_id":${project_id},"inodes_per_mb":${PHASE_SLOT_INODES_PER_MB},"inode_hard_limit":${inode_hard_limit}}
EOF
  if getent passwd "$SERVICE_USER" >/dev/null 2>&1 && getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "$slot_path"
  fi
}

ensure_phase_slots() {
  local mount_path="$1"
  local phase="$2"
  local class_index=0
  local class_mb
  local slot_index
  local project_id

  ensure_positive_integer "$PHASE_SLOTS_PER_CLASS" "AGENTICS_DGX_PHASE_SLOTS_PER_CLASS"
  ensure_positive_integer "$PHASE_PROJECT_ID_BASE" "AGENTICS_DGX_PHASE_PROJECT_ID_BASE"

  for class_mb in $(phase_slot_classes); do
    slot_index=1
    while [ "$slot_index" -le "$PHASE_SLOTS_PER_CLASS" ]; do
      project_id=$((PHASE_PROJECT_ID_BASE + class_index * PHASE_SLOTS_PER_CLASS + slot_index))
      ensure_quota_slot "$mount_path" "$phase" "$class_mb" "$slot_index" "$project_id"
      slot_index=$((slot_index + 1))
    done
    class_index=$((class_index + 1))
  done
}

require_command findmnt
require_command mkfs.xfs
require_command mount
require_command truncate
require_command xfs_quota
ensure_positive_integer "$PHASE_SLOT_INODES_PER_MB" "AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB"
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
  ensure_phase_slots "$mount_path" "$phase"
done

if getent passwd "$SERVICE_USER" >/dev/null 2>&1 && getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
  chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${STATE_ROOT}/storage" "${STATE_ROOT}/challenges" "${STATE_ROOT}/runtime" "$PHASE_MOUNT_ROOT"
fi

cat <<EOF
DGX storage preparation completed.

Docker data root: $DOCKER_DATA_ROOT
Phase mount root: $PHASE_MOUNT_ROOT
Phase quota slot classes: $(phase_slot_classes)
Phase quota slots per class: $PHASE_SLOTS_PER_CLASS
Phase slot inode hard limit: ${PHASE_SLOT_INODES_PER_MB} inodes per MiB
EOF

if [ "$PERSIST_FSTAB" = "1" ]; then
  printf 'Idempotent /etc/fstab entries are present for the loopback mounts.\n'
else
  printf 'Set AGENTICS_DGX_PERSIST_FSTAB=1 to write idempotent /etc/fstab entries.\n'
fi
