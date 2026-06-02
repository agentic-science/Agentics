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
- `docker/runner-images/`：public first-party runner image definitions，由
  targets 和 challenge specs 引用。
- `deploy/`：internal Compose development/test/production configuration，以及
  platform service image definitions。
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
just dev::up
```

Dev 和 test Compose stacks 中的 Rust services 使用内部
`agentics-rust-toolchain:bookworm-llvm22-local` image。该 image 由
`deploy/service-images/rust-toolchain/` 构建，并安装 Homebrew LLVM 22、Homebrew
`cargo-binstall` 和 Wild 0.9.0。它的 Cargo config 在 Linux ARM64 和 Linux AMD64
builds 中使用 `clang` 加 Wild。只有在明确测试另一个内部 toolchain image 时，才覆盖
`AGENTICS_RUST_TOOLCHAIN_IMAGE`。

这个命令会启动 Postgres、RustFS、API、worker 和 Next.js frontend，执行 migrations，
从 `challenge-repos/agentics-challenges/dev/challenges` 准备 local development
challenge catalog，并把
`challenge-repos/agentics-challenges/dev/test-solutions` 中匹配的 public test
solutions 作为 official submissions 写入队列。Local dev stack 不再启动，也不要求
persistent private-bundle backup RustFS service。Source files 会 bind mount 到 Rust
和 Bun containers 中，因此平时改代码不需要同步或复制文件。Cargo build output、Bun
dependencies 和 Postgres data 放在 Compose volumes 中；dev storage 和 runner work
roots 默认放在 `.agentics-compose/dev/` 下。

Dev database 名称是 `agentics_dev`。如果本地 Compose Postgres volume 是改名前创建的，
里面仍然有 `agentics_demo`，请先重置这个 disposable dev volume，再运行
`just dev::up`。

Local dev 使用 `AGENTICS_OFFICIAL_LOG_REDACTION=contract_based`。Public-only
dev challenges 的 official evaluations 会保留 runner diagnostics，因此缺少声明
output files 等失败应产生可操作的 logs。带 private benchmark data 或 official
setup-generated inputs 的 challenges 仍会 redacted。

Worker 会使用 host Docker socket 来创建 sibling runner containers。这些 runner
containers 会带上 `AGENTICS_RUNNER_NAMESPACE` label；只有在明确需要另一个 cleanup
namespace 时才覆盖它：

```bash
AGENTICS_RUNNER_NAMESPACE=agentics-dev-$USER just dev::up
```

Compose project name 会隔离 Compose-owned containers、networks 和 volumes。
它不会隔离通过 host Docker socket 创建的 runner containers，因此 runner cleanup
和 reconciliation 依赖 `AGENTICS_RUNNER_NAMESPACE`。

项目仍处于 pre-MVP 阶段，因此团队有时会在显式重置 baseline schema 时 squash
database migration history。发生 migration history reset 后，请先重建本地 dev/test
databases 或 Compose Postgres volumes，再重新运行 migrations；旧的
`_sqlx_migrations` rows 不会匹配新的 baseline checksums。

默认情况下，dev API 和 web ports 会 bind 到 `127.0.0.1`。如果要通过
Tailscale 或可信 LAN 从另一台机器访问 frontend，请只 bind 到对应的网络接口，
并允许浏览器实际使用的 hostname：

```bash
AGENTICS_COMPOSE_BIND_IP=100.x.y.z \
AGENTICS_WEB_BASE_URL=http://your-host.tailnet.ts.net:3001 \
AGENTICS_CORS_ALLOWED_ORIGINS=http://127.0.0.1:3001,http://localhost:3001,http://your-host.tailnet.ts.net:3001 \
AGENTICS_WEB_ALLOWED_DEV_ORIGINS=your-host.tailnet.ts.net \
just dev::up
```

如果要在远程 hostname 上测试 auth flows，请使用 HTTPS，例如 Tailscale Serve。
当 API 可以被其他机器访问时，dev cookies 会被标记为 secure。

停止 dev stack：

```bash
just dev::down
```

查看并持续跟随 logs：

```bash
just dev::logs
```

项目验证使用 Docker Compose test harness。先准备一次 Linux test storage root，然后启动专用
test Docker daemon：

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just test-env-up
```

CPU-only full suite：

```bash
just test-env-status-cpu
just test-all-cpu
```

在有 NVIDIA GPU support 的 Linux host 上运行 full suite，包括 ignored CUDA/GPU tests：

```bash
just test-env-status
just test-all
```

这两个 suite 都会启动 test-scoped Postgres 和 RustFS services，初始化 test S3
bucket，并在与 dev services 相同的内部 LLVM/Wild Rust toolchain image 中运行 Rust
integration crate。它们使用 `unix:///srv/agentics-test/docker.sock` 上的专用 test
Docker daemon，其 data root 是 `/srv/agentics-test/docker-data-root`，因此 Docker
layer quota 会在 overlay2 on XFS with `prjquota` 上测试，而不是依赖 workstation
daemon。Wrapper 会为每次运行使用唯一的 Compose project 和 runner namespace，并在
tests service 退出后删除 test-scoped Compose volumes。Cargo registry、Git 和
target caches 默认使用 persistent Docker volumes，这样重复的本地运行可以保持 warm
cache。设置 `AGENTICS_TEST_DISABLE_CARGO_CACHE=true` 可以运行 cold-cache
verification；使用 `just test-purge-cargo-cache` 可以删除 persistent Cargo cache
volumes。完成后只停止专用 test daemon：

