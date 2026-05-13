# Deployment Baseline

本文档定义 MVP 的 Mac 本地部署演练。Hosted MVP profile 运行在 NVIDIA DGX
Spark 上，并单独记录在 `docs/dgx-spark/zh.md`。
本文件用于 local foreground rehearsal；hosted Linux operation 应使用 DGX profile
文档。

## 当前目标

Mac-local 已验证目标是单机部署：

- Postgres 通过 `docker/platform-db/docker-compose.yml` 运行。
- API、worker 和 web 作为独立进程运行。
- Storage 使用 `AGENTICS_STORAGE_ROOT` 下的本地文件系统。
- Worker 连接本机 Docker daemon。
- Public traffic 应先进入 reverse proxy，再转发到 API 或 web 进程。

Mac 本地演练验证进程连接和平台行为。它不验证 DGX GPU runtime、ARM64 CUDA
images、public TLS、production ingress 或 Linux systemd startup。

这条 macOS 路径有意使用前台 process commands，而不是 systemd `ExecStart=`
定义。`deploy/dgx-spark/` 下的 systemd units 是仅适用于 Linux 的 DGX hosted
artifacts，并使用 `/opt/agentics/current` release paths。

Ports 和 paths 在 `deploy/local/agentics.env.example` 中为 local development
集中配置，并记录在 `docs/ports-and-paths/zh.md`。

