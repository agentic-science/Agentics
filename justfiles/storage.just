import 'common.just'

# Start RustFS S3-compatible object storage for local storage tests
[working-directory: repo]
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
[working-directory: repo]
rustfs-down:
    docker rm -f {{rustfs_container}} >/dev/null 2>&1 || true

# Stop RustFS test service and remove its named volume
[working-directory: repo]
rustfs-purge: rustfs-down
    docker volume rm {{rustfs_volume}} >/dev/null 2>&1 || true

# Run S3 storage tests against the local RustFS service
[working-directory: repo]
s3-test:
    AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-${AGENTICS_RUSTFS_ACCESS_KEY:-agenticsrustfs}}" AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-${AGENTICS_RUSTFS_SECRET_KEY:-agenticsrustfssecret}}" AGENTICS_S3_TEST_ENDPOINT="${AGENTICS_S3_TEST_ENDPOINT:-http://127.0.0.1:${AGENTICS_RUSTFS_PORT:-9000}}" AGENTICS_S3_TEST_BUCKET="${AGENTICS_S3_TEST_BUCKET:-agentics-test}" AGENTICS_S3_FORCE_PATH_STYLE="${AGENTICS_S3_FORCE_PATH_STYLE:-true}" cargo test -p agentics-storage rustfs_s3_storage_round_trips_when_configured -- --nocapture

# Start the persistent RustFS backup store for migrated challenge private bundles
[working-directory: repo]
backup-up:
    test -f deploy/compose/env/rustfs-private-backup.env
    {{compose_rustfs_private_backup}} up -d --remove-orphans

# Stop the persistent RustFS private bundle store without deleting data
[working-directory: repo]
backup-down:
    test -f deploy/compose/env/rustfs-private-backup.env
    {{compose_rustfs_private_backup}} down --remove-orphans

# Show persistent RustFS private bundle store status
[working-directory: repo]
backup-ps:
    test -f deploy/compose/env/rustfs-private-backup.env
    {{compose_rustfs_private_backup}} ps

# Follow persistent RustFS private bundle store logs
[working-directory: repo]
backup-logs:
    test -f deploy/compose/env/rustfs-private-backup.env
    {{compose_rustfs_private_backup}} logs -f
