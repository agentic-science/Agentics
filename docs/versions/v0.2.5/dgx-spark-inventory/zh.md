# v0.2.5 DGX Spark Host Inventory

本文档记录 `M0.2.5-DGX-1` 的第一轮 DGX Spark host inventory。它记录当前 host
已经确认的信息，以及在该 host 上采集到的 DGX-2 deployment-profile evidence。

## 可重复检查

在 DGX host 上运行带 Linux gate 的 inventory script：

```bash
scripts/ops/check-dgx-spark-host.sh
```

该脚本会在非 Linux host 上退出，不会运行 DGX checks。若要运行 NVIDIA Docker
smoke check，需要使用能够访问 Docker daemon 的 operator account，并显式启用。
如果 Docker access 需要通过 sudo，设置
`AGENTICS_DGX_DOCKER_CLI='sudo -n docker'`：

```bash
AGENTICS_DGX_DOCKER_CLI='sudo -n docker' \
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:12.9.1-base-ubuntu24.04 \
scripts/ops/check-dgx-spark-host.sh
```

## 已采集 Host

采集时间为 2026-05-12 至 2026-05-13，host 为 `MapleSpark`。

| Area | Result |
| --- | --- |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) |
| Kernel | `6.17.0-1014-nvidia` |
| Architecture | `aarch64` |
| GPU | NVIDIA GB10 |
| NVIDIA driver | `580.142` |
| Driver reported CUDA | `13.0` |
| NVIDIA container toolkit | `nvidia-container-toolkit 1.19.0-1`，`libnvidia-container1 1.19.0-1` |
| Docker client | Docker Engine Community client `29.2.1`，Buildx `v0.31.1`，Compose `v5.0.2` |
| Default Docker server | 可通过 Docker-only passwordless sudo 读取；Docker Engine `29.2.1`，API `1.53`，`overlay2` on `extfs`，cgroup driver `systemd`，cgroup v2，Docker root `/var/lib/docker` |
| Default Docker runtimes | `io.containerd.runc.v2` 和 `runc`；虽然没有名为 `nvidia` 的 runtime，但安装的 NVIDIA toolkit/CDI path 可通过 `--gpus all` 使用 GPU |
| NVIDIA Docker smoke | `sudo -n docker run --rm --pull=never --network none --gpus all nvidia/cuda:12.9.1-base-ubuntu24.04 nvidia-smi` 已通过 |
| Agentics Docker daemon | 已运行在 `unix:///run/agentics/docker.sock`，group 为 `agentics`，Docker Engine `29.2.1`，`overlay2` on XFS，data root 为 `/srv/agentics/docker-data-root`，可见名为 `nvidia` 的 runtime，并使用独立 containerd namespaces `agentics` 和 `agentics-plugins` |
| Root storage | `/dev/nvme0n1p2` 以 `ext4` 挂载到 `/`，总容量 3.7 TiB，约 3.5 TiB 可用 |
| Current XFS mounts | `/srv/agentics/docker-data-root` 为 200 GiB，五个 `/srv/agentics/phase-mounts/*` mounts 各为 20 GiB，全部是带 `prjquota` 的 loopback XFS |
| XFS tools | 已安装 `mkfs.xfs`、`xfs_quota` 和 `xfs_info` |
| XFS kernel support | `modinfo xfs` 显示 NVIDIA kernel 有 in-tree XFS module |
| Loopback tools | 已安装 `losetup` 和 `truncate` |
| Current ingress | 本轮 inventory 未发现 Agentics public reverse proxy |

## Evidence

Host identity：

```text
Linux MapleSpark 6.17.0-1014-nvidia #14-Ubuntu SMP PREEMPT_DYNAMIC Tue Mar 17 19:01:40 UTC 2026 aarch64 aarch64 aarch64 GNU/Linux
```

OS release：

```text
PRETTY_NAME="Ubuntu 24.04.4 LTS"
VERSION_CODENAME=noble
```

GPU：

```text
NVIDIA-SMI 580.142
Driver Version: 580.142
CUDA Version: 13.0
GPU 0: NVIDIA GB10
```

当前 user 的 Docker access：

```text
uid=1000(maplespark) gid=1000(maplespark) groups=1000(maplespark),4(adm),27(sudo),29(audio),30(dip),46(plugdev),100(users),122(lpadmin)
srw-rw---- root docker /var/run/docker.sock
permission denied while trying to connect to the docker API at unix:///var/run/docker.sock
```

通过 Docker-only sudo 读取 default Docker 的 evidence：

