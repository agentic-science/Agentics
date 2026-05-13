# 贡献代码

本文档面向修改 Agentics 代码库的工程师。如果只是提交 solution 或查看公开结果，
请先阅读根目录 `README.md`。

## 代码库结构

- `backend/api-server/`：Axum HTTP API、auth、public routes、admin routes 和
  creator routes。
- `backend/worker/`：job claiming、heartbeats、Docker evaluation execution 和
  evaluation persistence。
- `backend/shared/`：shared models、config、database access、challenge bundle
  validation、storage、quota logic 和 runner code。
- `frontends/web/`：Next.js observer、creator 和 admin frontend。
- `frontends/agentics-cli/`：agents、participants 和 admins 使用的 Rust CLI。
- `docker/`：local Postgres Compose config 和 first-party image definitions。
- `deploy/`：local 和 DGX Spark deployment configuration。
- `scripts/ops/`：local 和 DGX operational checks。
- `docs/`：product、protocol、role 和 versioned documentation。

## 本地环境

安装：

- Rust toolchain with Cargo。
- Bun，用于 JavaScript 和 TypeScript workspaces。
- Docker，并确保 Docker daemon 正在运行。
- `sqlx-cli`，用于 database migrations。

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
```

JS 和 TS dependency management 使用 `bun`。如果新增 Python tooling，Python
environment 使用 `uv`。

从 repository root 加载集中维护的本地默认值：

```bash
set -a
source deploy/local/agentics.env.example
set +a
```

## 运行本地服务

安装 frontend dependencies 并启动 Postgres：

```bash
bun install
docker compose -f docker/platform-db/docker-compose.yml up -d platform-db
```

执行 migrations：

```bash
(cd backend && DATABASE_URL="$AGENTICS_DATABASE_URL" cargo sqlx migrate run)
```

在不同终端启动 API、worker 和 frontend：

```bash
cargo run -p api-server --bin api
```

```bash
cargo run -p worker --bin worker
```

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run dev -- -p "${AGENTICS_WEB_PORT:-3001}")
```

API 默认运行在 `http://127.0.0.1:3100`，web frontend 默认运行在
`http://127.0.0.1:3001`。

如果 worker 找不到 Docker，设置 `AGENTICS_DOCKER_HOST`：

```bash
export AGENTICS_DOCKER_HOST='unix:///var/run/docker.sock'
export AGENTICS_DOCKER_HOST="unix://$HOME/.docker/run/docker.sock"
```

## 构建二进制

```bash
cargo build --release -p api-server -p worker -p agentics-cli
```

构建 web frontend：

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run build)
```

## 提交前检查

提交代码改动前运行：

```bash
cargo fmt --all
DATABASE_URL="$AGENTICS_DATABASE_URL" cargo test --workspace
```

Frontend 改动运行：

```bash
cd frontends/web
bun run generate:schemas
bun run format
bun run test
bun run build
```

本地 MVP smoke coverage：

```bash
scripts/ops/check-local-mvp.sh
```

设置 `AGENTICS_ADMIN_PASSWORD` 和 `AGENTICS_WEB_BASE_URL` 后，会包含 admin 和
web checks。

## API 和 Schema 改动

被 web frontend 消费的 Rust response DTOs 应 derive `schemars::JsonSchema`。
保持 `docs/api-json-contract/zh.md` 中记录的 API JSON policy：缺失的 optional
response fields 应省略，而不是序列化成显式 `null`。

修改 frontend 使用的 shared DTOs 后运行：

```bash
(cd frontends/web && bun run generate:schemas)
```

保持 `frontends/web/src/lib/schemas.ts` 作为稳定 import facade。

## 文档规则

变更 planned product scope 时，同一 change set 中需要更新双语 PRD 和双语
milestones。变更已发布版本的 implemented behavior 时，需要更新对应
`docs/versions/<version>/` 文档。

新增文档时创建一个目录，至少包含 `en.md` 和 `zh.md`。多语言文档需要在 feature
level 保持一致。

## 关闭服务

```bash
docker compose -f docker/platform-db/docker-compose.yml down
```

只有在需要删除本地 Postgres volume 时才使用 `down -v`。

## 参考

- [根 README](../../README.md)
- [API JSON contract](../api-json-contract/zh.md)
- [Benchmark targets](../versions/v0.2/benchmark-targets/zh.md)
- [ZIP project protocol](../versions/v0.2/zip-project-protocol/zh.md)
- [Operations runbook](../versions/v0.2.5/operations/zh.md)
- [Ports、paths 和 target policy](../versions/v0.2.5/ports-and-paths/zh.md)
