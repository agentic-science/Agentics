import 'common.just'

rehearsal_env := "deploy/compose/env/rehearsal.env"
rehearsal_api_url := "http://127.0.0.1:13100"
rehearsal_web_url := "http://127.0.0.1:13001"

# Prepare root-owned storage for the disposable rehearsal environment
[working-directory: repo]
prepare-storage *args:
    @set -eu; \
      test -f {{rehearsal_env}}; \
      set -a; . {{rehearsal_env}}; set +a; \
      if [ "$(id -u)" -ne 0 ]; then \
        printf 'Preparing rehearsal storage requires root. Run: sudo just rehearsal::prepare-storage\n' >&2; \
        exit 2; \
      fi; \
      AGENTICS_DGX_CONFIRM=prepare-storage cargo run -p agentics-ops --bin agentics-prepare-dgx-spark-storage -- {{args}}

# Start the dedicated rehearsal runner Docker daemon
[working-directory: repo]
runner-docker-up *args:
    {{compose_rehearsal}} runner-docker-up {{args}}

# Stop the dedicated rehearsal runner Docker daemon
[working-directory: repo]
runner-docker-down *args:
    {{compose_rehearsal}} runner-docker-down {{args}}

# Build rehearsal Compose images
[working-directory: repo]
build:
    {{compose_rehearsal}} build

# Start the disposable rehearsal Compose stack
[working-directory: repo]
up:
    {{compose_rehearsal}} up

# Stop the disposable rehearsal Compose stack; pass --runner keep or --runner clean
[working-directory: repo]
down *args:
    {{compose_rehearsal}} down {{args}}

# Show rehearsal Compose status
[working-directory: repo]
ps:
    {{compose_rehearsal}} ps

# Follow rehearsal Compose logs
[working-directory: repo]
logs:
    {{compose_rehearsal}} logs

# Run production checks against the rehearsal Compose stack
[working-directory: repo]
check:
    {{compose_rehearsal}} check

# Run the full production rehearsal and require GPU worker evidence
[working-directory: repo]
run *args:
    @set -eu; \
      test -f {{rehearsal_env}}; \
      set -a; . {{rehearsal_env}}; set +a; \
      AGENTICS_S3_ENDPOINT_URL="${AGENTICS_REHEARSAL_HOST_S3_ENDPOINT_URL:-http://127.0.0.1:${AGENTICS_RUSTFS_PORT:-19000}}" \
      AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-$AGENTICS_RUSTFS_ACCESS_KEY}" \
      AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-$AGENTICS_RUSTFS_SECRET_KEY}" \
      cargo run -p agentics-ops --bin agentics-rehearse-production -- run \
        --env-file {{rehearsal_env}} \
        --api-base-url {{rehearsal_api_url}} \
        --web-base-url {{rehearsal_web_url}} \
        --confirm-disposable-staging \
        --gpu require \
        {{args}}

# Run the CPU-only production rehearsal against the rehearsal stack
[working-directory: repo]
run-cpu *args:
    @set -eu; \
      test -f {{rehearsal_env}}; \
      set -a; . {{rehearsal_env}}; set +a; \
      AGENTICS_S3_ENDPOINT_URL="${AGENTICS_REHEARSAL_HOST_S3_ENDPOINT_URL:-http://127.0.0.1:${AGENTICS_RUSTFS_PORT:-19000}}" \
      AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-$AGENTICS_RUSTFS_ACCESS_KEY}" \
      AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-$AGENTICS_RUSTFS_SECRET_KEY}" \
      cargo run -p agentics-ops --bin agentics-rehearse-production -- run \
        --env-file {{rehearsal_env}} \
        --api-base-url {{rehearsal_api_url}} \
        --web-base-url {{rehearsal_web_url}} \
        --confirm-disposable-staging \
        --gpu skip \
        {{args}}

# Purge disposable rehearsal Compose resources, runner namespace, and storage roots
[working-directory: repo]
purge-data *args:
    {{compose_rehearsal}} purge-rehearsal-data {{args}}
