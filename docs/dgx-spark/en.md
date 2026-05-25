# DGX Spark Host Preparation

This document is the DGX Spark operator reference for the production Compose
model. Agentics services are started by Docker Compose. DGX-specific work is
limited to host inventory, Docker/GPU readiness, runner quota storage
preparation, and strict profile checks for the Compose worker.

## Scope

The DGX Spark hosted target is Linux-only and supports the MVP hosted targets:

- `linux-arm64-cpu`
- `linux-arm64-cuda`

`linux-amd64-cpu` and `linux-amd64-cuda` remain post-MVP targets until AMD64
Linux deployment capacity exists.

The production service graph lives in `deploy/compose/compose.prod.yml` and is
operated through `agentics-compose-prod` or the `compose-prod-*` just recipes.

## Host Inventory

The first inventory was captured on `MapleSpark` on May 12-13, 2026.

| Area | Result |
| --- | --- |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) |
| Kernel | `6.17.0-1014-nvidia` |
| Architecture | `aarch64` |
| GPU | NVIDIA GB10 |
| NVIDIA driver | `580.142` |
| Driver-reported CUDA | `13.0` |
| NVIDIA container toolkit | `nvidia-container-toolkit 1.19.0-1`, `libnvidia-container1 1.19.0-1` |
| Production Docker socket | `${AGENTICS_DOCKER_SOCKET_PATH:-/var/run/docker.sock}` |
| Runner quota slots | 64 MiB, 256 MiB, 1 GiB, and 4 GiB XFS project-quota slots for each runner phase, 100 slots per class, with 256 inodes per MiB |

Run the repeatable Linux-gated inventory check on the DGX host:

```bash
agentics-check-dgx-spark-host
```

To include the NVIDIA Docker smoke check, use an operator account that can
access the intended Docker daemon:

```bash
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:13.0.1-base-ubuntu24.04 \
agentics-check-dgx-spark-host
```

The Rust checker uses the Docker API directly. Configure Docker access through
`AGENTICS_DOCKER_HOST` or the standard Docker socket environment; do not wrap it
in a Docker CLI script.

## Production Compose Configuration

Create the production env file from the Compose template:

```bash
cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
```

DGX production should keep these values aligned:

```bash
AGENTICS_COMPOSE_PROD_PROJECT=agentics-prod
AGENTICS_DOCKER_SOCKET_PATH=/var/run/docker.sock
AGENTICS_RUNTIME_UID=10001
AGENTICS_RUNTIME_GID=10001
AGENTICS_DOCKER_SOCKET_GID=<host-docker-socket-group-gid>
AGENTICS_WORKER_ACCELERATORS=gpu
AGENTICS_WORKER_GPU_PROBE_IMAGE=ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:8e3da4a65e297e3b1e9800da001fa2bbac9ed48453a6972117a0c3ad1d1eef13
AGENTICS_RUNNER_SECURITY_PROFILE=production
AGENTICS_HOST_PROBE_MODE=require
AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
```

Change `AGENTICS_ADMIN_PASSWORD` and all storage, database, OAuth, and RustFS
secrets before exposing the stack. Public ingress and TLS stay outside Compose.

## Persistent Layout

| Purpose | Path |
| --- | --- |
| State root | `/srv/agentics` |
| Storage work root | `/srv/agentics/storage-work` |
| Runtime root | `/srv/agentics/runtime` |
| Host Docker socket | `/var/run/docker.sock` by default |
| Docker data root prepared for quota-capable hosts | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |

The phase mount root has one loopback XFS mount for each writable runner class:

- `solution-setup`
- `solution-build`
- `solution-run`
- `evaluator-setup`
- `evaluator-score`

Production Compose uses RustFS/S3 for durable object storage. `AGENTICS_STORAGE_WORK_ROOT`
is local scratch for bundle packing, unpacking, and S3 downloads; runner quota
slots remain under `AGENTICS_RUNNER_PHASE_MOUNT_ROOT`.

## Storage Preparation

Run only on Linux and only with operator privileges:

```bash
AGENTICS_DGX_CONFIRM=prepare-storage \
AGENTICS_RUNTIME_UID=10001 \
AGENTICS_RUNTIME_GID=10001 \
AGENTICS_DGX_PERSIST_FSTAB=1 \
AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096' \
AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=100 \
AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB=256 \
agentics-prepare-dgx-spark-storage
```

