# v0.2.5 DGX Spark Host Inventory

This document captures the first DGX Spark host inventory for `M0.2.5-DGX-1`.
It records what is known from the current host and what remains blocked before
the deployment profile can be finalized.

## Repeatable Check

Run the Linux-gated inventory script on the DGX host:

```bash
scripts/ops/check-dgx-spark-host.sh
```

The script exits before running any DGX checks on non-Linux hosts. To run the
NVIDIA Docker smoke check, use an operator account that can access the Docker
daemon and opt in explicitly:

```bash
AGENTICS_DGX_RUN_DOCKER_SMOKE=1 \
AGENTICS_DGX_DOCKER_PULL_POLICY=missing \
scripts/ops/check-dgx-spark-host.sh
```

## Captured Host

Captured on May 12, 2026 from host `MapleSpark`.

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
| Docker server | Not readable from the current user because `/var/run/docker.sock` is `root:docker` and the current user is not in the `docker` group |
| Root storage | `/dev/nvme0n1p2` mounted at `/` as `ext4`, 3.7 TiB total, about 3.5 TiB free |
| Current XFS mounts | None found |
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
nvme0n1     disk   3.7T
├─nvme0n1p1 part   512M vfat     /boot/efi
└─nvme0n1p2 part   3.7T ext4     /
```

The root filesystem is not the planned hosted Docker data-root. Docker
writable-layer quota validation still needs the Agentics-owned Docker daemon
with a loopback XFS data-root.

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
| Strict probe mode | `AGENTICS_HOST_PROBE_MODE=require` |

Do not run public jobs on the operator's default Docker daemon. DGX-2 should
define a separate Agentics-owned Docker daemon and prove Docker writable-layer
quota behavior there.

## Blockers

- Docker server details, Docker storage driver, Docker runtime list, and the
  NVIDIA Docker smoke command could not be verified from the current user.
- The current user has sudo membership, but passwordless sudo is not available
  in this shell. An operator must either run the inventory script with suitable
  privileges or add the deployment user to the correct Docker access path.
- Loopback XFS project-quota behavior is not mounted or proven yet. DGX-2 must
  create the loop image, mount it with project quotas, and run quota probes
  before public worker execution.
- Public ingress, TLS termination, DNS, and operator-only admin access remain
  deployment-profile decisions for DGX-2.

## Next Step

Proceed to `M0.2.5-DGX-2` after Docker daemon access is available. DGX-2 should
add the deployment profile and the strict Linux-host probes that fail closed
when `AGENTICS_HOST_PROBE_MODE=require`.
