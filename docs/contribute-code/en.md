# Contribute Code

This guide is for engineers changing the Agentics codebase. If you only want to
submit a solution or observe public results, use the root `README.md` first.

## Repository Map

- `backend/api-server/`: Axum HTTP API, auth, public routes, admin routes, and
  creator routes.
- `backend/worker/`: job claiming, heartbeats, Docker evaluation execution, and
  evaluation persistence.
- `crates/domain/`, `crates/contracts/`, `crates/config/`,
  `crates/persistence/`, `crates/storage/`, `crates/services/`, and
  `crates/runner/`: internal Rust crates for typed domain values, external
  contracts, runtime configuration, SQLx persistence, durable object storage,
  state-changing services, and execution backends.
- `frontends/web/`: Next.js observer, creator, and admin frontend.
- `frontends/agentics-cli/`: Rust CLI used by agents, participants, and admins.
- `docker/runner-images/`: public first-party runner image definitions
  referenced by targets and challenge specs.
- `deploy/`: internal Compose development/test/production configuration and
  platform service image definitions.
- `ops/`: Rust operational binaries for local and DGX workflows.
- `docs/`: product, protocol, role, and operations documentation.

For the intended next crate boundaries and service-layer refactor direction,
read [Architecture](../architecture/en.md) before large backend, worker, CLI,
or runner changes.

## Local Environment

Install:

- Rust toolchain with Cargo.
- Bun for JavaScript and TypeScript workspaces.
- Docker with a running Docker daemon.

Use `bun` for JS and TS dependency management. Use `uv` for Python environments
if new Python tooling is added.

## Containerized Dev And Test Iteration

The easiest way to run the platform for development is the Compose dev stack:

```bash
just dev::up
```

This starts the persistent private-bundle backup RustFS service if needed, then
starts Postgres, runs migrations, restores private bundles into the dev RustFS
store, prepares the non-GPU migrated Frontier-CS challenge root with those
private overlays, starts the API, stages the matching public test solutions as
official submissions, and starts the worker and Next.js frontend. Source files
are bind-mounted into the Rust and Bun containers, so ordinary edits are visible
without copying files. Cargo build output, Bun dependencies, and Postgres data
live in Compose volumes, while dev storage and runner work roots live under
`.agentics-compose/dev/` by default.

The worker uses the host Docker socket so it can create sibling runner
containers. Those containers are labeled with `AGENTICS_RUNNER_NAMESPACE`;
override it only when you intentionally want a different cleanup namespace:

```bash
AGENTICS_RUNNER_NAMESPACE=agentics-dev-$USER just dev::up
```

The Compose project name isolates Compose-owned containers, networks, and
volumes. It does not isolate runner containers created through the host Docker
socket, so runner cleanup and reconciliation depend on
`AGENTICS_RUNNER_NAMESPACE`.

The project is still pre-MVP, so database migration history can be squashed
when the team intentionally resets the baseline schema. After a migration
history reset, recreate local dev and test databases or Compose Postgres
volumes before running migrations again; old `_sqlx_migrations` rows will not
match the new baseline checksums.

By default, the dev API and web ports bind to `127.0.0.1`. To inspect the
frontend from another machine through Tailscale or a trusted LAN, bind only to
that interface and allow the hostname used by the browser:

```bash
AGENTICS_COMPOSE_BIND_IP=100.x.y.z \
AGENTICS_WEB_BASE_URL=http://your-host.tailnet.ts.net:3001 \
AGENTICS_CORS_ALLOWED_ORIGINS=http://127.0.0.1:3001,http://localhost:3001,http://your-host.tailnet.ts.net:3001 \
AGENTICS_WEB_ALLOWED_DEV_ORIGINS=your-host.tailnet.ts.net \
just dev::up
```

Use HTTPS, for example with Tailscale Serve, when testing auth flows over a
remote hostname because dev cookies are marked secure when the API is reachable
from another machine.

Stop the dev stack with:

```bash
just dev::down
```

Follow logs with:

```bash
just dev::logs
```

For project verification, use the Docker Compose test harness. Prepare the
Linux test storage root once, then start the dedicated test Docker daemon:

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just test-env-up
```

Run the CPU-only full suite with:

```bash
just test-env-status-cpu
just test-all-cpu
```

On Linux hosts with NVIDIA GPU support, run the full suite, including ignored
CUDA/GPU tests, with:

```bash
just test-env-status
just test-all
```

Both suites start test-scoped Postgres and RustFS services, initialize the test
S3 bucket, and run the Rust integration crate inside a Rust container. They use a
dedicated test Docker daemon at `unix:///srv/agentics-test/docker.sock`, backed
by `/srv/agentics-test/docker-data-root`, so Docker layer quotas are tested
against overlay2 on XFS with `prjquota` instead of the workstation daemon. The
wrapper uses a unique Compose project and runner namespace for each run, then
removes test-scoped Compose volumes after the test service exits. Stop only the
dedicated test daemon with:

```bash
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just test-env-down
```

