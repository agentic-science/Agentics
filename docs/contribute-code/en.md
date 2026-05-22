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
- `ops/`: Rust operational binaries for local and DGX workflows.
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

## Frontend Demo Data

To inspect the observer frontend with deterministic fake results, run:

```bash
just local-demo up
```

This starts local Postgres, recreates a throwaway `agentics_demo` database, runs
migrations, starts the API, seeds fake public leaderboards and completed
submissions for the example challenges, then starts the Next.js frontend. It
does not start the worker because the demo results are written directly to the
local database.

The local demo intentionally uses ports separate from the normal foreground
development defaults: API `13100` and web `13001`. By default both services bind
to `127.0.0.1`. Use `just local-demo up --lan` to bind the API and web frontend
to `0.0.0.0` so another machine on the same network can inspect the frontend.
In LAN mode the script prints both loopback and LAN URLs when it can detect a
LAN address, and adds the LAN host to Next.js dev-server allowed origins for
HMR.

Stop the demo processes with:

```bash
just local-demo down
```

Use `just local-demo down --db` to also stop the local Postgres container.
Use `just local-demo down --purge-data` for a full cleanup that also removes
generated demo logs, seeded artifact ZIPs, and the local Postgres volume.

## Build Binaries

```bash
cargo build --release -p api-server -p worker -p agentics-cli -p agentics-ops
test -x target/release/agentics-check-dgx-spark-profile
```

Build the web frontend:

```bash
(cd frontends/web && \
  AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
  bun run build)
```

## Checks Before Commit

Install the repository hook once with `just setup-hooks`. The hook delegates to
the Rust `agentics-pre-commit` ops binary, runs independent checks concurrently,
and always checks the human/agent docs policy and large-file threshold before a
non-empty commit.

Run checks before committing code changes:

```bash
cargo fmt --all
DATABASE_URL="$AGENTICS_DATABASE_URL" cargo test --workspace
```

For frontend changes:

```bash
cd frontends/web
bun run generate:schemas
bun run generate:schemas:check
bun run format
bun run test
bun run build
```

For local MVP smoke coverage:

```bash
agentics-check-local-mvp
```

Set `AGENTICS_ADMIN_PASSWORD` and `AGENTICS_WEB_BASE_URL` to include admin and
web checks.

For Rust change-risk coverage, use `cargo llvm-cov` to write LCOV and
`cargo crap` to rank complex, under-covered functions:

```bash
just rust-risk-unit
```

This unit/package workflow excludes the `integration-tests` crate so it does
not require a database or prepared DGX quota storage. The LCOV file is written
to `target/llvm-cov/agentics-workspace.lcov`.

For a fuller signal that includes DB-backed integration tests, start the local
Postgres service and run:

```bash
just infra-up
AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics_test' \
  just rust-risk-integration
```

`rust-risk-integration` skips only the two quota-root integration tests that
require `agentics-prepare-dgx-spark-test-storage`; the rest of the integration
suite contributes coverage before the CRAP report is produced. Set
`AGENTICS_CRAP_TOP` to change how many ranked functions are printed.

On Linux DGX development hosts, quota-sensitive runner tests need a test-owned
XFS quota root. Prepare it separately from the production `/srv/agentics`
runtime tree:

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

Then run quota-sensitive integration tests with:

```bash
export AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
export AGENTICS_TEST_RUNNER_RUNTIME_ROOT=/srv/agentics-test/runtime
export AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics-test/phase-mounts
export AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
```

On Linux, quota-sensitive integration tests fail fast when these variables are
missing, malformed, or do not point at a prepared bounded test quota root.
Non-Linux hosts skip the Linux-only quota probes.

These test variables intentionally point at `/srv/agentics-test` so local
verification does not change production runner slot ownership.

## API And Schema Changes

Rust response DTOs consumed by the web frontend should derive
`schemars::JsonSchema`. Preserve the API JSON policy documented in
`docs/api-json-contract/en.md`: absent optional response fields should be
omitted rather than serialized as explicit `null`.

After changing shared DTOs used by the frontend, run:

```bash
(cd frontends/web && bun run generate:schemas)
(cd frontends/web && bun run generate:schemas:check)
```

Keep `frontends/web/src/lib/schemas.ts` as the stable import facade.

External contract validation that is shared by backend, worker, CLI, or web
belongs in `backend/shared/src/validation/`. Keep archive envelope checks, text
limits, target selection, public API query bounds, GitHub PR provenance, and
web schema exports there instead of duplicating them in handlers or frontend
helpers. Database admission controls and guarded state transitions stay with the
DB/API modules that own those durable invariants.

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
