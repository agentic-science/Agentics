# Agentics Containerization Plan

This note records the containerization direction discussed for Agentics ops and
the first dev/test implementation pass. Public contributor docs are the
human-facing runbook for commands that already exist.

## Goals

- Reduce manual setup for local development, integration testing, and
  production deployment.
- Use Docker Compose to run the long-lived platform services consistently.
- Keep runner execution on the host Docker daemon, using the existing Docker
  API based runner model.
- Implement the rollout in this order:
  1. local development;
  2. containerized integration testing;
  3. production deployment.
- Avoid Docker-in-Docker for the platform runner path.
- Keep the implementation aligned with the operational scripting policy:
  Rust-first ops wrappers, typed config, deterministic cleanup, idempotence, and
  no duplicated constants.

## Key Decisions From Discussion

### Use Host Docker Socket, Not Docker-In-Docker

The chosen model is:

```text
worker container -> host Docker socket -> sibling runner containers
```

The worker container will mount the host Docker socket and create solution,
evaluator, permission-repair, and probe containers as host-level sibling
containers.

This means:

- runner containers are not children of the worker container;
- runner containers are not removed automatically when the worker exits;
- `docker compose down` does not remove runner containers created by the worker;
- every runner container must be labeled explicitly by Agentics;
- cleanup and reconciliation must be implemented by Agentics, not assumed from
  Docker or Compose.

The rejected model is true Docker-in-Docker:

```text
worker container -> nested dockerd -> nested runner containers
```

True Docker-in-Docker can be made to work, including with GPUs, but it requires
privileged containers, nested Docker storage, nested NVIDIA runtime setup, and
more complicated debugging. It is not the preferred path for this project.

### Compose Owns Platform Services

Docker Compose should own long-lived services:

- API server;
- web frontend;
- worker process;
- Postgres;
- RustFS or another S3-compatible object store;
- migration and seed one-shot jobs;
- optional health/check jobs.

Compose should not be treated as the owner of per-submission runner containers.
The worker owns those through the host Docker API.

### Same Docker Socket For Now

Production will use the same host Docker socket for now. A dedicated runner
Docker daemon/socket can be revisited later if operational isolation becomes
worth the extra host-level setup.

Only the services that need Docker should mount the socket:

- worker in dev/prod;
- tests container in containerized integration tests;
- ops/check containers that intentionally inspect Docker.

API, web, Postgres, and RustFS should not mount the Docker socket.

### Three Compose Environments

Use separate Compose project names to isolate dev, test, and prod stacks on one
host:

```bash
docker compose -p agentics-dev-$USER ...
docker compose -p agentics-test-$RUN_ID ...
docker compose -p agentics-prod ...
```

The project name isolates Compose-created containers, networks, and volumes.
It does not isolate runner containers created through the host Docker socket;
runner labels must do that.

## File Layout

The exact paths can change during implementation, but a likely structure is:

```text
deploy/compose/
  compose.yml               # shared base services
  compose.dev.yml           # local development overrides
  compose.test.yml          # integration-test overrides
  compose.prod.yml          # production overrides
  env/
    dev.env.example
    test.env.example
    prod.env.example
```

Implemented so far:

```text
deploy/compose/compose.yml
deploy/compose/compose.dev.yml
deploy/compose/compose.test.yml
deploy/compose/compose.prod.yml
deploy/compose/env/dev.env.example
deploy/compose/env/test.env.example
deploy/compose/env/prod.env.example
docker/app/Dockerfile
docker/web/Dockerfile
just compose-dev-up
just compose-dev-down
just compose-dev-logs
just compose-test-docker-up
just compose-test-docker-down
just compose-test-integration
just compose-prod-build
just compose-prod-up
just compose-prod-down --runner keep|clean
just compose-prod-check
```

