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
| Agentics Docker daemon | `unix:///run/agentics/docker.sock`，`overlay2` on XFS，data root `/srv/agentics/docker-data-root`，Docker GPU device requests 已启用 |
| Runner quota slots | 每个 runner phase 都有 64 MiB、256 MiB、1 GiB 和 4 GiB XFS project-quota slots，每个 class 四个 slots，每 MiB 256 个 inodes |

在 DGX host 上运行可重复的 Linux-gated inventory check：

```bash
agentics-check-dgx-spark-host
```

若要包含 NVIDIA Docker smoke check，使用能够访问目标 Docker daemon 的 operator
account：

```bash
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:13.0.1-base-ubuntu24.04 \
agentics-check-dgx-spark-host
```

Rust checker 直接使用 Docker API。请通过目标 daemon socket 配置 Docker 访问，
例如设置 `DOCKER_HOST`，不要使用 Docker CLI wrapper。

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
| `agentics-worker.service` | Worker systemd unit；worker 会强制执行配置的 host probe mode |
| `agentics-web.service` | Web frontend systemd unit |

Linux-gated operational binaries 位于 `agentics-ops` package。Packaged
deployment 会将它们安装到 `/opt/agentics/current/bin`；从 source checkout
运行时，使用 `cargo run -p agentics-ops --bin <binary> -- ...`。

| Binary | Purpose |
| --- | --- |
| `agentics-manage-dgx-spark-profile` | 安装、启动、停止和卸载 DGX systemd profile |
| `agentics-prepare-dgx-spark-storage` | 创建 loopback XFS images、使用 project quotas 挂载，并准备 runner quota slots |
| `agentics-prepare-dgx-spark-test-storage` | 创建单独的 `/srv/agentics-test` quota root，并归属给调用测试的用户 |
| `agentics-check-dgx-spark-profile` | 检查 runtime profile、Docker runtime-root visibility、Docker quota behavior、phase mounts 和 quota-slot probes |

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
- `evaluator-setup`
- `evaluator-score`

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
AGENTICS_WORKER_ACCELERATORS=gpu
AGENTICS_WORKER_GPU_PROBE_IMAGE=ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:8e3da4a65e297e3b1e9800da001fa2bbac9ed48453a6972117a0c3ad1d1eef13
AGENTICS_RUNNER_SECURITY_PROFILE=production
AGENTICS_HOST_PROBE_MODE=require
AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
AGENTICS_RUNNER_MAX_OUTPUT_FILES=8192
AGENTICS_RUNNER_MAX_OUTPUT_DIRS=1024
AGENTICS_RUNNER_MAX_OUTPUT_DEPTH=32
AGENTICS_RUNNER_MAX_RUNS=100
AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES=4194304
AGENTICS_RUNNER_MAX_PUBLIC_RESULTS=1024
AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES=262144
AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION=16777216
AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS=2
```

公开 hosted profile 前必须使用非默认 `AGENTICS_ADMIN_PASSWORD`。开放 creator
routes 前需要准备 GitHub OAuth credentials。
Storage/profile setup 会保持 `/srv/agentics/runtime` 和
`/srv/agentics/phase-mounts` 由 `agentics` service user 拥有，且权限为 `0700`；
如果这些 roots 可被 group/world 遍历，worker 和 DGX profile check 都会 fail
closed。

## Storage Preparation

仅在 Linux 上、使用 operator privileges 运行：

```bash
AGENTICS_DGX_CONFIRM=prepare-storage \
AGENTICS_DGX_PERSIST_FSTAB=1 \
AGENTICS_DGX_PHASE_SLOT_CLASSES_MB='64 256 1024 4096' \
AGENTICS_DGX_PHASE_SLOTS_PER_CLASS=4 \
AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB=256 \
agentics-prepare-dgx-spark-storage
```

storage preparer 在未设置 `AGENTICS_DGX_CONFIRM=prepare-storage` 时会拒绝运行。它创建
persistent directory layout，格式化缺失的 loopback XFS images，使用 `prjquota`
挂载，并在每个 phase mount 下准备 quota slots。设置
`AGENTICS_DGX_PERSIST_FSTAB=1` 后，它会追加 idempotent `/etc/fstab` entries。

在 `MapleSpark` 上，DGX-2 run 已挂载：

- `/srv/agentics/docker-data-root`，200 GiB loopback XFS with `prjquota`。
- 五个 phase mounts，每个 20 GiB，覆盖 solution setup/build/run 和 evaluator
  prepare/score。
- 每个 class 和 phase 四个 quota slots，覆盖 64 MiB、256 MiB、1024 MiB 和
  4096 MiB limits。默认每 MiB `256` 个 inodes，因此这些 slots 的 inode hard
  limits 分别是 16384、65536、262144 和 1048576。

Worker 会选择不小于 effective phase `disk_limit_mb` 的最小 configured slot
class。如果需要 exact hard phase limit，应让 challenge resource profiles 与 slot
classes 对齐。单独的 evaluator-visible run tree cap 默认为 8192 个 files、1024 个
directories 和 32 层 depth；setup/build dependency installs 由 XFS byte 和 inode
quota 约束。每次 evaluation 最多运行 100 个 solution invocations。持久化 runner
logs 按实际 run count 每个 1 MiB 限制，`result.json` 在解析前限制为 4 MiB，
public evaluator feedback 限制为 1024 个 entries，embedded evaluator result logs 限制为
256 KiB。`piped_stdio` interaction traffic 每个方向限制为 16 MiB。

## Service Startup

安装 profile files 并准备 storage：

```bash
just dgx-profile install
```

替换 `/etc/agentics/agentics.env` 中的 placeholders，然后启动：

```bash
just dgx-profile start
```

停止或卸载 profile：

```bash
just dgx-profile stop
just dgx-profile uninstall
just dgx-profile uninstall --purge-data
```

当 `AGENTICS_HOST_PROBE_MODE=warn` 或 `require` 时，worker process 会在 startup
期间运行 `agentics-check-dgx-spark-profile`。当
`AGENTICS_RUNNER_SECURITY_PROFILE=production` 和
`AGENTICS_HOST_PROBE_MODE=require` 同时设置时，如果 Linux host profile 未被证明、
probe binary 无法运行，或 bounded runner storage 与 Docker writable-layer quota 未启用，
worker 会 fail closed。
Packaged worker 默认使用 `bin/agentics-check-dgx-spark-profile`；只有当 deployment
刻意把 probe binary 安装到其他位置时，才设置 `AGENTICS_HOST_PROBE_COMMAND`。
当 `AGENTICS_WORKER_ACCELERATORS=gpu` 时，startup 还会要求
`AGENTICS_WORKER_GPU_PROBE_IMAGE` 能通过 Docker GPU device requests 看到至少一个
GPU，否则 worker 会 fail closed。

普通 `uninstall` 会删除 services 和 quota storage，但保留 config、release files
和 durable state。`uninstall --purge-data` 还会删除 `/etc/agentics`、
`/opt/agentics/current`、`/srv/agentics`、`/srv/agentics-test` 和 `agentics` service
identity。

## Verification

先运行 non-mutating profile check：

```bash
AGENTICS_HOST_PROBE_MODE=warn \
AGENTICS_RUNNER_SECURITY_PROFILE=production \
agentics-check-dgx-spark-profile
```

Agentics-owned Docker daemon 和 phase mounts 配置完成后，运行包含 mutating probes
的 strict check：

```bash
docker --host unix:///run/agentics/docker.sock pull busybox:1.36
sudo -u agentics env \
  AGENTICS_HOST_PROBE_MODE=require \
  AGENTICS_RUNNER_SECURITY_PROFILE=production \
  AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots \
  AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime \
  AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts \
  AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096 \
  AGENTICS_DGX_RUN_MUTATING_PROBES=1 \
  AGENTICS_DGX_DOCKER_PULL_POLICY=never \
  agentics-check-dgx-spark-profile
