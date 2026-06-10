set fallback

# Canonical full-suite test workflows.
mod test 'justfiles/test.just'

# Containerized local development stack.
mod dev 'justfiles/dev.just'

# Production Compose operations.
mod prod 'justfiles/prod.just'

# Disposable production-like rehearsal environment.
mod rehearsal 'justfiles/rehearsal.just'

# RustFS, S3, and private-bundle backup helpers.
mod storage 'justfiles/storage.just'

# Rust component checks.
mod rust 'justfiles/rust.just'

# Web component checks.
mod web 'justfiles/web.just'

# Database maintenance helpers.
mod db 'justfiles/db.just'

# Rust risk and coverage reports.
mod risk 'justfiles/risk.just'

# Repository maintenance helpers.
mod maintenance 'justfiles/maintenance.just'

# CPU-only full test suite
test-all-cpu:
    @just test::all-cpu

# Full test suite, including ignored GPU/CUDA tests
test-all:
    @just test::all

# Start the dedicated Docker daemon used by containerized integration tests
test-env-up:
    @just test::env-up

# Stop the dedicated Docker daemon used by containerized integration tests
test-env-down:
    @just test::env-down

# Check the CPU-only full-suite test environment
test-env-status-cpu:
    @just test::env-status-cpu

# Check the full GPU test environment
test-env-status:
    @just test::env-status

# Remove persistent Cargo cache volumes used by the Compose test harness
test-purge-cargo-cache:
    @just test::purge-cargo-cache

# Publish workspace packages through crates.io-aware checks
publish *args:
    @cargo run -p agentics-ops --bin agentics-publish -- {{args}}
