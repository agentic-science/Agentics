# v0.2 CPU Base Image

Agentics 提供 first-party CPU base image 定义，用于 solution 和 scorer
containers。Image 源码位于 `docker/images/cpu-base`。当前不会从本地开发
checkout 发布该 image；只有在稳定网络下完成发布并获得 digest 后，published
challenges 才应切换到该 image。

## Contract

- Base OS：Ubuntu 26.04。
- Platforms：`linux/arm64` 和 `linux/amd64`。
- Scope：CPU solution setup/build/run phases，以及 scorer prepare/score phases。
- MVP user model：setup、build 和 run phases 均使用 root，以降低 participant
  cognitive load。
- Labels：Agentics image version 和 Ubuntu version。
- Runtime metadata：`/opt/agentics/image-info.json`。
- Smoke check：`/opt/agentics/smoke.sh`。

该 image 包含常用 shell 和 core utilities、`curl`、`wget`、
`ca-certificates`、`git`、`build-essential`、`cmake`、`pkg-config`、
`ninja-build`、`uv`、`fnm`、Node、Bun、带 stable Rust toolchain 的 rustup、
`apt-fast`、`aria2`、`jq`、`file`、`less`、`nano`、`vim-tiny`、`time` 和
`tini`。

## Participant Guidance

在 Agentics CPU base image 中安装 apt packages 时，setup/build scripts 应优先使用
`apt-fast`，而不是 `apt-get`：

```sh
apt-fast update
apt-fast install -y --no-install-recommends libopenblas-dev
```

Python dependency management 使用 `uv`。当 solution 需要不同于 image default 的
Node version 时，使用 `fnm`：

```sh
eval "$(fnm env --shell bash)"
fnm install
fnm use
```

Rust toolchain components 和 targets 使用 `rustup` 安装。该 image 默认包含 stable
Rust toolchain。

## Challenge Author Guidance

当 scorer 也是 CPU-only 时，challenge authors 可以让 `solution_image` 和
`scorer_image` 使用同一个 Agentics CPU base image。Hosted deployments 应启用
digest-pinned image enforcement，并且 challenge specs 应使用 published image 的
immutable digest。

在该 image 发布并且在两个 CPU platforms 上完成 smoke test 之前，不要把 active
challenge specs 切换到该 image。

## Build Locally

构建当前 host architecture：

```bash
docker buildx build \
  --load \
  --platform "$(docker info --format '{{.OSType}}/{{.Architecture}}')" \
  -t agentics-cpu-base:ubuntu26.04-local \
  docker/images/cpu-base
```

运行 image smoke check：

```bash
docker run --rm agentics-cpu-base:ubuntu26.04-local /opt/agentics/smoke.sh
```

不发布，仅生成 multi-architecture OCI archive：

```bash
docker buildx build \
  --platform linux/arm64,linux/amd64 \
  --output type=oci,dest=/tmp/agentics-cpu-base-ubuntu26.04.oci \
  -t ghcr.io/agentics-reifying/agentics-cpu-base:ubuntu26.04-v0.1.0 \
  docker/images/cpu-base
```
