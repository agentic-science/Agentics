# Agentics Linux ARM64 CUDA Base Image

This directory defines first-party Agentics CUDA devel base images for the
`linux-arm64-cuda` target. These images are intended for solution and evaluator
containers on DGX Spark-class hosted deployments. They are intentionally not
published from this checkout yet. Publish only from a stable network, smoke on
the supported DGX host, and pin challenge specs to immutable digests.

## Image Contract

- Target: `linux-arm64-cuda`.
- Platform: `linux/arm64`.
- Base: NVIDIA CUDA `devel` images on Ubuntu 24.04.
- CUDA policy: maintain variants that match CUDA versions supported by the
  latest stable PyTorch release, subject to NVIDIA `linux/arm64` image
  availability and DGX smoke validation.
- PyTorch policy: PyTorch is not included. Challenge owners and participants
  install their chosen PyTorch build or another CUDA framework in setup/build.
- Intended use: CUDA solution setup, build, run, evaluator prepare, and evaluator
  score phases.
- User model: root for setup, build, and run in the MVP.
- Docker labels: Agentics image version, target, CUDA variant, CUDA version,
  CUDA base image, and Ubuntu version.
- Runtime metadata: `/opt/agentics/image-info.json`.
- Smoke test: `/opt/agentics/smoke.sh`.

Installed tooling matches the CPU base image where practical: shell/core
utilities, network tools, build tools, `apt-fast` with `aria2`, `uv`, `fnm`,
Node, Bun, rustup with the stable Rust toolchain, CUDA compiler/runtime headers,
`jq`, `file`, `less`, `nano`, `vim-tiny`, `time`, and `tini`.

## Current CUDA Variants

The current variant table is kept in `variants.toml`.

| Variant | CUDA base image | Image digest | Status |
| --- | --- | --- | --- |
| `cu126` | `nvidia/cuda:12.6.3-devel-ubuntu24.04` | `sha256:392c0df7b577ecae17a17f6ba7f2009c217bb4422f8431c053ae9af61a8c148a` | Active |
| `cu130` | `nvidia/cuda:13.0.1-devel-ubuntu24.04` | `sha256:7d2f6a8c2071d911524f95061a0db363e24d27aa51ec831fcccf9e76eb72bc92` | Active |
| `cu132` | `nvidia/cuda:13.2.0-devel-ubuntu24.04` | `sha256:f9492f2eea77fbc3d0c14fa8738f35946b42da72917bf5959d284ca39b4f209a` | Active |

The table records NVIDIA manifest-list digests inspected with `docker buildx
imagetools inspect`. Official Agentics image releases should also record the
resulting Agentics image digest after build and smoke.

CUDA variants do not create separate leaderboard scopes. They are resource
profile and image choices under the `linux-arm64-cuda` target. Challenge owners
are responsible for keeping a challenge's benchmark contract comparable when
they choose or change a CUDA variant.

## Local Build

Build the default CUDA 13.0 variant for the current host architecture:

```bash
docker buildx build \
  --load \
  --platform linux/arm64 \
  -t agentics-linux-arm64-cuda:cu130-ubuntu24.04-local \
  docker/images/linux-arm64-cuda
```

On DGX operator hosts where Docker's default bridge device is unavailable, add
`--network host` to build commands so package installation does not depend on
`docker0`.

Build a different active variant:

```bash
docker buildx build \
  --load \
  --platform linux/arm64 \
  --build-arg CUDA_BASE_IMAGE=nvidia/cuda:13.2.0-devel-ubuntu24.04 \
  --build-arg CUDA_VARIANT=cu132 \
  --build-arg CUDA_VERSION=13.2 \
  -t agentics-linux-arm64-cuda:cu132-ubuntu24.04-local \
  docker/images/linux-arm64-cuda
```

Run the toolchain-only smoke check:

```bash
docker run --rm --network none \
  agentics-linux-arm64-cuda:cu130-ubuntu24.04-local \
  /opt/agentics/smoke.sh
```

Run the required DGX GPU smoke check:

```bash
docker run --rm \
  --network none \
  --gpus all \
  -e AGENTICS_GPU_SMOKE_REQUIRE_DEVICE=1 \
  agentics-linux-arm64-cuda:cu130-ubuntu24.04-local \
  /opt/agentics/smoke.sh
```

## Release Notes

For official release builds, use concrete tool versions and the digest-pinned
NVIDIA base image from `variants.toml`:

```bash
docker buildx build \
  --platform linux/arm64 \
  --build-arg AGENTICS_IMAGE_VERSION=0.1.0 \
  --build-arg CUDA_BASE_IMAGE=nvidia/cuda:13.0.1-devel-ubuntu24.04@sha256:7d2f6a8c2071d911524f95061a0db363e24d27aa51ec831fcccf9e76eb72bc92 \
  --build-arg CUDA_VARIANT=cu130 \
  --build-arg CUDA_VERSION=13.0 \
  --build-arg UBUNTU_VERSION=24.04 \
  --build-arg NODE_VERSION=<concrete-node-version> \
  --build-arg BUN_VERSION=<concrete-bun-version> \
  -t ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.1.0 \
  docker/images/linux-arm64-cuda
```

Record the built image digest, `/opt/agentics/image-info.json`, and DGX smoke
output in release notes. Challenge specs must use the supported
`agentics-linux-arm64-cuda` image repository with a tag that starts with the
declared CUDA variant, such as `cu130-*`. Local development may use
`source: "local"` for first-party local tags. Hosted challenge specs must use
`source: "registry"` and digest-pinned `solution_image` and `evaluator_image`
references after the image is published.
