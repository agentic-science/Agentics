import 'common.just'

# Start the dedicated Docker daemon used by the development worker services
[working-directory: repo]
runner-docker-up:
    @set -eu; \
      dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      if [ -z "$dev_user" ] || { [ "$dev_user" = "root" ] && [ -z "${AGENTICS_COMPOSE_DEV_PROJECT:-}" ]; }; then \
        echo "Refusing to infer dev runner namespace from user '$dev_user'. Set AGENTICS_DEV_USER or AGENTICS_COMPOSE_DEV_PROJECT explicitly." >&2; \
        exit 1; \
      fi; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-$dev_user}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_DOCKER_HOST:-unix://$socket}"; \
      data_root="${AGENTICS_DEV_DOCKER_DATA_ROOT:-$root/docker-data-root}"; \
      exec_root="${AGENTICS_DEV_DOCKER_EXEC_ROOT:-$root/docker-exec}"; \
      pidfile="${AGENTICS_DEV_DOCKER_PIDFILE:-$root/docker.pid}"; \
      logfile="${AGENTICS_DEV_DOCKER_LOG:-$root/dockerd.log}"; \
      bridge="${AGENTICS_DEV_DOCKER_BRIDGE:-agenticsdv0}"; \
      bridge_cidr="${AGENTICS_DEV_DOCKER_BRIDGE_CIDR:-172.29.0.1/16}"; \
      if docker --host "$host" info >/dev/null 2>&1; then \
        printf 'Dedicated dev Docker daemon is already reachable at %s.\n' "$host"; \
        exit 0; \
      fi; \
      if [ "$(id -u)" -ne 0 ]; then \
        printf 'Starting the dedicated dev Docker daemon requires root. Run: sudo env AGENTICS_DEV_USER=%s just dev::runner-docker-up\n' "$dev_user" >&2; \
        exit 2; \
      fi; \
      socket_dir="$(dirname "$socket")"; \
      mkdir -p "$socket_dir" "$data_root" "$exec_root"; \
      rm -f "$socket" "$pidfile"; \
      group="${AGENTICS_DEV_DOCKER_GROUP:-$(id -gn "${SUDO_USER:-$dev_user}" 2>/dev/null || id -gn)}"; \
      if ! ip link show dev "$bridge" >/dev/null 2>&1; then \
        ip link add name "$bridge" type bridge; \
      fi; \
      if ! ip addr show dev "$bridge" | grep -q "$bridge_cidr"; then \
        ip addr add "$bridge_cidr" dev "$bridge"; \
      fi; \
      ip link set dev "$bridge" up; \
      nohup dockerd \
        --data-root "$data_root" \
        --exec-root "$exec_root" \
        --host "$host" \
        --pidfile "$pidfile" \
        --storage-driver overlay2 \
        --bridge "$bridge" \
        --iptables=true \
        --ip-forward=true \
        --ip-masq=true \
        --live-restore=false \
        --log-driver json-file \
        --log-opt max-file=3 \
        --log-opt max-size=10m \
        --containerd-namespace "$namespace" \
        --containerd-plugins-namespace "$namespace-plugins" \
        --group "$group" \
        >"$logfile" 2>&1 & \
      for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
        if docker --host "$host" info >/dev/null 2>&1; then \
          printf 'Dedicated dev Docker daemon is ready at %s.\n' "$host"; \
          exit 0; \
        fi; \
        sleep 1; \
      done; \
      printf 'Dedicated dev Docker daemon did not become ready. Last log lines from %s:\n' "$logfile" >&2; \
      tail -n 40 "$logfile" >&2 || true; \
      exit 1

