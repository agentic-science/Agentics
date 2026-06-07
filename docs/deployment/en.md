# Deployment Baseline

This document defines the local Compose deployment rehearsal and first
single-host production Compose stack for the MVP. The hosted MVP target runs on
NVIDIA DGX Spark and is documented separately in `docs/dgx-spark/en.md`. Use
this document for containerized local and production operation, and the DGX
host-preparation docs for Linux host setup.

## Current Target

The local verified target is a single-machine Compose deployment:

- Postgres, API, worker, and web run as Compose services.
- Durable storage defaults to RustFS/S3. Local filesystem storage is an
  explicit escape hatch via `AGENTICS_STORAGE_BACKEND=local`.
- The worker talks to the configured runner Docker daemon and creates sibling runner containers.
- Public traffic should terminate at a reverse proxy before reaching the API or web process.

The production Compose target is a single-machine project named `agentics-prod`:

- Postgres and RustFS run as Compose-managed durable services.
- API, worker, checks, and migrations use the locally built production app
  image. Its builder stage installs the internal Homebrew LLVM 22 plus Wild
  Rust toolchain; the final runtime image contains only the built binaries and
  runtime packages.
- Web uses a locally built production Next.js image served by Bun.
- The API and web ports bind to `AGENTICS_COMPOSE_BIND_IP`, defaulting to
  `127.0.0.1`, so public ingress and TLS remain outside Compose.
- Only worker and check services mount the host Docker socket.

The local Compose rehearsal validates service wiring and platform behavior. It
does not validate DGX GPU runtime, ARM64 CUDA images, public TLS, or production
ingress.

## Runner Container Ownership

Agentics workers create solution, evaluator, permission-repair, and probe
containers through the configured runner Docker daemon. Those runner containers
are host-level sibling containers, not children of the worker container.
Stopping a Compose project therefore does not automatically remove runner
containers created by the worker.

Every runner container must carry exact Agentics labels, including
`agentics.runner=zip_project`, `agentics.runner_scope`, and
`agentics.runner_namespace`. Compose project names isolate Compose-owned
services, networks, and volumes, but they do not isolate runner containers
created through the shared Docker socket. Runner reconciliation and cleanup must
filter by the configured namespace.

Local Compose defaults live in `deploy/compose/env/dev.env.example`.
Production Compose defaults and placeholders live in
`deploy/compose/env/prod.env.example`. Ports and paths are documented in
`docs/ports-and-paths/en.md`.

## Required Services

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `just dev::up` service `postgres` | `55432` host port in `dev.env.example` |
| API | `just dev::up` service `api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `just dev::up` service `worker` | none |
| Web | `just dev::up` service `web` | `${AGENTICS_WEB_PORT:-3001}` |
| RustFS | `just dev::up` and `just prod::up` service `rustfs` | dev host ports `9000`/`9001`; production internal `9000`/`9001` |

## Environment

Local Compose environment source:

```bash
deploy/compose/env/dev.env.example
```

Production Compose environment source:

```bash
cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
```

Use the `just prod::*` recipes or `agentics-compose-prod` wrapper for normal
operations. The env example also sets
`AGENTICS_COMPOSE_PROD_SERVICE_ENV_FILE=./env/prod.env` so direct Docker
Compose inspection loads the same service env file instead of the placeholder
template.

Local and production Compose both use `AGENTICS_STORAGE_BACKEND=s3` with RustFS
at `http://rustfs:9000` by default. Replace every placeholder before starting
production. External S3 is an env-only production override: change the S3
endpoint, bucket, prefix, force-path-style flag, and credentials provider
without changing the Compose graph.

Rust services validate environment values at startup. Malformed
`AGENTICS_POSTGRES_PORT`, `AGENTICS_API_PORT`, and `AGENTICS_WEB_PORT` fail
startup instead of falling back to local defaults. When host probing is enabled,
`AGENTICS_HOST_PROBE_COMMAND` must be non-empty.

Environment variables in the stage env examples are part of the startup
contract. Every new or renamed variable must have matching validation code:
required values fail fast when unset, blank, or still set to hosted
placeholders; optional values print a startup warning that includes the default;
deprecated names are rejected or explicitly warned as ignored.

