# DGX Spark Operations

This document is the current operator reference for running Agentics on a single
NVIDIA DGX Spark host. It replaces the older split inventory, deployment, and
smoke-evidence notes.

## Scope

The DGX Spark profile is Linux-only. The systemd units and storage scripts under
`deploy/dgx-spark/` must not be used as macOS startup definitions. macOS
rehearsal uses the foreground `cargo` and `bun` flow in
[deployment](../deployment/en.md).

MVP hosted deployment targets are:

- `linux-arm64-cpu`
- `linux-arm64-cuda`

`linux-amd64-cpu` and `linux-amd64-cuda` remain post-MVP targets until AMD64
Linux deployment capacity exists.

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
| Agentics Docker daemon | `unix:///run/agentics/docker.sock`, `overlay2` on XFS, data root `/srv/agentics/docker-data-root`, named `nvidia` runtime visible |
| Runner quota slots | 64 MiB, 256 MiB, 1 GiB, and 4 GiB XFS project-quota slots for each runner phase, four slots per class, with 256 inodes per MiB |

Run the repeatable Linux-gated inventory check on the DGX host:

```bash
scripts/ops/check-dgx-spark-host.sh
```

To include the NVIDIA Docker smoke check, use an operator account that can
access the intended Docker daemon:

```bash
AGENTICS_DGX_DOCKER_CLI='sudo -n docker' \
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:13.0.1-base-ubuntu24.04 \
scripts/ops/check-dgx-spark-host.sh
```

Do not run public Agentics jobs on the operator's default Docker daemon. The
Agentics-owned Docker daemon and root-prepared quota slots are the hosted
storage boundary for public jobs.

## Deployment Artifacts

Deployment artifacts live in `deploy/dgx-spark/`:

| File | Purpose |
| --- | --- |
| `agentics.env.example` | `/etc/agentics/agentics.env` template |
| `dockerd-agentics.json` | Agentics-owned Docker daemon config |
| `agentics-docker.service` | Root-owned Docker daemon service |
| `agentics-api.service` | API server systemd unit |
| `agentics-worker.service` | Worker systemd unit; the worker enforces the configured host probe mode |
| `agentics-web.service` | Web frontend systemd unit |

Linux-gated operational scripts:

| Script | Purpose |
| --- | --- |
| `scripts/ops/manage-dgx-spark-profile.sh` | Installs, starts, stops, and uninstalls the DGX systemd profile |
| `scripts/ops/prepare-dgx-spark-storage.sh` | Creates loopback XFS images, mounts them with project quotas, and prepares runner quota slots |
| `scripts/ops/prepare-dgx-spark-test-storage.sh` | Creates a separate `/srv/agentics-test` quota root owned by the invoking test user |
| `scripts/ops/check-dgx-spark-profile.sh` | Checks runtime profile, Docker runtime-root visibility, Docker quota behavior, phase mounts, and quota-slot probes |

## Persistent Layout

| Purpose | Path |
| --- | --- |
| Config root | `/etc/agentics` |
| Environment file | `/etc/agentics/agentics.env` |
| Release symlink | `/opt/agentics/current` |
| Release versions | `/opt/agentics/releases/<release-id>` |
| State root | `/srv/agentics` |
| Storage root | `/srv/agentics/storage` |
| Challenge checkout root | `/srv/agentics/challenges` |
| Runtime root | `/srv/agentics/runtime` |
| Agentics Docker socket | `/run/agentics/docker.sock` |
| Agentics Docker data root | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |

The phase mount root has one loopback XFS mount for each writable runner class:

- `solution-setup`
- `solution-build`
- `solution-run`
- `evaluator-prepare`
- `evaluator-score`

## Required Environment

Copy `deploy/dgx-spark/agentics.env.example` to
`/etc/agentics/agentics.env` and replace all placeholders.

Required hosted profile values:

```bash
AGENTICS_API_HOST=127.0.0.1
AGENTICS_API_PORT=3100
AGENTICS_WEB_PORT=3001
AGENTICS_API_BASE_URL=https://<public-hostname>
AGENTICS_WEB_BASE_URL=https://<public-hostname>
AGENTICS_CORS_ALLOWED_ORIGINS=https://<public-hostname>
AGENTICS_WEB_SESSION_COOKIE_SECURE=true
AGENTICS_DOCKER_HOST=unix:///run/agentics/docker.sock
AGENTICS_HOST_PROBE_MODE=require
AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
AGENTICS_RUNNER_MAX_OUTPUT_FILES=8192
AGENTICS_RUNNER_MAX_OUTPUT_DIRS=1024
AGENTICS_RUNNER_MAX_OUTPUT_DEPTH=32
AGENTICS_RUNNER_MAX_RUNS=12
AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES=4194304
AGENTICS_RUNNER_MAX_PUBLIC_RESULTS=1024
AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES=262144
AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION=16777216
AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS=2
```

Use a non-default `AGENTICS_ADMIN_PASSWORD` before exposing the hosted profile.
Prepare GitHub OAuth credentials before creator routes are exposed.

## Storage Preparation

Run only on Linux and only with operator privileges:

