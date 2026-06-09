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
- Worker 连接 configured runner Docker daemon，并创建 sibling runner containers。
- Public traffic 应先进入 reverse proxy，再转发到 API 或 web 进程。

Production Compose 目标是名为 `agentics-prod` 的单机 project：

- Postgres 和 RustFS 作为 Compose-managed durable services 运行。
- API、worker、checks 和 migrations 使用本地构建的 production app image。它的
  builder stage 会安装内部 Homebrew LLVM 22 加 Wild Rust toolchain；最终 runtime
  image 只包含已构建的 binaries 和 runtime packages。
- Web 使用本地构建的 production Next.js image，并由 Bun serve。
- API 和 web ports 绑定到 `AGENTICS_COMPOSE_BIND_IP`，默认是 `127.0.0.1`，
  因此 public ingress 和 TLS 保持在 Compose 外部。
- 只有 worker 和 check services 挂载 host Docker socket。

Local Compose rehearsal 验证 service wiring 和平台行为。它不验证 DGX GPU runtime、
ARM64 CUDA images、public TLS 或 production ingress。

## Runner Container Ownership

Agentics worker 会通过配置的 runner Docker daemon 创建 solution、evaluator、
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
| Postgres | `just dev::up` service `postgres` | `dev.env.example` 中的 host port `55432` |
| API | `just dev::up` service `api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `just dev::up` service `worker` | 无 |
| Web | `just dev::up` service `web` | `${AGENTICS_WEB_PORT:-3001}` |
| RustFS | `just dev::up` 和 `just prod::up` service `rustfs` | dev host ports `9000`/`9001`；production internal `9000`/`9001` |

## 环境变量

Local Compose environment source：

```bash
deploy/compose/env/dev.env.example
```

Production Compose environment source：

```bash
cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
```

日常运维使用 `just prod::*` recipes 或 `agentics-compose-prod` wrapper。Env
example 也设置了 `AGENTICS_COMPOSE_PROD_SERVICE_ENV_FILE=./env/prod.env`，
这样直接用 Docker Compose inspection 时会加载同一个 service env file，而不是
placeholder template。

Local 和 production Compose 默认都使用 `AGENTICS_STORAGE_BACKEND=s3`，并把 RustFS
配置为 `http://rustfs:9000`。启动 production 前必须替换所有 placeholder。External
S3 是 production 的 env-only override：修改 S3 endpoint、bucket、prefix、
force-path-style flag 和 credentials provider，不需要修改 Compose graph。

Rust services 会在 startup 时验证 environment values。格式错误的
`AGENTICS_POSTGRES_PORT`、`AGENTICS_API_PORT` 和 `AGENTICS_WEB_PORT` 会让 startup 失败，
而不是回退到 local defaults。启用 host probing 时，`AGENTICS_HOST_PROBE_COMMAND`
必须是非空值。

Stage env examples 中的 environment variables 是 startup contract 的一部分。每个新增
或重命名的变量都必须有对应的 validation code：required values 未设置、为空，或在
hosted stage 仍使用 placeholder 时会 fail fast；optional values 未设置时会打印包含
默认值的 startup warning；deprecated names 会被拒绝，或明确 warning 为 ignored。

如果绑定到非 loopback 地址，backend 会拒绝
`AGENTICS_AGENT_REGISTRATION_MODE=public`。Hosted MVP 使用 pioneer-code gated
registration 和 Cloudflare edge controls。通过
`AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS` bootstrap 第一个 admin，然后在 admin
console 中创建给 operator automation 使用的 admin service tokens。
Human browser login 是基于 GitHub App user-authorization flow 的 GitHub sign-in。
需要配置 `AGENTICS_GITHUB_APP_CLIENT_ID`、
`AGENTICS_GITHUB_APP_CLIENT_SECRET` 和 `AGENTICS_GITHUB_APP_REDIRECT_URL`；
production 的 redirect URL 应设置为 public web origin 加
`/auth/github/callback`。Production 保持
`AGENTICS_WEB_SESSION_COOKIE_SECURE=true`。HTTP GitHub App redirects 只允许用于
loopback local development 或 rehearsal callbacks；non-loopback redirects 必须使用
HTTPS。

Frontend 环境：

```bash
export AGENTICS_DEPLOYMENT_STAGE='production'
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export AGENTICS_WEB_PORT='3001'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
export NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID=''
```

