import 'common.just'

# Rust coverage + CRAP report from unit and package tests only
[working-directory: repo]
unit:
    mkdir -p target/llvm-cov
    cargo llvm-cov --workspace --exclude integration-tests --lcov --output-path {{crap_lcov_unit}}
    cargo crap --workspace --lcov {{crap_lcov_unit}} --top "${AGENTICS_CRAP_TOP:-30}"

# Rust coverage + CRAP report including all integration tests
[working-directory: repo]
integration:
    mkdir -p target/llvm-cov
    @database_url="${DATABASE_URL:-${AGENTICS_DATABASE_URL:-}}"; \
      if [ -z "$database_url" ]; then \
        printf 'DATABASE_URL or AGENTICS_DATABASE_URL must be set for integration coverage.\n' >&2; \
        exit 2; \
      fi; \
      DATABASE_URL="$database_url" cargo llvm-cov --workspace --lcov --output-path {{crap_lcov_integration}} -- --include-ignored
    cargo crap --workspace --lcov {{crap_lcov_integration}} --top "${AGENTICS_CRAP_TOP:-30}"
