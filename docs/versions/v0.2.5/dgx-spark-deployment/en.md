# v0.2.5 DGX Spark Deployment Profile

This document defines the `M0.2.5-DGX-2` hosted MVP deployment profile for a
single NVIDIA DGX Spark host.

The profile is Linux-only. DGX scripts refuse to run on non-Linux hosts.
The `ExecStart=` commands in `deploy/dgx-spark/*.service` are therefore DGX
Linux systemd startup definitions. macOS rehearsal remains the foreground
`cargo` and `bun` process flow documented in
`docs/versions/v0.2.5/deployment/en.md`.

Port and path defaults are centralized in `deploy/dgx-spark/agentics.env.example`
and summarized in `docs/versions/v0.2.5/ports-and-paths/en.md`.

## Preconditions

- `M0.2.5-DGX-1` host inventory has been reviewed.
- An operator can manage systemd services, mounts, Docker daemon configuration,
  and reverse-proxy configuration.
- A non-default `AGENTICS_ADMIN_PASSWORD` is available.
- GitHub OAuth credentials are available before creator routes are exposed.
- Docker daemon access is available for the Agentics-owned Docker daemon at
  `unix:///run/agentics/docker.sock`.

The current `MapleSpark` inventory confirms the host OS, GPU, NVIDIA toolkit,
XFS tools, loopback tools, default Docker GPU smoke behavior, and the
Agentics-owned Docker daemon profile.

## Artifacts

The deployment artifacts live in `deploy/dgx-spark/`:

| File | Purpose |
| --- | --- |
| `agentics.env.example` | `/etc/agentics/agentics.env` template |
| `dockerd-agentics.json` | Agentics-owned Docker daemon config |
| `agentics-docker.service` | Root-owned Docker daemon service |
| `agentics-api.service` | API server systemd unit |
| `agentics-worker.service` | Worker systemd unit with profile preflight |
| `agentics-web.service` | Web frontend systemd unit |
| `nginx-agentics.conf.example` | Reverse-proxy shape and public route limits |

The Agentics-owned Docker daemon disables Docker's default bridge by setting
`"bridge": "none"`. Public runner execution should continue to use explicit
network policy, and DGX-3 must include the no-egress runner smoke before public
jobs are accepted. The systemd unit also sets a separate containerd namespace
for the Agentics daemon so it does not share default Docker's `moby` namespace.

The Linux-gated scripts are:

| Script | Purpose |
| --- | --- |
| `scripts/ops/prepare-dgx-spark-storage.sh` | Explicitly confirmed storage layout setup for loopback XFS images |
| `scripts/ops/check-dgx-spark-profile.sh` | Runtime profile checks, Docker quota probe, and phase-mount canary probe |

## Persistent Layout

| Purpose | Path |
| --- | --- |
| Release root | `/opt/agentics/current` |
| Config root | `/etc/agentics` |
| State root | `/srv/agentics` |
| Storage root | `/srv/agentics/storage` |
| Challenge checkout root | `/srv/agentics/challenges` |
| Runtime root | `/srv/agentics/runtime` |
| Agentics Docker data-root mount | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Docker data-root loop image | `/srv/agentics/loop-images/docker-data-root.xfs` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| API binary | `/opt/agentics/current/bin/api` |
| Worker binary | `/opt/agentics/current/bin/worker` |
| CLI binary | `/opt/agentics/current/bin/agentics` |
| Web build | `/opt/agentics/current/frontends/web/.next` |

The phase mount root contains one XFS loopback mount for each writable runner
class:

- `solution-setup`
- `solution-build`
- `solution-run`
- `scorer-prepare`
- `scorer-score`

Each phase mount contains root-prepared XFS project-quota slots. The worker
leases one slot for each writable container mount, binds only that slot's clean
`work` directory into Docker, and keeps the slot locked until phase output is
copied back to durable runner artifacts. Docker `storage_opt.size` bounds writes
that land in the container root filesystem.

