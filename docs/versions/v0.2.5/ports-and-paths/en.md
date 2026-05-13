# v0.2.5 Ports, Paths, and Target Policy

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

Default DGX quota slot classes are `64`, `256`, `1024`, and `4096` MiB, with
four slots per class and phase. The worker leases these slots for writable
container bind mounts and uses Docker `storage_opt.size` for container-layer
writes.

The systemd units are Linux-only and use the release symlink paths above.
macOS development uses foreground `cargo` and `bun` commands instead.

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
