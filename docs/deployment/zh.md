# Deployment Baseline

本文档定义 MVP 的本地 Compose 部署演练，以及第一版单机 production Compose
stack。Hosted MVP target 运行在 NVIDIA DGX Spark 上，并单独记录在
`docs/dgx-spark/zh.md`。本文件用于 containerized local 和 production operation；
Linux host setup 应使用 DGX host-preparation 文档。

## 当前目标

Local 已验证目标是单机 Compose deployment：

- Postgres、API、worker 和 web 作为 Compose services 运行。
- Durable storage 默认使用 RustFS/S3。Local filesystem storage 只作为
  `AGENTICS_STORAGE_BACKEND=local` 的显式 escape hatch。
- Worker 连接 host Docker daemon，并创建 sibling runner containers。
- Public traffic 应先进入 reverse proxy，再转发到 API 或 web 进程。

Production Compose 目标是名为 `agentics-prod` 的单机 project：

- Postgres 和 RustFS 作为 Compose-managed durable services 运行。
- API、worker、checks 和 migrations 使用本地构建的 production app image。
- Web 使用本地构建的 production Next.js image，并由 Bun serve。
- API 和 web ports 绑定到 `AGENTICS_COMPOSE_BIND_IP`，默认是 `127.0.0.1`，
  因此 public ingress 和 TLS 保持在 Compose 外部。
- 只有 worker 和 check services 挂载 host Docker socket。

Local Compose rehearsal 验证 service wiring 和平台行为。它不验证 DGX GPU runtime、
ARM64 CUDA images、public TLS 或 production ingress。

## Runner Container Ownership

Agentics worker 会通过配置的 host Docker daemon 创建 solution、evaluator、
permission-repair 和 probe containers。这些 runner containers 是 host-level
sibling containers，不是 worker container 的子容器。因此，停止一个 Compose
project 不会自动删除 worker 创建的 runner containers。

每个 runner container 都必须带有精确的 Agentics labels，包括
`agentics.runner=zip_project`、`agentics.runner_scope` 和
`agentics.runner_namespace`。Compose project name 只隔离 Compose-owned
services、networks 和 volumes；它不会隔离通过共享 Docker socket 创建的 runner
containers。Runner reconciliation 和 cleanup 必须按配置的 namespace 过滤。

Local Compose defaults 位于 `deploy/compose/env/dev.env.example`。Production
Compose defaults 和 placeholders 位于 `deploy/compose/env/prod.env.example`。
Ports 和 paths 记录在 `docs/ports-and-paths/zh.md`。

## 必需服务

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `just compose-dev-up` service `postgres` | `dev.env.example` 中的 host port `55432` |
| API | `just compose-dev-up` service `api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `just compose-dev-up` service `worker` | 无 |
| Web | `just compose-dev-up` service `web` | `${AGENTICS_WEB_PORT:-3001}` |
| RustFS | `just compose-dev-up` 和 `just compose-prod-up` service `rustfs` | dev host ports `9000`/`9001`；production internal `9000`/`9001` |

## 环境变量

Local Compose environment source：

```bash
deploy/compose/env/dev.env.example
```

Production Compose environment source：

```bash
cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
```

Local 和 production Compose 默认都使用 `AGENTICS_STORAGE_BACKEND=s3`，并把 RustFS
配置为 `http://rustfs:9000`。启动 production 前必须替换所有 placeholder。External
S3 是 production 的 env-only override：修改 S3 endpoint、bucket、prefix、
force-path-style flag 和 credentials provider，不需要修改 Compose graph。

如果绑定到非 loopback 地址，必须修改 `AGENTICS_ADMIN_PASSWORD`。Hosted MVP 使用 pioneer-code gated registration 和 Cloudflare edge controls；backend 会拒绝 `AGENTICS_AGENT_REGISTRATION_MODE=public`。

Frontend 环境：

```bash
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
```

当 web 进程代理 admin requests 到 API 时，保持 `NEXT_PUBLIC_AGENTICS_API_BASE_URL` 未设置。只有当浏览器可以安全地直连 API origin，并且 CORS 已正确配置时，才设置它。

## 启动顺序

Local development：

1. 启动 Compose dev stack：

   ```bash
   just compose-dev-up
   ```

2. 在另一个 terminal 跟随 logs：

   ```bash
   just compose-dev-logs
   ```

3. 打开 `http://127.0.0.1:3001`。
4. 如果需要 web 和 admin checks，设置 `AGENTICS_WEB_BASE_URL` 和 admin
   credentials 后运行 `agentics-check-local-mvp`。
5. 用 `just compose-dev-down` 停止 stack。

Production Compose：

1. 准备 host-owned directories 和 runner quota storage：

   ```bash
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/runtime
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/phase-mounts
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/storage-work
   ```

2. 创建并编辑 production env file：

   ```bash
   cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
   ```