Potential later ops binaries include focused wrappers such as
`agentics-compose-dev` and `agentics-compose-test`. Production now uses the
focused `agentics-compose-prod` wrapper because build, up, down, logs, checks,
and runner cleanup are one cohesive task family. Do not add one giant unrelated
ops executable. Use separate binaries for separate operational tasks, or
cohesive subcommands only when the task family is clearly one thing.

## Shared Compose Base

The first base Compose file defines shared Postgres and reusable volumes. Later
production work can grow the common graph toward:

```text
postgres
rustfs
api
web
worker
migrate
seed or setup
```

Recommended base properties:

- one internal network for service-to-service traffic;
- explicit healthchecks for Postgres, RustFS, API, and web;
- named volumes for database and object storage, overridden per environment;
- environment variables sourced from env files and shared constants where
  possible;
- no hardcoded duplicated ports, paths, mode strings, or credentials;
- API depends on migrations and storage readiness;
- worker depends on API config, database, storage, and Docker socket readiness;
- web depends on API reachability or its configured API base URL.

RustFS/S3 is the default durable storage path for dev, test, and production.
`agentics-local-demo` uploads deterministic fake solution artifacts through the
same storage abstraction as real submissions. Local filesystem storage remains
available only as an explicit backend override and for focused local-storage
tests.

## Runner Container Ownership

Runner containers carry labels that make cleanup deterministic:

```text
agentics.runner=zip_project
agentics.runner_scope=hosted-worker | local-validation
agentics.runner_namespace=<compose-project-or-run-id>
agentics.job_id=<evaluation-job-id>
agentics.worker_id=<worker-id>
agentics.attempt_count=<attempt-count>
agentics.phase=<setup|build|run|evaluator|...>
```

The important new idea is `agentics.runner_namespace`. Current runner cleanup
already separates hosted-worker and local-validation scopes, but that is too
broad once multiple Compose projects share one host Docker daemon.

Without a namespace, a dev worker, a prod worker, and a test worker can all see
containers with `agentics.runner_scope=hosted-worker`. One worker might fail to
find another worker's job in its own database and incorrectly remove the other
runner container.

Runner listing and cleanup should filter by:

```text
agentics.runner=zip_project
agentics.runner_scope=<scope>
agentics.runner_namespace=<namespace>
```

This namespace is now part of runner config as `AGENTICS_RUNNER_NAMESPACE` and
is passed through runner container creation and cleanup paths.

## Host Path Rule

Any container that creates runner containers through the host Docker socket must
use host-visible paths.

Bad:

```text
tests container creates /tmp/foo
host Docker daemon cannot see /tmp/foo
runner bind mount fails or points at the wrong host path
```

Good:

```text
host path:      /srv/agentics-test/<run-id>/tmp
tests path:     /srv/agentics-test/<run-id>/tmp
runner mount:   /srv/agentics-test/<run-id>/tmp
```

Mount host runner roots into the worker or tests container at the same absolute
path. Avoid container-only temp directories for runner workspaces, storage work
roots, challenge materialization roots, and quota slot paths.

## GPU Handling

The worker container usually does not need direct GPU access. It needs Docker
socket access so it can ask the host Docker daemon to create runner containers
with GPU device requests.

Requirements:

- host Docker daemon has NVIDIA Container Toolkit configured;
- GPU worker config sets `AGENTICS_WORKER_ACCELERATORS=gpu`;
- GPU worker config sets `AGENTICS_WORKER_GPU_PROBE_IMAGE`;
- runner creation keeps using Docker GPU device requests;
- Compose may expose GPUs to an ops/probe service when that service itself must
  call GPU tooling.

GPU workers should be opt-in in Compose, likely behind a `gpu` profile.

## Development First

### Development Objective

Developers should be able to start a local platform with one command while
still editing code normally.

Implemented shape:

```bash
just compose-dev-up
just compose-dev-down
just compose-dev-logs
```

### Development Services

Initial dev stack:

- `postgres-dev`;
- `api-dev`;
- `web-dev`;
- `worker-dev`;
- `migrate-dev`;
- `seed-dev`.