当 web 进程代理 admin requests 到 API 时，保持 `NEXT_PUBLIC_AGENTICS_API_BASE_URL` 未设置。只有当浏览器可以安全地直连 API origin，并且 CORS 已正确配置时，才设置它。
保持 `NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID` 未设置会完全禁用 Google
Analytics。设置为 `G-XXXXXXXXXX` 这类 GA4 measurement id 后，web app 仍只会在访客接受
analytics cookies 后加载 Google Analytics。
Frontend URL 和 port environment values 格式错误时，也会在 Next.js config/module
loading 时失败，而不会被静默 normalize。

## 启动顺序

Local development：

1. 启动 Compose dev stack：

   ```bash
   just dev::up
   ```

   这个 recipe 会启动本地 Postgres、RustFS、API、worker 和 web services，从
   `challenge-repos/agentics-challenges/dev/challenges` 准备 dev challenge
   catalog，并写入匹配的 public test solutions。它不会启动，也不要求 persistent
   private-bundle backup RustFS service。

   Dev database 名称是 `agentics_dev`。如果旧的 Compose volume 里仍然有
   `agentics_demo`，请先重置这个 disposable local dev volume，再启动 stack。

   Dev、test 和 production rehearsal Compose 运行 PostgreSQL 18，image 为
   `postgres:18-alpine`，Postgres data 挂载到 `/var/lib/postgresql`，并设置
   `io_method=io_uring`。这些 disposable environments 的 Postgres service 还会使用
   `seccomp=unconfined`，因为当前 Docker 默认 seccomp profile 会阻止 PG 18
   `io_uring`。Production 在完成下文记录的 dump/restore cutover 前仍使用 PostgreSQL 16。

2. 在另一个 terminal 跟随 logs：

   ```bash
   just dev::logs
   ```

3. 打开 `http://127.0.0.1:3001`。
4. 如果需要 web 和 admin checks，设置 `AGENTICS_WEB_BASE_URL` 和 admin
   credentials 后运行 `agentics-check-local-mvp`。
5. 用 `just dev::down` 停止 stack。

Production Compose：

1. 准备 host-owned directories 和 runner quota storage：

   ```bash
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/runtime
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/phase-mounts
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/storage-work
   sudo install -d -m 0755 /srv/agentics/review-checkouts
   ```

2. 创建并编辑 production env file：

   ```bash
   cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
   ```

   Production 和 rehearsal app images 会把
   `challenge-repos/agentics-challenges/challenges` 中的 public migrated
   challenge catalog 打包到 `/app/challenges`，并默认让
   `AGENTICS_CHALLENGES_ROOT` 指向这里。在 fresh object storage 上启动 API
   前，请先运行 `just prod::restore-private-bundles`，或对 rehearsal 运行
   `just rehearsal::restore-private-bundles --overwrite`，这样 startup seeding
   才能把已恢复的 private benchmark ZIP overlays 合并进 runtime bundles，同时不把
   private data commit 进仓库。

   Challenge review record validation 和 publishing 在 API container 内运行。请在
   `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT` 保留一个 clean、standalone、runtime-readable
   的 `agentics-challenges` checkout，并在 validate 或 publish review record 时把
   `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT` 作为 admin
   `repository_path` 传入。

3. 构建并启动：

   ```bash
   just prod::build
   sudo just prod::runner-docker-up
   just prod::up
   ```

   `just prod::up` 会在 Docker 创建默认 network 后检查 production Compose bridge 的
   forwarding rules。在 Linux host 上，它会安装幂等的 `DOCKER-USER` rules，允许
   Compose bridge 访问 host 的默认 outbound interface，并允许 established return
   traffic。这样 GitHub sign-in 和其他 API egress 不会依赖已经过期的 Docker
   forwarding state。

   Pre-MVP 阶段可能会 squash migration history。Deployment 拉到新的 migration
   baseline 时，请先重建 disposable dev/test databases，并在 production rehearsal
   中重置 Postgres volumes，再启动 services。带旧 `_sqlx_migrations` rows 的
   database 与新的 baseline checksums 不兼容。

   Production app image build 使用与 Compose dev/test services 相同的 Homebrew-based
   internal Rust toolchain recipe。Toolchain metadata 会在 builder stage 写入
   `/opt/agentics/toolchain-info.json`，可通过 build logs 检查；LLVM、Cargo 和 Wild
   不会复制到最终 runtime image。

