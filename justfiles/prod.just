import 'common.just'

# Build production Compose images
[working-directory: repo]
build:
    {{compose_prod}} build

# Start the production Compose stack
[working-directory: repo]
up:
    {{compose_prod}} up

# Start the dedicated production runner Docker daemon
[working-directory: repo]
runner-docker-up *args:
    {{compose_prod}} runner-docker-up {{args}}

# Stop the dedicated production runner Docker daemon
[working-directory: repo]
runner-docker-down *args:
    {{compose_prod}} runner-docker-down {{args}}

# Stop the production Compose stack; requires --runner keep or --runner clean
[working-directory: repo]
down *args:
    {{compose_prod}} down {{args}}

# Follow production Compose logs
[working-directory: repo]
logs:
    {{compose_prod}} logs

# Show production Compose status
[working-directory: repo]
ps:
    {{compose_prod}} ps

# Run production checks from the Compose check service
[working-directory: repo]
check:
    {{compose_prod}} check

# Copy backed-up migrated challenge private bundles into production RustFS
[working-directory: repo]
restore-private-bundles:
    {{compose_prod}} restore-private-bundles

# Clean production runner containers by exact namespace labels
[working-directory: repo]
clean-runners *args:
    {{compose_prod}} clean-runners {{args}}