# Stop the dedicated Docker daemon used by the development worker services
[working-directory: repo]
runner-docker-down:
    @set -eu; \
      dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      pidfile="${AGENTICS_DEV_DOCKER_PIDFILE:-$root/docker.pid}"; \
      if [ "$(id -u)" -ne 0 ]; then \
        printf 'Stopping the dedicated dev Docker daemon requires root. Run: sudo env AGENTICS_DEV_USER=%s just dev::runner-docker-down\n' "$dev_user" >&2; \
        exit 2; \
      fi; \
      if [ ! -f "$pidfile" ]; then \
        rm -f "$socket"; \
        printf 'No dedicated dev Docker pidfile found at %s.\n' "$pidfile"; \
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
          printf 'Dedicated dev Docker daemon pid %s did not stop; inspect it before retrying.\n' "$pid" >&2; \
          exit 1; \
        fi; \
      fi; \
      rm -f "$pidfile" "$socket"; \
      printf 'Dedicated dev Docker daemon stopped.\n'

# Start the containerized development stack with local dev challenge data
[working-directory: repo]
up:
    @dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      if [ -z "$dev_user" ] || { [ "$dev_user" = "root" ] && [ -z "${AGENTICS_COMPOSE_DEV_PROJECT:-}" ]; }; then \
        echo "Refusing to infer dev Compose project from user '$dev_user'. Set AGENTICS_DEV_USER or AGENTICS_COMPOSE_DEV_PROJECT explicitly." >&2; \
        exit 1; \
      fi; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-$dev_user}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_DOCKER_HOST:-unix://$socket}"; \
      profiles="${COMPOSE_PROFILES-gpu}"; \
      mkdir -p "$root/runtime" "$root/phase-mounts" "$root/storage-work" "$root/tmp"; \
      if ! docker --host "$host" info >/dev/null 2>&1; then \
        echo "Refusing to start dev: dedicated runner Docker is not reachable at $host. Run sudo env AGENTICS_DEV_USER=$dev_user just dev::runner-docker-up first." >&2; \
        exit 2; \
      fi; \
      if [ -f "$root/github-app.env" ]; then set -a; . "$root/github-app.env"; set +a; fi; \
      if [ -z "${AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS:-}" ] && [ -f "$root/.github-user-id" ]; then \
        AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS="$(tr -d '[:space:]' < "$root/.github-user-id")"; \
        export AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS; \
      fi; \
      api_port="${AGENTICS_API_HOST_PORT:-3110}"; \
      web_port="${AGENTICS_WEB_HOST_PORT:-3010}"; \
      if [ "$api_port" = "$web_port" ]; then \
        echo "Refusing to start dev: API and web host ports both resolve to $api_port." >&2; \
        exit 1; \
      fi; \
      project_containers="$(COMPOSE_PROFILES="$profiles" AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" AGENTICS_DOCKER_SOCKET_PATH="$socket" AGENTICS_DOCKER_HOST="$host" {{compose_dev}} -p "$project" ps -q 2>/dev/null || true)"; \
      check_dev_port() { \
        port="$1"; \
        label="$2"; \
        case "$port" in \
          ''|*[!0-9]*) echo "Refusing to start dev: $label host port must be numeric, got '$port'." >&2; exit 1 ;; \
        esac; \
        if [ "$port" = "0" ]; then \
          echo "Refusing to start dev: $label host port must be explicit, got 0." >&2; \
          exit 1; \
        fi; \
        case " 3100 3001 13100 13001 " in \
          *" $port "*) echo "Refusing to start dev: $label host port $port is reserved for production or rehearsal." >&2; exit 1 ;; \
        esac; \
        published="$(docker ps --filter "publish=$port" -q)"; \
        for container in $published; do \
          case " $project_containers " in \
            *" $container "*) ;; \
            *) echo "Refusing to start dev: $label host port $port is already published by another container ($container)." >&2; exit 1 ;; \
          esac; \
        done; \
      }; \
      check_dev_port "$api_port" "API"; \
      check_dev_port "$web_port" "web"; \
      COMPOSE_PROFILES="$profiles" AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" AGENTICS_DOCKER_SOCKET_PATH="$socket" AGENTICS_DOCKER_HOST="$host" {{compose_dev}} -p "$project" up --build --remove-orphans