```bash
AGENTICS_DGX_CONFIRM=prepare-storage \
AGENTICS_DGX_PERSIST_FSTAB=1 \
AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096' \
AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=4 \
AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB=256 \
scripts/ops/prepare-dgx-spark-storage.sh
```

The script refuses to run unless `AGENTICS_DGX_CONFIRM=prepare-storage` is set.
It creates the persistent directory layout, formats missing loopback XFS images,
mounts them with `prjquota`, and prepares quota slots under each phase mount.
Set `AGENTICS_DGX_PERSIST_FSTAB=1` to append idempotent `/etc/fstab` entries.

On `MapleSpark`, the DGX-2 run mounted:

- `/srv/agentics/docker-data-root`, 200 GiB loopback XFS with `prjquota`.
- Five phase mounts, 20 GiB each, for solution setup/build/run and evaluator
  prepare/score.
- Four quota slots per class and phase for 64 MiB, 256 MiB, 1024 MiB, and
  4096 MiB limits. With the default `256` inodes per MiB, those slots have
  inode hard limits of 16384, 65536, 262144, and 1048576.

The worker chooses the smallest configured slot class that is at least the
effective phase `disk_limit_mb`. Align challenge resource profiles to slot
classes when an exact hard phase limit is required. The separate evaluator-visible
run tree cap defaults to 8192 files, 1024 directories, and depth 32; setup/build
dependency installs are governed by the XFS byte and inode quota instead. Each
evaluation may run at most 12 solution invocations. Persisted runner logs are
capped at one MiB per concrete run, `result.json` is capped at 4 MiB before
parsing, public evaluator feedback is capped at 1024 entries, and embedded evaluator
result logs are capped at 256 KiB. `piped_stdio` interaction traffic is capped
at 16 MiB in each direction.

## Service Startup

Install profile files and prepare storage:

```bash
just dgx-profile install
```

Replace placeholders in `/etc/agentics/agentics.env`, then start:

```bash
just dgx-profile start
```

Stop or uninstall the profile with:

```bash
just dgx-profile stop
just dgx-profile uninstall
just dgx-profile uninstall --purge-data
```

The worker process runs `scripts/ops/check-dgx-spark-profile.sh` during startup
when `AGENTICS_HOST_PROBE_MODE=warn` or `require`. With
`AGENTICS_HOST_PROBE_MODE=require`, the worker fails closed if the Linux host
profile is not proven or the probe script cannot run.

Plain `uninstall` removes services and quota storage while preserving config,
release files, and durable state. `uninstall --purge-data` also removes
`/etc/agentics`, `/opt/agentics`, `/srv/agentics`, `/srv/agentics-test`, and the
`agentics` service identity.

## Verification

Run the non-mutating profile check first:

```bash
AGENTICS_HOST_PROBE_MODE=warn \
scripts/ops/check-dgx-spark-profile.sh
```

After the Agentics-owned Docker daemon and phase mounts are configured, run the
strict check with mutating probes:

```bash
docker --host unix:///run/agentics/docker.sock pull busybox:1.36
sudo -u agentics env \
  AGENTICS_HOST_PROBE_MODE=require \
  AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots \
  AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime \
  AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
  AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096 \
  AGENTICS_DGX_RUN_MUTATING_PROBES=1 \
  AGENTICS_DGX_DOCKER_PULL_POLICY=never \
  scripts/ops/check-dgx-spark-profile.sh
```

For developer-run integration tests on the DGX host, prepare a separate
test-owned quota root instead of reusing production runner slots:

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  scripts/ops/prepare-dgx-spark-test-storage.sh
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

The `/srv/agentics-test` root is intentionally separate from `/srv/agentics` so
local test permissions do not mutate hosted worker slot ownership.

Then run:

```bash
AGENTICS_ADMIN_PASSWORD='<admin-password>' \
AGENTICS_WEB_BASE_URL='https://<public-hostname>' \
scripts/ops/check-local-mvp.sh
```

Finally, run the CLI submitter flow from the root `README.md` against the hosted
endpoint and inspect the submitter-private status with `agentics submissions
status`.

## Smoke Evidence

Strict DGX-2 profile verification and DGX-3 hosted application smoke passed on
`MapleSpark` on May 13, 2026.

The smoke covered:

- local MVP health checks,
- strict DGX profile checks,
- hosted CLI onboarding,
- matrix validation and official submission on `linux-arm64-cpu`,
- no-egress runner enforcement,
- storage-quota escape failure,
- capacity and worker heartbeat inspection.

The storage escape run failed as expected with the worker error
`phase exceeded disk limit: 100663583 > 67108864 bytes`. The failure was
contained to the job disk limit and did not exhaust host storage.

## Launch Cutover

Remaining cutover work before public traffic:

- keep `/opt/agentics/current` and `/etc/agentics/agentics.env` aligned with
  each promoted build,
- configure public ingress, DNS, and TLS,
- keep `/admin` and `/admin-api` operator-restricted unless public admin access
  is intentionally allowed,
- use Cloudflare edge controls for TLS, routing, and unauthenticated route
  rate limits. Application-level pioneer-code registration gating remains the
  primary registration control.

Use NVIDIA's DGX Spark documentation as the operational source of truth:

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