### Development Volumes And Paths

Use persistent but clearly dev-scoped state by default:

```text
.agentics-compose/dev/runtime
.agentics-compose/dev/phase-mounts
.agentics-compose/dev/storage
.agentics-compose/dev/storage-work
Compose volumes for Postgres, Cargo, and Bun state
```

Developers can override `AGENTICS_DEV_ROOT`; the path is mounted into containers
at the same absolute path so runner bind mounts remain host-visible.

### Development Images

Two implementation options:

1. Build API, worker, ops, and web images locally through Compose.
2. Use bind-mounted source with `cargo run` and `bun dev` inside containers.

The first option is closer to production. The second option is faster for daily
frontend/backend iteration. The first pass uses:

- Postgres as a normal service container;
- API/worker/web as local build containers with bind-mounted source;
- local filesystem storage so the fake demo seed can write artifacts directly;
- later add production-like image builds.

### Development Runner Policy

Development can use relaxed runner security profile, but it must still label
runner containers with a namespace and must still clean up stale containers.

Development should not require DGX quota storage by default.

### Development Remote Access

The dev Compose override binds API and web host ports to `127.0.0.1` by
default. When a developer wants to inspect the frontend from another trusted
machine, such as over Tailscale, set:

```bash
AGENTICS_COMPOSE_BIND_IP=<tailscale-or-lan-ip>
AGENTICS_WEB_BASE_URL=http://<browser-hostname>:3001
AGENTICS_CORS_ALLOWED_ORIGINS=http://127.0.0.1:3001,http://localhost:3001,http://<browser-hostname>:3001
AGENTICS_WEB_ALLOWED_DEV_ORIGINS=<browser-hostname>
```

Bind to the specific Tailscale or LAN IP instead of `0.0.0.0` unless the wider
exposure is intentional. Auth flows should use HTTPS, for example through
Tailscale Serve, because non-loopback API bindings require secure cookies.

## Containerized Integration Tests Second

### Test Objective

Containerize the existing Rust integration test suite first:

```bash
cargo test -p integration-tests -- --include-ignored
```

This is not initially a black-box test of separately deployed API, worker, and
web containers.

Current integration tests:

- connect to a real Postgres database;
- spawn the API router in-process on an ephemeral port;
- run worker cycles in-process;
- use a dedicated test Docker daemon for runner containers;
- use local temporary storage roots unless a test overrides config;
- include ignored DGX/GPU/quota tests when run with `--include-ignored`.

### Test Compose Shape

Initial test stack:

```text
postgres-test
tests
```

The `tests` service runs the Rust test command and mounts:

```text
/srv/agentics-test/docker.sock:/srv/agentics-test/docker.sock
/srv/agentics-test:/srv/agentics-test
repo checkout at the same absolute path, or a build context image containing it
```

Before running the test Compose project, start a dedicated test Docker daemon:

```bash
sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just compose-test-docker-up
```

That daemon listens on `unix:///srv/agentics-test/docker.sock`, uses
`/srv/agentics-test/docker-data-root`, and must run overlay2 on an XFS data root
mounted with `prjquota`. `just compose-test-integration` verifies the daemon is
reachable, loads `agentics-linux-arm64-cpu:ubuntu26.04-local` into it when
missing, and passes `AGENTICS_TEST_DOCKER_HOST` to the test container. This keeps
quota-sensitive runner containers off the workstation Docker daemon and lets the
`--storage-opt size=...` Docker layer quota path be tested for real.

The test command pattern is:

```bash
docker compose -p agentics-test-$RUN_ID \
  -f deploy/compose/compose.yml \
  -f deploy/compose/compose.test.yml \
  up --abort-on-container-exit --exit-code-from tests

docker compose -p agentics-test-$RUN_ID \
  -f deploy/compose/compose.yml \
  -f deploy/compose/compose.test.yml \
  down -v
```

