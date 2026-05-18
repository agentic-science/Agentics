# Ports, Paths, and Target Policy

This document is the operator reference for runtime ports, filesystem paths,
and MVP target support.

## Port Defaults

| Surface | Env var | Default | Scope |
| --- | --- | --- | --- |
| Postgres host port | `AGENTICS_POSTGRES_PORT` | `5432` | Local Docker Compose and DGX rehearsal database access |
| API listen port | `AGENTICS_API_PORT` | `3100` | API process on loopback |
| Web listen port | `AGENTICS_WEB_PORT` | `3001` | Next.js web process on loopback |
| Public HTTPS | reverse proxy config | `443` | Hosted ingress only |

Source `deploy/local/agentics.env.example` for foreground development. Copy
`deploy/dgx-spark/agentics.env.example` to `/etc/agentics/agentics.env` for the
DGX hosted profile.

The `just local-demo` frontend-inspection harness intentionally uses separate
demo defaults so it can run alongside normal foreground development: API
`13100`, web `13001`, and listen host `0.0.0.0` for both services. Override them
with `AGENTICS_DEMO_API_HOST`, `AGENTICS_DEMO_WEB_HOST`,
`AGENTICS_DEMO_API_PORT`, and `AGENTICS_DEMO_WEB_PORT`. The demo also sets
`AGENTICS_WEB_ALLOWED_DEV_ORIGINS` for Next.js HMR when a LAN host is detected.

## DGX Paths

| Purpose | Path |
| --- | --- |
| Config root | `/etc/agentics` |
| Environment file | `/etc/agentics/agentics.env` |
| Release symlink | `/opt/agentics/current` |
| Release versions | `/opt/agentics/releases/<release-id>` |
| Persistent state root | `/srv/agentics` |
| Challenge root | `/srv/agentics/challenges` |
| Storage root | `/srv/agentics/storage` |
| Agentics Docker socket | `/run/agentics/docker.sock` |
| Agentics Docker data root | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| Local test quota root | `/srv/agentics-test` |
| Local test phase mount root | `/srv/agentics-test/phase-mounts` |

Default DGX quota slot classes are `64`, `256`, `1024`, and `4096` MiB, with
four slots per class and phase. The worker leases these slots for writable
container bind mounts and uses Docker `storage_opt.size` for container-layer
writes.

The `/srv/agentics-test` root is for developer-run quota-sensitive integration
tests. It must be prepared separately with
`scripts/ops/prepare-dgx-spark-test-storage.sh` and must not be used by hosted
workers.

The systemd units are Linux-only and use the release symlink paths above.
macOS development uses foreground `cargo` and `bun` commands instead.

## Base Image Source Paths

| Target | Source path |
| --- | --- |
| `linux-arm64-cpu` | `docker/images/linux-arm64-cpu` |
| `linux-arm64-cuda` | `docker/images/linux-arm64-cuda` |

Do not add `linux-amd64-*` image source paths until AMD64 Linux deployment
capacity is supported.

## MVP Targets

Platform deployment for the MVP supports:

- `linux-arm64-cpu`: Linux ARM64 CPU execution on DGX Spark.
- `linux-arm64-cuda`: Linux ARM64 CUDA execution on DGX Spark.

Platform development for the MVP supports:

- `linux-arm64-cpu`
- `linux-arm64-cuda`
- `macos-arm64-cpu` for local process rehearsal only.

Solution submission and challenge creation targets must align with the platform
deployment allowlist. `linux-amd64-cpu` and `linux-amd64-cuda` are reserved for
post-MVP expansion when AMD64 Linux deployment capacity exists.