For a non-loopback bind, `AGENTICS_AGENT_REGISTRATION_MODE=public` is rejected.
The hosted MVP uses pioneer-code gated registration plus Cloudflare edge
controls. Bootstrap the first admin through
`AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS`, then create admin service tokens for
operator automation from the admin console.
Human browser login is GitHub sign-in backed by a GitHub App user-authorization
flow. Configure `AGENTICS_GITHUB_APP_CLIENT_ID`,
`AGENTICS_GITHUB_APP_CLIENT_SECRET`, and `AGENTICS_GITHUB_APP_REDIRECT_URL`;
production should set the redirect URL to the public web origin plus
`/auth/github/callback`. Production keeps
`AGENTICS_WEB_SESSION_COOKIE_SECURE=true`. HTTP GitHub App redirects are valid
only for loopback local development or rehearsal callbacks; non-loopback
redirects must use HTTPS.

Frontend environment:

```bash
export AGENTICS_DEPLOYMENT_STAGE='production'
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export AGENTICS_WEB_PORT='3001'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
export NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID=''
```

Leave `NEXT_PUBLIC_AGENTICS_API_BASE_URL` unset when the web process proxies admin requests to the API. Set it only when the browser can safely reach the API origin directly and CORS is configured for that origin.
Leave `NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID` unset to disable Google
Analytics entirely. When set to a GA4 measurement id such as `G-XXXXXXXXXX`,
the web app still loads Google Analytics only after the visitor accepts
analytics cookies.
Malformed frontend URL and port environment values also fail during Next.js
config/module loading instead of being normalized silently.

## Startup Order

For local development:

1. Start the Compose dev stack:

   ```bash
   just dev::up
   ```

   The recipe starts the local Postgres, RustFS, API, worker, and web services,
   prepares the dev challenge catalog from
   `challenge-repos/agentics-challenges/dev/challenges`, and stages matching
   public test solutions. It does not start or require the persistent
   private-bundle backup RustFS service.

   The dev database name is `agentics_dev`. If an older Compose volume still
   contains `agentics_demo`, reset the disposable local dev volume before
   starting the stack.

2. Follow logs from another terminal:

   ```bash
   just dev::logs
   ```

3. Open `http://127.0.0.1:3001`.
4. Run `agentics-check-local-mvp` with `AGENTICS_WEB_BASE_URL` and admin
   credentials when you want web and admin checks.
5. Stop the stack with `just dev::down`.

For production Compose:

1. Prepare host-owned directories and runner quota storage:

   ```bash
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/runtime
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/phase-mounts
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/storage-work
   sudo install -d -m 0755 /srv/agentics/review-checkouts
   ```

2. Create and edit the production env file:

   ```bash
   cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
   ```

   Production and rehearsal app images include the public migrated challenge
   catalog from `challenge-repos/agentics-challenges/challenges` at
   `/app/challenges`, and `AGENTICS_CHALLENGES_ROOT` points there by default.
   Before starting the API against fresh object storage, run
   `just prod::restore-private-bundles` or
   `just rehearsal::restore-private-bundles --overwrite` so startup seeding can
   merge the restored private benchmark ZIP overlays into runtime bundles
   without committing private data.

   Challenge review record validation and publishing run inside the API container. Keep
   a clean, standalone, runtime-readable `agentics-challenges` checkout at
   `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT`, then pass
   `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT` as the admin
   `repository_path` when validating or publishing review records.

3. Build and start:

   ```bash
   just prod::build
   sudo just prod::runner-docker-up
   just prod::up
   ```

   `just prod::up` also verifies the production Compose bridge forwarding
   rules after Docker creates the default network. On Linux hosts it installs
   idempotent `DOCKER-USER` rules that allow the Compose bridge to reach the
   host default outbound interface and accept established return traffic. This
   keeps GitHub sign-in and other API egress from depending on stale Docker
   forwarding state.

   Pre-MVP migration history may be squashed. When a deployment picks up a new
   migration baseline, recreate disposable dev/test databases and reset
   production-rehearsal Postgres volumes before starting services. Existing
   databases with old `_sqlx_migrations` rows are incompatible with the new
   baseline checksums.

   The production app image build runs inside the same Homebrew-based internal
   Rust toolchain recipe used by Compose dev and test services. The toolchain
   metadata is written to `/opt/agentics/toolchain-info.json` in the builder
   stage for inspection during image build logs, but LLVM, Cargo, and Wild are
   not copied into the final runtime image.

4. Run production checks and inspect logs:

   ```bash
   just prod::check
   just prod::logs
   ```

