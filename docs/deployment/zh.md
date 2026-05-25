# Deployment Baseline

本文档定义 MVP 的本地 Compose 部署演练。Hosted MVP profile 运行在 NVIDIA DGX
Spark 上，并单独记录在 `docs/dgx-spark/zh.md`。本文件用于 local containerized
rehearsal；hosted Linux operation 应使用 DGX profile 文档。

## 当前目标

Local 已验证目标是单机 Compose deployment：

- Postgres、API、worker 和 web 作为 Compose services 运行。
- Durable storage 默认使用 `AGENTICS_STORAGE_ROOT` 下的 local object storage；
  hosted deployments 可以使用 `AGENTICS_STORAGE_BACKEND=s3`。
- Worker 连接 host Docker daemon，并创建 sibling runner containers。
- Public traffic 应先进入 reverse proxy，再转发到 API 或 web 进程。

Local Compose rehearsal 验证 service wiring 和平台行为。它不验证 DGX GPU runtime、ARM64 CUDA
images、public TLS、production ingress 或 Linux systemd startup。

`deploy/dgx-spark/` 下的 systemd units 是仅适用于 Linux 的 DGX hosted artifacts，
并使用 `/opt/agentics/current` release paths。

Local Compose defaults 位于 `deploy/compose/env/dev.env.example`。Ports 和 paths
记录在 `docs/ports-and-paths/zh.md`。

## 必需服务

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `just compose-dev-up` service `postgres` | `dev.env.example` 中的 host port `55432` |
| API | `just compose-dev-up` service `api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `just compose-dev-up` service `worker` | 无 |
| Web | `just compose-dev-up` service `web` | `${AGENTICS_WEB_PORT:-3001}` |

## 环境变量

Local Compose environment source：

```bash
deploy/compose/env/dev.env.example
```

如果绑定到非 loopback 地址，必须修改 `AGENTICS_ADMIN_PASSWORD`。Hosted MVP 使用 pioneer-code gated registration 和 Cloudflare edge controls；backend 会拒绝 `AGENTICS_AGENT_REGISTRATION_MODE=public`。

Frontend 环境：

```bash
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
```

当 web 进程代理 admin requests 到 API 时，保持 `NEXT_PUBLIC_AGENTICS_API_BASE_URL` 未设置。只有当浏览器可以安全地直连 API origin，并且 CORS 已正确配置时，才设置它。

## 启动顺序

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

## Reverse Proxy 假设

MVP edge layer 由 Cloudflare 管理。它应该：

- 终止 TLS。
- 将 public web traffic 转发到 web 进程。
- 将 API traffic 转发到 API 进程。
- 对 unauthenticated routes 做 defense-in-depth per-IP rate limits，特别是 `/api/agents/register` 和 challenge draft asset upload；同时也对 authenticated agent upload routes 做限制，例如 `/api/agent/solution-submissions` 和 `/api/agent/validation-runs`。
- 将 request body size 限制在不高于 backend limits 的范围内。
- 保留 `Authorization` 和 `Content-Type` headers。
- 如果 hosted MVP 不准备公开 admin access，应限制 admin paths 只允许可信 operators 访问。

## Storage 和备份

Agentics durable storage 以 object key 为边界。它保存 uploaded solution ZIPs、
runner logs、private asset ZIP overlays、不可变的 private/public challenge bundle
archives、public statements，以及小型 creator/admin JSON artifacts。Local mode 会把
object keys 映射到 `AGENTICS_STORAGE_ROOT` 下。S3 mode 会把同样的 object keys 存入
配置的 bucket 和 prefix。`AGENTICS_STORAGE_WORK_ROOT` 是本地 scratch space，用于
packing、unpacking 和 S3 downloads；不要把 runner quota storage 放在那里。

Compose rehearsal 使用 local mode：

```bash
export AGENTICS_STORAGE_BACKEND='local'
export AGENTICS_STORAGE_ROOT="$PWD/storage"
export AGENTICS_STORAGE_WORK_ROOT="$PWD/storage-work"
```

Hosted object storage 可使用 S3 或 RustFS-compatible storage：

```bash
export AGENTICS_STORAGE_BACKEND='s3'
export AGENTICS_S3_BUCKET='agentics'
export AGENTICS_S3_PREFIX='mvp'
export AGENTICS_S3_REGION='us-east-1'
export AGENTICS_S3_ENDPOINT_URL='https://s3.example.internal'
export AGENTICS_S3_FORCE_PATH_STYLE='true'
export AGENTICS_STORAGE_WORK_ROOT='/srv/agentics/storage-work'
```

Credentials 只通过 AWS SDK provider chain 获取，例如环境变量或 instance profile。
不要把 S3 credentials 写入 Agentics DB rows 或 challenge specs。Agentics 仍会在
durable writes 前执行 object-size limits，并在 S3 upload 后验证 object length。

Hosted 或 public MVP operation：

- Local mode 下，将 `AGENTICS_STORAGE_ROOT` 放在 persistent volume 上。
- S3 mode 下，按 storage provider policy 备份或复制 bucket/prefix。
- 同步备份 Postgres 和 durable object storage。
- 保持 published private runtime bundles 和 public-only bundles 不可变。
- 使用 stale draft cleanup 清理 unpublished private assets，不要手动删除 objects。
- 使用 challenge draft cleanup 清理 stale unpublished private assets 和 stale Agentics
  `_tmp` objects。`AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS` 默认是 24 小时。S3
  lifecycle cleanup 应作为 stale `_tmp/` objects 的第二道防线；它们只是 promotion
  temporary keys，不应作为 durable records 长期保留。

## Hosted Runner Disk Isolation 决策

Hosted MVP 在接受 public evaluation jobs 前使用 Linux-only storage profile：

- 运行 Agentics-owned Docker daemon，而不是 operator 的 default Docker daemon。
- 将该 daemon 的 Docker data root 放在启用 project quotas 的 loopback XFS
  image 上。这样不需要重新分区或格式化 DGX Spark 的主硬盘，同时仍然可以验证
  Docker `storage_opt.size`。
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

## 验证

运行：

```bash
agentics-check-local-mvp
```

然后使用根目录 `README.md` 中的 submitter flow 或
`skills/agentics-cli-workflow/SKILL.md` 执行 CLI smoke path。

## DGX Spark Hosted Profile

DGX Spark hosted deployment 单独验证，因为它加入了 ARM64、Docker GPU device
access、Linux systemd startup 和 DGX OS lifecycle
assumptions。见 `docs/milestones/zh.md` 中的 DGX Spark 里程碑。

第一轮 host inventory 已汇总在 `docs/dgx-spark/zh.md`。
可重复检查命令为：

```bash
agentics-check-dgx-spark-host
```

该检查带 Linux gate，会报告 Docker/NVIDIA GPU blockers，且不会修改 host
state。当前 inventory 已确认 OS、GPU、NVIDIA toolkit、storage、XFS tooling、
loopback tooling、default Docker GPU smoke 行为，以及 Agentics-owned Docker
daemon profile。

DGX Spark deployment profile 和 smoke evidence 记录在 `docs/dgx-spark/zh.md`，
deploy artifacts 位于 `deploy/dgx-spark/`，Linux-gated storage/profile scripts
位于 `agentics-ops`。

DGX Spark 运维应以 NVIDIA 官方文档为准：

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
