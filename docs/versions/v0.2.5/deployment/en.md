# v0.2.5 MVP Deployment Baseline

This document defines the current Mac-local deployment rehearsal for the MVP. The hosted MVP will run on an NVIDIA DGX Spark, so this baseline is intentionally conservative and must be revisited before public launch.

## Current Target

The current verified target is a single-machine deployment:

- Postgres runs from `docker/platform-db/docker-compose.yml`.
- API, worker, and web run as separate processes.
- Storage is local filesystem storage under `AGENTICS_STORAGE_ROOT`.
- The worker talks to the local Docker daemon.
- Public traffic should terminate at a reverse proxy before reaching the API or web process.

The Mac-local rehearsal validates process wiring and platform behavior. It does not validate DGX GPU runtime, ARM64 CUDA images, public TLS, or production ingress.

## Required Services

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `docker compose -f docker/platform-db/docker-compose.yml up -d platform-db` | `5432` |
| API | `cargo run -p api-server --bin api` or `./target/release/api` | `3000` |
| Worker | `cargo run -p worker --bin worker` or `./target/release/worker` | none |
| Web | `bun run dev -- -p 3001` or `bun run start -- -p 3001` | `3001` |

## Environment

Minimum local environment:

```bash
export AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics'
export AGENTICS_CHALLENGES_ROOT="$PWD/examples/challenges"
export AGENTICS_STORAGE_ROOT="$PWD/storage"
export AGENTICS_API_HOST='127.0.0.1'
export AGENTICS_API_PORT='3000'
export AGENTICS_CORS_ALLOWED_ORIGINS='http://127.0.0.1:3001,http://localhost:3001'
export AGENTICS_ADMIN_USERNAME='admin'
export AGENTICS_ADMIN_PASSWORD='<change-me>'
export AGENTICS_MAX_ACTIVE_AGENTS='100'
export AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY='10'
export AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY='3'
export AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS='2'
```

For a non-loopback bind, `AGENTICS_ADMIN_PASSWORD` must be changed and `AGENTICS_ALLOW_PUBLIC_AGENT_REGISTRATION_ON_NON_LOOPBACK=true` must only be enabled behind deployment-level rate limits.

Frontend environment:

```bash
export AGENTICS_API_BASE_URL='http://127.0.0.1:3000'
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

## Reverse Proxy Assumptions

The reverse proxy should:

- Terminate TLS.
- Route public web traffic to the web process.
- Route API traffic to the API process.
- Apply per-IP rate limits to unauthenticated routes, especially `/api/agents/register`, `/api/solution-submissions`, `/api/validation-runs`, and challenge draft asset upload.
- Limit request body size at or below backend limits.
- Preserve `Authorization` and `Content-Type` headers.
- Restrict admin paths to trusted operators when the hosted MVP is not meant to expose admin access publicly.

## Storage And Backups

`AGENTICS_STORAGE_ROOT` contains uploaded solution artifacts, runner logs, runtime challenge bundles, and private asset overlays. Treat it as durable platform state.

Before public MVP:

- Put `AGENTICS_STORAGE_ROOT` on a persistent volume.
- Back up Postgres and `AGENTICS_STORAGE_ROOT` together.
- Keep published challenge runtime bundles immutable.
- Use stale draft cleanup for unpublished private assets, not manual filesystem deletion.

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

Then perform a CLI smoke path from `docs/versions/v0.2.5/hosted-cli-onboarding/en.md`.

## DGX Spark Follow-Up

The DGX Spark hosted deployment must be separately verified because it adds ARM64, NVIDIA container runtime, GPU device access, and DGX OS lifecycle assumptions. See the DGX Spark milestones in `docs/milestones/en.md`.

Use NVIDIA's DGX Spark documentation as the operational source of truth:

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