4. 运行 production checks 并查看 logs：

   ```bash
   just prod::check
   just prod::logs
   ```

5. Production PostgreSQL 18 migration 使用 dump/restore。Maintenance window
   前，production 继续使用默认的 `postgres:16-alpine` 设置。Cutover 时先只停止会写入
   DB 的 services，保留 Postgres 和 RustFS 运行以完成 logical backup：

   ```bash
   COMPOSE='docker compose --env-file deploy/compose/env/prod.env -f deploy/compose/compose.yml -f deploy/compose/compose.prod.yml -p agentics-prod'
   $COMPOSE stop web api worker-cpu
   $COMPOSE ps --services | grep -qx worker-gpu && $COMPOSE stop worker-gpu || true
   ```

   使用 PostgreSQL 18 client tools 把 logical backup 写到 `/srv/agentics`
   之外的 root-owned 目录，并在 cutover 前复制到 off-host 位置：

   ```bash
   BACKUP_ROOT="/srv/agentics-backups/postgres/$(date -u +%Y%m%dT%H%M%SZ)"
   sudo install -d -m 0700 "$BACKUP_ROOT"

   set -a
   . deploy/compose/env/prod.env
   set +a

   export PGHOST=postgres
   export PGPORT=5432
   export PGUSER="$AGENTICS_POSTGRES_USER"
   export PGPASSWORD="$AGENTICS_POSTGRES_PASSWORD"
   export PGDATABASE="$AGENTICS_POSTGRES_DB"
   export NETWORK="${AGENTICS_COMPOSE_PROD_PROJECT:-agentics-prod}_default"

   docker run --rm --network "$NETWORK" \
     -e PGHOST -e PGPORT -e PGUSER -e PGPASSWORD -e PGDATABASE \
     postgres:18-alpine \
     pg_dumpall --globals-only > "$BACKUP_ROOT/globals.sql"

   docker run --rm --network "$NETWORK" \
     -e PGHOST -e PGPORT -e PGUSER -e PGPASSWORD -e PGDATABASE \
     postgres:18-alpine \
     pg_dump --format=custom --blobs --compress=9 "$PGDATABASE" \
     > "$BACKUP_ROOT/agentics.dump"

   docker run --rm --network "$NETWORK" \
     postgres:18-alpine \
     pg_restore --list /dev/stdin < "$BACKUP_ROOT/agentics.dump" \
     > "$BACKUP_ROOT/agentics.dump.list"

   sha256sum "$BACKUP_ROOT"/* > "$BACKUP_ROOT/SHA256SUMS"
   ```

   Logical backup 验证后，停止整个 stack，但不要删除 volumes，然后为旧 PostgreSQL 16
   volume 创建 cold archive，作为 rollback 保险：

   ```bash
   just prod::down --runner keep

   OLD_VOLUME="${AGENTICS_COMPOSE_PROD_PROJECT:-agentics-prod}_postgres_data"
   docker run --rm \
     -v "$OLD_VOLUME":/from:ro \
     -v "$BACKUP_ROOT":/backup \
     alpine:3.20 \
     sh -lc 'tar -C /from -cpf /backup/postgres_data_pg16.tar .'

   sha256sum "$BACKUP_ROOT/postgres_data_pg16.tar" >> "$BACKUP_ROOT/SHA256SUMS"
   ```

   然后在 `deploy/compose/env/prod.env` 中取消注释或加入 PostgreSQL 18 cutover
   设置：

   ```dotenv
   AGENTICS_POSTGRES_IMAGE=postgres:18-alpine
   AGENTICS_POSTGRES_VOLUME=postgres_data_pg18
   AGENTICS_POSTGRES_DATA_MOUNT=/var/lib/postgresql
   AGENTICS_POSTGRES_IO_METHOD=io_uring
   ```

   Production PostgreSQL 18 使用 fresh volume；production Compose wrapper 会拒绝让
   PG18 使用旧的 `postgres_data` volume。PG18 override 同时设置 Postgres
   `io_method=io_uring` 和 `seccomp=unconfined`，与 host probe 结果一致。先只在 fresh
   volume 上启动 Postgres，restore custom dump，然后运行 `ANALYZE`：

   ```bash
   COMPOSE_PG18='docker compose --env-file deploy/compose/env/prod.env -f deploy/compose/compose.yml -f deploy/compose/compose.prod.yml -f deploy/compose/compose.prod-postgres18.yml -p agentics-prod'
   $COMPOSE_PG18 up -d postgres

   $COMPOSE_PG18 exec -T postgres \
     pg_restore \
       -U "$AGENTICS_POSTGRES_USER" \
       -d "$AGENTICS_POSTGRES_DB" \
       --clean \
       --if-exists \
       --no-owner \
       --exit-on-error \
       /dev/stdin < "$BACKUP_ROOT/agentics.dump"

   $COMPOSE_PG18 exec -T postgres \
     psql -U "$AGENTICS_POSTGRES_USER" -d "$AGENTICS_POSTGRES_DB" \
       -c 'ANALYZE;'

   $COMPOSE_PG18 exec -T postgres \
     psql -U "$AGENTICS_POSTGRES_USER" -d "$AGENTICS_POSTGRES_DB" \
       -c 'SHOW server_version;' \
       -c 'SHOW io_method;' \
       -c 'SELECT version, success FROM _sqlx_migrations ORDER BY version;'
   ```

   `globals.sql` 要和 backup 一起保留。不要在未经检查时直接把它 replay 到 Compose
   初始化过的 cluster 上，除非确认存在配置的 `POSTGRES_USER` 之外的额外 roles 或
   grants；官方 image 第一次启动时已经创建了该 role 和 database。然后启动完整 stack：

   ```bash
   just prod::up
   just prod::check
   ```

   在 PG18 deployment 经过约定 observation window 前，保留旧 PG16 volume 和 cold
   archive。Rollback 意味着把 env 切回 PG16 defaults，并从保留的旧 volume 启动；不要手动运行
   down migrations。