5. For a disposable staging stack, use the first-class rehearsal environment:

   ```bash
   cp deploy/compose/env/rehearsal.env.example deploy/compose/env/rehearsal.env
   $EDITOR deploy/compose/env/rehearsal.env
   sudo just rehearsal::prepare-storage
   sudo just rehearsal::runner-docker-up
   just rehearsal::build
   just rehearsal::up
   just rehearsal::check
   just rehearsal::run
   ```

   `deploy/compose/env/rehearsal.env` must keep
   `AGENTICS_DEPLOYMENT_STAGE=rehearsal`, project `agentics-rehearsal`, bucket
   `agentics-rehearsal`, prefix `rehearsal`, runner namespace
   `agentics-rehearsal`, and all mutable roots under `/srv/agentics-rehearsal`.
   The rehearsal stack uses loopback ports `13100` for API, `13001` for web,
   `15432` for Postgres, and `19000`/`19001` for RustFS.
   Because rehearsal uses an HTTP loopback web origin, its env example sets
   `AGENTICS_WEB_SESSION_COOKIE_SECURE=false`; do not copy that cookie setting
   to a public production origin.

   The rehearsal stack startup publishes the same real migrated challenge
   catalog used by production after private bundles have been restored. The
   `just rehearsal::run` harness still creates run-id-scoped CPU fixture
   challenges for lifecycle probes, registers a one-use agent with a temporary
   pioneer code, exercises validation and official submissions for
   `separated_evaluator`, `piped_stdio`, and `coexecuted_benchmark`, checks
   public redaction surfaces, runs adversarial ZIP/network/private-data probes,
   and optionally runs Playwright observer UI checks. Reports are written under
   `rehearsals/<run-id>/`. Use `just rehearsal::run-cpu` when the staging host
   is intentionally CPU-only or GPU worker evidence is out of scope.

   Stop with `just rehearsal::down --runner keep` for ordinary pauses. To
   destroy the disposable environment, first inspect with
   `just rehearsal::purge-data --dry-run`, then run
   `sudo just rehearsal::purge-data --confirm-rehearsal-purge`.

   Do not run rehearsal commands against a production database or storage
   bucket that is not explicitly disposable. The purge command refuses the
   `agentics-prod` project, requires `AGENTICS_DEPLOYMENT_STAGE=rehearsal`,
   and refuses destructive paths outside `/srv/agentics-rehearsal`.

6. Stop explicitly:

   ```bash
   just prod::down --runner keep --dry-run
   just prod::down --runner keep
   just prod::down --runner clean --dry-run
   just prod::down --runner clean
   sudo just prod::runner-docker-down
   ```

`--runner keep --dry-run` and `--runner clean --dry-run` never stop services.
`--runner keep` stops Compose services and leaves runner containers alone.
`--runner clean` stops worker services first, removes only production runner
containers with exact Agentics labels, then stops the rest of the Compose stack.
`just prod::runner-docker-up` and `just prod::runner-docker-down` manage the
dedicated runner Docker daemon at `AGENTICS_DOCKER_SOCKET_PATH`; keep it running
while workers need to create runner containers.

## Edge Assumptions

The production Compose stack does not include a reverse proxy or TLS service.
The MVP edge layer is Cloudflare-managed or otherwise externally managed. It
should:

- Terminate TLS.
- Route public web traffic to the web process.
- Route API traffic to the API process.
- Apply defense-in-depth per-IP rate limits to unauthenticated routes, especially `/api/agents/register` and challenge review record asset upload, and to authenticated agent upload routes such as `/api/agent/solution-submissions` and `/api/agent/validation-runs`.
- Limit request body size at or below backend limits.
- Preserve `Authorization` and `Content-Type` headers.
- Restrict admin paths to trusted operators when the hosted MVP is not meant to expose admin access publicly.

For production Compose, route API paths such as `/healthz`, `/api/*`, and
`/admin/*` to `${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_API_HOST_PORT:-3100}`,
and route web traffic to
`${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_WEB_HOST_PORT:-3001}`. The container
listen ports stay fixed at `3100` for API and `3001` for web in production
Compose.

## Storage And Backups

Agentics durable storage is object-key based. It stores uploaded solution ZIPs,
runner logs, private asset ZIP overlays, immutable private/public challenge
bundle archives, public statements, and small creator/admin JSON artifacts.
S3 mode stores object keys in the configured bucket and prefix. Local mode maps
the same keys under `AGENTICS_STORAGE_ROOT`, but it is now an explicit opt-in
for narrow local experiments. `AGENTICS_STORAGE_WORK_ROOT` is local scratch
space for packing, unpacking, and S3 downloads; do not put runner quota storage
there.

