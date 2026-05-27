# DGX Spark Host Preparation

本文档是 production Compose 模型下的 DGX Spark operator reference。Agentics
services 由 Docker Compose 启动。DGX-specific 工作现在只包括 host inventory、
Docker/GPU readiness、runner quota storage preparation，以及 Compose worker 使用的
strict profile checks。

## 范围

DGX Spark hosted target 仅适用于 Linux，并支持 MVP hosted targets：

- `linux-arm64-cpu`
- `linux-arm64-cuda`

`linux-amd64-cpu` 和 `linux-amd64-cuda` 在存在 AMD64 Linux deployment capacity
前仍是 post-MVP targets。

Production service graph 位于 `deploy/compose/compose.prod.yml`，并通过
`agentics-compose-prod` 或 `compose-prod-*` just recipes 运维。

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
| Production runner Docker socket | `${AGENTICS_DOCKER_SOCKET_PATH:-/srv/agentics/docker.sock}` |
| Runner quota slots | 每个 runner phase 都有 64 MiB、256 MiB、1 GiB、4 GiB、8 GiB、12 GiB 和 16 GiB XFS project-quota slots，每个 class 100 个 slots，每 MiB 256 个 inodes |

在 DGX host 上运行可重复的 Linux-gated inventory check：

```bash
agentics-check-dgx-spark-host
```

若要包含 NVIDIA Docker smoke check，使用能够访问目标 Docker daemon 的 operator
account：

```bash
AGENTICS_DGX_RUN_DOCKER_SMOKE=true \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:13.0.1-base-ubuntu24.04 \
agentics-check-dgx-spark-host
```

Rust checker 直接使用 Docker API。请通过 `AGENTICS_DOCKER_HOST` 或标准 Docker
socket environment 配置 Docker access；不要套一层 Docker CLI script。

## Production Compose Configuration

从 Compose template 创建 production env file：

```bash
cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
```

DGX production 应保持以下值一致：

```bash
AGENTICS_COMPOSE_PROD_PROJECT=agentics-prod
AGENTICS_DOCKER_SOCKET_PATH=/srv/agentics/docker.sock
AGENTICS_DOCKER_HOST=unix:///srv/agentics/docker.sock
AGENTICS_RUNTIME_UID=10001
AGENTICS_RUNTIME_GID=10001
AGENTICS_DOCKER_SOCKET_GID=10001
AGENTICS_WORKER_ACCELERATORS=gpu
AGENTICS_WORKER_GPU_PROBE_IMAGE=ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:8e3da4a65e297e3b1e9800da001fa2bbac9ed48453a6972117a0c3ad1d1eef13
AGENTICS_RUNNER_SECURITY_PROFILE=production
AGENTICS_HOST_PROBE_MODE=require
AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
AGENTICS_DGX_DOCKER_DATA_ROOT=/srv/agentics/docker-data-root
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096,8192,12288,16384
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
```

公开 stack 前必须修改 `AGENTICS_ADMIN_PASSWORD` 以及所有 storage、database、OAuth
和 RustFS secrets。Public ingress 和 TLS 保持在 Compose 外部。

## Persistent Layout

| Purpose | Path |
| --- | --- |
| State root | `/srv/agentics` |
| Storage work root | `/srv/agentics/storage-work` |
| Runtime root | `/srv/agentics/runtime` |
| Runner Docker socket | 默认 `/srv/agentics/docker.sock` |
| 为 quota-capable host 准备的 Docker data root | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |

Phase mount root 为以下每类 writable runner class 准备一个 loopback XFS mount：

- `solution-setup`
- `solution-build`
- `solution-run`
- `evaluator-setup`
- `evaluator-score`

Production Compose 使用 RustFS/S3 作为 durable object storage。
`AGENTICS_STORAGE_WORK_ROOT` 是 bundle packing、unpacking 和 S3 downloads 的本地
scratch；runner quota slots 仍位于 `AGENTICS_RUNNER_PHASE_MOUNT_ROOT`。

## Storage Preparation

仅在 Linux 上、使用 operator privileges 运行：

```bash
AGENTICS_DGX_CONFIRM=prepare-storage \
AGENTICS_RUNTIME_UID=10001 \
AGENTICS_RUNTIME_GID=10001 \
AGENTICS_DGX_PERSIST_FSTAB=true \
AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096 8192 12288 16384' \
AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=100 \
AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB=256 \
agentics-prepare-dgx-spark-storage
```

storage preparer 在未设置 `AGENTICS_DGX_CONFIRM=prepare-storage` 时会拒绝运行。
它创建 persistent directory layout，格式化缺失的 loopback XFS images，使用
`prjquota` 挂载，在每个 phase mount 下准备 quota slots，并把 Compose 可见的
writable roots chown 给 `AGENTICS_RUNTIME_UID:AGENTICS_RUNTIME_GID`。设置
`AGENTICS_DGX_PERSIST_FSTAB=true` 后，它会追加 idempotent `/etc/fstab` entries。

Production Compose 默认使用 dedicated runner Docker socket。在完成 storage
preparation 后、`compose-prod-up` 前，用 production ops wrapper 启动它：

```bash
sudo just compose-prod-runner-docker-up
```

Wrapper 会用 `/srv/agentics/docker-data-root`、dedicated
`/srv/agentics/docker.sock` socket、overlay2、Docker `storage_opt.size`
支持，以及由 host bridge `agentics0` 支撑的默认 Docker `bridge` network 启动
`dockerd`。该 bridge 是声明 `network_access: enabled` 的 challenge setup
phases 所必需的，例如 CUDA 或 PyTorch/Triton dependency setup。需要时显式停止：

