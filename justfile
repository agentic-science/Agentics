set fallback := true

platform_db_compose := "docker compose -f docker/platform-db/docker-compose.yml"

# Install Git pre-commit hooks
setup-hooks:
    chmod +x .commit-hooks/pre-commit
    git config core.hooksPath .commit-hooks
    @echo "Pre-commit hooks installed."

# Start infrastructure (platform database)
infra-up:
    {{platform_db_compose}} up -d platform-db

# Stop infrastructure
infra-down:
    {{platform_db_compose}} down

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
    cd frontends/web && API_BASE_URL="${API_BASE_URL:-http://127.0.0.1:3000}" bun run dev -- -p 3001

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

# Frontend unit tests
test-web-unit:
    cd frontends/web && bun run test

# Frontend E2E tests
test-web-e2e:
    cd frontends/web && bunx playwright test

# All tests
test-all: test-rust test-web-unit test-web-e2e

# Lint Rust
cargo-fmt:
    cd backend && cargo fmt --all

cargo-clippy:
    cd backend && cargo clippy --workspace --all-targets -- -D warnings

# Lint frontend
web-lint:
    cd frontends/web && bun run lint

web-format:
    cd frontends/web && bun run format

# Prepare sqlx offline queries
sqlx-prepare:
    cd backend/shared && cargo sqlx prepare

# Clean build artifacts
clean:
    cd backend && cargo clean
    cd frontends/web && rm -rf .next
