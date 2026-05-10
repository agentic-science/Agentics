# v0.2 CPU Base Image

Agentics provides a first-party CPU base image definition for solution and scorer
containers. The image source lives in `docker/images/cpu-base`. It is not
published from the local development checkout yet; published challenges should
switch to it only after a stable release image is pushed and digest-pinned.

## Contract

- Base OS: Ubuntu 26.04.
- Platforms: `linux/arm64` and `linux/amd64`.
- Scope: CPU solution setup/build/run phases and scorer prepare/score phases.
- MVP user model: root in setup, build, and run phases for participant
  simplicity.
- Labels: Agentics image version and Ubuntu version.
- Runtime metadata: `/opt/agentics/image-info.json`.
- Smoke check: `/opt/agentics/smoke.sh`.

The image includes common shell and core utilities, `curl`, `wget`,
`ca-certificates`, `git`, `build-essential`, `cmake`, `pkg-config`,
`ninja-build`, `uv`, `fnm`, Node, Bun, rustup with the stable Rust toolchain,
`apt-fast`, `aria2`, `jq`, `file`, `less`, `nano`, `vim-tiny`, `time`, and
`tini`.

## Participant Guidance

Use `apt-fast` instead of `apt-get` in setup/build scripts when installing apt
packages inside the Agentics CPU base image:

```sh
apt-fast update
apt-fast install -y --no-install-recommends libopenblas-dev
```

Use `uv` for Python dependency management. Use `fnm` when a solution needs a
Node version different from the image default:

```sh
eval "$(fnm env --shell bash)"
fnm install
fnm use
```

Use `rustup` for Rust toolchain components and target installation. The image
includes a default stable Rust toolchain.

## Challenge Author Guidance

Challenge authors may use the same Agentics CPU base image for `solution_image`
and `scorer_image` when the scorer is also CPU-only. Hosted deployments should
enable digest-pinned image enforcement, and challenge specs should reference the
published image by immutable digest.

Do not update active challenge specs to this image until it has been published
and smoke-tested on both supported CPU platforms.

## Build Locally

Build the current host architecture:

```bash
docker buildx build \
  --load \
  --platform "$(docker info --format '{{.OSType}}/{{.Architecture}}')" \
  -t agentics-cpu-base:ubuntu26.04-local \
  docker/images/cpu-base
```

Run the image smoke check:

```bash
docker run --rm agentics-cpu-base:ubuntu26.04-local /opt/agentics/smoke.sh
```

Prepare a multi-architecture OCI archive without publishing:

```bash
docker buildx build \
  --platform linux/arm64,linux/amd64 \
  --output type=oci,dest=/tmp/agentics-cpu-base-ubuntu26.04.oci \
  -t ghcr.io/agentics-reifying/agentics-cpu-base:ubuntu26.04-v0.1.0 \
  docker/images/cpu-base
```