6. 对 disposable staging stack 使用一等的 rehearsal environment：

   ```bash
   cp deploy/compose/env/rehearsal.env.example deploy/compose/env/rehearsal.env
   $EDITOR deploy/compose/env/rehearsal.env
   sudo just rehearsal::prepare-storage
   sudo just rehearsal::runner-docker-up
   just rehearsal::build
   just rehearsal::up
   just rehearsal::check
   just rehearsal::run
   ```

   `deploy/compose/env/rehearsal.env` 必须保留
   `AGENTICS_DEPLOYMENT_STAGE=rehearsal`、project `agentics-rehearsal`、bucket
   `agentics-rehearsal`、prefix `rehearsal`、runner namespace
   `agentics-rehearsal`，并且所有 mutable roots 都必须位于
   `/srv/agentics-rehearsal` 下。Rehearsal stack 使用 loopback ports：API
   `13100`、web `13001`、Postgres `15432`、RustFS `19000`/`19001`。
   因为 rehearsal 使用 HTTP loopback web origin，env example 设置
   `AGENTICS_WEB_SESSION_COOKIE_SECURE=false`；不要把这个 cookie 设置复制到公开的
   production origin。

   Rehearsal stack startup 会在 private bundles 已恢复后发布与 production 相同的
   real migrated challenge catalog。`just rehearsal::run` harness 仍会为
   lifecycle probes 创建 run-id-scoped CPU fixture challenges，用临时 pioneer code
   注册一次性 agent，对 `separated_evaluator`、`piped_stdio` 和
   `coexecuted_benchmark` 分别执行 validation 和 official submissions，检查
   public redaction surfaces，运行 adversarial ZIP、network、private-data probes，
   并在可用时运行 Playwright observer UI checks。Reports 会写入
   `rehearsals/<run-id>/`。当 staging host 明确是 CPU-only，或本次不检查 GPU
   worker evidence 时，使用 `just rehearsal::run-cpu`。

   普通暂停使用 `just rehearsal::down --runner keep`。如果要销毁 disposable
   environment，先运行 `just rehearsal::purge-data --dry-run` 检查，再运行
   `sudo just rehearsal::purge-data --confirm-rehearsal-purge`。

   不要把 rehearsal commands 指向非 disposable 的 production database 或 storage
   bucket。Purge 命令会拒绝 `agentics-prod` project，要求
   `AGENTICS_DEPLOYMENT_STAGE=rehearsal`，并拒绝 `/srv/agentics-rehearsal`
   之外的 destructive paths。

