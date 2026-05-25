set fallback := true

platform_db_compose := "docker compose -f docker/platform-db/docker-compose.yml"
crap_lcov_unit := "target/llvm-cov/agentics-workspace.lcov"
crap_lcov_integration := "target/llvm-cov/agentics-workspace-with-integration.lcov"
rustfs_container := "agentics-rustfs-test"
rustfs_volume := "agentics-rustfs-test-data"

# Install Git pre-commit hooks
setup-hooks:
    chmod +x .commit-hooks/pre-commit
    git config core.hooksPath .commit-hooks
    @echo "Pre-commit hooks installed."

# Start infrastructure (platform database)
infra-up:
    AGENTICS_POSTGRES_PORT="${AGENTICS_POSTGRES_PORT:-5432}" {{platform_db_compose}} up -d platform-db

# Stop infrastructure
infra-down:
    {{platform_db_compose}} down

# Start RustFS S3-compatible object storage for local storage tests
rustfs-up:
    docker volume create {{rustfs_volume}}
    docker rm -f {{rustfs_container}} >/dev/null 2>&1 || true
    @if [ "${AGENTICS_RUSTFS_DOCKER_NETWORK:-host}" = "host" ]; then \
      network_args="--network host"; \
    else \
      network_args="-p ${AGENTICS_RUSTFS_PORT:-9000}:9000 -p ${AGENTICS_RUSTFS_CONSOLE_PORT:-9001}:9001"; \
    fi; \
    docker run -d --name {{rustfs_container}} $network_args -v {{rustfs_volume}}:/data -e RUSTFS_ACCESS_KEY="${AGENTICS_RUSTFS_ACCESS_KEY:-agenticsrustfs}" -e RUSTFS_SECRET_KEY="${AGENTICS_RUSTFS_SECRET_KEY:-agenticsrustfssecret}" -e RUSTFS_CONSOLE_ENABLE=true rustfs/rustfs:latest /data

# Stop RustFS test service
rustfs-down:
    docker rm -f {{rustfs_container}} >/dev/null 2>&1 || true

# Stop RustFS test service and remove its named volume
rustfs-purge: rustfs-down
    docker volume rm {{rustfs_volume}} >/dev/null 2>&1 || true

# Run S3 storage tests against the local RustFS service
test-storage-s3:
    AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-${AGENTICS_RUSTFS_ACCESS_KEY:-agenticsrustfs}}" AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-${AGENTICS_RUSTFS_SECRET_KEY:-agenticsrustfssecret}}" AGENTICS_S3_TEST_ENDPOINT="${AGENTICS_S3_TEST_ENDPOINT:-http://127.0.0.1:${AGENTICS_RUSTFS_PORT:-9000}}" AGENTICS_S3_TEST_BUCKET="${AGENTICS_S3_TEST_BUCKET:-agentics-test}" AGENTICS_S3_FORCE_PATH_STYLE="${AGENTICS_S3_FORCE_PATH_STYLE:-true}" cargo test -p agentics-storage rustfs_s3_storage_round_trips_when_configured -- --nocapture

# Manage the Linux-only DGX Spark systemd deployment profile
dgx-profile *args:
    cargo build -p agentics-ops --bin agentics-manage-dgx-spark-profile
    sudo -E target/debug/agentics-manage-dgx-spark-profile {{args}}

# Start a local demo stack with seeded fake frontend results
local-demo *args:
    cargo run -p agentics-ops --bin agentics-local-demo -- {{args}}

# Run database migrations
migrate:
    cd backend && cargo sqlx migrate run

# Dev: API server
dev-api:
    cd backend && cargo run -p api-server

# Dev: evaluation worker
dev-worker:
    cd backend && cargo run -p worker

# Dev: Next.js frontend (separate service)
dev-web:
    cd frontends/web && AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" bun run dev -- -p "${AGENTICS_WEB_PORT:-3001}"

# Dev: all three in parallel (requires tmux or multiple terminals)
dev-all:
    @echo "Run these in separate terminals:"
    @echo "  just dev-api"
    @echo "  just dev-worker"
    @echo "  just dev-web"

# Rust unit + integration tests
test-rust:
    cd backend && cargo test --workspace

# Rust integration tests only
test-rust-integration:
    cd backend && cargo test -p integration-tests

# Rust coverage + CRAP report from unit and package tests only
rust-risk-unit:
    mkdir -p target/llvm-cov
    cargo llvm-cov --workspace --exclude integration-tests --lcov --output-path {{crap_lcov_unit}}
    cargo crap --workspace --lcov {{crap_lcov_unit}} --top "${AGENTICS_CRAP_TOP:-30}"

# Rust coverage + CRAP report including all integration tests
rust-risk-integration:
    mkdir -p target/llvm-cov
    @database_url="${DATABASE_URL:-${AGENTICS_DATABASE_URL:-}}"; \
      if [ -z "$database_url" ]; then \
        printf 'DATABASE_URL or AGENTICS_DATABASE_URL must be set for integration coverage.\n' >&2; \
        exit 2; \
      fi; \
      DATABASE_URL="$database_url" cargo llvm-cov --workspace --lcov --output-path {{crap_lcov_integration}} -- --include-ignored
    cargo crap --workspace --lcov {{crap_lcov_integration}} --top "${AGENTICS_CRAP_TOP:-30}"

# Frontend unit tests
test-web-unit:
    cd frontends/web && bun run test

# All tests
test-all: test-rust test-web-unit

# Lint Rust
cargo-fmt:
    cd backend && cargo fmt --all

cargo-clippy:
    cd backend && cargo clippy --workspace --all-targets -- -D warnings

# Lint frontend
web-lint:
    cd frontends/web && bun run lint

web-schema-check:
    cd frontends/web && bun run generate:schemas:check

web-format:
    cd frontends/web && bun run format

# Prepare sqlx offline queries
sqlx-prepare:
    cargo sqlx prepare --workspace

# Clean build artifacts
clean:
    cd backend && cargo clean
    cd frontends/web && rm -rf .next
