# v0.2.5 DGX Spark Host Inventory

本文档记录 `M0.2.5-DGX-1` 的第一轮 DGX Spark host inventory。它记录当前 host
已经确认的信息，以及在最终确定 deployment profile 前仍然阻塞的部分。

## 可重复检查

在 DGX host 上运行带 Linux gate 的 inventory script：

```bash
scripts/ops/check-dgx-spark-host.sh
```

该脚本会在非 Linux host 上退出，不会运行 DGX checks。若要运行 NVIDIA Docker
smoke check，需要使用能够访问 Docker daemon 的 operator account，并显式启用：

```bash
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=missing \
scripts/ops/check-dgx-spark-host.sh
```

## 已采集 Host

采集时间为 2026-05-12，host 为 `MapleSpark`。

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
| Docker server | 当前 user 无法读取，因为 `/var/run/docker.sock` 是 `root:docker`，且当前 user 不在 `docker` group 中 |
| Root storage | `/dev/nvme0n1p2` 以 `ext4` 挂载到 `/`，总容量 3.7 TiB，约 3.5 TiB 可用 |
| Current XFS mounts | 未发现 |
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
nvme0n1     disk   3.7T
├─nvme0n1p1 part   512M vfat     /boot/efi
└─nvme0n1p2 part   3.7T ext4     /
```

Root filesystem 不是计划中的 hosted Docker data-root。Docker writable-layer
quota validation 仍需要 Agentics-owned Docker daemon 和 loopback XFS data-root。

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

## Blockers

- 当前 user 无法验证 Docker server details、Docker storage driver、Docker runtime
  list 和 NVIDIA Docker smoke command。
- 当前 user 属于 sudo group，但本 shell 不支持 passwordless sudo。Operator 必须用
  合适权限运行 inventory script，或为 deployment user 配置正确的 Docker access
  path。
- Loopback XFS project-quota behavior 尚未挂载或验证。DGX-2 必须创建 loop image，
  用 project quotas 挂载，并在 public worker execution 前运行 quota probes。
- Public ingress、TLS termination、DNS 和 operator-only admin access 仍是 DGX-2
  deployment-profile 决策。

## 下一步

Docker daemon access 可用后进入 `M0.2.5-DGX-2`。DGX-2 应添加 deployment profile
和 strict Linux-host probes，并在 `AGENTICS_HOST_PROBE_MODE=require` 时 fail
closed。
