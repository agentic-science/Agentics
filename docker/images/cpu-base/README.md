# Agentics CPU Base Image

This directory defines the first-party Agentics CPU base image for solution and
scorer containers. It is intentionally not published from this checkout yet.
Publish only from a stable network and pin challenge specs to immutable digests.

## Image Contract

- Base: Ubuntu 26.04.
- MVP platform: `linux/arm64`.
- Post-MVP platform: `linux/amd64`.
- Intended use: CPU solution setup, build, run, scorer prepare, and scorer score
  phases.
- User model: root for setup, build, and run in the MVP.
- Docker labels: Agentics image version and Ubuntu version.
- Runtime metadata: `/opt/agentics/image-info.json`.
- Smoke test: `/opt/agentics/smoke.sh`.

Installed tooling includes shell/core utilities, network tools, build tools,
`apt-fast` with `aria2`, `uv`, `fnm`, Node, Bun, rustup with the stable Rust
toolchain, `jq`, `file`, `less`, `nano`, `vim-tiny`, `time`, and `tini`.

## Local Build

Build the current host architecture only:

```bash
docker buildx build \
  --load \
  --platform "$(docker info --format '{{.OSType}}/{{.Architecture}}')" \
  -t agentics-cpu-base:ubuntu26.04-local \
  docker/images/cpu-base
```

Run the smoke check:

```bash
docker run --rm agentics-cpu-base:ubuntu26.04-local /opt/agentics/smoke.sh
```

Prepare an MVP ARM64 OCI archive without publishing:

```bash
docker buildx build \
  --platform linux/arm64 \
  --output type=oci,dest=/tmp/agentics-cpu-base-ubuntu26.04.oci \
  -t ghcr.io/agentics-reifying/agentics-cpu-base:ubuntu26.04-v0.1.0 \
  docker/images/cpu-base
```

## Release Notes

For official release builds, set concrete tool versions through build arguments
and record the resulting `/opt/agentics/image-info.json` in release notes:

```bash
docker buildx build \
  --platform linux/arm64 \
  --build-arg AGENTICS_IMAGE_VERSION=0.1.0 \
  --build-arg UBUNTU_VERSION=26.04 \
  --build-arg NODE_VERSION=<concrete-node-version> \
  --build-arg BUN_VERSION=<concrete-bun-version> \
  -t ghcr.io/agentics-reifying/agentics-cpu-base:ubuntu26.04-v0.1.0 \
  docker/images/cpu-base
```

Challenge specs should use digest-pinned `solution_image` and `scorer_image`
references after the image is published.

Do not publish `linux/amd64` variants until the platform has AMD64 Linux
deployment capacity.