```

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

在 DGX 上使用本地 integration database 运行端到端 worker CUDA smoke：

```bash
DATABASE_URL=postgres://agentics:agentics@127.0.0.1:5432/agentics_test \
  cargo test -p integration-tests --test cuda_smoke -- --ignored --nocapture
```

这个 ignored test 会发布一个临时 CUDA challenge，并使用不同的 public/private
bundle paths；随后 queue validation 和 official GPU jobs，以
`AGENTICS_WORKER_ACCELERATORS=gpu` 运行 worker，并验证 public leaderboard。

在 DGX host 上由开发者运行 integration tests 时，应准备一个由测试用户拥有的独立
quota root，不复用 production runner slots：

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

运行 quota-sensitive integration tests 时设置：

```bash
export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_RUNTIME_ROOT=/srv/agentics-test/runtime
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics-test/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
```

在 Linux 上，如果这些变量缺失、格式错误，或没有指向已准备好的
`/srv/agentics-test` quota root，quota-sensitive integration tests 会 fail fast。

`/srv/agentics-test` 和 `/srv/agentics` 故意分离，这样本地测试权限不会改变 hosted
worker slot ownership。

然后运行：

```bash
AGENTICS_ADMIN_PASSWORD='<admin-password>' \
AGENTICS_WEB_BASE_URL='https://<public-hostname>' \
agentics-check-local-mvp
```

最后针对 hosted endpoint 运行根目录 `README.md` 中的 CLI submitter flow，并用
`agentics submissions status` 查看 submitter-private status。

## Smoke Evidence

2026-05-13，`MapleSpark` 上 strict DGX-2 profile verification 和 DGX-3 hosted
application smoke 已通过。
2026-05-22，`MapleSpark` 上 CUDA image publication 和 DGX GPU worker smoke 已通过。

Smoke 覆盖：

- local MVP health checks；
- strict DGX profile checks；
- hosted CLI onboarding；
- `linux-arm64-cpu` 上的 matrix validation 和 official submission；
- no-egress runner enforcement；
- storage-quota escape failure；
- capacity 和 worker heartbeat inspection。

CUDA smoke 覆盖：

- 已发布的 `v0.2.5` `cu126`、`cu130` 和 `cu132` GHCR image digests；
- 每个 CUDA image 的 toolchain smoke；
- 使用 Docker `--gpus 1` 的每个 CUDA image GPU runtime smoke；
- ignored `cuda_smoke` integration test，覆盖 `linux-arm64-cuda` 上的 validation、
  official evaluation、result persistence 和 leaderboard update。

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
