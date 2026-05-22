# DGX Spark MVP Deployment Profile

This profile is the deployment target for a single DGX Spark host. It is Linux-only and assumes the host inventory summarized in `docs/dgx-spark/en.md` has been completed.

The `ExecStart=` paths in these service files are Linux systemd paths for the
DGX hosted profile. macOS rehearsal uses the foreground `cargo` and `bun`
commands documented in `docs/deployment/en.md`; do not reuse the DGX systemd
units as macOS startup definitions.

Operational commands referenced below are Rust binaries from the `agentics-ops`
package. Packaged deployments place them under `/opt/agentics/current/bin`; from
a source checkout, use `just dgx-profile ...` or
`cargo run -p agentics-ops --bin <binary> -- ...`.

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
  daemon. The worker binary enforces `AGENTICS_RUNNER_SECURITY_PROFILE` and
  `AGENTICS_HOST_PROBE_MODE` during startup.
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
| Developer test quota root | `/srv/agentics-test` |
| Runner writable storage mode | `xfs-project-quota-slots` |
| Runner quota slot classes | `64,256,1024,4096` MiB |
| Runner result/log caps | 12 runs, 4 MiB `result.json`, 1 MiB persisted logs per run |
| Runner security profile | `AGENTICS_RUNNER_SECURITY_PROFILE=production` |
| Probe mode | `AGENTICS_HOST_PROBE_MODE=require` |
| Worker accelerator capability | `AGENTICS_WORKER_ACCELERATORS=gpu` |
| GPU probe image | Digest-pinned `cu130` Agentics CUDA image |

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
2. Install the profile files and prepare storage:

   ```bash
   just dgx-profile install
   ```

   The install command copies the environment template, Docker daemon config,
   and systemd units, then prepares the Docker data-root loop image, per-phase
   XFS loop mounts, and root-prepared XFS project-quota slots.

3. Replace every placeholder secret and host name in
   `/etc/agentics/agentics.env`.
4. Start services:

   ```bash
   just dgx-profile start
   ```

5. Run the profile check:

   ```bash
  docker --host unix:///run/agentics/docker.sock pull busybox:1.36
  sudo -u agentics env \
    AGENTICS_HOST_PROBE_MODE=require \
    AGENTICS_RUNNER_SECURITY_PROFILE=production \
    AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots \
     AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime \
     AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
     AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096 \
     AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB=256 \
     AGENTICS_DGX_RUN_MUTATING_PROBES=1 \
     AGENTICS_DGX_DOCKER_PULL_POLICY=never \
     agentics-check-dgx-spark-profile
   ```

6. Run the hosted CLI onboarding smoke path and record DGX-3 evidence in
   `docs/dgx-spark/en.md`. For CUDA readiness, also run the image GPU smokes
   and the ignored `cuda_smoke` integration test documented there.

Stop or uninstall the profile with:

```bash
just dgx-profile stop
just dgx-profile uninstall
just dgx-profile uninstall --purge-data
```

Plain `uninstall` removes services and quota storage while preserving config,
release files, and durable state. `uninstall --purge-data` also removes
`/etc/agentics`, `/opt/agentics/current`, `/srv/agentics`,
`/srv/agentics-test`, and the `agentics` service identity.

Developer-run quota-sensitive integration tests should use a separate test
quota root rather than the hosted worker slots:

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_RUNTIME_ROOT=/srv/agentics-test/runtime
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics-test/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
```

The scripts intentionally fail on non-Linux hosts and do not infer strictness
from `CI=true`.
