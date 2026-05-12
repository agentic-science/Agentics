#!/usr/bin/env bash
set -u

section() {
  printf '\n## %s\n\n' "$1"
}

run() {
  printf '$ %s\n' "$*"
  "$@" 2>&1 || printf '[exit=%s]\n' "$?"
}

run_shell() {
  printf '$ %s\n' "$*"
  bash -lc "$*" 2>&1 || printf '[exit=%s]\n' "$?"
}

if [ "$(uname -s)" != "Linux" ]; then
  printf 'DGX Spark inventory checks are Linux-only. Detected: %s\n' "$(uname -s)" >&2
  exit 2
fi

section "Host"
run uname -a
run_shell 'sed -n "1,80p" /etc/os-release 2>/dev/null || true'
run uname -m

section "Storage"
run_shell 'findmnt -no SOURCE,TARGET,FSTYPE,OPTIONS / 2>&1 || true'
run_shell 'findmnt -no SOURCE,TARGET,FSTYPE,OPTIONS /home 2>&1 || true'
run_shell 'findmnt -no SOURCE,TARGET,FSTYPE,OPTIONS /var/lib/docker 2>&1 || true'
run_shell 'findmnt -t xfs -o SOURCE,TARGET,FSTYPE,OPTIONS 2>&1 || true'
run_shell 'lsblk -o NAME,TYPE,SIZE,FSTYPE,MOUNTPOINTS 2>&1 | sed -n "1,120p"'
run_shell 'df -h / /home /var/lib/docker 2>&1 || true'

section "XFS And Loopback Support"
run_shell 'command -v mkfs.xfs || true'
run_shell 'command -v xfs_quota || true'
run_shell 'command -v xfs_info || true'
run_shell 'command -v losetup || true'
run_shell 'command -v truncate || true'
run_shell 'grep -w xfs /proc/filesystems || true'
run_shell 'modinfo xfs 2>&1 | sed -n "1,40p" || true'

section "Docker"
run_shell 'id'
run_shell 'ls -l /var/run/docker.sock 2>&1 || true'
run_shell 'stat -c "%A %U %G %n" /var/run/docker.sock 2>&1 || true'
run_shell 'docker version --format "Client={{.Client.Version}} Server={{.Server.Version}} API={{.Server.APIVersion}}" 2>&1 || true'
run_shell 'docker info 2>&1 | sed -n "1,160p"'
run_shell 'docker context ls 2>&1 || true'

section "NVIDIA"
run_shell 'nvidia-smi 2>&1 | sed -n "1,120p"'
run_shell 'nvidia-container-cli --version 2>&1 || true'
run_shell 'dpkg -l "nvidia-container*" "libnvidia-container*" 2>/dev/null | sed -n "1,120p" || true'
run_shell 'docker info --format "{{json .Runtimes}}" 2>&1 || true'
run_shell 'nvidia-ctk runtime configure --runtime=docker --dry-run 2>&1 | sed -n "1,120p" || true'

section "NVIDIA Docker Smoke"
if [ "${AGENTICS_DGX_RUN_DOCKER_SMOKE:-0}" = "1" ]; then
  cuda_image="${AGENTICS_DGX_CUDA_IMAGE:-nvidia/cuda:13.0.0-base-ubuntu24.04}"
  pull_policy="${AGENTICS_DGX_DOCKER_PULL_POLICY:-never}"
  run docker run --rm --pull="$pull_policy" --gpus all "$cuda_image" nvidia-smi
else
  cat <<'EOF'
Skipped. Set AGENTICS_DGX_RUN_DOCKER_SMOKE=1 to run:

  docker run --rm --pull="${AGENTICS_DGX_DOCKER_PULL_POLICY:-never}" \
    --gpus all "${AGENTICS_DGX_CUDA_IMAGE:-nvidia/cuda:13.0.0-base-ubuntu24.04}" \
    nvidia-smi

Use an image that already exists locally or set AGENTICS_DGX_DOCKER_PULL_POLICY=missing.
EOF
fi
