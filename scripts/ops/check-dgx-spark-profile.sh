#!/usr/bin/env bash
set -u

MODE="${AGENTICS_HOST_PROBE_MODE:-off}"
STATE_ROOT="${AGENTICS_DGX_STATE_ROOT:-/srv/agentics}"
DOCKER_DATA_ROOT="${AGENTICS_DGX_DOCKER_DATA_ROOT:-${STATE_ROOT}/docker-data-root}"
PHASE_MOUNT_ROOT="${AGENTICS_DGX_PHASE_MOUNT_ROOT:-${STATE_ROOT}/phase-mounts}"
DOCKER_HOST_URI="${AGENTICS_DOCKER_HOST:-unix:///run/agentics/docker.sock}"
PROBE_IMAGE="${AGENTICS_DGX_PROBE_IMAGE:-busybox:1.36}"
PULL_POLICY="${AGENTICS_DGX_DOCKER_PULL_POLICY:-never}"
PHASES="${AGENTICS_DGX_PHASES:-solution-setup solution-build solution-run scorer-prepare scorer-score}"
RUN_MUTATING_PROBES="${AGENTICS_DGX_RUN_MUTATING_PROBES:-0}"
DOCKER_CLI="${AGENTICS_DGX_DOCKER_CLI:-docker}"
read -r -a DOCKER_CMD <<<"$DOCKER_CLI"
if [ "${#DOCKER_CMD[@]}" -eq 0 ]; then
  DOCKER_CMD=(docker)
fi

failures=0
docker_available=0

log() {
  printf '[agentics-dgx-check] %s\n' "$*"
}

record_failure() {
  failures=$((failures + 1))
  printf '[agentics-dgx-check] ERROR: %s\n' "$*" >&2
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    record_failure "missing required command: $1"
    return 1
  fi
}

check_xfs_prjquota_mount() {
  local path="$1"
  local label="$2"
  local fstype
  local options

  fstype="$(findmnt -no FSTYPE --target "$path" 2>/dev/null || true)"
  options="$(findmnt -no OPTIONS --target "$path" 2>/dev/null || true)"
  if [ "$fstype" != "xfs" ]; then
    record_failure "$label must be on an XFS mount: $path"
    return
  fi
  case ",$options," in
    *,prjquota,*|*,pquota,*) ;;
    *) record_failure "$label XFS mount must include prjquota or pquota: $path" ;;
  esac
}

docker_cmd() {
  "${DOCKER_CMD[@]}" --host "$DOCKER_HOST_URI" "$@"
}

if [ "$MODE" = "off" ]; then
  log "AGENTICS_HOST_PROBE_MODE=off; skipping DGX profile checks"
  exit 0
fi

if [ "$MODE" != "warn" ] && [ "$MODE" != "require" ]; then
  record_failure "AGENTICS_HOST_PROBE_MODE must be off, warn, or require"
fi

if [ "$(uname -s)" != "Linux" ]; then
  record_failure "DGX Spark profile checks are Linux-only; detected $(uname -s)"
else
  log "running Linux DGX profile checks"
fi

require_command "${DOCKER_CMD[0]}"
require_command findmnt
require_command df

if [ "$DOCKER_HOST_URI" != "unix:///run/agentics/docker.sock" ]; then
  record_failure "AGENTICS_DOCKER_HOST should target the Agentics-owned daemon: unix:///run/agentics/docker.sock"
fi

check_xfs_prjquota_mount "$DOCKER_DATA_ROOT" "Agentics Docker data root"
for phase in $PHASES; do
  check_xfs_prjquota_mount "${PHASE_MOUNT_ROOT}/${phase}" "phase mount ${phase}"
done

if command -v "${DOCKER_CMD[0]}" >/dev/null 2>&1; then
  if ! docker_cmd info >/tmp/agentics-dgx-docker-info.$$ 2>&1; then
    record_failure "cannot query Agentics Docker daemon at $DOCKER_HOST_URI: $(tr '\n' ' ' </tmp/agentics-dgx-docker-info.$$)"
  else
    docker_available=1
    storage_driver="$(docker_cmd info --format '{{.Driver}}' 2>/dev/null || true)"
    if [ "$storage_driver" != "overlay2" ]; then
      record_failure "Agentics Docker daemon should use overlay2; got ${storage_driver:-<unknown>}"
    fi
    runtimes_json="$(docker_cmd info --format '{{json .Runtimes}}' 2>/dev/null || true)"
    case "$runtimes_json" in
      *nvidia*) log "NVIDIA runtime is visible to the Agentics Docker daemon" ;;
      *) log "NVIDIA runtime is not visible; acceptable while GPU execution remains disabled" ;;
    esac
  fi
  rm -f /tmp/agentics-dgx-docker-info.$$
fi

if [ "$RUN_MUTATING_PROBES" = "1" ]; then
  if [ "$docker_available" = "1" ]; then
    log "running Docker writable-layer quota probe"
    if docker_cmd run --rm --pull="$PULL_POLICY" --storage-opt size=16m --network none "$PROBE_IMAGE" sh -c 'dd if=/dev/zero of=/agentics-quota-probe bs=1M count=64' >/tmp/agentics-dgx-quota-probe.$$ 2>&1; then
      record_failure "Docker writable-layer quota probe unexpectedly succeeded"
    elif grep -Eiq 'no space left on device|disk quota exceeded' /tmp/agentics-dgx-quota-probe.$$; then
      log "Docker writable-layer quota probe failed with expected quota exhaustion"
    else
      record_failure "Docker writable-layer quota probe failed for an unexpected reason: $(tr '\n' ' ' </tmp/agentics-dgx-quota-probe.$$)"
    fi
    rm -f /tmp/agentics-dgx-quota-probe.$$
  else
    record_failure "cannot run Docker writable-layer quota probe without Agentics Docker daemon access"
  fi

  log "running phase writable-mount canary probes"
  for phase in $PHASES; do
    phase_path="${PHASE_MOUNT_ROOT}/${phase}"
    if [ ! -d "$phase_path" ]; then
      record_failure "phase mount path is missing: ${phase_path}"
      continue
    fi
    canary="${phase_path}/agentics-dgx-canary.$$"
    phase_error="$(mktemp)"
    if ! printf 'agentics\n' 2>"$phase_error" >"$canary"; then
      record_failure "cannot write canary to phase mount ${phase}: $(tr '\n' ' ' <"$phase_error")"
    else
      rm -f "$canary"
    fi
    rm -f "$phase_error"
  done
else
  log "skipping mutating probes; set AGENTICS_DGX_RUN_MUTATING_PROBES=1 to run Docker and phase-mount probes"
  if [ "$MODE" = "require" ]; then
    record_failure "AGENTICS_DGX_RUN_MUTATING_PROBES=1 is required when AGENTICS_HOST_PROBE_MODE=require"
  fi
fi

if [ "$failures" -gt 0 ]; then
  if [ "$MODE" = "warn" ]; then
    log "$failures DGX profile check(s) failed in warn mode"
    exit 0
  fi
  printf '[agentics-dgx-check] %s DGX profile check(s) failed\n' "$failures" >&2
  exit 1
fi

log "DGX profile checks passed"