Meaning:

- `up` starts the test dependencies and the `tests` service;
- `--abort-on-container-exit` stops the project when the tests service exits;
- `--exit-code-from tests` returns the Rust test exit code;
- `down -v` removes test-scoped Compose volumes.

`just compose-test-integration` owns this sequence so humans and agents do not
have to remember the raw Compose flags. It creates a unique Compose project and
sets `AGENTICS_RUNNER_NAMESPACE` to that project name. The runtime root defaults
to `/srv/agentics-test/runtime/<run-id>`, while the quota slot root remains the
prepared `/srv/agentics-test/phase-mounts`.

### Test Improvements Needed

Before or during containerization, improve the integration test suite:

1. Add `AGENTICS_RUNNER_NAMESPACE` or equivalent to runner config. Done.
2. Add the namespace label to every runner container. Done.
3. Filter runner cleanup/reconciliation by namespace. Done.
4. Add tests proving cleanup in one namespace does not touch another namespace.
5. Add a test helper for host-visible temp roots instead of plain
   `tempfile::tempdir()` when Docker bind mounts are involved.
6. Make integration test database URL resolution explicitly env-driven.
7. Make Docker host/socket resolution explicitly env-driven.
8. Keep quota-sensitive and DGX GPU tests explicit; do not skip them silently.
9. Keep integration tests RustFS/S3-backed by default; local filesystem storage
   should appear only in explicit local-backend tests.
10. Collect Compose logs and runner cleanup diagnostics on failure.

### Quota And DGX Tests

Quota-sensitive tests should continue to require a prepared Linux quota root.
The test wrapper may verify the root and print the exact preparation command,
but it should not hide missing setup by skipping tests.

DGX GPU tests should continue to be `#[ignore]` in Rust so normal test runs do
not require hardware, but the full containerized integration workflow must pass
`--include-ignored` when the goal is full validation.

### Later Black-Box E2E

After the current suite is containerized, add a separate black-box E2E suite
that starts:

```text
api
worker
web
postgres
rustfs
```

and exercises the deployed HTTP surface from outside the service containers.
Do not combine this with the first containerization pass; it changes the meaning
of the tests.

## Production Deployment Third

### Production Objective

Production should start from a Compose project with durable state, prebuilt
images, explicit secrets, bounded runner storage, and deterministic cleanup.

Implemented first-pass shape:

```bash
just compose-prod-build
just compose-prod-up
just compose-prod-check
just compose-prod-down --runner keep
just compose-prod-down --runner clean
just compose-prod-clean-runners --namespace agentics-prod
```

Upgrade, backup, and rollback wrappers are still later work. The current
rollback path is explicit stop, external restore, image rebuild or replacement,
and restart.

### Production Services

Production stack:

- `postgres`;
- `rustfs` or external S3-compatible storage;
- `api`;
- `web`;
- `worker-cpu`;
- optional `worker-gpu` profile;
- `migrate` one-shot job;
- optional admin/check jobs.

The first pass builds images locally from the checkout. Registry publishing can
reuse `AGENTICS_APP_IMAGE` and `AGENTICS_WEB_IMAGE` overrides later.

### Production State

Use durable volumes or externally managed storage:

```text
postgres data volume
rustfs data volume, unless using external S3
/srv/agentics/runtime
/srv/agentics/phase-mounts
/srv/agentics/storage-work
```

Production runner storage should keep the existing DGX/XFS quota model where
applicable:

```text
AGENTICS_RUNNER_SECURITY_PROFILE=production
AGENTICS_HOST_PROBE_MODE=require
AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots
AGENTICS_RUNNER_RUNTIME_ROOT=/srv/agentics/runtime
AGENTICS_RUNNER_PHASE_MOUNT_ROOT=/srv/agentics/phase-mounts
AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB=64,256,1024,4096
AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true
```

### Production Secrets