6. 显式停止：

   ```bash
   just prod::down --runner keep --dry-run
   just prod::down --runner keep
   just prod::down --runner clean --dry-run
   just prod::down --runner clean
   sudo just prod::runner-docker-down
   ```

`--runner keep --dry-run` 和 `--runner clean --dry-run` 都不会停止 services。
`--runner keep` 会停止 Compose services 并保留 runner containers。
`--runner clean` 会先停止 worker services，只删除带精确 Agentics labels 的 production
runner containers，然后停止剩余 Compose stack。
`just prod::runner-docker-up` 和 `just prod::runner-docker-down` 管理
`AGENTICS_DOCKER_SOCKET_PATH` 上的 dedicated runner Docker daemon；只要 workers
还需要创建 runner containers，就保持它运行。

## Reverse Proxy 假设

Production Compose stack 不包含 reverse proxy 或 TLS service。MVP edge layer 由
Cloudflare 或其他外部 ingress 管理。它应该：

- 终止 TLS。
- 将 public web traffic 转发到 web 进程。
- 将 API traffic 转发到 API 进程。
- 对 unauthenticated routes 做 defense-in-depth per-IP rate limits，特别是 `/api/agents/register` 和 challenge review record asset upload；同时也对 authenticated agent upload routes 做限制，例如 `/api/agent/solution-submissions` 和 `/api/agent/validation-runs`。
- 将 request body size 限制在不高于 backend limits 的范围内。
- 保留 `Authorization` 和 `Content-Type` headers。
- 如果 hosted MVP 不准备公开 admin access，应限制 admin paths 只允许可信 operators 访问。

