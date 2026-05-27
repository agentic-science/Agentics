import 'common.just'

# Start the containerized development stack with migrated Frontier-CS dev data
[working-directory: repo]
up:
    test -f deploy/compose/env/rustfs-private-backup.env
    {{compose_rustfs_private_backup}} up -d --remove-orphans
    @root="${AGENTICS_DEV_ROOT:-$PWD/.agentics-compose/dev}"; \
      project="${AGENTICS_COMPOSE_DEV_PROJECT:-agentics-dev-${USER:-local}}"; \
      namespace="${AGENTICS_RUNNER_NAMESPACE:-$project}"; \
      mkdir -p "$root/runtime" "$root/phase-mounts" "$root/storage-work" "$root/tmp"; \
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
