# v0.2.5 MVP 部署基线

本文档定义当前在 Mac 本地演练的 MVP 部署基线。正式 hosted MVP 会运行在 NVIDIA DGX Spark 上，因此这个基线是保守的，公开上线前必须重新审视。

## 当前目标

当前已验证目标是单机部署：

- Postgres 通过 `docker/platform-db/docker-compose.yml` 运行。
- API、worker 和 web 作为独立进程运行。
- Storage 使用 `AGENTICS_STORAGE_ROOT` 下的本地文件系统。
- Worker 连接本机 Docker daemon。
- Public traffic 应先进入 reverse proxy，再转发到 API 或 web 进程。

Mac 本地演练验证进程连接和平台行为。它不验证 DGX GPU runtime、ARM64 CUDA images、public TLS 或 production ingress。

## 必需服务

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `docker compose -f docker/platform-db/docker-compose.yml up -d platform-db` | `5432` |
| API | `cargo run -p api-server --bin api` 或 `./target/release/api` | `3000` |
| Worker | `cargo run -p worker --bin worker` 或 `./target/release/worker` | 无 |
| Web | `bun run dev -- -p 3001` 或 `bun run start -- -p 3001` | `3001` |

## 环境变量

最小本地环境：

```bash
export AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics'
export AGENTICS_CHALLENGES_ROOT="$PWD/examples/challenges"
export AGENTICS_STORAGE_ROOT="$PWD/storage"
export AGENTICS_API_HOST='127.0.0.1'
export AGENTICS_API_PORT='3000'
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
export AGENTICS_API_BASE_URL='http://127.0.0.1:3000'
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

公开 MVP 之前：

- 将 `AGENTICS_STORAGE_ROOT` 放在 persistent volume 上。
- 同步备份 Postgres 和 `AGENTICS_STORAGE_ROOT`。
- 保持 published challenge runtime bundles 不可变。
- 使用 stale draft cleanup 清理 unpublished private assets，不要手动删除文件系统内容。

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

然后执行 `docs/versions/v0.2.5/hosted-cli-onboarding/zh.md` 中的 CLI smoke path。

## DGX Spark 后续工作

DGX Spark hosted deployment 必须单独验证，因为它加入了 ARM64、NVIDIA container runtime、GPU device access 和 DGX OS lifecycle assumptions。见 `docs/milestones/zh.md` 中的 DGX Spark 里程碑。

DGX Spark 运维应以 NVIDIA 官方文档为准：

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
