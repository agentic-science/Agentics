# syntax=docker/dockerfile:1.7

ARG UBUNTU_VERSION=26.04
FROM ubuntu:${UBUNTU_VERSION}

ARG AGENTICS_IMAGE_VERSION=0.1.0
ARG UBUNTU_VERSION=26.04
ARG NODE_VERSION=24
ARG BUN_VERSION=latest

LABEL org.opencontainers.image.title="Agentics CPU Base" \
      org.opencontainers.image.description="First-party Agentics CPU base image for solution and evaluator containers" \
      ai.agentics.image.version="${AGENTICS_IMAGE_VERSION}" \
      ai.agentics.target.name="linux-arm64-cpu" \
      ai.agentics.ubuntu.version="${UBUNTU_VERSION}"

ENV DEBIAN_FRONTEND=noninteractive \
    FNM_DIR=/opt/fnm \
    BUN_INSTALL=/opt/bun \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/fnm:/opt/bun/bin:/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

SHELL ["/bin/bash", "-euxo", "pipefail", "-c"]

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      apt-transport-https \
      aria2 \
      bash \
      build-essential \
      ca-certificates \
      cmake \
      coreutils \
      curl \
      dash \
      dirmngr \
      file \
      findutils \
      gawk \
      git \
      gnupg \
      grep \
      gzip \
      jq \
      less \
      nano \
      ninja-build \
      pkg-config \
      python-is-python3 \
      python3 \
      python3-venv \
      sed \
      software-properties-common \
      tar \
      time \
      tini \
      unzip \
      vim-tiny \
      wget \
      xz-utils \
      zip \
    && rm -rf /var/lib/apt/lists/*

RUN printf '%s\n' \
      'apt-fast apt-fast/maxdownloads string 16' \
      'apt-fast apt-fast/dlflag boolean true' \
      'apt-fast apt-fast/aptmanager string apt-get' \
      | debconf-set-selections \
    && add-apt-repository -y ppa:apt-fast/stable \
    && apt-get update \
    && apt-get install -y --no-install-recommends apt-fast \
    && printf '%s\n' \
      '_APTMGR=apt-get' \
      '_DOWNLOADBEFORE=true' \
      '_MAXNUM=16' \
      'MIRRORS=()' \
      >> /etc/apt-fast.conf \
    && rm -rf /var/lib/apt/lists/*

RUN curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR=/usr/local/bin sh

RUN mkdir -p "${FNM_DIR}" \
    && curl -fsSL https://fnm.vercel.app/install \
      | bash -s -- --install-dir "${FNM_DIR}" --skip-shell --force-install \
    && chmod +x "${FNM_DIR}/fnm" \
    && eval "$("${FNM_DIR}/fnm" env --shell bash)" \
    && fnm install "${NODE_VERSION}" \
    && fnm use "${NODE_VERSION}" \
    && resolved_node_version="$(fnm current)" \
    && node_bin_dir="$(dirname "$(fnm exec --using "${resolved_node_version}" which node)")" \
    && ln -sf "${node_bin_dir}/node" /usr/local/bin/node \
    && ln -sf "${node_bin_dir}/npm" /usr/local/bin/npm \
    && ln -sf "${node_bin_dir}/npx" /usr/local/bin/npx \
    && if [[ -x "${node_bin_dir}/corepack" ]]; then ln -sf "${node_bin_dir}/corepack" /usr/local/bin/corepack; fi

RUN if [[ "${BUN_VERSION}" == "latest" ]]; then \
      curl -fsSL https://bun.sh/install | bash; \
    else \
      curl -fsSL https://bun.sh/install | bash -s "bun-v${BUN_VERSION}"; \
    fi \
    && ln -sf "${BUN_INSTALL}/bin/bun" /usr/local/bin/bun \
    && ln -sf "${BUN_INSTALL}/bin/bun" /usr/local/bin/bunx

RUN curl -fsSL https://sh.rustup.rs \
      | sh -s -- -y --profile default --default-toolchain stable \
    && chmod -R a+rX "${RUSTUP_HOME}" "${CARGO_HOME}"

RUN printf '%s\n' \
      'export FNM_DIR=/opt/fnm' \
      'export BUN_INSTALL=/opt/bun' \
      'export RUSTUP_HOME=/opt/rustup' \
      'export CARGO_HOME=/opt/cargo' \
      'export PATH=/opt/fnm:/opt/bun/bin:/opt/cargo/bin:$PATH' \
      > /etc/profile.d/agentics-toolchains.sh

RUN mkdir -p /opt/agentics \
    && printf '{\n' > /opt/agentics/image-info.json \
    && printf '  "agentics_image_version": "%s",\n' "${AGENTICS_IMAGE_VERSION}" >> /opt/agentics/image-info.json \
    && printf '  "target": "linux-arm64-cpu",\n' >> /opt/agentics/image-info.json \
    && printf '  "ubuntu_version": "%s",\n' "${UBUNTU_VERSION}" >> /opt/agentics/image-info.json \
    && printf '  "architecture": "%s",\n' "$(dpkg --print-architecture)" >> /opt/agentics/image-info.json \
    && printf '  "python_version": "%s",\n' "$(python --version | awk '{print $2}')" >> /opt/agentics/image-info.json \
    && printf '  "uv_version": "%s",\n' "$(uv --version | awk '{print $2}')" >> /opt/agentics/image-info.json \
    && printf '  "node_version": "%s",\n' "$(node --version)" >> /opt/agentics/image-info.json \
    && printf '  "bun_version": "%s",\n' "$(bun --version)" >> /opt/agentics/image-info.json \
    && printf '  "rustc_version": "%s",\n' "$(rustc --version)" >> /opt/agentics/image-info.json \
    && printf '  "cargo_version": "%s"\n' "$(cargo --version)" >> /opt/agentics/image-info.json \
    && printf '}\n' >> /opt/agentics/image-info.json

COPY smoke.sh /opt/agentics/smoke.sh
RUN chmod +x /opt/agentics/smoke.sh

WORKDIR /workspace
CMD ["bash"]
