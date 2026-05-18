# DGX Spark Operations

本文档是当前在单台 NVIDIA DGX Spark host 上运行 Agentics 的 operator reference。
它替代旧的 inventory、deployment 和 smoke evidence 分散文档。

## 范围

DGX Spark profile 仅适用于 Linux。`deploy/dgx-spark/` 下的 systemd units 和
storage scripts 不能作为 macOS startup definitions 使用。macOS rehearsal 使用
[deployment](../deployment/zh.md) 中的前台 `cargo` 和 `bun` flow。

MVP hosted deployment targets：

- `linux-arm64-cpu`
- `linux-arm64-cuda`

`linux-amd64-cpu` 和 `linux-amd64-cuda` 在存在 AMD64 Linux deployment capacity
前仍是 post-MVP targets。

## Host Inventory

第一轮 inventory 于 2026-05-12 至 2026-05-13 在 `MapleSpark` 上采集。

| Area | Result |
| --- | --- |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) |
| Kernel | `6.17.0-1014-nvidia` |
| Architecture | `aarch64` |
| GPU | NVIDIA GB10 |
| NVIDIA driver | `580.142` |
| Driver-reported CUDA | `13.0` |
| NVIDIA container toolkit | `nvidia-container-toolkit 1.19.0-1`，`libnvidia-container1 1.19.0-1` |
| Agentics Docker daemon | `unix:///run/agentics/docker.sock`，`overlay2` on XFS，data root `/srv/agentics/docker-data-root`，可见 named `nvidia` runtime |
| Runner quota slots | 每个 runner phase 都有 64 MiB、256 MiB、1 GiB 和 4 GiB XFS project-quota slots，每个 class 四个 slots |

在 DGX host 上运行可重复的 Linux-gated inventory check：

```bash
scripts/ops/check-dgx-spark-host.sh
```

若要包含 NVIDIA Docker smoke check，使用能够访问目标 Docker daemon 的 operator
account：

```bash
AGENTICS_DGX_DOCKER_CLI='sudo -n docker' \
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:13.0.1-base-ubuntu24.04 \
scripts/ops/check-dgx-spark-host.sh
```

不要在 operator 的 default Docker daemon 上运行 public Agentics jobs。
Agentics-owned Docker daemon 和 root-prepared quota slots 是 public jobs 的
hosted storage boundary。

## Deployment Artifacts

Deployment artifacts 位于 `deploy/dgx-spark/`：

| File | Purpose |
| --- | --- |
| `agentics.env.example` | `/etc/agentics/agentics.env` template |
| `dockerd-agentics.json` | Agentics-owned Docker daemon config |
| `agentics-docker.service` | Root-owned Docker daemon service |
| `agentics-api.service` | API server systemd unit |
| `agentics-worker.service` | 带 profile preflight 的 worker systemd unit |
| `agentics-web.service` | Web frontend systemd unit |

Linux-gated operational scripts：

| Script | Purpose |
| --- | --- |
| `scripts/ops/prepare-dgx-spark-storage.sh` | 创建 loopback XFS images、使用 project quotas 挂载，并准备 runner quota slots |
| `scripts/ops/prepare-dgx-spark-test-storage.sh` | 创建单独的 `/srv/agentics-test` quota root，并归属给调用测试的用户 |
| `scripts/ops/check-dgx-spark-profile.sh` | 检查 runtime profile、Docker quota behavior、phase mounts 和 quota-slot probes |

## Persistent Layout

| Purpose | Path |
| --- | --- |
| Config root | `/etc/agentics` |
| Environment file | `/etc/agentics/agentics.env` |
| Release symlink | `/opt/agentics/current` |
| Release versions | `/opt/agentics/releases/<release-id>` |
| State root | `/srv/agentics` |
| Storage root | `/srv/agentics/storage` |
| Challenge checkout root | `/srv/agentics/challenges` |
| Runtime root | `/srv/agentics/runtime` |
| Agentics Docker socket | `/run/agentics/docker.sock` |
| Agentics Docker data root | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |

Phase mount root 为以下每类 writable runner class 准备一个 loopback XFS mount：

- `solution-setup`
- `solution-build`
- `solution-run`
- `scorer-prepare`
- `scorer-score`

## Required Environment

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

公开 hosted profile 前必须使用非默认 `AGENTICS_ADMIN_PASSWORD`。开放 creator
routes 前需要准备 GitHub OAuth credentials。

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
`AGENTICS_DGX_PERSIST_FSTAB=1` 后，它会追加 idempotent `/etc/fstab` entries。

在 `MapleSpark` 上，DGX-2 run 已挂载：

- `/srv/agentics/docker-data-root`，200 GiB loopback XFS with `prjquota`。
- 五个 phase mounts，每个 20 GiB，覆盖 solution setup/build/run 和 scorer
  prepare/score。
- 每个 class 和 phase 四个 quota slots，覆盖 64 MiB、256 MiB、1024 MiB 和
  4096 MiB limits。

Worker 会选择不小于 effective phase `disk_limit_mb` 的最小 configured slot
class。如果需要 exact hard phase limit，应让 challenge resource profiles 与 slot
classes 对齐。

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

## Verification

先运行 non-mutating profile check：

```bash
AGENTICS_HOST_PROBE_MODE=warn \
scripts/ops/check-dgx-spark-profile.sh
```

Agentics-owned Docker daemon 和 phase mounts 配置完成后，运行包含 mutating probes
的 strict check：

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

在 DGX host 上由开发者运行 integration tests 时，应准备一个由测试用户拥有的独立
quota root，不复用 production runner slots：

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  scripts/ops/prepare-dgx-spark-test-storage.sh
```

运行 quota-sensitive integration tests 时设置：

```bash
export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics-test/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
```

`/srv/agentics-test` 和 `/srv/agentics` 故意分离，这样本地测试权限不会改变 hosted
worker slot ownership。

然后运行：

```bash
AGENTICS_ADMIN_PASSWORD='<admin-password>' \
AGENTICS_WEB_BASE_URL='https://<public-hostname>' \
scripts/ops/check-local-mvp.sh
```

最后针对 hosted endpoint 运行根目录 `README.md` 中的 CLI submitter flow，并用
`agentics submissions show` 查看结果。

## Smoke Evidence

2026-05-13，`MapleSpark` 上 strict DGX-2 profile verification 和 DGX-3 hosted
application smoke 已通过。

Smoke 覆盖：

- local MVP health checks；
- strict DGX profile checks；
- hosted CLI onboarding；
- `linux-arm64-cpu` 上的 matrix validation 和 official submission；
- no-egress runner enforcement；
- storage-quota escape failure；
- capacity 和 worker heartbeat inspection。

Storage escape run 按预期失败，worker error 为
`phase exceeded disk limit: 100663583 > 67108864 bytes`。该失败被限制在 job disk
limit 内，没有耗尽 host storage。

## Launch Cutover

Public traffic 前剩余的 cutover work：

- 保持 `/opt/agentics/current` 和 `/etc/agentics/agentics.env` 与每次 promoted
  build 对齐；
- 配置 public ingress、DNS 和 TLS；
- 除非明确允许 public admin access，否则保持 `/admin` 和 `/admin-api`
  operator-restricted；
- 使用 Cloudflare edge controls 处理 TLS、routing 和 unauthenticated route rate
  limits。Application-level pioneer-code registration gating 仍是主要 registration
  control。

DGX Spark 运维应以 NVIDIA 官方文档为准：

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