```bash
sudo just compose-prod-runner-docker-down
```

Worker container 通常不需要直接获得 GPU access。它需要 Docker socket access，
以便请求 runner Docker daemon 创建带 GPU device requests 的 runner containers。
因此 GPU execution 依赖 runner Docker daemon 和 NVIDIA Container Toolkit 配置，
而不是把 GPU devices 暴露给 API、web、Postgres、RustFS 或 CPU-only worker
services。

## Profile Check

先运行 non-mutating profile check：

```bash
AGENTICS_DOCKER_HOST=unix:///srv/agentics/docker.sock \
AGENTICS_DOCKER_SOCKET_PATH=/srv/agentics/docker.sock \
AGENTICS_HOST_PROBE_MODE=warn \
AGENTICS_RUNNER_SECURITY_PROFILE=production \
agentics-check-dgx-spark-profile
```

配置好 phase mounts 和 Docker quota behavior 后，运行包含 mutating probes 的 strict
check：

```bash
docker pull busybox:1.36
AGENTICS_DOCKER_HOST=unix:///srv/agentics/docker.sock \
AGENTICS_DOCKER_SOCKET_PATH=/srv/agentics/docker.sock \
AGENTICS_HOST_PROBE_MODE=require \
AGENTICS_RUNNER_SECURITY_PROFILE=production \
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots \
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime \
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096,8192,12288,16384 \
AGENTICS_DGX_RUN_MUTATING_PROBES=true \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
agentics-check-dgx-spark-profile
```

当 `AGENTICS_HOST_PROBE_MODE=warn` 或 `require` 时，worker 会在 startup
期间运行同一个 profile checker。在 production security 和 require mode 下，如果
bounded runner storage、Docker quota behavior、默认 Docker bridge network、
digest-pinned images 或 GPU probing 无法证明，worker 会 fail closed。在 worker
container 内，`xfs_quota` 可能无法读取
bind-mounted loop devices 的 host project quota rows；遇到这种情况时，checker 会把
row inspection 视为 inconclusive，并依赖 slot metadata 和 required mutating
quota-exhaustion probes。

## Production Startup

通过 Compose 启动 services：

```bash
just compose-prod-build
sudo just compose-prod-runner-docker-up
just compose-prod-up
just compose-prod-check
```

停止 stack 时必须显式选择 runner policy：

```bash
just compose-prod-down --runner keep --dry-run
just compose-prod-down --runner keep
just compose-prod-down --runner clean --dry-run
just compose-prod-down --runner clean
sudo just compose-prod-runner-docker-down
```

`--runner clean` 只会删除带精确 Agentics labels 的 matching production runner
containers。它不会修改 database job state。

## CUDA And Integration Smokes

使用 Docker GPU device requests 运行 CUDA image GPU smokes：

```bash
for image in \
  ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu126-ubuntu24.04-v0.2.5@sha256:d2913c5e027e95b67ab4dea49fafd0e8b12a741ec11f125b6d3807c2ac662295 \
  ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:8e3da4a65e297e3b1e9800da001fa2bbac9ed48453a6972117a0c3ad1d1eef13 \
  ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu132-ubuntu24.04-v0.2.5@sha256:ce63970cfc2024d729d786c63d9c8e95e4b352a03e507358ff4a82987ccfd50e
do
  docker run --rm --gpus 1 --platform linux/arm64 \
    -e AGENTICS_GPU_SMOKE_REQUIRE_DEVICE=1 \
    "$image" /opt/agentics/smoke.sh
done
```

在 Linux 上由开发者运行 integration tests 时，应准备一个由测试用户拥有的独立 quota
root，不复用 production runner slots：

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

运行 CPU-only full suite：

```bash
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just test-env-up
just test-env-status-cpu
just test-all-cpu
```

在有 NVIDIA GPU support 的 Linux host 上，使用 `just test-env-status` 和
`just test-all` 覆盖 ignored CUDA/GPU tests。这些命令使用专用 test Docker daemon
以及 disposable Compose Postgres/RustFS services。

## Smoke Evidence

2026-05-13，`MapleSpark` 上 strict DGX-2 profile verification 和 DGX-3 hosted
application smoke 已通过。2026-05-22，`MapleSpark` 上 CUDA image publication 和
DGX GPU worker smoke 已通过。

Smoke 覆盖 local MVP health checks、strict DGX profile checks、hosted CLI
onboarding、`linux-arm64-cpu` 上的 matrix validation 和 official submission、
no-egress runner enforcement、storage-quota escape failure、capacity、worker
heartbeat inspection、CUDA image GPU runtime，以及 `linux-arm64-cuda` 上 ignored
`cuda_smoke` integration path。

## Launch Cutover

Public traffic 前剩余的 cutover work：

- 在 Compose 外配置 public ingress、DNS 和 TLS；
- 除非明确允许 public admin access，否则保持 `/admin` 和 admin API paths
  operator-restricted；
- 使用 Cloudflare edge controls 处理 TLS、routing 和 unauthenticated route rate
  limits；
- destructive runner cleanup 前先运行 `just compose-prod-down --runner clean --dry-run`。

DGX Spark host 和 GPU 运维应以 NVIDIA 官方文档为准：

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
