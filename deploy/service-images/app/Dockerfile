# syntax=docker/dockerfile:1.7

ARG RUST_BASE_IMAGE=rust:1-trixie@sha256:fb328f0f58becb23ba1719940a2c94ece8b0b48afa837d05b79ef64bc1e18f6e
FROM ${RUST_BASE_IMAGE} AS builder

ARG RUST_BASE_IMAGE=rust:1-trixie@sha256:fb328f0f58becb23ba1719940a2c94ece8b0b48afa837d05b79ef64bc1e18f6e
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
    -p agentics-api-server --bin api \
    -p agentics-worker --bin worker \
    -p agentics-ops --bin agentics-check-local-mvp \
    -p agentics-ops --bin agentics-check-dgx-spark-profile \
    -p agentics-ops --bin agentics-migrate \
    -p agentics-ops --bin agentics-compose-prod \
    -p agentics-ops --bin agentics-copy-private-bundle-backups \
    -p agentics-ops --bin agentics-rehearse-production

FROM debian:trixie-slim@sha256:b6e2a152f22a40ff69d92cb397223c906017e1391a73c952b588e51af8883bf8 AS runtime
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
COPY --from=builder /workspace/Agentics/challenge-repos/agentics-challenges/challenges /app/challenges
RUN chmod -R a+rX /app/challenges
ENV AGENTICS_CHALLENGES_ROOT=/app/challenges
