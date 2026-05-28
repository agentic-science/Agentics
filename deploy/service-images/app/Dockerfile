# syntax=docker/dockerfile:1.7

ARG RUST_BASE_IMAGE=rust:1-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1
FROM ${RUST_BASE_IMAGE} AS builder

ARG RUST_BASE_IMAGE=rust:1-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1
ARG LLVM_MAJOR=22
ARG WILD_LINKER_VERSION=0.9.0

ENV HOMEBREW_PREFIX=/home/linuxbrew/.linuxbrew \
    HOMEBREW_NO_ANALYTICS=1 \
    HOMEBREW_NO_ENV_HINTS=1 \
    CARGO_HOME=/opt/cargo \
    PATH=/home/linuxbrew/.linuxbrew/opt/llvm/bin:/home/linuxbrew/.linuxbrew/opt/lld/bin:/home/linuxbrew/.linuxbrew/bin:/home/linuxbrew/.linuxbrew/sbin:/opt/cargo/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

WORKDIR /workspace/Agentics

COPY deploy/service-images/rust-toolchain/install-toolchain.sh /usr/local/bin/agentics-install-rust-toolchain
COPY deploy/service-images/rust-toolchain/smoke.sh /opt/agentics/smoke-rust-toolchain.sh
RUN chmod +x /usr/local/bin/agentics-install-rust-toolchain /opt/agentics/smoke-rust-toolchain.sh \
    && LLVM_MAJOR="${LLVM_MAJOR}" \
       WILD_LINKER_VERSION="${WILD_LINKER_VERSION}" \
       RUST_BASE_IMAGE="${RUST_BASE_IMAGE}" \
       /usr/local/bin/agentics-install-rust-toolchain \
    && LLVM_MAJOR="${LLVM_MAJOR}" /opt/agentics/smoke-rust-toolchain.sh

COPY . .
RUN cargo build --release \
    -p api-server --bin api \
    -p worker --bin worker \
    -p agentics-ops --bin agentics-check-local-mvp \
    -p agentics-ops --bin agentics-check-dgx-spark-profile \
    -p agentics-ops --bin agentics-migrate \
    -p agentics-ops --bin agentics-compose-prod \
    -p agentics-ops --bin agentics-copy-private-bundle-backups \
    -p agentics-ops --bin agentics-rehearse-production

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git xfsprogs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /workspace/Agentics/target/release/api /usr/local/bin/api
COPY --from=builder /workspace/Agentics/target/release/worker /usr/local/bin/worker
COPY --from=builder /workspace/Agentics/target/release/agentics-check-local-mvp /usr/local/bin/agentics-check-local-mvp
COPY --from=builder /workspace/Agentics/target/release/agentics-check-dgx-spark-profile /usr/local/bin/agentics-check-dgx-spark-profile
COPY --from=builder /workspace/Agentics/target/release/agentics-migrate /usr/local/bin/agentics-migrate
COPY --from=builder /workspace/Agentics/target/release/agentics-compose-prod /usr/local/bin/agentics-compose-prod
COPY --from=builder /workspace/Agentics/target/release/agentics-copy-private-bundle-backups /usr/local/bin/agentics-copy-private-bundle-backups
COPY --from=builder /workspace/Agentics/target/release/agentics-rehearse-production /usr/local/bin/agentics-rehearse-production
ENV AGENTICS_CHALLENGES_ROOT=/app/no-seeded-challenges
