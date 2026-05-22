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
- `ops/`：local 和 DGX workflows 使用的 Rust operational binaries。
- `docs/`：product、protocol、role 和 operations documentation。

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

## Frontend Demo Data

如果需要用确定性的 fake results 检查 observer frontend，运行：

```bash
just local-demo up
```

该命令会启动 local Postgres、重建 disposable `agentics_demo` database、执行
migrations、启动 API、为 example challenges 写入 fake public leaderboards 和
completed submissions，然后启动 Next.js frontend。它不会启动 worker，因为 demo
results 会直接写入 local database。

Local demo 会刻意使用与普通 foreground development 不同的 ports：API `13100`，
web `13001`。默认情况下两个服务都会 bind 到 `127.0.0.1`。使用
`just local-demo up --lan` 可以将 API 和 web frontend bind 到 `0.0.0.0`，方便
同一网络内的其他机器检查 frontend。LAN mode 下，如果脚本能检测到 LAN address，
它会同时打印 loopback 和 LAN URLs，并把 LAN host 加入 Next.js dev-server
allowed origins，保证 HMR 可用。

停止 demo processes：

```bash
just local-demo down
```

使用 `just local-demo down --db` 可以同时停止 local Postgres container。
使用 `just local-demo down --purge-data` 可以执行完整清理，并删除 generated demo
logs、seeded artifact ZIPs 和 local Postgres volume。

## 构建二进制

```bash
cargo build --release -p api-server -p worker -p agentics-cli -p agentics-ops
test -x target/release/agentics-check-dgx-spark-profile
```

构建 web frontend：

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run build)
```

## 提交前检查

先运行一次 `just setup-hooks` 安装 repository hook。该 hook 会委托给 Rust
`agentics-pre-commit` ops binary，并发运行相互独立的检查，并在每次非空 commit
前检查 human/agent docs policy 和 large-file threshold。

提交代码改动前运行：

```bash
cargo fmt --all
DATABASE_URL="$AGENTICS_DATABASE_URL" cargo test --workspace
```

Frontend 改动运行：

```bash
cd frontends/web
bun run generate:schemas
bun run generate:schemas:check
bun run format
bun run test
bun run build
```

本地 MVP smoke coverage：

```bash
agentics-check-local-mvp
```

设置 `AGENTICS_ADMIN_PASSWORD` 和 `AGENTICS_WEB_BASE_URL` 后，会包含 admin 和
web checks。

Rust change-risk coverage 使用 `cargo llvm-cov` 写出 LCOV，再用 `cargo crap`
排序复杂且覆盖不足的函数：

```bash
just rust-risk-unit
```

这个 unit/package workflow 会排除 `integration-tests` crate，因此不需要数据库或已
准备好的 DGX quota storage。LCOV 文件会写到
`target/llvm-cov/agentics-workspace.lcov`。

如果需要包含 DB-backed integration tests 的更完整信号，先启动本地 Postgres，
然后运行：

```bash
just infra-up
AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics_test' \
  just rust-risk-integration
```

`rust-risk-integration` 会先运行完整 Rust test set，包括 `#[ignore]` hardware
tests，然后再生成 CRAP report。它不跳过 quota-root 或 DGX CUDA smoke tests，
因此需要先准备 quota-sensitive 和 hardware test environment。设置
`AGENTICS_CRAP_TOP` 可以调整输出的 ranked functions 数量。

在 Linux DGX development hosts 上，quota-sensitive runner tests 需要一个由
测试用户拥有的 XFS quota root。使用与 production `/srv/agentics` runtime tree
分离的 test root：

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

然后用以下环境变量运行 quota-sensitive integration tests：

```bash
export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_RUNTIME_ROOT=/srv/agentics-test/runtime
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics-test/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
```

在 Linux 上，如果这些变量缺失、格式错误，或没有指向已准备好的 bounded test quota
root，quota-sensitive integration tests 会 fail fast。非 Linux hosts 会跳过这些
Linux-only quota probes。

这些 test variables 故意指向 `/srv/agentics-test`，这样本地验证不会改变
production runner slot ownership。

## API 和 Schema 改动

被 web frontend 消费的 Rust response DTOs 应 derive `schemars::JsonSchema`。
保持 `docs/api-json-contract/zh.md` 中记录的 API JSON policy：缺失的 optional
response fields 应省略，而不是序列化成显式 `null`；API errors 应使用嵌套的
`ErrorResponse { error: { code, message, details? } }` envelope。

修改 frontend 使用的 shared DTOs 后运行：

```bash
(cd frontends/web && bun run generate:schemas)
(cd frontends/web && bun run generate:schemas:check)
```

保持 `frontends/web/src/lib/schemas.ts` 作为稳定 import facade。

凡是 backend、worker、CLI 或 web 共同使用的 external contract validation，都应放在
`backend/shared/src/validation/`。Archive envelope checks、text limits、target
selection、public API query bounds、GitHub PR provenance 和 web schema exports
应在这里维护，不要在 handlers 或 frontend helpers 中重复实现。Database admission
controls 和 guarded state transitions 仍保留在拥有这些 durable invariants 的 DB/API
modules 中。

## 文档规则

变更 planned product scope 时，同一 change set 中需要更新双语 PRD 和双语
milestones。变更 implemented behavior 时，需要在同一 change set 中更新相关 current
docs。

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
- [Targets](../targets/zh.md)
- [Solution protocol](../solution-protocol/zh.md)
- [Operations runbook](../operations/zh.md)
- [Ports、paths 和 target policy](../ports-and-paths/zh.md)
- [Visual identity system](../visual-identity-system/zh.md)
- [Rust feature review reference](../new-rust-features-apis/en.md)
