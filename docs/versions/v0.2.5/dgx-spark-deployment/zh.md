# v0.2.5 DGX Spark Deployment Profile

本文档定义单台 NVIDIA DGX Spark host 的 `M0.2.5-DGX-2` hosted MVP deployment
profile。

该 profile 仅适用于 Linux。DGX scripts 会拒绝在非 Linux host 上运行。
因此，`deploy/dgx-spark/*.service` 中的 `ExecStart=` commands 是 DGX Linux
systemd startup definitions。macOS 演练仍使用
`docs/versions/v0.2.5/deployment/zh.md` 中记录的前台 `cargo` 和 `bun` process
flow。

Port 和 path defaults 在 `deploy/dgx-spark/agentics.env.example` 中集中配置，并在
`docs/versions/v0.2.5/ports-and-paths/zh.md` 中汇总。

## 前置条件

- 已 review `M0.2.5-DGX-1` host inventory。
- Operator 可以管理 systemd services、mounts、Docker daemon configuration 和
  reverse-proxy configuration。
- 已准备非默认 `AGENTICS_ADMIN_PASSWORD`。
- Creator routes 对外开放前，已准备 GitHub OAuth credentials。
- Agentics-owned Docker daemon 可通过 `unix:///run/agentics/docker.sock` 访问。

当前 `MapleSpark` inventory 已确认 host OS、GPU、NVIDIA toolkit、XFS tools 和
loopback tools、default Docker GPU smoke 行为，以及 Agentics-owned Docker daemon
profile。

## Artifacts

Deployment artifacts 位于 `deploy/dgx-spark/`：

| File | Purpose |
| --- | --- |
| `agentics.env.example` | `/etc/agentics/agentics.env` template |
| `dockerd-agentics.json` | Agentics-owned Docker daemon config |
| `agentics-docker.service` | Root-owned Docker daemon service |
| `agentics-api.service` | API server systemd unit |
| `agentics-worker.service` | 带 profile preflight 的 worker systemd unit |
| `agentics-web.service` | Web frontend systemd unit |
| `nginx-agentics.conf.example` | Reverse-proxy shape 和 public route limits |

Agentics-owned Docker daemon 通过设置 `"bridge": "none"` 禁用 Docker default
bridge。Public runner execution 应继续使用显式 network policy，DGX-3 必须在接受
public jobs 前包含 no-egress runner smoke。Systemd unit 还为 Agentics daemon 设置
独立的 containerd namespace，避免共享 default Docker 的 `moby` namespace。

Linux-gated scripts：

| Script | Purpose |
| --- | --- |
| `scripts/ops/prepare-dgx-spark-storage.sh` | 需要显式确认的 loopback XFS images storage layout setup |
| `scripts/ops/check-dgx-spark-profile.sh` | Runtime profile checks、Docker quota probe 和 phase-mount canary probe |

## Persistent Layout

| Purpose | Path |
| --- | --- |
| Release root | `/opt/agentics/current` |
| Config root | `/etc/agentics` |
| State root | `/srv/agentics` |
| Storage root | `/srv/agentics/storage` |
| Challenge checkout root | `/srv/agentics/challenges` |
| Runtime root | `/srv/agentics/runtime` |
| Agentics Docker data-root mount | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Docker data-root loop image | `/srv/agentics/loop-images/docker-data-root.xfs` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| API binary | `/opt/agentics/current/bin/api` |
| Worker binary | `/opt/agentics/current/bin/worker` |
| CLI binary | `/opt/agentics/current/bin/agentics` |
| Web build | `/opt/agentics/current/frontends/web/.next` |

Phase mount root 为以下每类 writable runner path 准备一个 XFS loopback mount：

- `solution-setup`
- `solution-build`
- `solution-run`
- `scorer-prepare`
- `scorer-score`

每个 phase mount 都包含由 root 预先准备的 XFS project-quota slots。Worker 会为
每个 writable container mount 租用一个 slot，只把该 slot 中干净的 `work`
directory 绑定进 Docker，并在 phase output 复制回 durable runner artifacts 之前
保持该 slot locked。Docker `storage_opt.size` 约束写入 container root filesystem
的内容。

## Environment

将 `deploy/dgx-spark/agentics.env.example` 复制到
`/etc/agentics/agentics.env`，并替换所有 placeholders。

