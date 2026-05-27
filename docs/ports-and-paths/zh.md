# Ports、Paths 和 Target Policy

本文档是 runtime ports、filesystem paths 和 MVP target support 的 operator
reference。

## Port Defaults

| Surface | Env var | Default | Scope |
| --- | --- | --- | --- |
| Compose dev Postgres host port | `AGENTICS_POSTGRES_PORT` | `deploy/compose/env/dev.env.example` 中的 `55432` | Local Compose development |
| Production Compose bind address | `AGENTICS_COMPOSE_BIND_IP` | `127.0.0.1` | Production API 和 web host publishes |
| API listen port | `AGENTICS_API_PORT` | `3100` | API service，默认 loopback |
| Web listen port | `AGENTICS_WEB_PORT` | `3001` | Next.js web service，默认 loopback |
| Production RustFS S3 endpoint | `AGENTICS_S3_ENDPOINT_URL` | `http://rustfs:9000` | Internal production Compose storage |
| RustFS S3 test port | `AGENTICS_RUSTFS_PORT` | `9000` | Local Docker RustFS test service |
| RustFS console test port | `AGENTICS_RUSTFS_CONSOLE_PORT` | `9001` | Local Docker RustFS console |
| Persistent private-bundle backup RustFS S3 port | `AGENTICS_RUSTFS_BACKUP_API_PORT` | `9100` | LAN-accessible private bundle backup store |
| Persistent private-bundle backup RustFS console port | `AGENTICS_RUSTFS_BACKUP_CONSOLE_PORT` | `9101` | LAN-accessible private bundle backup console |
| Public HTTPS | reverse proxy config | `443` | 仅 hosted ingress |

Local Compose development 读取 `deploy/compose/env/dev.env.example`。Production
Compose 使用 `deploy/compose/env/prod.env`，从
`deploy/compose/env/prod.env.example` 复制得到。DGX-specific host settings 位于同一个
production Compose env file。

## DGX Paths

| Purpose | Path |
| --- | --- |
| Persistent state root | `/srv/agentics` |
| Storage work root | `/srv/agentics/storage-work` |
| Runner runtime root | `/srv/agentics/runtime` |
| Production Compose storage work root | `/srv/agentics/storage-work` |
| API container 内的 production challenge review checkout | `/srv/agentics/review-checkouts/agentics-challenges` |
| Production runner Docker socket | 默认 `/srv/agentics/docker.sock` |
| 为 quota-capable host 准备的 Docker data root | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| Local test quota root | `/srv/agentics-test` |
| Local test Docker socket | `/srv/agentics-test/docker.sock` |
| Local test runtime root | `/srv/agentics-test/runtime` |
| Local test phase mount root | `/srv/agentics-test/phase-mounts` |
| Persistent private-bundle backup RustFS data root | `/srv/agentics/private-bundle-backups/rustfs-data` |

DGX 默认 quota slot classes 为 `64`、`256`、`1024` 和 `4096` MiB，每个 phase
和 class 有 100 个 slots。Worker 会为 writable container bind mounts 租用这些
slots，并使用 Docker `storage_opt.size` 约束 container-layer writes。Slots 还会按默认
每 MiB `256` 个 inodes 设置 inode hard limits：默认 classes 分别是 `16384`、
`65536`、`262144` 和 `1048576` 个 inodes。MVP production Compose environment
会为 setup-heavy Frontier-CS migration rehearsals 额外准备 `8192`、`12288` 和
`16384` MiB slots。Evaluator-visible run trees 另行限制为
`8192` 个 files、`1024` 个 directories 和 `32` 层 depth。
Production runner containers 使用由 `just prod::runner-docker-up` 启动的
dedicated Docker daemon；它的默认 bridge network 由 host bridge `agentics0` 支撑。

`/srv/agentics-test` 用于开发者运行 quota-sensitive integration tests。它必须用
`agentics-prepare-dgx-spark-test-storage` 单独准备，且 hosted workers 不应使用。
`just test-env-up` 会在 `/srv/agentics-test/docker.sock` 启动专用 test Docker
daemon；`just test-all-cpu` 使用它运行 CPU-only Compose integration tests，而
`just test-all` 还要求 NVIDIA GPU support，并包含 ignored CUDA/GPU tests。

Production Compose 会把 standalone `agentics-challenges` checkout 从
`AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT` bind-mount 到
`AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT`。Challenge draft validation 和
publishing 的 admin `repository_path` 应使用这个 container path。Host checkout 必须
clean、位于 reviewed commit，并且 production API runtime user 可读。

## Durable Object Storage

`AGENTICS_STORAGE_BACKEND=s3` 是 dev、testing 和 production 的默认 durable storage
mode。它会把 object keys 存入 `AGENTICS_S3_BUCKET` 和可选
`AGENTICS_S3_PREFIX`；credentials 来自 AWS SDK provider chain。
`AGENTICS_STORAGE_BACKEND=local` 是显式 opt-in，会把 object keys 映射到
`AGENTICS_STORAGE_ROOT` 下。`AGENTICS_STORAGE_WORK_ROOT` 是 host-local scratch，用于
bundle archives、unpacked bundles 和 S3 downloads。Stale `_tmp/` durable objects 会在
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
just storage::rustfs-up
just storage::s3-test
just storage::rustfs-down
```

RustFS container 使用官方 `rustfs/rustfs` image 和 Docker named volume。`just
rustfs-up` helper 默认使用 `--network host`，因为部分 DGX Docker bridge profiles
会被有意关闭。如果 `AGENTICS_RUSTFS_PORT` 或
`AGENTICS_RUSTFS_CONSOLE_PORT` 被设置为非默认端口，且
`AGENTICS_RUSTFS_DOCKER_NETWORK` 未设置，helper 会切换到 bridge mode 并 publish 指定
ports。如果显式设置 `AGENTICS_RUSTFS_DOCKER_NETWORK=host`，custom ports 会被拒绝，因为
host networking 不能 remap ports。如果改用 bind mounts，RustFS container 以 UID
`10001` 运行，因此 host directory 必须允许该 UID 写入。

Persistent private-bundle backup store 与 storage test helper 分离，并且不是
Agentics durable storage backend：

```bash
cp deploy/compose/env/rustfs-private-backup.env.example deploy/compose/env/rustfs-private-backup.env
just storage::backup-up
```

它使用 `deploy/compose/compose.rustfs-private-backup.yml`，将 object data 保存在
`AGENTICS_RUSTFS_BACKUP_DATA_DIR` 下，并且 `just storage::backup-down` 停止时不会
删除 data。如果 production rehearsal 需要复用备份的 private challenge bundles，需要将
objects 从这个 backup bucket 复制到该 rehearsal 使用的 storage bucket：

```bash
just prod::restore-private-bundles
```

restore service 会写入 production bucket 中配置的 `AGENTICS_S3_PREFIX` 下，并使用
`private-bundle-backups/` logical prefix。

Production deployment 使用 Compose prod stack。Local development 使用 Compose dev
stack。

## Runner Image Source Paths

| Target | Source path |
| --- | --- |
| `linux-arm64-cpu` | `docker/runner-images/linux-arm64-cpu` |
| `linux-arm64-cuda` | `docker/runner-images/linux-arm64-cuda` |

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
