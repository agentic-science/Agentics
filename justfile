set fallback := true

platform_db_compose := "docker compose -f docker/platform-db/docker-compose.yml"
compose_dev := "docker compose --env-file deploy/compose/env/dev.env.example -f deploy/compose/compose.yml -f deploy/compose/compose.dev.yml"
compose_test := "docker compose --env-file deploy/compose/env/test.env.example -f deploy/compose/compose.yml -f deploy/compose/compose.test.yml"
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
    @requested_network="${AGENTICS_RUSTFS_DOCKER_NETWORK:-}"; \
    custom_ports=0; \
    if [ -n "${AGENTICS_RUSTFS_PORT:-}" ] && [ "${AGENTICS_RUSTFS_PORT}" != "9000" ]; then custom_ports=1; fi; \
    if [ -n "${AGENTICS_RUSTFS_CONSOLE_PORT:-}" ] && [ "${AGENTICS_RUSTFS_CONSOLE_PORT}" != "9001" ]; then custom_ports=1; fi; \
    if [ "$requested_network" = "host" ] && [ "$custom_ports" = "1" ]; then \
      echo "AGENTICS_RUSTFS_DOCKER_NETWORK=host cannot honor custom RustFS ports; unset it or set it to bridge" >&2; \
      exit 2; \
    fi; \
    if [ -z "$requested_network" ] && [ "$custom_ports" = "1" ]; then requested_network="bridge"; fi; \
    if [ "${requested_network:-host}" = "host" ]; then \
      network_args="--network host"; \
    elif [ "$requested_network" = "bridge" ]; then \
      network_args="-p ${AGENTICS_RUSTFS_PORT:-9000}:9000 -p ${AGENTICS_RUSTFS_CONSOLE_PORT:-9001}:9001"; \
    else \
      echo "AGENTICS_RUSTFS_DOCKER_NETWORK must be host or bridge" >&2; \
      exit 2; \
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

# Start the containerized development stack with seeded fake data
compose-dev-up:
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      mkdir -p "$root/runtime" "$root/phase-mounts" "$root/storage" "$root/storage-work" "$root/tmp"; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" up --remove-orphans

# Stop the containerized development stack
compose-dev-down:
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" down --remove-orphans

# Follow logs from the containerized development stack
compose-dev-logs:
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" logs -f

# Start the dedicated Docker daemon used by containerized integration tests
compose-test-docker-up:
    @set -eu; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_TEST_DOCKER_HOST:-unix://$socket}"; \
      data_root="${AGENTICS_TEST_DOCKER_DATA_ROOT:-$root/docker-data-root}"; \
      exec_root="${AGENTICS_TEST_DOCKER_EXEC_ROOT:-$root/docker-exec}"; \
      pidfile="${AGENTICS_TEST_DOCKER_PIDFILE:-$root/docker.pid}"; \
      logfile="${AGENTICS_TEST_DOCKER_LOG:-$root/dockerd.log}"; \
      if docker --host "$host" info >/dev/null 2>&1; then \
        printf 'Dedicated test Docker daemon is already reachable at %s.\n' "$host"; \
        exit 0; \
      fi; \
      if [ "$(id -u)" -ne 0 ]; then \
        printf 'Starting the dedicated test Docker daemon requires root. Run: sudo env AGENTICS_TEST_ROOT=%s just compose-test-docker-up\n' "$root" >&2; \
        exit 2; \
      fi; \
      if [ ! -d "$data_root" ]; then \
        printf 'Prepared Docker data root is required at %s. Run agentics-prepare-dgx-spark-test-storage as root first.\n' "$data_root" >&2; \
        exit 2; \
      fi; \
      socket_dir="$(dirname "$socket")"; \
      mkdir -p "$socket_dir" "$exec_root"; \
      rm -f "$socket" "$pidfile"; \
      group="${AGENTICS_TEST_DOCKER_GROUP:-$(id -gn "${SUDO_USER:-$(id -un)}")}"; \
      nohup dockerd \
        --data-root "$data_root" \
        --exec-root "$exec_root" \
        --host "unix://$socket" \
        --pidfile "$pidfile" \
        --storage-driver overlay2 \
        --bridge none \
        --iptables=true \
        --live-restore=false \
        --log-driver json-file \
        --log-opt max-file=3 \
        --log-opt max-size=10m \
        --containerd-namespace agentics-test \
        --containerd-plugins-namespace agentics-test-plugins \
        --group "$group" \
        >"$logfile" 2>&1 & \
      for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
        if docker --host "$host" info >/dev/null 2>&1; then \
          printf 'Dedicated test Docker daemon is ready at %s.\n' "$host"; \
          exit 0; \
        fi; \
        sleep 1; \
      done; \
      printf 'Dedicated test Docker daemon did not become ready. Last log lines from %s:\n' "$logfile" >&2; \
      tail -n 40 "$logfile" >&2 || true; \
      exit 1