Hosted profile 必需值：

```bash
AGENTICS_API_HOST=127.0.0.1
AGENTICS_API_PORT=3100
AGENTICS_WEB_PORT=3001
AGENTICS_API_BASE_URL=https://<public-hostname>
AGENTICS_WEB_BASE_URL=https://<public-hostname>
AGENTICS_CORS_ALLOWED_ORIGINS=https://<public-hostname>
AGENTICS_WEB_SESSION_COOKIE_SECURE=true
AGENTICS_DOCKER_HOST=unix:///run/agentics/docker.sock
AGENTICS_HOST_PROBE_MODE=require
AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
```

MVP deployment 在 DGX Spark 上支持 `linux-arm64-cpu` 和 `linux-arm64-cuda`
targets。AMD64 Linux targets 保留给 post-MVP deployment expansion。

## Storage Preparation

仅在 Linux 上、使用 operator privileges 运行：

```bash
AGENTICS_DGX_CONFIRM=prepare-storage \
AGENTICS_DGX_PERSIST_FSTAB=1 \
AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096' \
AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=4 \
scripts/ops/prepare-dgx-spark-storage.sh
```

该脚本在未设置 `AGENTICS_DGX_CONFIRM=prepare-storage` 时会拒绝运行。它创建
persistent directory layout，格式化缺失的 loopback XFS images，使用 `prjquota`
挂载，并在每个 phase mount 下准备 quota slots。设置
`AGENTICS_DGX_PERSIST_FSTAB=1` 后，它会为 loopback mounts 追加 idempotent
`/etc/fstab` entries。

在 `MapleSpark` 上，DGX-2 run 已挂载：

- `/srv/agentics/docker-data-root`，200 GiB loopback XFS with `prjquota`；
- `/srv/agentics/phase-mounts/solution-setup`，20 GiB loopback XFS with
  `prjquota`；
- `/srv/agentics/phase-mounts/solution-build`，20 GiB loopback XFS with
  `prjquota`；
- `/srv/agentics/phase-mounts/solution-run`，20 GiB loopback XFS with
  `prjquota`；
- `/srv/agentics/phase-mounts/scorer-prepare`，20 GiB loopback XFS with
  `prjquota`；
- `/srv/agentics/phase-mounts/scorer-score`，20 GiB loopback XFS with
  `prjquota`。

每个 phase mount 的默认 slot layout 是：

- `slots/64mb/slot-001` 到 `slot-004`
- `slots/256mb/slot-001` 到 `slot-004`
- `slots/1024mb/slot-001` 到 `slot-004`
- `slots/4096mb/slot-001` 到 `slot-004`

Worker 会选择不小于 effective phase `disk_limit_mb` 的最小 configured slot
class。如果需要 exact hard phase limit，应让 challenge resource profiles 与
configured slot classes 对齐。

## Service Startup

安装文件：

```bash
getent group agentics >/dev/null || groupadd --system agentics
getent passwd agentics >/dev/null || useradd --system --gid agentics --home-dir /srv/agentics --shell /usr/sbin/nologin agentics
install -d /etc/agentics /etc/systemd/system
install -m 0640 deploy/dgx-spark/agentics.env.example /etc/agentics/agentics.env
install -m 0644 deploy/dgx-spark/dockerd-agentics.json /etc/agentics/dockerd-agentics.json
install -m 0644 deploy/dgx-spark/*.service /etc/systemd/system/
```

替换 `/etc/agentics/agentics.env` 中的 placeholders，然后启动：

```bash
systemctl daemon-reload
systemctl enable --now agentics-docker.service
systemctl start agentics-api.service
systemctl start agentics-worker.service
systemctl start agentics-web.service
```

Worker unit 会在启动前运行 `scripts/ops/check-dgx-spark-profile.sh`。当
`AGENTICS_HOST_PROBE_MODE=require` 时，如果 Linux host profile 未被证明，worker
会 fail closed。

## Release And Backup Paths

Release artifacts 应部署到 versioned directory，并通过更新 `/opt/agentics/current`
来 promote：

```text
/opt/agentics/releases/<git-sha>/
/opt/agentics/current -> /opt/agentics/releases/<git-sha>
```

以下路径应与 Postgres 一起备份：

