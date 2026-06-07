import 'common.just'

# Install frontend dependencies from the lockfile
[working-directory: repo]
install:
    cd frontends/web && bun install --frozen-lockfile

# Lint frontend
[working-directory: repo]
lint: install
    cd frontends/web && bun run lint

# Check generated frontend schemas
[working-directory: repo]
schema-check: install
    cd frontends/web && bun run generate:schemas:check

# Run all frontend checks
[working-directory: repo]
check: install
    cd frontends/web && bun run generate:schemas:check && bun run lint && bunx tsc --noEmit && bun run check:vis && bun run test

# Format frontend code
[working-directory: repo]
format: install
    cd frontends/web && bun run format