# Stop the containerized development stack
[working-directory: repo]
down:
    @dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      if [ -z "$dev_user" ] || { [ "$dev_user" = "root" ] && [ -z "${AGENTICS_COMPOSE_DEV_PROJECT:-}" ]; }; then \
        echo "Refusing to infer dev Compose project from user '$dev_user'. Set AGENTICS_DEV_USER or AGENTICS_COMPOSE_DEV_PROJECT explicitly." >&2; \
        exit 1; \
      fi; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-$dev_user}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_DOCKER_HOST:-unix://$socket}"; \
      profiles="${COMPOSE_PROFILES-gpu}"; \
      COMPOSE_PROFILES="$profiles" AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" AGENTICS_DOCKER_SOCKET_PATH="$socket" AGENTICS_DOCKER_HOST="$host" {{compose_dev}} -p "$project" down --remove-orphans

# Show the containerized development stack status
[working-directory: repo]
ps:
    @dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      if [ -z "$dev_user" ] || { [ "$dev_user" = "root" ] && [ -z "${AGENTICS_COMPOSE_DEV_PROJECT:-}" ]; }; then \
        echo "Refusing to infer dev Compose project from user '$dev_user'. Set AGENTICS_DEV_USER or AGENTICS_COMPOSE_DEV_PROJECT explicitly." >&2; \
        exit 1; \
      fi; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-$dev_user}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_DOCKER_HOST:-unix://$socket}"; \
      profiles="${COMPOSE_PROFILES-gpu}"; \
      COMPOSE_PROFILES="$profiles" AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" AGENTICS_DOCKER_SOCKET_PATH="$socket" AGENTICS_DOCKER_HOST="$host" {{compose_dev}} -p "$project" ps

# Follow logs from the containerized development stack
[working-directory: repo]
logs:
    @dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      if [ -z "$dev_user" ] || { [ "$dev_user" = "root" ] && [ -z "${AGENTICS_COMPOSE_DEV_PROJECT:-}" ]; }; then \
        echo "Refusing to infer dev Compose project from user '$dev_user'. Set AGENTICS_DEV_USER or AGENTICS_COMPOSE_DEV_PROJECT explicitly." >&2; \
        exit 1; \
      fi; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-$dev_user}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_DOCKER_HOST:-unix://$socket}"; \
      profiles="${COMPOSE_PROFILES-gpu}"; \
      COMPOSE_PROFILES="$profiles" AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" AGENTICS_DOCKER_SOCKET_PATH="$socket" AGENTICS_DOCKER_HOST="$host" {{compose_dev}} -p "$project" logs -f

# Check the containerized development stack service shape
[working-directory: repo]
check:
    @set -eu; \
      dev_user="${AGENTICS_DEV_USER:-${SUDO_USER:-${USER:-local}}}"; \
      if [ -z "$dev_user" ] || { [ "$dev_user" = "root" ] && [ -z "${AGENTICS_COMPOSE_DEV_PROJECT:-}" ]; }; then \
        echo "Refusing to infer dev Compose project from user '$dev_user'. Set AGENTICS_DEV_USER or AGENTICS_COMPOSE_DEV_PROJECT explicitly." >&2; \
        exit 1; \
      fi; \
      root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-$dev_user}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      socket="${AGENTICS_DOCKER_SOCKET_PATH:-$root/docker.sock}"; \
      host="${AGENTICS_DOCKER_HOST:-unix://$socket}"; \
      profiles="${COMPOSE_PROFILES-gpu}"; \
      services="$(COMPOSE_PROFILES="$profiles" AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" AGENTICS_DOCKER_SOCKET_PATH="$socket" AGENTICS_DOCKER_HOST="$host" {{compose_dev}} -p "$project" ps --services --status running)"; \
      printf '%s\n' "$services" | grep -qx worker-cpu || { echo "FAIL dev worker-cpu is not running" >&2; exit 1; }; \
      case ",$profiles," in \
        *,gpu,*) printf '%s\n' "$services" | grep -qx worker-gpu || { echo "FAIL dev gpu profile is enabled but worker-gpu is not running" >&2; exit 1; } ;; \
      esac; \
      cargo run -p agentics-ops --bin agentics-check-local-mvp -- \
        --api-base-url "http://127.0.0.1:${AGENTICS_API_HOST_PORT:-3110}" \
        --web-base-url "http://127.0.0.1:${AGENTICS_WEB_HOST_PORT:-3010}"