The storage preparer refuses to run unless
`AGENTICS_DGX_CONFIRM=prepare-storage` is set. It creates the persistent
directory layout, formats missing loopback XFS images, mounts them with
`prjquota`, prepares quota slots under each phase mount, and chowns
Compose-visible writable roots to `AGENTICS_RUNTIME_UID:AGENTICS_RUNTIME_GID`.
Set `AGENTICS_DGX_PERSIST_FSTAB=1` to append idempotent `/etc/fstab` entries.

Production Compose uses the host Docker socket. If Docker writable-layer quotas
are required, the host Docker daemon behind that socket must use a storage
driver and data root that support `storage_opt.size`; Agentics no longer ships a
project-owned Docker daemon service for production.

## Profile Check

Run the non-mutating profile check first:

```bash
AGENTICS_DOCKER_HOST=unix:///var/run/docker.sock \
AGENTICS_DOCKER_SOCKET_PATH=/var/run/docker.sock \
AGENTICS_HOST_PROBE_MODE=warn \
AGENTICS_RUNNER_SECURITY_PROFILE=production \
agentics-check-dgx-spark-profile
```

After the phase mounts and Docker quota behavior are configured, run the strict
check with mutating probes:

```bash
docker pull busybox:1.36
AGENTICS_DOCKER_HOST=unix:///var/run/docker.sock \
AGENTICS_DOCKER_SOCKET_PATH=/var/run/docker.sock \
AGENTICS_HOST_PROBE_MODE=require \
AGENTICS_RUNNER_SECURITY_PROFILE=production \
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots \
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime \
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096 \
AGENTICS_DGX_RUN_MUTATING_PROBES=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
agentics-check-dgx-spark-profile
```

The worker runs the same profile checker during startup when
`AGENTICS_HOST_PROBE_MODE=warn` or `require`. With production security and
require mode, the worker fails closed if bounded runner storage, Docker quota
behavior, digest-pinned images, or GPU probing are not proven.

## Production Startup

Start the services through Compose:

```bash
just compose-prod-build
just compose-prod-up
just compose-prod-check
```

Stop the stack with an explicit runner policy:

```bash
just compose-prod-down --runner keep --dry-run
just compose-prod-down --runner keep
just compose-prod-down --runner clean --dry-run
just compose-prod-down --runner clean
```

`--runner clean` removes only matching production runner containers with exact
Agentics labels. It does not mutate database job state.

## CUDA And Integration Smokes

Run CUDA image GPU smokes with Docker GPU device requests:

```bash
for image in \
  ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu126-ubuntu24.04-v0.2.5@sha256:d2913c5e027e95b67ab4dea49fafd0e8b12a741ec11f125b6d3807c2ac662295 \
  ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:8e3da4a65e297e3b1e9800da001fa2bbac9ed48453a6972117a0c3ad1d1eef13 \
  ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu132-ubuntu24.04-v0.2.5@sha256:ce63970cfc2024d729d786c63d9c8e95e4b352a03e507358ff4a82987ccfd50e
do
  docker run --rm --gpus 1 --platform linux/arm64 \
    -e AGENTICS_GPU_SMOKE_REQUIRE_DEVICE=1 \
    "$image" /opt/agentics/smoke.sh
done
```

For developer-run integration tests on the DGX host, prepare a separate
test-owned quota root instead of reusing production runner slots:

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

Run quota-sensitive integration tests with:

```bash
export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_RUNTIME_ROOT=/srv/agentics-test/runtime
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics-test/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
```

On Linux, quota-sensitive integration tests fail fast when these variables are
missing, malformed, or do not point at the prepared `/srv/agentics-test` quota
root.

## Smoke Evidence

Strict DGX-2 profile verification and DGX-3 hosted application smoke passed on
`MapleSpark` on May 13, 2026. CUDA image publication and DGX GPU worker smoke
passed on `MapleSpark` on May 22, 2026.

The smoke covered local MVP health checks, strict DGX profile checks, hosted CLI
onboarding, matrix validation and official submission on `linux-arm64-cpu`,
no-egress runner enforcement, storage-quota escape failure, capacity, worker
heartbeat inspection, CUDA image GPU runtime, and the ignored `cuda_smoke`
integration path on `linux-arm64-cuda`.

## Launch Cutover

Remaining cutover work before public traffic:

- configure public ingress, DNS, and TLS outside Compose;
- keep `/admin` and admin API paths operator-restricted unless public admin
  access is intentionally allowed;
- use Cloudflare edge controls for TLS, routing, and unauthenticated route rate
  limits;
- use `just compose-prod-down --runner clean --dry-run` before destructive
  runner cleanup.

Use NVIDIA's DGX Spark documentation as the host and GPU operational reference:

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
