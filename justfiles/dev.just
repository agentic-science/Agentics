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