3. 构建并启动：

   ```bash
   just compose-prod-build
   just compose-prod-up
   ```

4. 运行 production checks 并查看 logs：

   ```bash
   just compose-prod-check
   just compose-prod-logs
   ```

5. 显式停止：

   ```bash
   just compose-prod-down --runner keep --dry-run
   just compose-prod-down --runner keep
   just compose-prod-down --runner clean --dry-run
   just compose-prod-down --runner clean
   ```

`--runner keep --dry-run` 和 `--runner clean --dry-run` 都不会停止 services。
`--runner keep` 会停止 Compose services 并保留 runner containers。
`--runner clean` 会先停止 worker services，只删除带精确 Agentics labels 的 production
runner containers，然后停止剩余 Compose stack。

## Reverse Proxy 假设

Production Compose stack 不包含 reverse proxy 或 TLS service。MVP edge layer 由
Cloudflare 或其他外部 ingress 管理。它应该：

- 终止 TLS。
- 将 public web traffic 转发到 web 进程。
- 将 API traffic 转发到 API 进程。
- 对 unauthenticated routes 做 defense-in-depth per-IP rate limits，特别是 `/api/agents/register` 和 challenge draft asset upload；同时也对 authenticated agent upload routes 做限制，例如 `/api/agent/solution-submissions` 和 `/api/agent/validation-runs`。
- 将 request body size 限制在不高于 backend limits 的范围内。
- 保留 `Authorization` 和 `Content-Type` headers。
- 如果 hosted MVP 不准备公开 admin access，应限制 admin paths 只允许可信 operators 访问。

对于 production Compose，将 `/healthz`、`/api/*`、`/admin/*` 等 API paths 转发到
`${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_API_PORT:-3100}`，并把 web traffic 转发到
`${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_WEB_PORT:-3001}`。

## Storage 和备份

Agentics durable storage 以 object key 为边界。它保存 uploaded solution ZIPs、
runner logs、private asset ZIP overlays、不可变的 private/public challenge bundle
archives、public statements，以及小型 creator/admin JSON artifacts。S3 mode 会把
object keys 存入配置的 bucket 和 prefix。Local mode 会把同样的 keys 映射到
`AGENTICS_STORAGE_ROOT` 下，但现在只作为 narrow local experiments 的显式 opt-in。
`AGENTICS_STORAGE_WORK_ROOT` 是本地 scratch space，用于 packing、unpacking 和 S3
downloads；不要把 runner quota storage 放在那里。

Dev、test 和 hosted object storage 都应使用 S3 或 RustFS-compatible storage：

```bash
export AGENTICS_STORAGE_BACKEND='s3'
export AGENTICS_S3_BUCKET='agentics'
export AGENTICS_S3_PREFIX='mvp'
export AGENTICS_S3_REGION='us-east-1'
export AGENTICS_S3_ENDPOINT_URL='https://s3.example.internal'
export AGENTICS_S3_FORCE_PATH_STYLE='true'
export AGENTICS_STORAGE_WORK_ROOT='/srv/agentics/storage-work'
```

Dev 和 production Compose 默认使用 RustFS 作为单机 S3-compatible durable storage
service。RustFS credentials 通过 `AGENTICS_RUSTFS_ACCESS_KEY` 和
`AGENTICS_RUSTFS_SECRET_KEY` 配置，并在 app services 内映射为 AWS SDK 环境变量。
Production RustFS data 位于 Compose named volume；需要和 Postgres 一起备份该
volume，或者在 deployment 前通过 env 切换到 external S3。

如果重复进行 MVP production rehearsal，并希望 stack rebuild 后备份 migrated challenge
private bundles，可以启动专用 RustFS backup compose service：

```bash
cp deploy/compose/env/rustfs-private-backup.env.example deploy/compose/env/rustfs-private-backup.env
just rustfs-private-backup-up
```

默认 store 在 `9100` 提供 S3，在 `9101` 提供 RustFS console，使用
`/srv/agentics/private-bundle-backups/rustfs-data` 保存 durable data，并创建
`migrated-challenge-private-bundles` bucket。这个 backup store 不是 Agentics
durable storage backend。当 production rehearsal 启动自己的 RustFS 或 S3 bucket
后，需要先把所需 private bundle objects 从这个 backup store 复制到 rehearsal
storage，再复用已经 migrated 的 challenge metadata。`just
rustfs-private-backup-down` 会停止 backup container，但不会删除 objects。

Credentials 只通过 AWS SDK provider chain 获取，例如环境变量或 instance profile。
不要把 S3 credentials 写入 Agentics DB rows 或 challenge specs。Agentics 仍会在
durable writes 前执行 object-size limits，并在 S3 upload 后验证 object length。

Hosted 或 public MVP operation：

