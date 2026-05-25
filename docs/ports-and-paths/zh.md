# Ports、Paths 和 Target Policy

本文档是 runtime ports、filesystem paths 和 MVP target support 的 operator
reference。

## Port Defaults

| Surface | Env var | Default | Scope |
| --- | --- | --- | --- |
| Compose dev Postgres host port | `AGENTICS_POSTGRES_PORT` | `deploy/compose/env/dev.env.example` 中的 `55432` | Local Compose development |
| API listen port | `AGENTICS_API_PORT` | `3100` | API service，默认 loopback |
| Web listen port | `AGENTICS_WEB_PORT` | `3001` | Next.js web service，默认 loopback |
| RustFS S3 test port | `AGENTICS_RUSTFS_PORT` | `9000` | Local Docker RustFS test service |
| RustFS console test port | `AGENTICS_RUSTFS_CONSOLE_PORT` | `9001` | Local Docker RustFS console |
| Public HTTPS | reverse proxy config | `443` | 仅 hosted ingress |

Local Compose development 读取 `deploy/compose/env/dev.env.example`。DGX hosted
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
| Storage work root | `/srv/agentics/storage-work` |
| Runner runtime root | `/srv/agentics/runtime` |
| Agentics Docker socket | `/run/agentics/docker.sock` |
| Agentics Docker data root | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| Local test quota root | `/srv/agentics-test` |
| Local test phase mount root | `/srv/agentics-test/phase-mounts` |

DGX 默认 quota slot classes 为 `64`、`256`、`1024` 和 `4096` MiB，每个 phase
和 class 有 100 个 slots。Worker 会为 writable container bind mounts 租用这些
slots，并使用 Docker `storage_opt.size` 约束 container-layer writes。Slots 还会按默认
每 MiB `256` 个 inodes 设置 inode hard limits：默认 classes 分别是 `16384`、
`65536`、`262144` 和 `1048576` 个 inodes。Evaluator-visible run trees 另行限制为
`8192` 个 files、`1024` 个 directories 和 `32` 层 depth。

`/srv/agentics-test` 用于开发者运行 quota-sensitive integration tests。它必须用
`agentics-prepare-dgx-spark-test-storage` 单独准备，且 hosted workers 不应使用。

## Durable Object Storage

`AGENTICS_STORAGE_BACKEND=local|s3` 选择 durable storage。Local mode 会把 object
keys 映射到 `AGENTICS_STORAGE_ROOT` 下。S3 mode 会把同样的 keys 存入
`AGENTICS_S3_BUCKET` 和可选 `AGENTICS_S3_PREFIX`；credentials 来自 AWS SDK
provider chain。`AGENTICS_STORAGE_WORK_ROOT` 是 host-local scratch，用于 bundle
archives、unpacked bundles 和 S3 downloads。Stale `_tmp/` durable objects 会在
`AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS` 后由 Agentics cleanup 清理，默认是 24 小时。

当前 object-key prefixes：

| Prefix | Contents |
| --- | --- |
| `solution-submissions/` | Uploaded solution ZIPs |
| `eval-artifacts/` | Runner logs 和 evaluation artifacts |
| `challenge-drafts/<draft-id>/private-assets/` | Uploaded private asset ZIP overlays |
| `challenge-bundles/` | 不可变 private challenge bundle tar archives |
| `challenge-public-bundles/` | 不可变 public-only challenge bundle tar archives |
| `challenge-statements/` | Public `statement.md` objects |
| `challenge-shortlists/` | Creator/admin shortlist JSON artifacts |
| `_tmp/` | Temporary write/promote objects；stale 后可安全过期 |

RustFS local testing 只通过 Docker：

```bash
just rustfs-up
just test-storage-s3
just rustfs-down
```

RustFS container 使用官方 `rustfs/rustfs` image 和 Docker named volume。`just
rustfs-up` helper 默认使用 `--network host`，因为部分 DGX Docker bridge profiles
会被有意关闭。如果 `AGENTICS_RUSTFS_PORT` 或
`AGENTICS_RUSTFS_CONSOLE_PORT` 被设置为非默认端口，且
`AGENTICS_RUSTFS_DOCKER_NETWORK` 未设置，helper 会切换到 bridge mode 并 publish 指定
ports。如果显式设置 `AGENTICS_RUSTFS_DOCKER_NETWORK=host`，custom ports 会被拒绝，因为
host networking 不能 remap ports。如果改用 bind mounts，RustFS container 以 UID
`10001` 运行，因此 host directory 必须允许该 UID 写入。

Systemd units 仅适用于 Linux，并使用上述 release symlink paths。Local development
使用 Compose dev stack。

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
- `macos-arm64-cpu`，仅用于 local Compose rehearsal。

Solution submission 和 challenge creation targets 必须与 platform deployment
allowlist 对齐。`linux-amd64-cpu` 和 `linux-amd64-cuda` 保留给 post-MVP
扩展，前提是已有 AMD64 Linux deployment capacity。
