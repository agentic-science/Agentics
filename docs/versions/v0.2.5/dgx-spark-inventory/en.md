# v0.2.5 DGX Spark Host Inventory

This document captures the first DGX Spark host inventory for `M0.2.5-DGX-1`.
It records the current host facts and the DGX-2 deployment-profile evidence
captured on the host.

## Repeatable Check

Run the Linux-gated inventory script on the DGX host:

```bash
scripts/ops/check-dgx-spark-host.sh
```

The script exits before running any DGX checks on non-Linux hosts. To run the
NVIDIA Docker smoke check, use an operator account that can access the Docker
daemon and opt in explicitly. If Docker access is available through sudo, set
`AGENTICS_DGX_DOCKER_CLI='sudo -n docker'`:

```bash
AGENTICS_DGX_DOCKER_CLI='sudo -n docker' \
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=never \
AGENTICS_DGX_CUDA_IMAGE=nvidia/cuda:12.9.1-base-ubuntu24.04 \
scripts/ops/check-dgx-spark-host.sh
```

## Captured Host

Captured on May 12-13, 2026 from host `MapleSpark`.

| Area | Result |
| --- | --- |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) |
| Kernel | `6.17.0-1014-nvidia` |
| Architecture | `aarch64` |
| GPU | NVIDIA GB10 |
| NVIDIA driver | `580.142` |
| CUDA reported by driver | `13.0` |
| NVIDIA container toolkit | `nvidia-container-toolkit 1.19.0-1`, `libnvidia-container1 1.19.0-1` |
| Docker client | Docker Engine Community client `29.2.1`, Buildx `v0.31.1`, Compose `v5.0.2` |
| Default Docker server | Readable through Docker-only sudo; Docker Engine `29.2.1`, API `1.53`, `overlay2` on `extfs`, cgroup driver `systemd`, cgroup v2, Docker root `/var/lib/docker` |
| Default Docker runtimes | `io.containerd.runc.v2` and `runc`; NVIDIA GPU access works through the installed toolkit/CDI path even though `nvidia` is not listed as a named runtime |
| NVIDIA Docker smoke | Passes with `sudo -n docker run --rm --pull=never --network none --gpus all nvidia/cuda:12.9.1-base-ubuntu24.04 nvidia-smi` |
| Agentics Docker daemon | Running at `unix:///run/agentics/docker.sock`, group `agentics`, Docker Engine `29.2.1`, `overlay2` on XFS, data root `/srv/agentics/docker-data-root`, named `nvidia` runtime visible, separate containerd namespaces `agentics` and `agentics-plugins` |
| Root storage | `/dev/nvme0n1p2` mounted at `/` as `ext4`, 3.7 TiB total, about 3.5 TiB free |
| Current XFS mounts | `/srv/agentics/docker-data-root` at 200 GiB and five `/srv/agentics/phase-mounts/*` mounts at 20 GiB each, all loopback XFS with `prjquota` |
| Runner quota slots | Each phase mount has 64 MiB, 256 MiB, 1 GiB, and 4 GiB XFS project-quota slots, four slots per class |
| XFS tools | `mkfs.xfs`, `xfs_quota`, and `xfs_info` are installed |
| XFS kernel support | `modinfo xfs` reports an in-tree XFS module for the NVIDIA kernel |
| Loopback tools | `losetup` and `truncate` are installed |
| Current ingress | No Agentics public reverse proxy was identified during this inventory |

## Evidence

Host identity:

```text
Linux MapleSpark 6.17.0-1014-nvidia #14-Ubuntu SMP PREEMPT_DYNAMIC Tue Mar 17 19:01:40 UTC 2026 aarch64 aarch64 aarch64 GNU/Linux
```

OS release:

```text
PRETTY_NAME="Ubuntu 24.04.4 LTS"
VERSION_CODENAME=noble
```

GPU:

```text
NVIDIA-SMI 580.142
Driver Version: 580.142
CUDA Version: 13.0
GPU 0: NVIDIA GB10
```

Docker access from the current user:

```text
uid=1000(maplespark) gid=1000(maplespark) groups=1000(maplespark),4(adm),27(sudo),29(audio),30(dip),46(plugdev),100(users),122(lpadmin)
srw-rw---- root docker /var/run/docker.sock
permission denied while trying to connect to the docker API at unix:///var/run/docker.sock
```

Default Docker evidence through Docker-only sudo:

```text
Client=29.2.1 Server=29.2.1 API=1.53
Storage Driver: overlay2
Backing Filesystem: extfs
Cgroup Driver: systemd
Cgroup Version: 2
Runtimes: io.containerd.runc.v2 runc
Docker Root Dir: /var/lib/docker
```

NVIDIA Docker smoke:

```text
sudo -n docker run --rm --pull=never --network none --gpus all nvidia/cuda:12.9.1-base-ubuntu24.04 nvidia-smi
NVIDIA-SMI 580.142
Driver Version: 580.142
CUDA Version: 13.0
GPU 0: NVIDIA GB10
```

Agentics-owned Docker evidence:

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

NVIDIA container runtime packages and dry-run configuration:

```text
libnvidia-container-tools     1.19.0-1     arm64
libnvidia-container1:arm64    1.19.0-1     arm64
nvidia-container-toolkit      1.19.0-1     arm64
nvidia-container-toolkit-base 1.19.0-1     arm64
```

`nvidia-ctk runtime configure --runtime=docker --dry-run` emits the expected
Docker runtime stanza:

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

Storage:

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

The root filesystem is not used as the hosted Docker data-root. The Agentics
daemon data root and phase writable mounts are backed by loopback XFS images
mounted with project quotas.

## Decisions For DGX-2

Use these paths for the DGX Spark deployment profile unless host policy changes:

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
| Runner quota slot root | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| Strict probe mode | `AGENTICS_HOST_PROBE_MODE=require` |

Do not run public jobs on the operator's default Docker daemon. The Agentics
Docker daemon and root-prepared runner quota slots are the hosted storage
boundary for public jobs.

## Remaining Work

No DGX-1 inventory blockers remain. The default Docker daemon is verified for
host inventory and GPU smoke only; do not run public Agentics jobs on it because
its data root is backed by `ext4`.

Remaining deployment tasks before public traffic:

- Keep the installed release under `/opt/agentics/current` and
  `/etc/agentics/agentics.env` aligned with each promoted build.
- Configure public ingress, TLS termination, DNS, and operator-only admin
  access.

## Next Step

The DGX deployment profile is documented in
`docs/versions/v0.2.5/dgx-spark-deployment/en.md`. The DGX-3 hosted smoke
evidence is documented in `docs/versions/v0.2.5/dgx-spark-smoke/en.md`; continue
with public ingress, DNS, TLS, and operator access cutover.
