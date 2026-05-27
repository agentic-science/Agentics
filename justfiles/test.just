import 'common.just'

# Start the dedicated Docker daemon used by containerized integration tests
[working-directory: repo]
env-up:
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
        printf 'Starting the dedicated test Docker daemon requires root. Run: sudo env AGENTICS_TEST_ROOT=%s just test-env-up\n' "$root" >&2; \
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
[working-directory: repo]
env-down:
    @set -eu; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      pidfile="${AGENTICS_TEST_DOCKER_PIDFILE:-$root/docker.pid}"; \
      if [ "$(id -u)" -ne 0 ]; then \
        printf 'Stopping the dedicated test Docker daemon requires root. Run: sudo env AGENTICS_TEST_ROOT=%s just test-env-down\n' "$root" >&2; \
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

# Check the CPU-only full-suite test environment
[working-directory: repo]
env-status-cpu:
    @set -eu; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_TEST_DOCKER_HOST:-unix://$socket}"; \
      runner_image="${AGENTICS_TEST_RUNNER_IMAGE:-{{cpu_runner_image}}}"; \
      status=0; \
      pass() { printf 'PASS %s - %s\n' "$1" "$2"; }; \
      fail() { printf 'FAIL %s - %s\n' "$1" "$2" >&2; status=1; }; \
      if [ "$(uname -s)" = "Linux" ]; then pass "Linux host" "$(uname -srm)"; else fail "Linux host" "test-all-cpu requires Linux"; fi; \
      if docker compose version >/dev/null 2>&1; then pass "Docker Compose" "$(docker compose version --short 2>/dev/null || docker compose version)"; else fail "Docker Compose" "install Docker Compose"; fi; \
      if [ -d "$root/runtime" ] && [ -d "$root/phase-mounts" ] && [ -d "$root/docker-data-root" ]; then pass "test storage" "$root is prepared"; else fail "test storage" "run: sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage agentics-prepare-dgx-spark-test-storage"; fi; \
      if docker --host "$host" info >/dev/null 2>&1; then pass "dedicated test Docker" "$host is reachable"; else fail "dedicated test Docker" "run: sudo env AGENTICS_TEST_ROOT=$root just test-env-up"; fi; \
      if docker --host "$host" image inspect "$runner_image" >/dev/null 2>&1; then \
        pass "CPU runner image" "$runner_image is loaded in dedicated test Docker"; \
      elif docker image inspect "$runner_image" >/dev/null 2>&1; then \
        pass "CPU runner image" "$runner_image is available on the default daemon and will be loaded"; \
      elif [ -d docker/runner-images/linux-arm64-cpu ]; then \
        pass "CPU runner image" "$runner_image can be built from docker/runner-images/linux-arm64-cpu"; \
      else \
        fail "CPU runner image" "$runner_image is missing and no build context was found"; \
      fi; \
      exit "$status"

# Check the full GPU test environment
[working-directory: repo]
env-status: env-status-cpu
    @set -eu; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_TEST_DOCKER_HOST:-unix://$socket}"; \
      cuda_image="${AGENTICS_TEST_CUDA_IMAGE:-{{cuda_runner_image}}}"; \
      status=0; \
      pass() { printf 'PASS %s - %s\n' "$1" "$2"; }; \
      fail() { printf 'FAIL %s - %s\n' "$1" "$2" >&2; status=1; }; \
      if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi -L >/dev/null 2>&1; then pass "NVIDIA GPU" "$(nvidia-smi -L | head -n 1)"; else fail "NVIDIA GPU" "nvidia-smi must see at least one GPU"; fi; \
      if ! docker --host "$host" image inspect "$cuda_image" >/dev/null 2>&1; then \
        if docker image inspect "$cuda_image" >/dev/null 2>&1; then \
          tmpdir="$root/runtime/test-env-status"; \
          mkdir -p "$tmpdir"; \
          image_tar="$tmpdir/cuda-runner-image.tar"; \
          docker image save -o "$image_tar" "$cuda_image"; \
          docker --host "$host" image load -i "$image_tar" >/dev/null; \
          rm -f "$image_tar"; \
        else \
          if ! docker --host "$host" pull "$cuda_image" >/dev/null; then \
            fail "CUDA runner image" "$cuda_image is not loaded locally and could not be pulled by the dedicated test Docker daemon"; \
          fi; \
        fi; \
      fi; \
      if docker --host "$host" image inspect "$cuda_image" >/dev/null 2>&1; then pass "CUDA runner image" "$cuda_image is loaded in dedicated test Docker"; else fail "CUDA runner image" "$cuda_image is not available"; fi; \
      if docker --host "$host" run --rm --gpus 1 --platform linux/arm64 -e AGENTICS_GPU_SMOKE_REQUIRE_DEVICE=1 "$cuda_image" /opt/agentics/smoke.sh >/dev/null; then pass "Docker GPU runtime" "device request smoke passed"; else fail "Docker GPU runtime" "dedicated test Docker cannot run the CUDA smoke image with --gpus 1"; fi; \
      exit "$status"

# Run non-ignored integration tests in the containerized test harness
[working-directory: repo]
integration-cpu:
    @just test::_integration false