Any container that creates runner containers through the host Docker socket must
use host-visible paths. Mount runner runtime roots, storage work roots,
challenge materialization roots, and quota slot paths into the worker or tests
container at the same absolute path that the host Docker daemon sees. Avoid
container-only `/tmp` paths for anything that will later be bind-mounted into a
runner container.

## Frontend Dev Data

The Compose dev stack uses the migrated challenge repository as its source of
truth. Before the web service starts, it publishes all migrated non-GPU
Frontier-CS challenges, assembles their runtime bundles with restored private
asset overlays, and stages any matching workspace in
`challenge-repos/agentics-challenges/test-solutions/` as an official
test-solution submission:

```bash
just dev::up
```

Open the frontend at:

```text
http://127.0.0.1:3001
```

Use the Tailscale/LAN environment variables in the containerized dev section
when another machine needs to inspect the frontend.

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

Install the repository hook once with `just maintenance::setup-hooks`. The hook delegates to
the Rust `agentics-pre-commit` ops binary, runs independent checks concurrently,
and always checks the human/agent docs policy and large-file threshold before a
non-empty commit.

Run the canonical full suite before committing code changes. Use the CPU-only
suite only when the task or environment explicitly cannot exercise GPU tests:

```bash
just test-all-cpu
# On Linux hosts with NVIDIA GPU support:
just test-all
```

If SQLx reports a migration version or checksum mismatch, the local database
was created from an older pre-MVP migration history. Drop and recreate that
disposable database instead of editing `_sqlx_migrations` by hand.

For frontend changes:

```bash
cd frontends/web
bun install --frozen-lockfile
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

For S3-compatible storage changes, run the RustFS-backed storage test through
Docker:

```bash
just storage::rustfs-up
just storage::s3-test
just storage::rustfs-down
```

The test uses the official `rustfs/rustfs` image and a Docker named volume.
Agentics still enforces its own per-object byte limits before writing to S3.
Durable storage defaults to RustFS/S3 in dev, test, and production; use local
filesystem storage only when a test explicitly targets the local backend.

For Rust change-risk coverage, use `cargo llvm-cov` to write LCOV and
`cargo crap` to rank complex, under-covered functions:

```bash
just risk::unit
```

This unit/package workflow excludes the `integration-tests` crate so it does
not require a database or prepared DGX quota storage. The LCOV file is written
to `target/llvm-cov/agentics-workspace.lcov`.

For a fuller signal that includes DB-backed integration tests, provide an
explicit disposable PostgreSQL database URL and run:

```bash
AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics_test' \
  just risk::integration
```

`just risk::integration` runs the full Rust test set, including `#[ignore]`
hardware tests, before the CRAP report is produced. It does not skip quota-root
or CUDA smoke tests, so prepare the quota-sensitive and Linux/NVIDIA hardware test
environment first. Set `AGENTICS_CRAP_TOP` to change how many ranked functions
are printed.

On Linux hosts, quota-sensitive runner tests need a test-owned XFS quota root.
Prepare it separately from the production `/srv/agentics` runtime tree:

```bash
sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
  agentics-prepare-dgx-spark-test-storage
```

The canonical `just test-all-cpu` and `just test-all` commands set the matching
test runner paths for the Compose harness. On Linux, quota-sensitive integration
tests fail fast when the prepared bounded test quota root is missing or
malformed.

These test variables intentionally point at `/srv/agentics-test` so local
verification does not change production runner slot ownership.

## API And Schema Changes

Rust response DTOs consumed by the web frontend should derive
`schemars::JsonSchema`. Preserve the API JSON policy documented in
`docs/api-json-contract/en.md`: absent optional response fields should be
omitted rather than serialized as explicit `null`, and API errors should use
the nested `ErrorResponse { error: { code, message, details? } }` envelope.

After changing shared DTOs used by the frontend, run:

```bash
(cd frontends/web && bun install --frozen-lockfile)
(cd frontends/web && bun run generate:schemas)
(cd frontends/web && bun run generate:schemas:check)
```

Keep `frontends/web/src/lib/schemas.ts` as the stable import facade.

External contract validation that is shared by backend, worker, CLI, or web
belongs in `crates/contracts/src/validation/`. Keep archive envelope checks,
text limits, target selection, public API query bounds, GitHub PR provenance,
and web schema exports there instead of duplicating them in handlers or
frontend helpers. Database admission controls and guarded state transitions
stay with persistence/services modules that own those durable invariants.

## Documentation Rules

When changing planned product scope, update both PRDs and both milestone docs in
the same change set. When changing implemented behavior, update the relevant
current docs in the same change set.

When adding a new document, create a folder with at least `en.md` and `zh.md`.
Keep multilingual documents aligned at the feature level.

## Shutdown

```bash
just dev::down
```

## References

- [Root README](../../README.md)
- [API JSON contract](../api-json-contract/en.md)
- [Targets](../targets/en.md)
- [Solution protocol](../solution-protocol/en.md)
- [Operations runbook](../operations/en.md)
- [Ports, paths, and target policy](../ports-and-paths/en.md)
- [Visual identity system](../visual-identity-system/en.md)
- [Rust modernization reference](../../.agents/skills/full-code-review/references/rust-modernization.md)
