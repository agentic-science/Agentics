#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Linux" ]; then
  printf 'DGX Spark test storage preparation is Linux-only. Detected: %s\n' "$(uname -s)" >&2
  exit 2
fi

if [ "${AGENTICS_DGX_TEST_CONFIRM:-}" != "prepare-test-storage" ]; then
  cat >&2 <<'EOF'
Refusing to prepare DGX test storage without explicit confirmation.

Set AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage and run with privileges
to create directories, format loopback XFS images, and mount filesystems.

This script prepares a separate test quota root. It does not mutate the
production /srv/agentics phase-mount ownership.
EOF
  exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INVOKING_USER="${AGENTICS_DGX_TEST_USER:-${SUDO_USER:-${USER}}}"
INVOKING_GROUP="${AGENTICS_DGX_TEST_GROUP:-$(id -gn "$INVOKING_USER")}"
TEST_STATE_ROOT="${AGENTICS_DGX_TEST_STATE_ROOT:-/srv/agentics-test}"
PRODUCTION_STATE_ROOT="${AGENTICS_DGX_PRODUCTION_STATE_ROOT:-/srv/agentics}"

if [ "$TEST_STATE_ROOT" = "$PRODUCTION_STATE_ROOT" ]; then
  printf 'Refusing to use production state root as DGX test storage: %s\n' "$TEST_STATE_ROOT" >&2
  exit 2
fi

export AGENTICS_DGX_CONFIRM=prepare-storage
export AGENTICS_DGX_STATE_ROOT="$TEST_STATE_ROOT"
export AGENTICS_DGX_DOCKER_LOOP_SIZE="${AGENTICS_DGX_TEST_DOCKER_LOOP_SIZE:-32G}"
export AGENTICS_DGX_PHASE_LOOP_SIZE="${AGENTICS_DGX_TEST_PHASE_LOOP_SIZE:-8G}"
export AGENTICS_DGX_PHASE_SLOT_CLASSES_MB="${AGENTICS_DGX_TEST_PHASE_SLOT_CLASSES_MB:-64 256 1024 4096}"
export AGENTICS_DGX_PHASE_SLOTS_PER_CLASS="${AGENTICS_DGX_TEST_PHASE_SLOTS_PER_CLASS:-4}"
export AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB="${AGENTICS_DGX_TEST_PHASE_SLOT_INODES_PER_MB:-256}"
export AGENTICS_DGX_SERVICE_USER="$INVOKING_USER"
export AGENTICS_DGX_SERVICE_GROUP="$INVOKING_GROUP"
export AGENTICS_DGX_PERSIST_FSTAB="${AGENTICS_DGX_TEST_PERSIST_FSTAB:-0}"

"${SCRIPT_DIR}/prepare-dgx-spark-storage.sh"

cat <<EOF

DGX test runner environment:

export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_RUNTIME_ROOT=${AGENTICS_DGX_STATE_ROOT}/runtime
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=${AGENTICS_DGX_STATE_ROOT}/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=$(printf '%s' "$AGENTICS_DGX_PHASE_SLOT_CLASSES_MB" | tr ' ' ',')
# Slot inode quotas use ${AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB} inodes per MiB.

Use these variables when running quota-sensitive integration tests from the
${INVOKING_USER} account. Use AGENTICS_TEST_DOCKER_HOST only when the chosen
Docker daemon is accessible to that account.
EOF
