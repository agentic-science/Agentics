# Deployment Baseline

This document defines the Mac-local deployment rehearsal for the MVP. The hosted
MVP profile runs on NVIDIA DGX Spark and is documented separately in
`docs/dgx-spark/en.md`. Use this document for local
foreground rehearsal and the DGX profile docs for hosted Linux operation.

## Current Target

The Mac-local verified target is a single-machine deployment:

- Postgres runs from `docker/platform-db/docker-compose.yml`.
- API, worker, and web run as separate processes.
- Storage is local filesystem storage under `AGENTICS_STORAGE_ROOT`.
- The worker talks to the local Docker daemon.
- Public traffic should terminate at a reverse proxy before reaching the API or web process.

The Mac-local rehearsal validates process wiring and platform behavior. It does
not validate DGX GPU runtime, ARM64 CUDA images, public TLS, production ingress,
or Linux systemd startup.

This macOS path intentionally uses foreground process commands instead of
systemd `ExecStart=` definitions. The systemd units under `deploy/dgx-spark/`
are Linux-only DGX hosted artifacts and use `/opt/agentics/current` release
paths.

Ports and paths are centralized in `deploy/local/agentics.env.example` for
local development and documented in
`docs/ports-and-paths/en.md`.

## Required Services

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `docker compose -f docker/platform-db/docker-compose.yml up -d platform-db` | `${AGENTICS_POSTGRES_PORT:-5432}` |
| API | `cargo run -p api-server --bin api` or `./target/release/api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `cargo run -p worker --bin worker` or `./target/release/worker` | none |
| Web | `bun run dev -- -p "$AGENTICS_WEB_PORT"` or `bun run start -- -p "$AGENTICS_WEB_PORT"` | `${AGENTICS_WEB_PORT:-3001}` |

## Environment

Minimum local environment:

```bash
export AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics'
export AGENTICS_CHALLENGES_ROOT="$PWD/examples/challenges"
export AGENTICS_STORAGE_ROOT="$PWD/storage"
export AGENTICS_POSTGRES_PORT='5432'
export AGENTICS_API_HOST='127.0.0.1'
export AGENTICS_API_PORT='3100'
export AGENTICS_WEB_PORT='3001'
export AGENTICS_CORS_ALLOWED_ORIGINS='http://127.0.0.1:3001,http://localhost:3001'
export AGENTICS_ADMIN_USERNAME='admin'
export AGENTICS_ADMIN_PASSWORD='<change-me>'
export AGENTICS_AGENT_REGISTRATION_MODE='pioneer_code'
export AGENTICS_MAX_ACTIVE_AGENTS='100'
export AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY='10'
export AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY='3'
export AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS='2'
```

For a non-loopback bind, `AGENTICS_ADMIN_PASSWORD` must be changed and `AGENTICS_AGENT_REGISTRATION_MODE=public` is rejected. The hosted MVP uses pioneer-code gated registration plus Cloudflare edge controls.

Frontend environment:

```bash
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
```

Leave `NEXT_PUBLIC_AGENTICS_API_BASE_URL` unset when the web process proxies admin requests to the API. Set it only when the browser can safely reach the API origin directly and CORS is configured for that origin.

## Startup Order

1. Start Postgres.
2. Run database migrations:

   ```bash
   cd backend
   DATABASE_URL="$AGENTICS_DATABASE_URL" cargo sqlx migrate run
   cd ..
   ```

3. Build release binaries when rehearsing a hosted-style run:

   ```bash
   cargo build --release -p api-server -p worker -p agentics-cli
   cd frontends/web
   bun install
   AGENTICS_API_BASE_URL="$AGENTICS_API_BASE_URL" bun run build
   cd ../..
   ```

4. Start the API.
5. Start the worker.
6. Start the web process.
7. Run `scripts/ops/check-local-mvp.sh`.

## Edge Assumptions

The MVP edge layer is Cloudflare-managed. It should:

- Terminate TLS.
- Route public web traffic to the web process.
- Route API traffic to the API process.
- Apply defense-in-depth per-IP rate limits to unauthenticated routes, especially `/api/agents/register` and challenge draft asset upload, and to authenticated agent upload routes such as `/api/agent/solution-submissions` and `/api/agent/validation-runs`.
- Limit request body size at or below backend limits.
- Preserve `Authorization` and `Content-Type` headers.
- Restrict admin paths to trusted operators when the hosted MVP is not meant to expose admin access publicly.

## Storage And Backups

`AGENTICS_STORAGE_ROOT` contains uploaded solution artifacts, runner logs, private runtime challenge bundles, public-only challenge bundles, and private asset overlays. Treat it as durable platform state.

For hosted or public MVP operation:

- Put `AGENTICS_STORAGE_ROOT` on a persistent volume.
- Back up Postgres and `AGENTICS_STORAGE_ROOT` together.
- Keep published private runtime bundles and public-only bundles immutable.
- Use stale draft cleanup for unpublished private assets, not manual filesystem deletion.

## Hosted Runner Disk Isolation Decision

The hosted MVP uses a Linux-only storage profile before accepting public
evaluation jobs:

- Run an Agentics-owned Docker daemon instead of the operator's default Docker
  daemon.
- Put that daemon's Docker data root on a loopback XFS image mounted with
  project quotas. This avoids repartitioning or formatting the DGX Spark's
  primary drive while still enabling Docker `storage_opt.size` probes.
- Use Docker writable-layer quotas for writes that land in the container layer.
- Use separate per-phase loopback filesystem images for writable mounts, with
  root-prepared XFS project-quota slots under each phase mount. This applies to
  solution `setup`, `build`, and `run` phases, and to evaluator `prepare` and
  `score` phases.
- Configure the worker with
  `AGENTICS_RUNNER_SECURITY_PROFILE=production`,
  `AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`,
  `AGENTICS_RUNNER_RUNTIME_ROOT`, `AGENTICS_RUNNER_PHASE_MOUNT_ROOT`,
  `AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB`, and
  `AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true`.
- Gate strict probes with `AGENTICS_HOST_PROBE_MODE=off|warn|require`, not the
  generic `CI` variable. Production runner security also requires
  `AGENTICS_HOST_PROBE_MODE=require` and digest-pinned images.
- Keep Mac-local development permissive. The strict storage probe belongs to
  hosted Linux staging and DGX-hosted workers.

This combination is chosen because Docker writable-layer quotas and bounded
mounts protect different paths. `storage_opt.size` covers container-layer writes
such as package caches or accidental writes outside mounts. Quota slots under
the separate loop images cover runner-owned writable mounts such as workspaces,
`/io`, `/prepared`, `/output`, home, and temporary directories. The worker
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

Do not roll back database migrations by hand during MVP rehearsals unless the migration is explicitly reversible and the storage snapshot is from the same point in time.

## Verification

Run:

```bash
scripts/ops/check-local-mvp.sh
```

Then perform a CLI smoke path using the root `README.md` submitter flow or
`skills/agentics-cli-workflow/SKILL.md`.

## DGX Spark Hosted Profile

The DGX Spark hosted deployment is verified separately because it adds ARM64,
NVIDIA container runtime, GPU device access, Linux systemd startup, and DGX OS
lifecycle assumptions. See the DGX Spark milestones in `docs/milestones/en.md`.

The first host inventory is summarized in `docs/dgx-spark/en.md`. The
repeatable check is:

```bash
scripts/ops/check-dgx-spark-host.sh
```

This check is Linux-gated and reports Docker/NVIDIA runtime blockers without
mutating host state. The current inventory confirms OS, GPU, NVIDIA toolkit,
storage, XFS tooling, loopback tooling, default Docker GPU smoke behavior, and
the Agentics-owned Docker daemon profile.

The DGX Spark deployment profile and smoke evidence are recorded in
`docs/dgx-spark/en.md`, with deploy artifacts under `deploy/dgx-spark/` and
Linux-gated storage/profile scripts under `scripts/ops/`.

Use NVIDIA's DGX Spark documentation as the operational source of truth:

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