```bash
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just test-env-down
```

任何通过 host Docker socket 创建 runner containers 的 container，都必须使用
host-visible paths。Runner runtime roots、storage work roots、challenge
materialization roots 和 quota slot paths 应该以 host Docker daemon 看到的同一个
absolute path 挂载进 worker 或 tests container。不要把稍后要 bind mount 到 runner
container 的内容放在 container-only `/tmp` 下。

## Frontend Dev Data

Compose dev stack 使用 local dev catalog 作为 source of truth。Web service 启动前，
它会发布 `challenge-repos/agentics-challenges/dev/challenges/` 下所有 eligible CPU
challenges，跳过任何仍然要求 private assets 的 configured challenge，并把
`challenge-repos/agentics-challenges/dev/test-solutions/` 中匹配的 workspace 作为
official test-solution submission 写入队列：

```bash
just dev::up
```

打开 frontend：

```text
http://127.0.0.1:3001
```

如果需要从另一台机器检查 frontend，请使用容器化开发章节中的 Tailscale/LAN
环境变量。

## 构建二进制

```bash
cargo build --release -p api-server -p worker -p agentics-cli -p agentics-ops -p agentics-pre-commit
test -x target/release/agentics-check-dgx-spark-profile
```

构建 web frontend：

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run build)
```

## 提交前检查

先运行一次 `just maintenance::setup-hooks` 安装 repository hook。该 hook 会委托给独立的
Rust `agentics-pre-commit` binary，从 Git index 读取 staged paths，并且只有 staged
commit 触及匹配文件时才运行 Rust、web、docs 和 large-file checks。Root hook 会把
submodule changes 视为 pointer updates，不会检查 `challenge-repos/agentics-challenges`
内部文件。

提交代码改动前运行 canonical full suite。仅当任务或环境明确不能覆盖 GPU tests 时，
才使用 CPU-only suite：

```bash
just test-all-cpu
# 在有 NVIDIA GPU support 的 Linux host 上：
just test-all
```

如果 SQLx 报告 migration version 或 checksum mismatch，说明这个本地数据库来自旧的
pre-MVP migration history。请 drop 并重建这个 disposable database，不要手动编辑
`_sqlx_migrations`。

Frontend 改动运行：

```bash
cd frontends/web
bun install --frozen-lockfile
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
just storage::rustfs-up
just storage::s3-test
just storage::rustfs-down
```

该测试使用官方 `rustfs/rustfs` image 和 Docker named volume。Agentics 仍会在写入
S3 前执行自己的 per-object byte limits。Dev、test 和 production 的 durable storage
默认都是 RustFS/S3；只有测试明确针对 local backend 时才使用 local filesystem storage。

Rust change-risk coverage 使用 `cargo llvm-cov` 写出 LCOV，再用 `cargo crap`
排序复杂且覆盖不足的函数：

```bash
just risk::unit
```

这个 unit/package workflow 会排除 `integration-tests` crate，因此不需要数据库或已
准备好的 DGX quota storage。LCOV 文件会写到
`target/llvm-cov/agentics-workspace.lcov`。

如果需要包含 DB-backed integration tests 的更完整信号，提供一个明确的 disposable
PostgreSQL database URL，然后运行：

```bash
AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics_test' \
  just risk::integration
```

`just risk::integration` 会先运行完整 Rust test set，包括 `#[ignore]` hardware
tests，然后再生成 CRAP report。它不跳过 quota-root 或 CUDA smoke tests，
因此需要先准备 quota-sensitive 和 Linux/NVIDIA hardware test environment。设置
`AGENTICS_CRAP_TOP` 可以调整输出的 ranked functions 数量。

在 Linux hosts 上，quota-sensitive runner tests 需要一个由测试用户拥有的 XFS quota
root。使用与 production `/srv/agentics` runtime tree 分离的 test root：

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

Canonical `just test-all-cpu` 和 `just test-all` 会为 Compose harness 设置匹配的
test runner paths。在 Linux 上，如果已准备好的 bounded test quota root 缺失或格式错误，
quota-sensitive integration tests 会 fail fast。

这些 test variables 故意指向 `/srv/agentics-test`，这样本地验证不会改变
production runner slot ownership。

## API 和 Schema 改动

被 web frontend 消费的 Rust response DTOs 应 derive `schemars::JsonSchema`。
保持 `docs/api-json-contract/zh.md` 中记录的 API JSON policy：缺失的 optional
response fields 应省略，而不是序列化成显式 `null`；API errors 应使用嵌套的
`ErrorResponse { error: { code, message, details? } }` envelope。

修改 frontend 使用的 shared DTOs 后运行：

```bash
(cd frontends/web && bun install --frozen-lockfile)
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
just dev::down
```

## 参考

- [根 README](../../README.md)
- [API JSON contract](../api-json-contract/zh.md)
- [Targets](../targets/zh.md)
- [Solution protocol](../solution-protocol/zh.md)
- [Operations runbook](../operations/zh.md)
- [Ports、paths 和 target policy](../ports-and-paths/zh.md)
- [Visual identity system](../visual-identity-system/zh.md)
- [Rust modernization reference](../../.agents/skills/full-code-review/references/rust-modernization.md)
