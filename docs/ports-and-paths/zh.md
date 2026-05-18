# Ports、Paths 和 Target Policy

本文档是 runtime ports、filesystem paths 和 MVP target support 的 operator
reference。

## Port Defaults

| Surface | Env var | Default | Scope |
| --- | --- | --- | --- |
| Postgres host port | `AGENTICS_POSTGRES_PORT` | `5432` | Local Docker Compose 和 DGX rehearsal database access |
| API listen port | `AGENTICS_API_PORT` | `3100` | API process on loopback |
| Web listen port | `AGENTICS_WEB_PORT` | `3001` | Next.js web process on loopback |
| Public HTTPS | reverse proxy config | `443` | 仅 hosted ingress |

Foreground development 使用 `deploy/local/agentics.env.example`。DGX hosted
profile 将 `deploy/dgx-spark/agentics.env.example` 复制到
`/etc/agentics/agentics.env`。

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

DGX 默认 quota slot classes 为 `64`、`256`、`1024` 和 `4096` MiB，每个 phase
和 class 有四个 slots。Worker 会为 writable container bind mounts 租用这些
slots，并使用 Docker `storage_opt.size` 约束 container-layer writes。

`/srv/agentics-test` 用于开发者运行 quota-sensitive integration tests。它必须用
`scripts/ops/prepare-dgx-spark-test-storage.sh` 单独准备，且 hosted workers 不应使用。

Systemd units 仅适用于 Linux，并使用上述 release symlink paths。macOS
development 使用前台 `cargo` 和 `bun` commands。

## Base Image Source Paths

| Target | Source path |
| --- | --- |
| `linux-arm64-cpu` | `docker/images/linux-arm64-cpu` |
| `linux-arm64-cuda` | `docker/images/linux-arm64-cuda` |

在 AMD64 Linux deployment capacity 支持之前，不要添加 `linux-amd64-*` image
source paths。

## MVP Targets

MVP 的 platform deployment 支持：

- `linux-arm64-cpu`：DGX Spark 上的 Linux ARM64 CPU execution。
- `linux-arm64-cuda`：DGX Spark 上的 Linux ARM64 CUDA execution。

MVP 的 platform development 支持：

- `linux-arm64-cpu`
- `linux-arm64-cuda`
- `macos-arm64-cpu`，仅用于 local process rehearsal。

Solution submission 和 challenge creation targets 必须与 platform deployment
allowlist 对齐。`linux-amd64-cpu` 和 `linux-amd64-cuda` 保留给 post-MVP
扩展，前提是已有 AMD64 Linux deployment capacity。
