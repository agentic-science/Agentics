# Agentics Linux ARM64 CPU Base Image

This directory defines the first-party Agentics CPU base image for solution and
evaluator containers. It is intentionally not published from this checkout yet.
Publish only from a stable network and pin challenge specs to immutable digests.

## Image Contract

- Base: Ubuntu 26.04.
- Target: `linux-arm64-cpu`.
- MVP platform: `linux/arm64`.
- Intended use: CPU solution setup, build, run, evaluator prepare, and evaluator score
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
  -t agentics-linux-arm64-cpu:ubuntu26.04-local \
  docker/images/linux-arm64-cpu
```

On DGX operator hosts where Docker's default bridge device is unavailable, add
`--network host` to build commands so package installation does not depend on
`docker0`.

Run the smoke check:

```bash
docker run --rm --network none \
  agentics-linux-arm64-cpu:ubuntu26.04-local \
  /opt/agentics/smoke.sh
```

Prepare an MVP ARM64 OCI archive without publishing:

```bash
docker buildx build \
  --platform linux/arm64 \
  --output type=oci,dest=/tmp/agentics-linux-arm64-cpu-ubuntu26.04.oci \
  -t ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0 \
  docker/images/linux-arm64-cpu
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
  -t ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0 \
  docker/images/linux-arm64-cpu
```

Challenge specs may use `source: "local"` for the local tag above during
development. Hosted challenge specs must use `source: "registry"` with the
published `ghcr.io/agentic-science/agentics-linux-arm64-cpu` repository, an
`ubuntu26.04-*` tag, and digest-pinned `solution_image` and `evaluator_image`
references after the image is published.

Do not publish `linux-amd64-cpu` variants until the platform has AMD64 Linux
deployment capacity. Add a separate target-named image directory when that
target is supported.