Use Compose secrets or mounted secret files where the application supports file
inputs. If the application still requires environment variables, keep the env
file out of git and document the secret lifecycle clearly.

Secrets must not be printed in logs, Compose output, test snapshots, or failure
diagnostics.

### Production Cleanup

Production cleanup must not rely on Compose alone.

Required cleanup paths:

- worker startup reconciliation by namespace;
- worker-cycle reconciliation by namespace;
- explicit ops cleanup command by namespace;
- post-down cleanup option for intentionally stopping the platform.

`agentics-compose-prod down` requires `--runner keep` or `--runner clean`.
Choosing `--runner clean` is the explicit confirmation. The dry-run variants are
strictly non-mutating:

```bash
just compose-prod-down --runner keep --dry-run
just compose-prod-down --runner clean --dry-run
```

Cleanup uses exact labels:

```text
agentics.runner=zip_project
agentics.runner_scope=hosted-worker
agentics.runner_namespace=<namespace>
```

It reports job id, worker id, attempt count, phase, and database claim status
when the production database is reachable. It does not mutate database state;
stale job repair remains worker reconciliation and stale-lease behavior.

## Implementation Phases

### Phase 1: Runner Namespace

- Add typed runner namespace config. Done.
- Default local namespace conservatively. Done.
- Pass namespace into runner labels. Done.
- Filter Docker runner listing and cleanup by namespace. Done.
- Add tests for cross-namespace isolation. Partially done with label filtering;
  add Docker-adapter tests for cleanup behavior when fakes are available.
- Update docs that describe runner cleanup. Done for contributor docs and this
  planning note.

### Phase 2: Dev Compose

- Add base and dev Compose files. Done.
- Add dev env example. Done.
- Add API, web, worker, Postgres, migrate, and seed services. Done.
- Add RustFS. Deferred.
- Add Rust ops wrapper or focused just recipes. Done with just recipes.
- Add dev runner host-path setup and cleanup. Host-path setup done; explicit
  cleanup command deferred.
- Verify local submission/validation flow.

### Phase 3: Containerized Integration Tests

- Add test Compose override and `tests` service. Done.
- Add host-visible test root management. Done through `/srv/agentics-test`
  checks and per-run runtime roots.
- Add test env resolution for database, Docker host, and runner namespace.
  Database and runner namespace done; Docker socket uses the mounted local
  default.
- Run `cargo test -p integration-tests -- --include-ignored` inside the tests
  container. Done through `just compose-test-integration`.
- Add RustFS/S3 integration smoke.
- Capture logs and cleanup runner leftovers on failure.

### Phase 4: Production Compose

- Add production Compose override. Done.
- Add production env example and secret guidance. Done.
- Add local production image build flow. Done.
- Add production startup checks. Done through the `check` service and wrapper.
- Add production cleanup/dry-run commands. Done.
- Update operator docs in English and Chinese. Done.
- Add registry publishing, backup, upgrade, and rollback wrappers. Later.

### Phase 5: Black-Box E2E

- Add a separate deployed-stack E2E suite.
- Test real API/worker/web containers through HTTP.
- Keep it separate from the Rust integration suite.

## Open Questions

- Should dev API/worker/web run from bind-mounted source for fast iteration, or
  from production-like images for closer parity?
- Should RustFS be mandatory in dev/test, or should local filesystem storage
  remain available for the fastest inner loop?
- Where should host-visible dev/test roots live by default on non-DGX machines?
- When production moves beyond the first Compose version, is a dedicated Docker
  daemon/socket worth the extra operational setup?

## Non-Goals For The First Pass

- Do not replace the current Rust integration tests with black-box E2E tests.
- Do not introduce Kubernetes, Nomad, Slurm, or true Docker-in-Docker.
- Do not mount the Docker socket into API, web, Postgres, or RustFS.
- Do not rely on Compose to clean runner containers.
- Do not silently skip quota-sensitive or GPU integration tests in the full
  validation workflow.
