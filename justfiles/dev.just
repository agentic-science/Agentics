import 'common.just'

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
      mkdir -p "$root/runtime" "$root/phase-mounts" "$root/storage-work" "$root/tmp"; \
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
      project_containers="$(AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" ps -q 2>/dev/null || true)"; \
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
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" up --build --remove-orphans

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
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" down --remove-orphans

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
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" ps

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
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" logs -f