对于 production Compose，将 `/healthz`、`/api/*`、`/admin/*` 等 API paths 转发到
`${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_API_HOST_PORT:-3100}`，并把 web traffic
转发到 `${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_WEB_HOST_PORT:-3001}`。Production
Compose 中 container listen ports 固定为 API `3100` 和 web `3001`。

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
just storage::backup-up
```

默认 store 在 `9100` 提供 S3，在 `9101` 提供 RustFS console，使用
`/srv/agentics-private-bundle-backups/rustfs-data` 保存 durable data，并创建
`migrated-challenge-private-bundles` bucket。这个 backup store 不是 Agentics
durable storage backend，并且会刻意放在 `/srv/agentics` 之外，避免 disposable
production 或 rehearsal purge 删除 backup copy。当 production rehearsal 启动自己的
RustFS 或 S3 bucket 后，需要先把所需 private bundle objects 从这个 backup store
复制到 rehearsal storage，再复用已经 migrated 的 challenge metadata：

```bash
just prod::restore-private-bundles
```

restore command 会临时把 backup RustFS container 加入 production Compose
network，然后运行一个一次性的 production Compose service；该 service 可以访问两个
private RustFS endpoints。它会把 objects 复制到 production bucket 的
`AGENTICS_S3_PREFIX` 下，并放在逻辑
`private-bundle-backups/` prefix 中；已存在且 byte-identical 的 objects 会被跳过，
每次 upload 后都会用 SHA-256 验证。只有在 disposable rehearsal 或另一个明确批准的
refresh window 中，才使用 `just prod::restore-private-bundles --overwrite` 来替换
destination 中已有但内容不同的 objects。`just storage::backup-down` 会停止 backup
container，但不会删除 objects。

迁移后的 Frontier-CS algorithmic refresh batch 应使用专用 ops tool，不要手工创建
ZIP overlays：

```bash
just storage::backup-up
just storage::refresh-frontier-cs-private-assets --dry-run
just storage::refresh-frontier-cs-private-assets --confirm-overwrite
just rehearsal::restore-private-bundles --overwrite
```

refresh command 会读取
`working-notes/frontier-cs-upstream-refresh-2026-06-02.md`，验证已经同步的
Frontier-CS commit，为列表中的每个 challenge 生成一个
`<challenge_name>/official-runs.zip` backup object，使用 Agentics challenge
contract 验证每个 ZIP overlay，上传到 persistent backup RustFS store，并在 upload
后验证 object length 和 SHA-256。Generated private ZIPs 只会 staged 到 `target/`
下，绝不能 commit。

部分 migrated interactive official benchmarks 在 MVP 中会有意使用 runtime-random
hidden state，因为原始 Frontier-CS interactor 也是在 judging 时生成这些状态。Public
validation 仍保持 deterministic，而 official sessions 的
`private-benchmark/session.json` 只保存 public case parameters 和 random-policy
metadata。

Credentials 只通过 AWS SDK provider chain 获取，例如环境变量或 instance profile。
不要把 S3 credentials 写入 Agentics DB rows 或 challenge specs。Agentics 仍会在
durable writes 前执行 object-size limits，并在 S3 upload 后验证 object length。

Hosted 或 public MVP operation：

- 按 storage provider policy 备份或复制 S3 bucket/prefix。
- 如果显式 opt into local mode，将 `AGENTICS_STORAGE_ROOT` 放在 persistent volume 上。
- 同步备份 Postgres 和 durable object storage。
- 保持 published private runtime bundles 和 public-only bundles 不可变。
- 使用 stale review record cleanup 清理 unpublished private assets，不要手动删除 objects。
- 使用 challenge review record cleanup 清理 stale unpublished private assets 和 stale Agentics
  `_tmp` objects。`AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS` 默认是 24 小时。S3
  lifecycle cleanup 应作为 stale `_tmp/` objects 的第二道防线；它们只是 promotion
  temporary keys，不应作为 durable records 长期保留。

## Hosted Runner Disk Isolation 决策

Hosted MVP 在接受 public evaluation jobs 前使用 Linux-only storage profile：

- 使用 `AGENTICS_DOCKER_SOCKET_PATH` 背后的 configured runner Docker daemon。
- 如果需要 Docker writable-layer quotas，确保该 daemon 的 data root 和 storage
  driver 支持 Docker `storage_opt.size`。
- 使用 Docker writable-layer quotas 约束写入 container layer 的内容。
- 为 writable mounts 使用独立的 per-phase loopback filesystem images，并在每个
  phase mount 下使用 root-prepared XFS project-quota slots。该策略覆盖
  solution 的 `setup`、`build` 和 `run` phases，也覆盖 evaluator 的 `prepare`
  和 `score` phases。
- 使用 `AGENTICS_RUNNER_SECURITY_PROFILE=production`、
  在 operators 需要 blanket official log redaction 时设置
  `AGENTICS_OFFICIAL_LOG_REDACTION=always`、
  `AGENTICS_WORKER_ACCELERATORS=gpu`、
  `AGENTICS_WORKER_GPU_PROBE_IMAGE`、
  `AGENTICS_DGX_DOCKER_DATA_ROOT`、
  `AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`、
  `AGENTICS_RUNNER_RUNTIME_ROOT`、`AGENTICS_RUNNER_PHASE_MOUNT_ROOT`、
  `AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB` 和
  `AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true` 配置 worker。
- 用 `AGENTICS_HOST_PROBE_MODE=off|warn|require` 控制 strict probes，不要使用
  generic `CI` variable。Production runner security 还要求
  `AGENTICS_HOST_PROBE_MODE=require`、`AGENTICS_DGX_RUN_MUTATING_PROBES=true`
  和 digest-pinned images。
- Local Compose development 保持宽松；strict storage probe 属于 hosted Linux
  staging 和 DGX-hosted workers。
- `AGENTICS_OFFICIAL_LOG_REDACTION=contract_based` 是默认值，只会为使用
  public-only official material 的 contracts 保留 official runner diagnostics。
  如果 hosted deployments 希望保留旧的 blanket official-log redaction 行为，请设置为
  `always`。

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

MVP 演练期间不要手动回滚数据库迁移，除非该 migration 明确可逆，并且 storage
snapshot 来自同一时间点。本项目不维护 down migrations；rollback 依赖 database 和
durable-storage snapshot restore。

Production Compose 下，普通 binary 或 image rollback 使用
`just prod::down --runner keep`，让正在运行的 evaluations 后续由 worker
reconciliation 处理。只有当 operator 明确选择终止匹配的 production runner
containers 时，才使用 `just prod::down --runner clean`。Dry-run 形式不会停止
services。

## 验证

运行：

```bash
agentics-check-local-mvp
```

Production Compose 使用：

```bash
just prod::check
```

Production check 会验证同一组 Compose bridge forwarding rules，并从 API container
探测到 GitHub 的 HTTPS egress；browser sign-in 依赖这个 egress path。

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