Use S3 or RustFS-compatible storage for dev, test, and hosted object storage:

```bash
export AGENTICS_STORAGE_BACKEND='s3'
export AGENTICS_S3_BUCKET='agentics'
export AGENTICS_S3_PREFIX='mvp'
export AGENTICS_S3_REGION='us-east-1'
export AGENTICS_S3_ENDPOINT_URL='https://s3.example.internal'
export AGENTICS_S3_FORCE_PATH_STYLE='true'
export AGENTICS_STORAGE_WORK_ROOT='/srv/agentics/storage-work'
```

Dev and production Compose use RustFS as the default single-host S3-compatible
durable storage service. The RustFS credentials are configured as
`AGENTICS_RUSTFS_ACCESS_KEY` and `AGENTICS_RUSTFS_SECRET_KEY` and are mapped to
the AWS SDK environment variables inside app services. The production RustFS
data lives in a Compose named volume; back up that volume together with
Postgres, or switch to external S3 by env before deployment.

For repeated MVP production rehearsals that need to back up migrated challenge
private bundles across stack rebuilds, start the dedicated RustFS backup
compose service:

```bash
cp deploy/compose/env/rustfs-private-backup.env.example deploy/compose/env/rustfs-private-backup.env
just storage::backup-up
```

The default store listens on `9100` for S3 and `9101` for the RustFS console,
uses `/srv/agentics-private-bundle-backups/rustfs-data` for durable data, and
creates the `migrated-challenge-private-bundles` bucket. This backup store is
not the Agentics durable storage backend and intentionally lives outside
`/srv/agentics`, so disposable production or rehearsal purges do not delete the
backup copy. When a production rehearsal starts with its own RustFS or S3
bucket, copy the needed private bundle objects from this backup store into the
rehearsal storage before reusing previously migrated challenge metadata:

```bash
just prod::restore-private-bundles
```

The restore command temporarily joins the backup RustFS container to the
production Compose network, then runs a one-shot production Compose service
with access to both private RustFS endpoints. It copies objects into the
production bucket under `AGENTICS_S3_PREFIX` and the logical
`private-bundle-backups/` prefix, skips existing byte-identical objects, and
verifies SHA-256 after each upload. Use `just prod::restore-private-bundles
--overwrite` only for a disposable rehearsal or another explicitly approved
refresh window where differing destination objects should be replaced.
`just storage::backup-down` stops the backup container without deleting objects.

For the migrated Frontier-CS algorithmic refresh batch, use the dedicated ops
tool instead of creating ZIP overlays by hand:

```bash
just storage::backup-up
just storage::refresh-frontier-cs-private-assets --dry-run
just storage::refresh-frontier-cs-private-assets --confirm-overwrite
just rehearsal::restore-private-bundles --overwrite
```

The refresh command reads
`working-notes/frontier-cs-upstream-refresh-2026-06-02.md`, verifies the synced
Frontier-CS commit, generates one `<challenge_name>/official-runs.zip` backup
object per listed challenge, validates every ZIP overlay against the Agentics
challenge contract, uploads to the persistent backup RustFS store, and verifies
object length plus SHA-256 after upload. Generated private ZIPs are staged under
`target/` and must not be committed.

Some migrated interactive official benchmarks intentionally use runtime-random
hidden state for MVP because the original Frontier-CS interactor generated that
state during judging. Public validation remains deterministic, while official
sessions store only public case parameters and random-policy metadata in
`private-benchmark/session.json`.

Credentials come only from the AWS SDK provider chain, for example environment
variables or an instance profile. Do not store S3 credentials in Agentics DB
rows or challenge specs. Agentics still enforces object-size limits before
durable writes and verifies S3 object length after upload.

For hosted or public MVP operation:

- Back up or replicate the S3 bucket/prefix according to the storage provider's
  policy.
- If you explicitly opt into local mode, put `AGENTICS_STORAGE_ROOT` on a
  persistent volume.
- Back up Postgres and durable object storage together.
- Keep published private runtime bundles and public-only bundles immutable.
- Use stale review record cleanup for unpublished private assets, not manual object
  deletion.
- Run challenge review record cleanup for stale unpublished private assets and stale
  Agentics `_tmp/` objects. `AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS` defaults
  to 24 hours. Configure S3 lifecycle cleanup for stale `_tmp/` objects as a
  second line of defense; they are temporary promotion keys and should not be
  retained as durable records.

## Hosted Runner Disk Isolation Decision

The hosted MVP uses a Linux-only storage profile before accepting public
evaluation jobs:

