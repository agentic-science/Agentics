# DGX Spark MVP Deployment Profile

This profile is the `M0.2.5-DGX-2` deployment target for a single DGX Spark
host. It is Linux-only and assumes the host inventory summarized in
`docs/dgx-spark/en.md` has been completed.

The `ExecStart=` paths in these service files are Linux systemd paths for the
DGX hosted profile. macOS rehearsal uses the foreground `cargo` and `bun`
commands documented in `docs/deployment/en.md`; do not reuse the DGX systemd
units as macOS startup definitions.

## Files

- `agentics.env.example`: environment template for API, worker, web, and CLI
  smoke checks.
- `dockerd-agentics.json`: Docker daemon configuration for the Agentics-owned
  Docker daemon. The daemon disables Docker's default bridge with
  `"bridge": "none"`; public runner network policy must be explicit. The
  systemd unit also uses a separate containerd namespace from default Docker.
- `agentics-docker.service`: root-owned Docker daemon with a dedicated socket
  and loopback XFS data root.
- `agentics-api.service`: API server service.
- `agentics-worker.service`: worker service using the Agentics-owned Docker
  daemon.
- `agentics-web.service`: Next.js web service.

The MVP edge layer is Cloudflare-managed. These files intentionally do not ship
a separate reverse-proxy profile; application-level pioneer-code registration
gating remains the authoritative access control.

## Profile Defaults

| Purpose | Value |
| --- | --- |
| Release root | `/opt/agentics/current` |
| Config root | `/etc/agentics` |
| Persistent state root | `/srv/agentics` |
| Postgres port | `${AGENTICS_POSTGRES_PORT:-5432}` |
| API port | `${AGENTICS_API_PORT:-3100}` |
| Web port | `${AGENTICS_WEB_PORT:-3001}` |
| Storage root | `/srv/agentics/storage` |
| Challenge checkout root | `/srv/agentics/challenges` |
| Runtime root | `/srv/agentics/runtime` |
| Agentics Docker socket | `unix:///run/agentics/docker.sock` |
| Agentics Docker data root | `/srv/agentics/docker-data-root` |
| Docker data-root loop image | `/srv/agentics/loop-images/docker-data-root.xfs` |
| Per-phase mount root | `/srv/agentics/phase-mounts` |
| Runner writable storage mode | `xfs-project-quota-slots` |
| Runner quota slot classes | `64,256,1024,4096` MiB |
| Probe mode | `AGENTICS_HOST_PROBE_MODE=require` |

MVP deployment supports `linux-arm64-cpu` and `linux-arm64-cuda` targets on the
DGX Spark host. AMD64 Linux targets remain post-MVP until matching deployment
capacity exists.

## Operator Flow

1. Create the `agentics` service user and copy release artifacts to
   `/opt/agentics/current`:

   ```bash
   getent group agentics >/dev/null || groupadd --system agentics
   getent passwd agentics >/dev/null || useradd --system --gid agentics --home-dir /srv/agentics --shell /usr/sbin/nologin agentics
   ```
2. Copy `agentics.env.example` to `/etc/agentics/agentics.env` and replace every
   placeholder secret and host name.
3. Prepare the storage layout using the Linux-gated script:

   ```bash
   AGENTICS_DGX_CONFIRM=prepare-storage \
   AGENTICS_DGX_PERSIST_FSTAB=1 \
   AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096' \
   AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=4 \
   scripts/ops/prepare-dgx-spark-storage.sh
   ```

   The script creates the Docker data-root loop image, per-phase XFS loop
   mounts, and root-prepared XFS project-quota slots under each phase mount.
   The unprivileged worker leases those slots at runtime; it does not set quota
   limits itself.

4. Install `dockerd-agentics.json` as `/etc/agentics/dockerd-agentics.json`.
5. Install the service files under `/etc/systemd/system/`.
6. Start services in this order:

   ```bash
   systemctl daemon-reload
   systemctl enable --now agentics-docker.service
   systemctl start agentics-api.service
   systemctl start agentics-worker.service
   systemctl start agentics-web.service
   ```

7. Run the profile check:

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

8. Run the hosted CLI onboarding smoke path and record DGX-3 evidence in
   `docs/dgx-spark/en.md`.

The scripts intentionally fail on non-Linux hosts and do not infer strictness
from `CI=true`.