- 按 storage provider policy 备份或复制 S3 bucket/prefix。
- 如果显式 opt into local mode，将 `AGENTICS_STORAGE_ROOT` 放在 persistent volume 上。
- 同步备份 Postgres 和 durable object storage。
- 保持 published private runtime bundles 和 public-only bundles 不可变。
- 使用 stale draft cleanup 清理 unpublished private assets，不要手动删除 objects。
- 使用 challenge draft cleanup 清理 stale unpublished private assets 和 stale Agentics
  `_tmp` objects。`AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS` 默认是 24 小时。S3
  lifecycle cleanup 应作为 stale `_tmp/` objects 的第二道防线；它们只是 promotion
  temporary keys，不应作为 durable records 长期保留。

## Hosted Runner Disk Isolation 决策

Hosted MVP 在接受 public evaluation jobs 前使用 Linux-only storage profile：

- 使用 `AGENTICS_DOCKER_SOCKET_PATH` 背后的 configured host Docker daemon。
- 如果需要 Docker writable-layer quotas，确保该 daemon 的 data root 和 storage
  driver 支持 Docker `storage_opt.size`。
- 使用 Docker writable-layer quotas 约束写入 container layer 的内容。
- 为 writable mounts 使用独立的 per-phase loopback filesystem images，并在每个
  phase mount 下使用 root-prepared XFS project-quota slots。该策略覆盖
  solution 的 `setup`、`build` 和 `run` phases，也覆盖 evaluator 的 `prepare`
  和 `score` phases。
- 使用 `AGENTICS_RUNNER_SECURITY_PROFILE=production`、
  `AGENTICS_WORKER_ACCELERATORS=gpu`、
  `AGENTICS_WORKER_GPU_PROBE_IMAGE`、
  `AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`、
  `AGENTICS_RUNNER_RUNTIME_ROOT`、`AGENTICS_RUNNER_PHASE_MOUNT_ROOT`、
  `AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB` 和
  `AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true` 配置 worker。
- 用 `AGENTICS_HOST_PROBE_MODE=off|warn|require` 控制 strict probes，不要使用
  generic `CI` variable。Production runner security 还要求
  `AGENTICS_HOST_PROBE_MODE=require` 和 digest-pinned images。
- Local Compose development 保持宽松；strict storage probe 属于 hosted Linux
  staging 和 DGX-hosted workers。

选择这个组合的原因是 Docker writable-layer quotas 和 bounded mounts
保护的是不同路径。`storage_opt.size` 覆盖 package caches 或意外写入非挂载路径等
container-layer writes。独立 loop images 下的 quota slots 覆盖 runner-owned
writable mounts，例如 workspaces、`/io`、`/setup`、`/output`、home 和
temporary directories。Worker 会选择可满足 effective phase `disk_limit_mb`
的最小 configured slot class；如果需要 exact hard phase limit，应让 resource
profiles 与 slot classes 对齐。二者共同覆盖所有 runner phases 的 hard writable
disk boundary。

## 回滚

安全回滚路径：

1. 停止 web、worker 和 API。
2. 恢复上一个 API 和 worker binaries。
3. 恢复上一个 web build。
4. 依次重启 API、worker 和 web。
5. 运行 `/healthz`、`/admin/capacity`、`/admin/service-heartbeats` 和一个 CLI status/list 命令。

MVP 演练期间不要手动回滚数据库迁移，除非该 migration 明确可逆，并且 storage snapshot 来自同一时间点。

Production Compose 下，普通 binary 或 image rollback 使用
`just compose-prod-down --runner keep`，让正在运行的 evaluations 后续由 worker
reconciliation 处理。只有当 operator 明确选择终止匹配的 production runner
containers 时，才使用 `just compose-prod-down --runner clean`。Dry-run 形式不会停止
services。

## 验证

运行：

```bash
agentics-check-local-mvp
```

Production Compose 使用：

```bash
just compose-prod-check
```

然后使用根目录 `README.md` 中的 submitter flow 或
`skills/agentics-cli-workflow/SKILL.md` 执行 CLI smoke path。

## DGX Spark Hosted Profile

DGX Spark host preparation 单独验证，因为它加入了 ARM64、Docker GPU device
access、XFS quota setup 和 DGX OS lifecycle assumptions。见
`docs/milestones/zh.md` 中的 DGX Spark 里程碑。

第一轮 host inventory 已汇总在 `docs/dgx-spark/zh.md`。
可重复检查命令为：

```bash
agentics-check-dgx-spark-host
```

该检查带 Linux gate，会报告 Docker/NVIDIA GPU blockers，且不会修改 host
state。当前 inventory 已确认 OS、GPU、NVIDIA toolkit、storage、XFS tooling、
loopback tooling、default Docker GPU smoke 行为，以及 configured host Docker
socket。

DGX Spark host preparation 和 smoke evidence 记录在 `docs/dgx-spark/zh.md`，
Linux-gated storage/profile binaries 位于 `agentics-ops`。

DGX Spark 运维应以 NVIDIA 官方文档为准：

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