# Stop the dedicated Docker daemon used by containerized integration tests
compose-test-docker-down:
    @set -eu; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      pidfile="${AGENTICS_TEST_DOCKER_PIDFILE:-$root/docker.pid}"; \
      if [ "$(id -u)" -ne 0 ]; then \
        printf 'Stopping the dedicated test Docker daemon requires root. Run: sudo env AGENTICS_TEST_ROOT=%s just compose-test-docker-down\n' "$root" >&2; \
        exit 2; \
      fi; \
      if [ ! -f "$pidfile" ]; then \
        rm -f "$socket"; \
        printf 'No dedicated test Docker pidfile found at %s.\n' "$pidfile"; \
        exit 0; \
      fi; \
      pid="$(cat "$pidfile")"; \
      if kill -0 "$pid" >/dev/null 2>&1; then \
        kill "$pid"; \
        for _ in 1 2 3 4 5 6 7 8 9 10; do \
          if ! kill -0 "$pid" >/dev/null 2>&1; then break; fi; \
          sleep 1; \
        done; \
        if kill -0 "$pid" >/dev/null 2>&1; then \
          printf 'Dedicated test Docker daemon pid %s did not stop; inspect it before retrying.\n' "$pid" >&2; \
          exit 1; \
        fi; \
      fi; \
      rm -f "$pidfile" "$socket"; \
      printf 'Dedicated test Docker daemon stopped.\n'

# Run the existing ignored integration suite in a containerized test harness
compose-test-integration:
    @set -eu; \
      run_id="${AGENTICS_COMPOSE_TEST_RUN_ID:-$(date +%Y%m%d%H%M%S)-$$}"; \
      project="${AGENTICS_COMPOSE_TEST_PROJECT:-agentics-test-$run_id}"; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      runtime_root="${AGENTICS_TEST_RUNNER_RUNTIME_ROOT:-$root/runtime/$run_id}"; \
      phase_root="${AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT:-$root/phase-mounts}"; \
      tmpdir="${AGENTICS_TEST_TMPDIR:-$runtime_root/tmp}"; \
      docker_socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      docker_host="${AGENTICS_TEST_DOCKER_HOST:-unix://$docker_socket}"; \
      runner_image="${AGENTICS_TEST_RUNNER_IMAGE:-agentics-linux-arm64-cpu:ubuntu26.04-local}"; \
      if [ ! -d "$root/runtime" ] || [ ! -d "$phase_root" ]; then \
        printf 'Prepared test root is required at %s. Run agentics-prepare-dgx-spark-test-storage as root before compose-test-integration.\n' "$root" >&2; \
        exit 2; \
      fi; \
      case "$docker_host" in unix://*) ;; *) printf 'compose-test-integration requires a Unix AGENTICS_TEST_DOCKER_HOST, got %s.\n' "$docker_host" >&2; exit 2 ;; esac; \
      if ! docker --host "$docker_host" info >/dev/null 2>&1; then \
        printf 'Dedicated test Docker daemon is required at %s. Run sudo env AGENTICS_TEST_ROOT=%s just compose-test-docker-up first.\n' "$docker_host" "$root" >&2; \
        exit 2; \
      fi; \
      mkdir -p "$runtime_root" "$tmpdir"; \
      if ! docker --host "$docker_host" image inspect "$runner_image" >/dev/null 2>&1; then \
        if ! docker image inspect "$runner_image" >/dev/null 2>&1; then \
          docker build --platform linux/arm64 -t "$runner_image" docker/images/linux-arm64-cpu; \
        fi; \
        image_tar="$runtime_root/runner-image.tar"; \
        docker image save -o "$image_tar" "$runner_image"; \
        docker --host "$docker_host" image load -i "$image_tar" >/dev/null; \
        rm -f "$image_tar"; \
      fi; \
      status=0; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_TEST_ROOT="$root" AGENTICS_TEST_RUNNER_RUNTIME_ROOT="$runtime_root" AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT="$phase_root" AGENTICS_TEST_TMPDIR="$tmpdir" AGENTICS_TEST_DOCKER_HOST="$docker_host" AGENTICS_TEST_DOCKER_SOCKET_PATH="$docker_socket" AGENTICS_RUNNER_NAMESPACE="$project" {{compose_test}} -p "$project" up --abort-on-container-exit --exit-code-from tests || status=$?; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_TEST_ROOT="$root" AGENTICS_TEST_RUNNER_RUNTIME_ROOT="$runtime_root" AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT="$phase_root" AGENTICS_TEST_TMPDIR="$tmpdir" AGENTICS_TEST_DOCKER_HOST="$docker_host" AGENTICS_TEST_DOCKER_SOCKET_PATH="$docker_socket" AGENTICS_RUNNER_NAMESPACE="$project" {{compose_test}} -p "$project" down -v --remove-orphans; \
      exit "$status"

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
