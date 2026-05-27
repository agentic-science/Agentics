# syntax=docker/dockerfile:1.7

ARG RUST_BASE_IMAGE=rust:1-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1
FROM ${RUST_BASE_IMAGE}

ARG RUST_BASE_IMAGE=rust:1-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1
ARG LLVM_MAJOR=22
ARG WILD_LINKER_VERSION=0.9.0

ENV HOMEBREW_PREFIX=/home/linuxbrew/.linuxbrew \
    HOMEBREW_NO_ANALYTICS=1 \
    CARGO_HOME=/opt/cargo \
    PATH=/home/linuxbrew/.linuxbrew/opt/llvm/bin:/home/linuxbrew/.linuxbrew/bin:/home/linuxbrew/.linuxbrew/sbin:/opt/cargo/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

SHELL ["/bin/bash", "-euxo", "pipefail", "-c"]

COPY install-toolchain.sh /usr/local/bin/agentics-install-rust-toolchain
COPY smoke.sh /opt/agentics/smoke-rust-toolchain.sh

RUN chmod +x /usr/local/bin/agentics-install-rust-toolchain /opt/agentics/smoke-rust-toolchain.sh \
    && LLVM_MAJOR="${LLVM_MAJOR}" \
       WILD_LINKER_VERSION="${WILD_LINKER_VERSION}" \
       RUST_BASE_IMAGE="${RUST_BASE_IMAGE}" \
       /usr/local/bin/agentics-install-rust-toolchain \
    && LLVM_MAJOR="${LLVM_MAJOR}" /opt/agentics/smoke-rust-toolchain.sh

WORKDIR /workspace/Agentics
CMD ["bash"]
