import 'common.just'

# Start the containerized development stack with local dev challenge data
[working-directory: repo]
up:
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
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
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" down --remove-orphans

# Show the containerized development stack status
[working-directory: repo]
ps:
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" ps

# Follow logs from the containerized development stack
[working-directory: repo]
logs:
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      AGENTICS_REPO_ROOT="$PWD" AGENTICS_DEV_ROOT="$root" AGENTICS_RUNNER_NAMESPACE="$namespace" {{compose_dev}} -p "$project" logs -f
