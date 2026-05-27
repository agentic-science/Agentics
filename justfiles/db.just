import 'common.just'

# Prepare sqlx offline queries
[working-directory: repo]
sqlx-prepare:
    cargo sqlx prepare --workspace