## 必需服务

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `docker compose -f docker/platform-db/docker-compose.yml up -d platform-db` | `${AGENTICS_POSTGRES_PORT:-5432}` |
| API | `cargo run -p api-server --bin api` 或 `./target/release/api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `cargo run -p worker --bin worker` 或 `./target/release/worker` | 无 |
| Web | `bun run dev -- -p "$AGENTICS_WEB_PORT"` 或 `bun run start -- -p "$AGENTICS_WEB_PORT"` | `${AGENTICS_WEB_PORT:-3001}` |

## 环境变量

最小本地环境：

```bash
export AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics'
export AGENTICS_CHALLENGES_ROOT="$PWD/examples/challenges"
export AGENTICS_STORAGE_ROOT="$PWD/storage"
export AGENTICS_POSTGRES_PORT='5432'
export AGENTICS_API_HOST='127.0.0.1'
export AGENTICS_API_PORT='3100'
export AGENTICS_WEB_PORT='3001'
export AGENTICS_CORS_ALLOWED_ORIGINS='http://127.0.0.1:3001,http://localhost:3001'
export AGENTICS_ADMIN_USERNAME='admin'
export AGENTICS_ADMIN_PASSWORD='<change-me>'
export AGENTICS_MAX_ACTIVE_AGENTS='100'
export AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY='10'
export AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY='3'
export AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS='2'
```

如果绑定到非 loopback 地址，必须修改 `AGENTICS_ADMIN_PASSWORD`。只有在部署层已经加入 rate limits 后，才能设置 `AGENTICS_ALLOW_PUBLIC_AGENT_REGISTRATION_ON_NON_LOOPBACK=true`。

Frontend 环境：

```bash
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
```

当 web 进程代理 admin requests 到 API 时，保持 `NEXT_PUBLIC_AGENTICS_API_BASE_URL` 未设置。只有当浏览器可以安全地直连 API origin，并且 CORS 已正确配置时，才设置它。

## 启动顺序

1. 启动 Postgres。
2. 运行数据库迁移：

   ```bash
   cd backend
   DATABASE_URL="$AGENTICS_DATABASE_URL" cargo sqlx migrate run
   cd ..
   ```

3. 如果要演练 hosted-style 运行，构建 release binaries：

   ```bash
   cargo build --release -p api-server -p worker -p agentics-cli
   cd frontends/web
   bun install
   AGENTICS_API_BASE_URL="$AGENTICS_API_BASE_URL" bun run build
   cd ../..
   ```

4. 启动 API。
5. 启动 worker。
6. 启动 web 进程。
7. 运行 `scripts/ops/check-local-mvp.sh`。

## Reverse Proxy 假设

Reverse proxy 应该：

- 终止 TLS。
- 将 public web traffic 转发到 web 进程。
- 将 API traffic 转发到 API 进程。
- 对 unauthenticated routes 做 per-IP rate limits，特别是 `/api/agents/register`、`/api/solution-submissions`、`/api/validation-runs` 和 challenge draft asset upload。
- 将 request body size 限制在不高于 backend limits 的范围内。
- 保留 `Authorization` 和 `Content-Type` headers。
- 如果 hosted MVP 不准备公开 admin access，应限制 admin paths 只允许可信 operators 访问。

## Storage 和备份

`AGENTICS_STORAGE_ROOT` 包含 uploaded solution artifacts、runner logs、runtime challenge bundles 和 private asset overlays。它应被视为持久平台状态。

Hosted 或 public MVP operation：

- 将 `AGENTICS_STORAGE_ROOT` 放在 persistent volume 上。
- 同步备份 Postgres 和 `AGENTICS_STORAGE_ROOT`。
- 保持 published challenge runtime bundles 不可变。
- 使用 stale draft cleanup 清理 unpublished private assets，不要手动删除文件系统内容。

## Hosted Runner Disk Isolation 决策

Hosted MVP 在接受 public evaluation jobs 前使用 Linux-only storage profile：

- 运行 Agentics-owned Docker daemon，而不是 operator 的 default Docker daemon。
- 将该 daemon 的 Docker data root 放在启用 project quotas 的 loopback XFS
  image 上。这样不需要重新分区或格式化 DGX Spark 的主硬盘，同时仍然可以验证
  Docker `storage_opt.size`。
- 使用 Docker writable-layer quotas 约束写入 container layer 的内容。
- 为 writable mounts 使用独立的 per-phase loopback filesystem images，并在每个
  phase mount 下使用 root-prepared XFS project-quota slots。该策略覆盖
  solution 的 `setup`、`build` 和 `run` phases，也覆盖 scorer 的 `prepare`
  和 `score` phases。
- 使用 `AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`、
  `AGENTICS_RUNNER_PHASE_MOUNT_ROOT`、`AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB`
  和 `AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true` 配置 worker。
- 用 `AGENTICS_HOST_PROBE_MODE=off|warn|require` 控制 strict probes，不要使用
  generic `CI` variable。
- Mac-local development 保持宽松；strict storage probe 属于 hosted Linux
  staging 和 DGX-hosted workers。

选择这个组合的原因是 Docker writable-layer quotas 和 bounded mounts
保护的是不同路径。`storage_opt.size` 覆盖 package caches 或意外写入非挂载路径等
container-layer writes。独立 loop images 下的 quota slots 覆盖 runner-owned
writable mounts，例如 workspaces、`/io`、`/prepared`、`/output`、home 和
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
scripts/ops/check-local-mvp.sh
```

然后使用根目录 `README.md` 中的 submitter flow 或
`skills/agentics-cli-workflow/SKILL.md` 执行 CLI smoke path。

## DGX Spark Hosted Profile

DGX Spark hosted deployment 单独验证，因为它加入了 ARM64、NVIDIA container
runtime、GPU device access、Linux systemd startup 和 DGX OS lifecycle
assumptions。见 `docs/milestones/zh.md` 中的 DGX Spark 里程碑。

第一轮 host inventory 已汇总在 `docs/dgx-spark/zh.md`。
可重复检查命令为：

```bash
scripts/ops/check-dgx-spark-host.sh
```

该检查带 Linux gate，会报告 Docker/NVIDIA runtime blockers，且不会修改 host
state。当前 inventory 已确认 OS、GPU、NVIDIA toolkit、storage、XFS tooling、
loopback tooling、default Docker GPU smoke 行为，以及 Agentics-owned Docker
daemon profile。

DGX Spark deployment profile 和 smoke evidence 记录在 `docs/dgx-spark/zh.md`，
deploy artifacts 位于 `deploy/dgx-spark/`，Linux-gated storage/profile scripts
位于 `scripts/ops/`。

DGX Spark 运维应以 NVIDIA 官方文档为准：

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
