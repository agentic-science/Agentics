import 'common.just'

# Lint frontend
[working-directory: repo]
lint:
    cd frontends/web && bun run lint

# Check generated frontend schemas
[working-directory: repo]
schema-check:
    cd frontends/web && bun run generate:schemas:check

# Format frontend code
[working-directory: repo]
format:
    cd frontends/web && bun run format