## Environment

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
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
```

MVP deployment supports `linux-arm64-cpu` and `linux-arm64-cuda` targets on DGX
Spark. AMD64 Linux targets are reserved for post-MVP deployment expansion.

## Storage Preparation

Run only on Linux and only with operator privileges:

```bash
AGENTICS_DGX_CONFIRM=prepare-storage \
AGENTICS_DGX_PERSIST_FSTAB=1 \
AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096' \
AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=4 \
scripts/ops/prepare-dgx-spark-storage.sh
```

The script refuses to run unless `AGENTICS_DGX_CONFIRM=prepare-storage` is set.
It creates the persistent directory layout, formats missing loopback XFS images,
mounts them with `prjquota`, and prepares quota slots under each phase mount.
Set `AGENTICS_DGX_PERSIST_FSTAB=1` to append idempotent `/etc/fstab` entries for
the loopback mounts.

On `MapleSpark`, the DGX-2 run mounted:

- `/srv/agentics/docker-data-root`, 200 GiB loopback XFS with `prjquota`,
- `/srv/agentics/phase-mounts/solution-setup`, 20 GiB loopback XFS with
  `prjquota`,
- `/srv/agentics/phase-mounts/solution-build`, 20 GiB loopback XFS with
  `prjquota`,
- `/srv/agentics/phase-mounts/solution-run`, 20 GiB loopback XFS with
  `prjquota`,
- `/srv/agentics/phase-mounts/scorer-prepare`, 20 GiB loopback XFS with
  `prjquota`,
- `/srv/agentics/phase-mounts/scorer-score`, 20 GiB loopback XFS with
  `prjquota`.

For each phase mount, the default slot layout is:

- `slots/64mb/slot-001` through `slot-004`
- `slots/256mb/slot-001` through `slot-004`
- `slots/1024mb/slot-001` through `slot-004`
- `slots/4096mb/slot-001` through `slot-004`

The worker chooses the smallest configured slot class that is at least the
effective phase `disk_limit_mb`. If an exact hard phase limit is required, keep
challenge resource profiles aligned to the configured slot classes.

## Service Startup

Install files:

```bash
getent group agentics >/dev/null || groupadd --system agentics
getent passwd agentics >/dev/null || useradd --system --gid agentics --home-dir /srv/agentics --shell /usr/sbin/nologin agentics
install -d /etc/agentics /etc/systemd/system
install -m 0640 deploy/dgx-spark/agentics.env.example /etc/agentics/agentics.env
install -m 0644 deploy/dgx-spark/dockerd-agentics.json /etc/agentics/dockerd-agentics.json
install -m 0644 deploy/dgx-spark/*.service /etc/systemd/system/
```

Replace placeholders in `/etc/agentics/agentics.env`, then start:

```bash
systemctl daemon-reload
systemctl enable --now agentics-docker.service
systemctl start agentics-api.service
systemctl start agentics-worker.service
systemctl start agentics-web.service
```

The worker unit runs `scripts/ops/check-dgx-spark-profile.sh` before starting.
With `AGENTICS_HOST_PROBE_MODE=require`, the worker fails closed if the Linux
host profile is not proven.

## Release And Backup Paths

Release artifacts should be deployed under a versioned directory and promoted by
updating `/opt/agentics/current`:

```text
/opt/agentics/releases/<git-sha>/
/opt/agentics/current -> /opt/agentics/releases/<git-sha>
```

Back up these paths together with Postgres:

- `/srv/agentics/storage`
- `/srv/agentics/challenges`
- `/etc/agentics/agentics.env`
- `/etc/agentics/dockerd-agentics.json`
- `/etc/systemd/system/agentics-*.service`
- the release identifier behind `/opt/agentics/current`

Do not back up Docker container writable layers as authoritative platform
state. They are execution scratch space.

## Reverse Proxy

Use `deploy/dgx-spark/nginx-agentics.conf.example` as the shape for TLS
termination and routing. The reverse proxy must:

- terminate TLS,
- apply unauthenticated route rate limits,
- preserve `Authorization`, `Content-Type`, and forwarded headers,
- keep `/admin` and `/admin-api` operator-restricted unless public admin access
  is intentionally allowed.

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
  AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
  AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096 \
  AGENTICS_DGX_RUN_MUTATING_PROBES=1 \
  AGENTICS_DGX_DOCKER_PULL_POLICY=never \
  scripts/ops/check-dgx-spark-profile.sh
```

This verifies:

- Linux host gate,
- Agentics Docker socket target,
- XFS `prjquota` on Docker data root,
- XFS `prjquota` on phase mounts,
- Docker daemon access and `overlay2`,
- Docker writable-layer quota behavior through `--storage-opt size=16m`,
- phase writable-mount canary writes,
- root-prepared bounded runner quota slots,
- per-phase Docker bind-mount quota exhaustion using the 64 MiB probe slot.

Then run:

```bash
AGENTICS_ADMIN_PASSWORD='<admin-password>' \
AGENTICS_WEB_BASE_URL='https://<public-hostname>' \
scripts/ops/check-local-mvp.sh
```

Finally, run the hosted CLI onboarding smoke path from
`docs/versions/v0.2.5/hosted-cli-onboarding/en.md`.

For a dry-run deployment rehearsal, run database migrations with the DGX env,
start API, worker, and web through systemd, then run the health/profile checks
above with non-default admin credentials.

## Current Verification Status

Strict DGX-2 profile verification and DGX-3 hosted application smoke passed on
`MapleSpark` on May 13, 2026. The strict profile check ran as the `agentics`
service user:

```text
[agentics-dgx-check] running Linux DGX profile checks
[agentics-dgx-check] NVIDIA runtime is visible to the Agentics Docker daemon
[agentics-dgx-check] running Docker writable-layer quota probe
[agentics-dgx-check] Docker writable-layer quota probe failed with expected quota exhaustion
[agentics-dgx-check] running phase writable-mount canary probes
[agentics-dgx-check] DGX profile checks passed
```

The host-level pieces are installed for DGX-2: service user, loopback XFS
mounts, idempotent `/etc/fstab` entries, Agentics-owned Docker config, and
enabled `agentics-docker.service`.

DGX-3 installed a release under `/opt/agentics/current`, provided
`/etc/agentics/agentics.env`, started Postgres, API, worker, web, and
Agentics-owned Docker services, and completed the hosted smoke path. The smoke
evidence is recorded in `docs/versions/v0.2.5/dgx-spark-smoke/en.md` and covers:

- local MVP health checks,
- strict DGX profile checks,
- hosted CLI onboarding,
- matrix validation and official submission on `linux-arm64-cpu`,
- no-egress runner enforcement,
- storage-quota escape failure,
- capacity and worker heartbeat inspection.

DNS, TLS, public ingress, and final operator access policy remain launch
cutover work.

## Next Step

Use the DGX-3 smoke document as the baseline evidence for launch cutover, then
complete public ingress, DNS, TLS, and operator-only admin access.
