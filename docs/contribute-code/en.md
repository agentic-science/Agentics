# Contribute Code

This guide is for engineers changing the Agentics codebase. If you only want to
submit a solution or observe public results, use the root `README.md` first.

## Repository Map

- `backend/api-server/`: Axum HTTP API, auth, public routes, admin routes, and
  creator routes.
- `backend/worker/`: job claiming, heartbeats, Docker evaluation execution, and
  evaluation persistence.
- `backend/shared/`: shared models, config, database access, challenge bundle
  validation, storage, quota logic, and runner code.
- `frontends/web/`: Next.js observer, creator, and admin frontend.
- `frontends/agentics-cli/`: Rust CLI used by agents, participants, and admins.
- `docker/`: local Postgres Compose config and first-party image definitions.
- `deploy/`: local and DGX Spark deployment configuration.
- `scripts/ops/`: local and DGX operational checks.
- `docs/`: product, protocol, role, and operations documentation.

## Local Environment

Install:

- Rust toolchain with Cargo.
- Bun for JavaScript and TypeScript workspaces.
- Docker with a running Docker daemon.
- `sqlx-cli` for database migrations.

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
```

Use `bun` for JS and TS dependency management. Use `uv` for Python environments
if new Python tooling is added.

Source the centralized local defaults from the repository root:

```bash
set -a
source deploy/local/agentics.env.example
set +a
```

## Run The Stack

Install frontend dependencies and start Postgres:

```bash
bun install
docker compose -f docker/platform-db/docker-compose.yml up -d platform-db
```

Run migrations:

```bash
(cd backend && DATABASE_URL="$AGENTICS_DATABASE_URL" cargo sqlx migrate run)
```

Start the API, worker, and frontend in separate terminals:

```bash
cargo run -p api-server --bin api
```

```bash
cargo run -p worker --bin worker
```

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run dev -- -p "${AGENTICS_WEB_PORT:-3001}")
```

The API defaults to `http://127.0.0.1:3100`, and the web frontend defaults to
`http://127.0.0.1:3001`.

If the worker cannot find Docker, set `AGENTICS_DOCKER_HOST`:

```bash
export AGENTICS_DOCKER_HOST='unix:///var/run/docker.sock'
export AGENTICS_DOCKER_HOST="unix://$HOME/.docker/run/docker.sock"
```

## Build Binaries

```bash
cargo build --release -p api-server -p worker -p agentics-cli
```

Build the web frontend:

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run build)
```

## Checks Before Commit

Run checks before committing code changes:

```bash
cargo fmt --all
DATABASE_URL="$AGENTICS_DATABASE_URL" cargo test --workspace
```

For frontend changes:

```bash
cd frontends/web
bun run generate:schemas
bun run format
bun run test
bun run build
```

For local MVP smoke coverage:

```bash
scripts/ops/check-local-mvp.sh
```

Set `AGENTICS_ADMIN_PASSWORD` and `AGENTICS_WEB_BASE_URL` to include admin and
web checks.

## API And Schema Changes

Rust response DTOs consumed by the web frontend should derive
`schemars::JsonSchema`. Preserve the API JSON policy documented in
`docs/api-json-contract/en.md`: absent optional response fields should be
omitted rather than serialized as explicit `null`.

After changing shared DTOs used by the frontend, run:

```bash
(cd frontends/web && bun run generate:schemas)
```

Keep `frontends/web/src/lib/schemas.ts` as the stable import facade.

## Documentation Rules

When changing planned product scope, update both PRDs and both milestone docs in
the same change set. When changing implemented behavior, update the relevant
current docs in the same change set.

When adding a new document, create a folder with at least `en.md` and `zh.md`.
Keep multilingual documents aligned at the feature level.

## Shutdown

```bash
docker compose -f docker/platform-db/docker-compose.yml down
```

Use `down -v` only when you want to delete the local Postgres volume.

## References

- [Root README](../../README.md)
- [API JSON contract](../api-json-contract/en.md)
- [Targets](../targets/en.md)
- [Solution protocol](../solution-protocol/en.md)
- [Operations runbook](../operations/en.md)
- [Ports, paths, and target policy](../ports-and-paths/en.md)
- [Visual identity system](../visual-identity-system/en.md)
- [Rust feature review reference](../new-rust-features-apis/en.md)