- Use the configured runner Docker daemon behind `AGENTICS_DOCKER_SOCKET_PATH`.
- If Docker writable-layer quotas are required, ensure that daemon's data root
  and storage driver support Docker `storage_opt.size`.
- Use Docker writable-layer quotas for writes that land in the container layer.
- Use separate per-phase loopback filesystem images for writable mounts, with
  root-prepared XFS project-quota slots under each phase mount. This applies to
  solution `setup`, `build`, and `run` phases, and to evaluator `prepare` and
  `score` phases.
- Configure the worker with
  `AGENTICS_RUNNER_SECURITY_PROFILE=production`,
  `AGENTICS_OFFICIAL_LOG_REDACTION=always` when operators want blanket official
  log redaction,
  `AGENTICS_WORKER_ACCELERATORS=gpu`,
  `AGENTICS_WORKER_GPU_PROBE_IMAGE`,
  `AGENTICS_DGX_DOCKER_DATA_ROOT`,
  `AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`,
  `AGENTICS_RUNNER_RUNTIME_ROOT`, `AGENTICS_RUNNER_PHASE_MOUNT_ROOT`,
  `AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB`, and
  `AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true`.
- Gate strict probes with `AGENTICS_HOST_PROBE_MODE=off|warn|require`, not the
  generic `CI` variable. Production runner security also requires
  `AGENTICS_HOST_PROBE_MODE=require`, `AGENTICS_DGX_RUN_MUTATING_PROBES=true`,
  and digest-pinned images.
- Keep local Compose development permissive. The strict storage probe belongs to
  hosted Linux staging and DGX-hosted workers.
- `AGENTICS_OFFICIAL_LOG_REDACTION=contract_based` is the default and keeps
  official runner diagnostics only for contracts that use public-only official
  material. Set it to `always` in hosted deployments that prefer the previous
  blanket official-log redaction behavior.

This combination is chosen because Docker writable-layer quotas and bounded
mounts protect different paths. `storage_opt.size` covers container-layer writes
such as package caches or accidental writes outside mounts. Quota slots under
the separate loop images cover runner-owned writable mounts such as workspaces,
`/io`, `/setup`, `/output`, home, and temporary directories. The worker
chooses the smallest configured slot class that can satisfy the effective phase
`disk_limit_mb`; align resource profiles to slot classes when an exact hard
phase limit is required. Both layers are needed for a hard writable disk
boundary across all runner phases.

## Rollback

The safe rollback path is:

1. Stop web, worker, and API.
2. Restore the previous API and worker binaries.
3. Restore the previous web build.
4. Restart API, then worker, then web.
5. Run `/healthz`, `/admin/capacity`, `/admin/service-heartbeats`, and one CLI status/list command.

Do not roll back database migrations by hand during MVP rehearsals unless the
migration is explicitly reversible and the storage snapshot is from the same
point in time. The project does not maintain down migrations; rollback is a
database and durable-storage snapshot restore.

For production Compose, use `just prod::down --runner keep` for ordinary
binary or image rollback when running evaluations can be allowed to reconcile
later. Use `just prod::down --runner clean` only when the operator has
chosen to terminate matching production runner containers. Dry-run forms do not
stop services.

## Verification

Run:

```bash
agentics-check-local-mvp
```

For production Compose, run:

```bash
just prod::check
```

The production check verifies the same Compose bridge forwarding rules and then
probes HTTPS egress from the API container to GitHub, which is required for
browser sign-in.

Then perform a CLI smoke path using the root `README.md` submitter flow or
`skills/agentics-cli-workflow/SKILL.md`.

## DGX Spark Hosted Profile

DGX Spark host preparation is verified separately because it adds ARM64, Docker
GPU device access, XFS quota setup, and DGX OS lifecycle assumptions. See the
DGX Spark milestones in `docs/milestones/en.md`.

The first host inventory is summarized in `docs/dgx-spark/en.md`. The
repeatable check is:

```bash
agentics-check-dgx-spark-host
```

This check is Linux-gated and reports Docker/NVIDIA GPU blockers without
mutating host state. The current inventory confirms OS, GPU, NVIDIA toolkit,
storage, XFS tooling, loopback tooling, default Docker GPU smoke behavior, and
the configured host Docker socket.

DGX Spark host preparation and smoke evidence are recorded in
`docs/dgx-spark/en.md`, with Linux-gated storage/profile binaries in
`agentics-ops`.

Use NVIDIA's DGX Spark documentation as the operational source of truth:

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