# Run all integration tests, including ignored GPU/CUDA tests, in the containerized test harness
[working-directory: repo]
integration:
    @just test::_integration true

[working-directory: repo]
_integration include_ignored:
    @set -eu; \
      run_id="${AGENTICS_COMPOSE_TEST_RUN_ID:-$(date +%Y%m%d%H%M%S)-$$}"; \
      project="${AGENTICS_COMPOSE_TEST_PROJECT:-agentics-test-$run_id}"; \
      root="${AGENTICS_TEST_ROOT:-/srv/agentics-test}"; \
      runtime_root="${AGENTICS_TEST_RUNNER_RUNTIME_ROOT:-$root/runtime/$run_id}"; \
      phase_root="${AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT:-$root/phase-mounts}"; \
      tmpdir="${AGENTICS_TEST_TMPDIR:-$runtime_root/tmp}"; \
      docker_socket="${AGENTICS_TEST_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      docker_host="${AGENTICS_TEST_DOCKER_HOST:-unix://$docker_socket}"; \
      runner_image="${AGENTICS_TEST_RUNNER_IMAGE:-{{cpu_runner_image}}}"; \
      cuda_image="${AGENTICS_TEST_CUDA_IMAGE:-{{cuda_runner_image}}}"; \
      if [ ! -d "$root/runtime" ] || [ ! -d "$phase_root" ]; then \
        printf 'Prepared test root is required at %s. Run agentics-prepare-dgx-spark-test-storage as root before just test::integration.\n' "$root" >&2; \
        exit 2; \
      fi; \
      case "$docker_host" in unix://*) ;; *) printf 'just test::integration requires a Unix AGENTICS_TEST_DOCKER_HOST, got %s.\n' "$docker_host" >&2; exit 2 ;; esac; \
      if ! docker --host "$docker_host" info >/dev/null 2>&1; then \
        printf 'Dedicated test Docker daemon is required at %s. Run sudo env AGENTICS_TEST_ROOT=%s just test-env-up first.\n' "$docker_host" "$root" >&2; \
        exit 2; \
      fi; \
      mkdir -p "$runtime_root" "$tmpdir"; \
      if ! docker --host "$docker_host" image inspect "$runner_image" >/dev/null 2>&1; then \
        if ! docker image inspect "$runner_image" >/dev/null 2>&1; then \
          docker build --platform linux/arm64 -t "$runner_image" docker/runner-images/linux-arm64-cpu; \
        fi; \
        image_tar="$runtime_root/runner-image.tar"; \
        docker image save -o "$image_tar" "$runner_image"; \
        docker --host "$docker_host" image load -i "$image_tar" >/dev/null; \
        rm -f "$image_tar"; \
      fi; \
      if [ "{{include_ignored}}" = "true" ] && ! docker --host "$docker_host" image inspect "$cuda_image" >/dev/null 2>&1; then \
        if docker image inspect "$cuda_image" >/dev/null 2>&1; then \
          image_tar="$runtime_root/cuda-runner-image.tar"; \
          docker image save -o "$image_tar" "$cuda_image"; \
          docker --host "$docker_host" image load -i "$image_tar" >/dev/null; \
          rm -f "$image_tar"; \
        else \
          docker --host "$docker_host" pull "$cuda_image" >/dev/null; \
        fi; \
      fi; \
      status=0; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_TEST_ROOT="$root" AGENTICS_TEST_RUNNER_RUNTIME_ROOT="$runtime_root" AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT="$phase_root" AGENTICS_TEST_TMPDIR="$tmpdir" AGENTICS_TEST_DOCKER_HOST="$docker_host" AGENTICS_TEST_DOCKER_SOCKET_PATH="$docker_socket" AGENTICS_RUNNER_NAMESPACE="$project" AGENTICS_COMPOSE_TEST_RUN_ID="$run_id" AGENTICS_COMPOSE_TEST_INCLUDE_IGNORED="{{include_ignored}}" AGENTICS_S3_PREFIX="test/$run_id" {{compose_test}} -p "$project" up --abort-on-container-exit --exit-code-from tests || status=$?; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_TEST_ROOT="$root" AGENTICS_TEST_RUNNER_RUNTIME_ROOT="$runtime_root" AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT="$phase_root" AGENTICS_TEST_TMPDIR="$tmpdir" AGENTICS_TEST_DOCKER_HOST="$docker_host" AGENTICS_TEST_DOCKER_SOCKET_PATH="$docker_socket" AGENTICS_RUNNER_NAMESPACE="$project" AGENTICS_COMPOSE_TEST_RUN_ID="$run_id" AGENTICS_COMPOSE_TEST_INCLUDE_IGNORED="{{include_ignored}}" AGENTICS_S3_PREFIX="test/$run_id" {{compose_test}} -p "$project" down -v --remove-orphans; \
      exit "$status"

# CPU-only full test suite
[working-directory: repo]
all-cpu: env-status-cpu
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace --exclude integration-tests
    just web::check
    just test::integration-cpu

# Full test suite, including ignored GPU/CUDA tests
[working-directory: repo]
all: env-status
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace --exclude integration-tests
    just web::check
    just test::integration
