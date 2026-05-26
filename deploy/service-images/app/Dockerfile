# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS builder
WORKDIR /workspace/Agentics

COPY . .
RUN cargo build --release \
    -p api-server --bin api \
    -p worker --bin worker \
    -p agentics-ops --bin agentics-check-local-mvp \
    -p agentics-ops --bin agentics-check-dgx-spark-profile \
    -p agentics-ops --bin agentics-migrate \
    -p agentics-ops --bin agentics-compose-prod

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates xfsprogs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /workspace/Agentics/target/release/api /usr/local/bin/api
COPY --from=builder /workspace/Agentics/target/release/worker /usr/local/bin/worker
COPY --from=builder /workspace/Agentics/target/release/agentics-check-local-mvp /usr/local/bin/agentics-check-local-mvp
COPY --from=builder /workspace/Agentics/target/release/agentics-check-dgx-spark-profile /usr/local/bin/agentics-check-dgx-spark-profile
COPY --from=builder /workspace/Agentics/target/release/agentics-migrate /usr/local/bin/agentics-migrate
COPY --from=builder /workspace/Agentics/target/release/agentics-compose-prod /usr/local/bin/agentics-compose-prod
COPY --from=builder /workspace/Agentics/examples /app/examples

ENV AGENTICS_CHALLENGES_ROOT=/app/examples/challenges