```text
Client=29.2.1 Server=29.2.1 API=1.53
Storage Driver: overlay2
Backing Filesystem: extfs
Cgroup Driver: systemd
Cgroup Version: 2
Runtimes: io.containerd.runc.v2 runc
Docker Root Dir: /var/lib/docker
```

NVIDIA Docker smoke：

```text
sudo -n docker run --rm --pull=never --network none --gpus all nvidia/cuda:12.9.1-base-ubuntu24.04 nvidia-smi
NVIDIA-SMI 580.142
Driver Version: 580.142
CUDA Version: 13.0
GPU 0: NVIDIA GB10
```

Agentics-owned Docker evidence：

```text
srw-rw---- root agentics /run/agentics/docker.sock
Client=29.2.1 Server=29.2.1 API=1.53
Driver=overlay2
BackingFilesystem=xfs
DockerRootDir=/srv/agentics/docker-data-root
CgroupDriver=systemd
CgroupVersion=2
Runtimes=io.containerd.runc.v2,nvidia,runc
```

NVIDIA container runtime packages 和 dry-run configuration：

```text
libnvidia-container-tools     1.19.0-1     arm64
libnvidia-container1:arm64    1.19.0-1     arm64
nvidia-container-toolkit      1.19.0-1     arm64
nvidia-container-toolkit-base 1.19.0-1     arm64
```

`nvidia-ctk runtime configure --runtime=docker --dry-run` 会输出预期的 Docker
runtime stanza：

```json
{
  "runtimes": {
    "nvidia": {
      "args": [],
      "path": "nvidia-container-runtime"
    }
  }
}
```

Storage：

```text
/dev/nvme0n1p2 / ext4 rw,relatime,errors=remount-ro
NAME        TYPE   SIZE FSTYPE   MOUNTPOINTS
loop17      loop   200G xfs      /srv/agentics/docker-data-root
loop18      loop    20G xfs      /srv/agentics/phase-mounts/solution-setup
loop19      loop    20G xfs      /srv/agentics/phase-mounts/solution-build
loop20      loop    20G xfs      /srv/agentics/phase-mounts/solution-run
loop21      loop    20G xfs      /srv/agentics/phase-mounts/scorer-prepare
loop22      loop    20G xfs      /srv/agentics/phase-mounts/scorer-score
nvme0n1     disk   3.7T
├─nvme0n1p1 part   512M vfat     /boot/efi
└─nvme0n1p2 part   3.7T ext4     /
```

Root filesystem 不作为 hosted Docker data-root。Agentics daemon data root 和各个
phase writable mounts 都由带 project quotas 的 loopback XFS images 支撑。

## DGX-2 决策

除非 host policy 后续改变，DGX Spark deployment profile 使用以下路径：

| Purpose | Path or value |
| --- | --- |
| Agentics service user | `agentics` |
| Release root | `/opt/agentics` |
| Persistent state root | `/srv/agentics` |
| Storage root | `/srv/agentics/storage` |
| Challenge checkout root | `/srv/agentics/challenges` |
| Runtime root | `/srv/agentics/runtime` |
| Agentics-owned Docker socket | `unix:///run/agentics/docker.sock` |
| Agentics-owned Docker data-root mount | `/srv/agentics/docker-data-root` |
| Docker data-root loop image | `/srv/agentics/loop-images/docker-data-root.xfs` |
| Per-phase loop images | `/srv/agentics/loop-images/phase-*.xfs` |
| Strict probe mode | `AGENTICS_HOST_PROBE_MODE=require` |

不要在 operator 的 default Docker daemon 上运行 public jobs。DGX-2 应定义独立的
Agentics-owned Docker daemon，并在该 daemon 上证明 Docker writable-layer quota
行为。

## 剩余工作

DGX-1 inventory 不再有阻塞项。Default Docker daemon 仅用于 host inventory 和 GPU
smoke，不要在其上运行 public Agentics jobs，因为它的 data root 由 `ext4` 支撑。

Public traffic 前剩余的 deployment tasks：

- 保持 `/opt/agentics/current` 下已安装 release 和 `/etc/agentics/agentics.env`
  与每次 promoted build 对齐。
- 配置 public ingress、TLS termination、DNS 和 operator-only admin access。

## 下一步

DGX deployment profile 已记录在
`docs/versions/v0.2.5/dgx-spark-deployment/zh.md`。DGX-3 hosted smoke evidence
已记录在 `docs/versions/v0.2.5/dgx-spark-smoke/zh.md`；接下来继续完成 public
ingress、DNS、TLS 和 operator access cutover。
