# 贡献代码

本文档面向修改 Agentics 代码库的工程师。如果只是提交 solution 或查看公开结果，
请先阅读根目录 `README.md`。

## 代码库结构

- `backend/api-server/`：Axum HTTP API、auth、public routes、admin routes 和
  creator routes。
- `backend/worker/`：job claiming、heartbeats、Docker evaluation execution 和
  evaluation persistence。
- `crates/domain/`、`crates/contracts/`、`crates/config/`、
  `crates/persistence/`、`crates/storage/`、`crates/services/` 和
  `crates/runner/`：typed domain values、external contracts、runtime
  configuration、SQLx persistence、durable object storage、state-changing
  services 和 execution backends 的内部 Rust crates。
- `frontends/web/`：Next.js observer、creator 和 admin frontend。
- `frontends/agentics-cli/`：agents、participants 和 admins 使用的 Rust CLI。
- `docker/`：first-party image definitions 和 test storage helpers。
- `deploy/`：Compose development/test 和 DGX Spark deployment configuration。
- `ops/`：local 和 DGX workflows 使用的 Rust operational binaries。
- `docs/`：product、protocol、role 和 operations documentation。

在进行较大的 backend、worker、CLI 或 runner 修改前，请先阅读
[Architecture](../architecture/zh.md)，了解下一步 crate boundaries 和 service-layer
refactor direction。

## 本地环境

安装：

- Rust toolchain with Cargo。
- Bun，用于 JavaScript 和 TypeScript workspaces。
- Docker，并确保 Docker daemon 正在运行。

JS 和 TS dependency management 使用 `bun`。如果新增 Python tooling，Python
environment 使用 `uv`。

## 容器化开发与本地测试迭代

开发时最简单的启动方式是 Compose dev stack：

```bash
just compose-dev-up
```

这个命令会启动 Postgres、执行 migrations、启动 API，写入用于 frontend
检查的确定性 fake challenges 和 completed submissions，然后启动 worker 和
Next.js frontend。Source files 会 bind mount 到 Rust 和 Bun containers 中，
因此平时改代码不需要同步或复制文件。Cargo build output、Bun dependencies 和
Postgres data 放在 Compose volumes 中；demo storage 和 runner work roots 默认放在
`.agentics-compose/dev/` 下。

Worker 会使用 host Docker socket 来创建 sibling runner containers。这些 runner
containers 会带上 `AGENTICS_RUNNER_NAMESPACE` label；只有在明确需要另一个 cleanup
namespace 时才覆盖它：

```bash
AGENTICS_RUNNER_NAMESPACE=agentics-dev-$USER just compose-dev-up
```

默认情况下，dev API 和 web ports 会 bind 到 `127.0.0.1`。如果要通过
Tailscale 或可信 LAN 从另一台机器访问 frontend，请只 bind 到对应的网络接口，
并允许浏览器实际使用的 hostname：

```bash
AGENTICS_COMPOSE_BIND_IP=100.x.y.z \
AGENTICS_WEB_BASE_URL=http://your-host.tailnet.ts.net:3001 \
AGENTICS_CORS_ALLOWED_ORIGINS=http://127.0.0.1:3001,http://localhost:3001,http://your-host.tailnet.ts.net:3001 \
AGENTICS_WEB_ALLOWED_DEV_ORIGINS=your-host.tailnet.ts.net \
just compose-dev-up
```

如果要在远程 hostname 上测试 auth flows，请使用 HTTPS，例如 Tailscale Serve。
当 API 可以被其他机器访问时，dev cookies 会被标记为 secure。

停止 dev stack：

```bash
just compose-dev-down
```

查看并持续跟随 logs：

```bash
just compose-dev-logs
```

本地 integration-test 迭代可以在容器中运行现有 Rust integration suite：

```bash
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just compose-test-docker-up
just compose-test-integration
```

该命令会启动 test-scoped Postgres service，并在 Rust container 内运行：

```bash
cargo test -p integration-tests -- --include-ignored
```

它使用 `unix:///srv/agentics-test/docker.sock` 上的专用 test Docker daemon，
其 data root 是 `/srv/agentics-test/docker-data-root`，因此 Docker layer quota
会在 overlay2 on XFS with `prjquota` 上测试，而不是依赖 workstation daemon。
先用 `agentics-prepare-dgx-spark-test-storage` 准备 Linux quota test root，再用上面的
rootful command 启动专用 daemon。Wrapper 会为每次运行使用唯一的 Compose project
和 runner namespace，并在 tests service 退出后删除 test-scoped Compose volumes。

## Frontend Demo Data

Compose dev stack 会在 web service 启动前写入确定性的 fake challenges、
public leaderboards 和 completed submissions：

```bash
just compose-dev-up
```

打开 frontend：

```text
http://127.0.0.1:3001
```

如果需要从另一台机器检查 frontend，请使用容器化开发章节中的 Tailscale/LAN
环境变量。

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

S3-compatible storage 改动需要通过 Docker 运行 RustFS-backed storage test：

```bash
just rustfs-up
just test-storage-s3
just rustfs-down
```

该测试使用官方 `rustfs/rustfs` image 和 Docker named volume。Agentics 仍会在写入
S3 前执行自己的 per-object byte limits。Dev、test 和 production 的 durable storage
默认都是 RustFS/S3；只有测试明确针对 local backend 时才使用 local filesystem storage。

Rust change-risk coverage 使用 `cargo llvm-cov` 写出 LCOV，再用 `cargo crap`
排序复杂且覆盖不足的函数：

```bash
just rust-risk-unit
```

这个 unit/package workflow 会排除 `integration-tests` crate，因此不需要数据库或已
准备好的 DGX quota storage。LCOV 文件会写到
`target/llvm-cov/agentics-workspace.lcov`。

如果需要包含 DB-backed integration tests 的更完整信号，提供一个明确的 disposable
PostgreSQL database URL，然后运行：

```bash
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
`crates/contracts/src/validation/`。Archive envelope checks、text limits、target
selection、public API query bounds、GitHub PR provenance 和 web schema exports
应在这里维护，不要在 handlers 或 frontend helpers 中重复实现。Database admission
controls 和 guarded state transitions 仍保留在拥有这些 durable invariants 的
persistence/services modules 中。

## 文档规则

变更 planned product scope 时，同一 change set 中需要更新双语 PRD 和双语
milestones。变更 implemented behavior 时，需要在同一 change set 中更新相关 current
docs。

新增文档时创建一个目录，至少包含 `en.md` 和 `zh.md`。多语言文档需要在 feature
level 保持一致。

## 关闭服务

```bash
just compose-dev-down
```

## 参考

- [根 README](../../README.md)
- [API JSON contract](../api-json-contract/zh.md)
- [Targets](../targets/zh.md)
- [Solution protocol](../solution-protocol/zh.md)
- [Operations runbook](../operations/zh.md)
- [Ports、paths 和 target policy](../ports-and-paths/zh.md)
- [Visual identity system](../visual-identity-system/zh.md)
- [Rust feature review reference](../new-rust-features-apis/en.md)