- `/srv/agentics/storage`
- `/srv/agentics/challenges`
- `/etc/agentics/agentics.env`
- `/etc/agentics/dockerd-agentics.json`
- `/etc/systemd/system/agentics-*.service`
- `/opt/agentics/current` 指向的 release identifier

不要把 Docker container writable layers 当作 authoritative platform state
备份。它们是 execution scratch space。

## Reverse Proxy

使用 `deploy/dgx-spark/nginx-agentics.conf.example` 作为 TLS termination 和 routing
的形状。Reverse proxy 必须：

- terminate TLS；
- 对 unauthenticated routes 应用 rate limits；
- preserve `Authorization`、`Content-Type` 和 forwarded headers；
- 除非明确允许 public admin access，否则保持 `/admin` 和 `/admin-api`
  operator-restricted。

## Verification

先运行 non-mutating profile check：

```bash
AGENTICS_HOST_PROBE_MODE=warn \
scripts/ops/check-dgx-spark-profile.sh
```

Agentics-owned Docker daemon 和 phase mounts 配置完成后，运行包含 mutating probes 的
strict check：

```bash
docker --host unix:///run/agentics/docker.sock pull busybox:1.36
sudo -u agentics env \
  AGENTICS_HOST_PROBE_MODE=require \
  AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots \
  AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
  AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096 \
  AGENTICS_DGX_RUN_MUTATING_PROBES=1 \
  AGENTICS_DGX_DOCKER_PULL_POLICY=never \
  scripts/ops/check-dgx-spark-profile.sh
```

这会验证：

- Linux host gate；
- Agentics Docker socket target；
- Docker data root 上的 XFS `prjquota`；
- phase mounts 上的 XFS `prjquota`；
- Docker daemon access 和 `overlay2`；
- 通过 `--storage-opt size=16m` 验证 Docker writable-layer quota 行为；
- phase writable-mount canary writes；
- root-prepared bounded runner quota slots；
- 使用 64 MiB probe slot 验证每个 phase 的 Docker bind-mount quota exhaustion。

然后运行：

```bash
AGENTICS_ADMIN_PASSWORD='<admin-password>' \
AGENTICS_WEB_BASE_URL='https://<public-hostname>' \
scripts/ops/check-local-mvp.sh
```

最后运行 `docs/versions/v0.2.5/hosted-cli-onboarding/zh.md` 中的 hosted CLI
onboarding smoke path。

Dry-run deployment rehearsal 应使用 DGX env 运行 database migrations，通过
systemd 启动 API、worker 和 web，然后使用非默认 admin credentials 运行上述
health/profile checks。

## 当前验证状态

2026-05-13，`MapleSpark` 上 strict DGX-2 profile verification 和 DGX-3 hosted
application smoke 已通过。其中 strict profile check 以 `agentics` service user
运行：

```text
[agentics-dgx-check] running Linux DGX profile checks
[agentics-dgx-check] NVIDIA runtime is visible to the Agentics Docker daemon
[agentics-dgx-check] running Docker writable-layer quota probe
[agentics-dgx-check] Docker writable-layer quota probe failed with expected quota exhaustion
[agentics-dgx-check] running phase writable-mount canary probes
[agentics-dgx-check] DGX profile checks passed
```

DGX-2 的 host-level pieces 已安装：service user、loopback XFS mounts、idempotent
`/etc/fstab` entries、Agentics-owned Docker config 和已启用的
`agentics-docker.service`。

DGX-3 已将 release 安装到 `/opt/agentics/current`，提供
`/etc/agentics/agentics.env`，启动 Postgres、API、worker、web 和 Agentics-owned
Docker services，并完成 hosted smoke path。Smoke evidence 已记录在
`docs/versions/v0.2.5/dgx-spark-smoke/zh.md`，覆盖：

- local MVP health checks；
- strict DGX profile checks；
- hosted CLI onboarding；
- `linux-arm64-cpu` 上的 matrix validation 和 official submission；
- no-egress runner enforcement；
- storage-quota escape failure；
- capacity 和 worker heartbeat inspection。

DNS、TLS、public ingress 和最终 operator access policy 仍属于 launch cutover work。

## 下一步

以 DGX-3 smoke 文档作为 launch cutover baseline evidence，然后完成 public
ingress、DNS、TLS 和 operator-only admin access。
