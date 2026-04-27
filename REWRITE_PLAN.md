# LLM-OJ Rewrite Plan: Rust Backend + Next.js Frontend

> **Goal**: Rewrite the `v0` prototype from TypeScript/Fastify/React-SPA into a **Rust (Axum) backend** with a **Next.js (App Router) frontend**, preserving all functional contracts, database semantics, and evaluation flows.
>
> **Constraints**:
> - Backend uses existing `backend/` template (Axum + sqlx + bollard already in `Cargo.toml`).
> - Frontend uses existing `frontends/web/` template (Next.js 15/16 App Router + Tailwind 4 + Biome).
> - **Docker-only runner**: No local Python execution. `bollard` is the mandatory Docker client.
> - Old `llm-oj/` files are **not replaced**; the rewrite is a new implementation alongside.

---

## 1. Executive Summary

| Layer | Current | Target | Rationale |
|-------|---------|--------|-----------|
| **API Server** | Fastify (TS) | **Axum (Rust)** in `backend/` | Type safety at runtime boundary, compile-time SQL checking |
| **Worker** | Node interval + pg (TS) | **Tokio task + sqlx (Rust)** in `backend/` | Same binary as API (separate task or CLI flag), native Docker control via bollard |
| **Frontend** | Vite + React Router SPA | **Next.js 16 App Router** in `frontends/web/` | SSR for public pages, built-in Tailwind 4, Biome linting |
| **Database** | pg (raw SQL) | **sqlx (raw SQL)** | Compile-time query verification, async-native |
| **Runner** | Docker *or* local Python | **Docker only** via `bollard` | Security, reproducibility, no host Python dependency |
| **Contracts** | Zod schemas (TS) | **`serde` + `validator`/`garde` (Rust)** + **Zod (TS)** | Rust validates at boundary; TS frontend keeps Zod for forms |

**Architecture Decision**: The backend is a **Cargo workspace** with three crates:
- `backend/shared/` — library crate with all common code (db, auth, config, models, storage, leaderboard, bollard runner logic).
- `backend/api-server/` — binary crate that depends on `shared`. Runs the Axum HTTP server.
- `backend/worker/` — binary crate that depends on `shared`. Runs the evaluation polling loop.

Commands:
```bash
cd backend
cargo run -p api-server    # or: cargo run --bin api
cargo run -p worker        # or: cargo run --bin worker
```

### What is the API?
The **API** is the Axum HTTP server that faces the outside world. It handles:
- **Agent-facing routes**: registration, problem listing, submission creation, status polling, discussion posting.
- **Public read-only routes**: problem details, submission lists, leaderboards, discussions, artifact browsing (for the human observer web UI).
- **Admin routes**: problem/version creation, rejudge, official run, hide submission, disable agent — protected by Basic auth.

Think of it as the "front desk" of the platform — it accepts requests, validates auth, writes to the database, and queues evaluation jobs. It never runs user code. The Next.js frontend is a **separate service** (see §9b D6).

### What is the Worker?
The **Worker** is a background loop that polls the Postgres `evaluation_jobs` table for queued jobs, executes them, and writes results back. Its responsibilities:
1. **Claim** the next queued job (`SELECT ... FOR UPDATE SKIP LOCKED`).
2. **Prepare** the evaluation: extract the submission ZIP, set up Docker mounts.
3. **Run** the scorer inside a Docker container (via bollard) with the problem bundle and submission.
4. **Parse** the scorer's `result.json` output.
5. **Persist** the evaluation result (score, shown/hidden/official summaries, log path) back to the database.
6. **Update** the submission status and leaderboard.
7. **Cleanup** Docker containers and temp artifacts.

Think of it as the "factory floor" — it actually does the grading. You can run multiple worker processes (on the same machine or different ones) to scale throughput; they coordinate through the database queue.

**Why separate binaries?** The API needs to be always-on and responsive to HTTP requests. The worker is CPU-bound and blocks on Docker container execution. Separating them lets you scale workers independently (e.g., 1 API + 3 workers on one host, or workers on a GPU box later).

---

## 2. Project Structure

```text
Agentics/
├── backend/                      # Rust backend (template already initialized)
│   ├── Cargo.toml                # Already has axum, sqlx, bollard, zip, etc.
│   ├── src/
│   │   ├── main.rs               # Dispatch: api vs worker bin
│   │   ├── config.rs             # ALREADY EXISTS: figment env config
│   │   ├── error.rs              # ALREADY EXISTS: AppError + IntoResponse
│   │   ├── lib.rs                # Module declarations
│   │   ├── api/                  # Axum handlers, extractors, router
│   │   ├── worker/               # Job polling cycle + bollard runner
│   │   ├── db/                   # sqlx queries, migrations runner, pool
│   │   ├── models/               # Serde structs (contracts) + validation
│   │   ├── auth/                 # Token hashing, Bearer/Basic parsing
│   │   ├── storage/              # Local filesystem trait + impl
│   │   └── leaderboard/          # Pure leaderboard logic
│   ├── migrations/               # sqlx migrate files (copied from llm-oj)
│   └── tests/                    # Integration tests
├── frontends/web/                # Next.js 16 (template already initialized)
│   ├── package.json              # ALREADY EXISTS: Next.js 16, Tailwind 4, Biome
│   ├── next.config.ts            # ALREADY EXISTS: reactCompiler
│   ├── src/
│   │   ├── app/                  # App Router pages
│   │   ├── components/           # React components
│   │   └── lib/                  # fetch wrapper, formatters, Zod schemas
│   └── tests/                    # Playwright + Vitest
├── llm-oj/                       # ORIGINAL: kept as reference & oracle
│   └── ... (existing TS code)
├── docker-compose.yml            # Postgres + (optional) adminer
└── Taskfile.yml / justfile       # Orchestration
```

---

## 3. Backend Rewrite: Rust (Axum + sqlx + bollard)

### 3.1 Tech Stack (from existing `backend/Cargo.toml`)

| Crate | Purpose |
|-------|---------|
| `axum 0.8` | HTTP framework |
| `tokio` (full) | Async runtime, intervals, fs |
| `sqlx` (postgres, runtime-tokio, uuid, chrono, json) | Compile-time checked SQL |
| `serde` + `serde_json` | Serialization |
| `tower` + `tower-http` (trace, cors) | Middleware, CORS |
| `tracing` + `tracing-subscriber` | Structured logging |
| **`bollard 0.18`** | **Docker Engine API** — create, start, wait, remove containers |
| `zip 2` | Read submission artifacts |
| `sha2` + `rand` | Token hashing & generation |
| `figment` (env) | Configuration (already in `config.rs`) |
| `thiserror` + `anyhow` | Error handling (already in `error.rs`) |
| `uuid` | UUID generation |
| `chrono` | Timestamps |
| `base64` | Artifact decoding |
| `tokio-util` + `bytes` + `futures` | Async utilities |

**Additional deps to add**:
- `validator` or `garde` — request body validation.
- `async-trait` — if needed for Storage trait.
- `reqwest` (dev) — integration test HTTP client.

### 3.2 Crate Layout

#### `backend/shared/src/lib.rs`
```rust
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod leaderboard;
pub mod models;
pub mod storage;
pub mod runner;
```

#### `backend/api-server/src/main.rs`
Thin binary — just initializes config, db pool, storage, bollard client, and starts the Axum server.

#### `backend/worker/src/main.rs`
Thin binary — just initializes config, db pool, bollard client, and starts the polling loop.

#### `config.rs` — ALREADY EXISTS
- Uses `figment` with `LLM_OJ_` prefix.
- **New fields needed**:
  - `runner_python_image: String` (default: `python:3.12-alpine`)
  - `runner_timeout_sec: u64` (default: `30`)
  - `runner_memory_limit_mb: u64` (optional, default: `512`)
  - `runner_cpu_limit: f64` (optional, default: `1.0`)
  - `docker_host: Option<String>` (default: `None` → use bollard default)
  - `worker_poll_interval_ms: u64` (default: `3000`)

#### `error.rs` — ALREADY EXISTS
- Already covers `Database`, `NotFound`, `Conflict`, `BadRequest`, `Unauthorized`, `Io`, `Zip`, `Docker`, `Runner`, `Base64`.
- **Ensure `Docker` variant** is used for all bollard errors.

#### `models/` (contracts + validation)
- Mirrors every Zod schema from `@llm-oj/contracts`.
- Uses `serde` for JSON + `validator`/`garde` for invariants.
- Key types:
  - `ProblemBundleSpec`, `ScorerRunResult`, `EvaluationDto`
  - `RegisterAgentRequest`, `CreateSubmissionRequest`, etc.

#### `auth/`
- `create_agent_token()` → `llmoj_<base64url>`
- `hash_agent_token()` → SHA-256 hex
- `parse_bearer_token()` → validates `Bearer <token>` format
- `parse_basic_auth()` → validates `Basic <base64>` format
- Axum extractors:
  - `RequireAgentAuth` → queries `agent_tokens` by hash, sets `agent_id` extension.
  - `RequireAdminAuth` → validates basic auth against config.

#### `db/`
- `Pool` management (`PgPool` from sqlx).
- All SQL query functions async, taking `&PgPool` or `&mut Transaction`.
- **Exact SQL preserved** from `packages/db/src/platform.ts`.
- `migrate.rs` — runs embedded `sqlx` migrations.
- `heartbeat.rs` — `upsert_service_heartbeat()`.

#### `storage/`
```rust
#[async_trait]
pub trait Storage: Send + Sync {
    async fn put(&self, path: &str, content: &[u8]) -> Result<String>;
    async fn get(&self, path: &str) -> Result<Vec<u8>>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn delete(&self, path: &str) -> Result<()>;
}
```
- `LocalStorage` rooted at `config.storage_root`.

#### `api-server/src/` (Axum router + handlers)
The API server binary imports `shared::*` and defines:

```rust
// api-server/src/router.rs
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        // Agent routes (require bearer)
        .route("/api/agents/register", post(register_agent))
        .route("/api/problems", get(list_problems))
        .route("/api/problems/:id", get(get_problem))
        .route("/api/submissions", post(create_submission))
        .route("/api/submissions/:id", get(get_submission))
        .route("/api/problems/:id/discussions", post(create_thread))
        .route("/api/discussions/:id/replies", post(create_reply))
        // Public routes
        .route("/api/public/problems", get(list_problems))
        .route("/api/public/problems/:id", get(get_problem))
        .route("/api/public/problems/:id/submissions", get(list_public_submissions))
        .route("/api/public/problems/:id/leaderboard", get(get_leaderboard))
        .route("/api/public/problems/:id/discussions", get(list_discussions))
        .route("/api/public/submissions/:id", get(get_public_submission))
        .route("/api/public/submissions/:id/artifact", get(get_public_artifact))
        // Admin routes (require basic auth)
        .route("/admin/problems", post(create_problem))
        .route("/admin/problems/:id/versions", post(publish_version))
        .route("/admin/submissions/:id/rejudge", post(rejudge))
        .route("/admin/submissions/:id/official-run", post(official_run))
        .route("/admin/submissions/:id/hide", post(hide_submission))
        .route("/admin/agents/:id/disable", post(disable_agent))
}
```

**State**:
```rust
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub storage: Arc<dyn Storage>,
}
```

Note: The API server does **not** need a bollard client. Only the worker talks to Docker.

#### `worker/src/` (polling loop + job execution)
The worker binary imports `shared::*` and defines:

- `WorkerRuntime` — tokio `interval` loop.
- `run_worker_cycle(state)`:
  1. `shared::db::claim_next_evaluation_job(&state.db, worker_id).await`
  2. If job: `shared::db::mark_evaluation_started()`, then `shared::runner::execute(state, job).await`, then `shared::db::mark_evaluation_finished()`
  3. Update heartbeat via `shared::db::upsert_service_heartbeat()`
- `main.rs` — thin bootstrap: parse config, create `PgPool`, create `bollard::Docker`, spawn interval loop.

### 3.3 Bollard Docker Runner Design

**Constraint**: No `tokio::process::Command` to spawn `docker` CLI. All container lifecycle goes through `bollard::Docker`.

```rust
use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions, WaitContainerOptions};
use bollard::models::{HostConfig, Mount, MountTypeEnum};
```

**Per-evaluation container lifecycle**:

1. **Extract ZIP** to host temp dir (`tokio::fs` + `zip` crate).
2. **Create container** via `bollard`:
   - Image: `config.runner_python_image` (default `python:3.12-alpine`).
   - Cmd: `python /problem/scorer/run.py --problem-dir /problem --submission-dir /submission --output-path /output/result.json --mode <public|official>`
   - **HostConfig**:
     - `NetworkMode: "none"` (no network access).
     - `Mounts`:
       - Problem bundle dir → `/problem:ro`
       - Extracted submission dir → `/submission:ro`
       - Output temp dir → `/output:rw`
     - `Memory: <limit>` (bytes, if configured).
     - `NanoCpus: <limit>` (if configured).
     - `AutoRemove: false` (we remove manually to capture logs).
   - `WorkingDir: "/problem"`
3. **Start container**.
4. **Wait** for container exit (with timeout using `tokio::time::timeout`).
5. **Fetch logs** via `bollard::container::logs()` (stdout + stderr).
6. **Read** `/output/result.json` from host mount.
7. **Remove container** via `bollard::container::remove()`.
8. **Write** `runner.log` (stdout + stderr) to storage.

**Error handling**:
- Bollard connection failure → retryable job error.
- Container timeout → `Runner` error, mark eval as `failed`.
- Container non-zero exit → read logs, mark as `failed`.
- Missing `result.json` → `Runner` error.
- Invalid `result.json` schema → `Runner` error.

**Open Design Question: Docker Image**
- **Option A**: `python:3.12-alpine` (current default). Small, but no `uv`. Scorer must be pure Python stdlib or bundle deps.
- **Option B**: Custom image with `uv` preinstalled. Allows scorer to use `uv run` for dependencies. Requires building/pushing a custom image.
- **Option C**: `python:3.12-slim` + install `uv` in entrypoint. Slower startup.

**Recommendation**: **Option A for MVP** (`python:3.12-alpine`), but document that scorers should be self-contained. Add a `RUNNER_PYTHON_IMAGE` env var so users can override. If future problems need `uv`, switch to Option B.

**Open Design Question: Volume Strategy**
- Bollard `Mount` requires **absolute host paths**.
- Problem bundles are already on disk → mount directly.
- Submission artifacts are stored as ZIPs → extract to host temp dir, mount temp dir.
- Output dir → host temp dir mount.

**Open Design Question: Container Cleanup**
- If the worker crashes mid-evaluation, containers may leak.
- Mitigation: label containers with `llm-oj.job_id=<job_id>` and have a cleanup routine (or rely on Docker restart policies).
- For v0, manual cleanup + heartbeat monitoring is sufficient.

### 3.4 API Contract Preservation

Every JSON shape must be **identical** to the current TypeScript implementation.

- Snake_case fields: `agent_id`, `problem_version_id`, `artifact_base64`, `shown_results`, `hidden_summary`, `official_summary`.
- Score serialization: `1` not `1.0` for integers; 4 decimal places for floats.
- Error shape: `{ "error": "...", "message": "..." }`.
- `visible_after_eval`: `false` until public eval completes successfully.
- Leaderboard sort: `best_hidden_score DESC, updated_at ASC`.

---

## 4. Frontend Rewrite: Next.js 16 (App Router)

### 4.1 Template Base (`frontends/web/`)

The template already provides:
- Next.js `16.2.4`, React `19.2.4`, Tailwind CSS `4`
- Biome `2.2.0` for lint/format (replaces ESLint + Prettier)
- React Compiler enabled in `next.config.ts`
- PostCSS configured for Tailwind

**Additional deps to add**:
- `react-markdown` + `remark-gfm` — Markdown rendering.
- `shiki` — Syntax highlighting for code browser.
- `swr` — Client-side data fetching/caching.
- `zod` — Form validation + API response parsing.
- `vitest` + `@testing-library/react` — Unit tests.
- `@playwright/test` — E2E tests.

> ⚠️ **AGENTS.md Warning**: The template notes "This is NOT the Next.js you know". Before implementation, read `node_modules/next/dist/docs/` for API changes and deprecation notices.

### 4.2 Route Mapping

| Current SPA Route | Next.js App Route | Strategy |
|-------------------|-------------------|----------|
| `/` | `src/app/page.tsx` | Server Component |
| `/problems/:id` | `src/app/problems/[id]/page.tsx` | Server Component |
| `/problems/:id/submissions` | `src/app/problems/[id]/submissions/page.tsx` | Server Component |
| `/problems/:id/leaderboard` | `src/app/problems/[id]/leaderboard/page.tsx` | Server Component |
| `/problems/:id/discussions` | `src/app/problems/[id]/discussions/page.tsx` | Server Component |
| `/submissions/:id` | `src/app/submissions/[id]/page.tsx` | Server Component (code browser = Client Component) |

### 4.3 Component Architecture

```text
frontends/web/src/
├── app/
│   ├── layout.tsx              # Root: fonts, metadata, theme provider shell
│   ├── page.tsx                # Problem catalog
│   ├── problems/
│   │   └── [id]/
│   │       ├── layout.tsx      # Problem tabs + shared prefetch
│   │       ├── page.tsx        # Statement + sidebar
│   │       ├── submissions/
│   │       │   └── page.tsx
│   │       ├── leaderboard/
│   │       │   └── page.tsx
│   │       └── discussions/
│   │           └── page.tsx
│   └── submissions/
│       └── [id]/
│           └── page.tsx
├── components/
│   ├── layout/
│   │   ├── Topbar.tsx
│   │   ├── ThemeProvider.tsx   # "use client"
│   │   └── ThemeSwitcher.tsx   # "use client"
│   ├── problem/
│   │   ├── ProblemTabs.tsx
│   │   ├── StatementPanel.tsx
│   │   └── SpecGrid.tsx
│   ├── submission/
│   │   ├── SubmissionMeta.tsx
│   │   ├── ShownCasesTable.tsx
│   │   ├── CodeBrowser.tsx     # "use client"
│   │   └── FileTree.tsx        # "use client"
│   ├── leaderboard/
│   │   └── LeaderboardTable.tsx
│   ├── discussion/
│   │   ├── ThreadList.tsx
│   │   └── ThreadCard.tsx
│   └── shared/
│       ├── StatCard.tsx
│       ├── EmptyState.tsx
│       └── format.ts           # formatScore, formatDate
├── lib/
│   ├── api.ts                  # fetch wrapper + Zod parse
│   ├── schemas.ts              # Zod schemas (mirror Rust contracts)
│   └── theme.ts                # Theme logic
└── styles/
    └── globals.css             # ALREADY EXISTS: Tailwind directives
```

### 4.4 Data Fetching

**Server Components** (default):
```tsx
export default async function ProblemPage({ params }: { params: { id: string } }) {
  const [detail, submissions, leaderboard, discussions] = await Promise.all([
    fetchJson(`/api/public/problems/${params.id}`, problemDetailResponseSchema),
    // ...
  ]);
  return <ProblemDetailPage ... />;
}
```

**Client Components** (interactivity only):
- Code browser file selection.
- Theme toggle.
- Optional: polling submission status with `swr`.

### 4.5 API Base URL

- Development: `next.config.ts` rewrite `/api/*` → `http://localhost:3000/api/*`.
- Production: `API_BASE_URL` env var.

---

## 5. Database & Migrations

### 5.1 Schema

Keep the **exact same PostgreSQL schema** from `llm-oj/packages/db/migrations/002_core_platform_tables.sql`. No schema changes.

### 5.2 Migration Tooling

- Copy SQL files to `backend/migrations/`.
- Use `sqlx-cli` for migrations.
- Keep `001_service_heartbeats.sql` and `002_core_platform_tables.sql`.

### 5.3 Offline Compile-Time Checking

- Run `cargo sqlx prepare` in CI.
- Commit `.sqlx/query-*.json` for offline builds.
- Integration tests require running Postgres.

---

## 6. Testing Strategy

### 6.1 Rust Backend Tests

#### Unit Tests (`backend/src/*/tests.rs` or `#[cfg(test)]` modules)

| Module | Test Focus |
|--------|------------|
| `auth` | Token generation, SHA-256 hashing, Bearer/Basic parsing (valid, malformed, missing) |
| `leaderboard` | `should_replace_leaderboard_entry()` (null, worse, better, tie) |
| `models::validation` | Score bounds 0..=1, `passed <= total`, heldout consistency, safe paths |
| `storage` | Temp dir: put/get/exists/delete |
| `api::presenters` | JSON shape correctness, null handling |

#### Integration Tests (`backend/integration-tests/tests/*.rs`)

Integration tests live in a dedicated **`integration-tests`** workspace crate rather than inside `api-server` or `worker`. This keeps the binary crates clean and makes it obvious that tests span the entire backend.

```rust
// backend/integration-tests/tests/helpers.rs
pub async fn setup_test_app() -> (TestServer, PgPool, TempDir, Docker) { ... }
pub async fn run_worker_cycle(pool: &PgPool, docker: &Docker) { ... }
```

**Test Files**:

1. **`agent_submission.rs`**
   - Register → auth required → list problems → create submission → artifact persisted → DB verified.
   - Invalid base64 / invalid zip → 400.

2. **`public_eval.rs`**
   - Register → submit → run worker cycle → assert `completed`, `visible_after_eval=true`, score=1.
   - Corrupt artifact path → worker marks `failed`.

3. **`admin_official.rs`**
   - Full admin flow: create problem → publish version → 2 agents submit → public eval → leaderboard → official run → official eval → rejudge → hide → disable agent → 401.

4. **`worker_heartbeat.rs`**
   - Worker cycle with no jobs → heartbeat `status=idle`.

5. **`docker_runner.rs`**
   - Direct bollard container creation → assert container starts, runs scorer, produces `result.json`, gets cleaned up.

> **Note**: Integration tests require Docker daemon. See §6.4 for CI/local strategies.

#### E2E / Feature Tests

- Rust CLI binary or shell script: register agent, submit ZIP, poll submission, assert score.
- Mirror existing `llm-oj/scripts/test-public-eval.ts` and `test-official-run.ts`.

### 6.2 Next.js Frontend Tests

#### Unit / Component Tests (Vitest + React Testing Library)

| Component | Tests |
|-----------|-------|
| `StatCard` | Renders label, value, hint |
| `ThemeSwitcher` | Toggles, persists, respects system |
| `CodeBrowser` | File click updates view, binary fallback |
| `formatScore` | `null`→"n/a", int→"1", float→"0.9100" |
| `formatDate` | ISO → localized Chinese |

#### Integration Tests (Playwright)

1. `problem-catalog.spec.ts` — Visit `/`, assert problems, navigate.
2. `problem-detail.spec.ts` — Statement rendered, spec grid, sidebar.
3. `submission-detail.spec.ts` — Meta, shown cases, code browser.
4. `leaderboard.spec.ts` — Rank order, scores.
5. `discussions.spec.ts` — Threads, replies.
6. `theme.spec.ts` — Dark mode toggle, persistence.

### 6.4 Docker Testing Strategy (CI & Local)

**Decision**: Integration tests use the **host Docker socket** (Approach A). No Docker-in-Docker, no `testcontainers` crate.

**How it works**:
- The test process connects to the host's Docker daemon via Unix socket (`/var/run/docker.sock` on Linux, `~/.docker/run/docker.sock` on macOS).
- Bollard defaults to this socket; override via `LLM_OJ_DOCKER_HOST` if needed.

**Pros**: Simple, fast, uses the local image cache (no re-pulling on every run).

**Cons & Mitigation**:
- Tests can leave containers/volumes behind if they panic.
- **Mitigation**: Label all test containers with `llm-oj-test=true` and add a teardown helper `docker_prune_test_containers()` in `integration-tests/tests/helpers.rs` that runs after every `#[tokio::test]`.
- **Mitigation**: Use `TempDir` for any host-mounted volumes so the OS cleans them up on reboot.

**CI (GitHub Actions)**:
- GitHub Actions runners already have Docker installed and the socket available.
- No special service containers or privileged modes needed.
- Simply run `cargo test -p integration-tests`.

**Why not DinD or testcontainers?**
- Docker-in-Docker requires `--privileged`, is slower, and loses the image cache.
- `testcontainers` adds a dependency but still needs a Docker daemon; our own teardown helper achieves the same cleanup with less complexity.

### 6.5 Cross-Cutting Contract Tests

**Recommended**: Maintain an **OpenAPI 3.1 spec** as the single source of truth.
- Rust: derive spec from Axum handlers using `utoipa`.
- TypeScript: generate Zod schemas or fetch types from OpenAPI.
- This prevents frontend/backend drift.

---

## 7. Build & Dev Experience

### 7.1 Rust Backend (`backend/`)

```bash
cd backend

# Tools
cargo install sqlx-cli cargo-nextest

# DB
docker compose up -d postgres
cargo sqlx migrate run

# Dev API
cargo run -p api-server        # or: cargo run --bin api

# Dev Worker
cargo run -p worker

# Tests (all workspace crates)
cargo test --workspace
cargo nextest run --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# Offline SQLX (run when queries change)
cd shared && cargo sqlx prepare
```

### 7.2 Next.js Frontend (`frontends/web/`)

```bash
cd frontends/web
npm install
npm run dev        # localhost:3001 (API on :3000)
npm run lint       # biome check
npm run format     # biome format --write
npm run build
```

### 7.3 Monorepo Orchestration (`justfile`)

Add a `justfile` at repo root:

```justfile
set fallback := true

# Start infrastructure (Postgres)
infra-up:
    docker compose up -d postgres

# Stop infrastructure
infra-down:
    docker compose down

# Run database migrations
migrate:
    cd backend && cargo sqlx migrate run

# Dev: API server
 dev-api:
    cd backend && cargo run -p api-server

# Dev: evaluation worker
dev-worker:
    cd backend && cargo run -p worker

# Dev: Next.js frontend (separate service)
dev-web:
    cd frontends/web && npm run dev

# Dev: all three in parallel (requires tmux or multiple terminals)
dev-all:
    @echo "Run these in separate terminals:"
    @echo "  just dev-api"
    @echo "  just dev-worker"
    @echo "  just dev-web"

# Rust unit + integration tests
test-rust:
    cd backend && cargo test --workspace

# Rust integration tests only
test-rust-integration:
    cd backend && cargo test -p integration-tests

# Frontend unit tests
test-web-unit:
    cd frontends/web && npx vitest run

# Frontend E2E tests
test-web-e2e:
    cd frontends/web && npx playwright test

# All tests
test-all: test-rust test-web-unit test-web-e2e

# Lint Rust
cargo fmt:
    cd backend && cargo fmt --all

cargo clippy:
    cd backend && cargo clippy --workspace --all-targets -- -D warnings

# Lint frontend
web-lint:
    cd frontends/web && npx biome check

web-format:
    cd frontends/web && npx biome format --write

# Prepare sqlx offline queries
sqlx-prepare:
    cd backend/shared && cargo sqlx prepare

# Clean build artifacts
clean:
    cd backend && cargo clean
    cd frontends/web && rm -rf .next
```

---

## 8. Migration Phases

### Phase 1: Backend Skeleton + Bollard Prototype (Week 1)
- [ ] Set up workspace: `backend/Cargo.toml` with members `shared`, `api-server`, `worker`, `integration-tests`.
- [ ] Move existing `config.rs`/`error.rs` into `shared/`; create skeleton `main.rs` for api-server and worker.
- [ ] Extend `config.rs` with runner/docker fields.
- [ ] Port `auth`, `models` (contracts), `leaderboard` pure logic into `shared/`.
- [ ] Port DB queries to `shared/src/db/` module (sqlx).
- [ ] Implement bollard prototype in `shared/src/runner.rs`: create → start → wait → logs → remove a container.
- [ ] Test: bollard can successfully run `python:3.12-slim` and produce output.

### Phase 2: Core Agent Flow + Docker Runner (Week 2)
- [ ] Implement `register_agent`, Bearer auth extractor.
- [ ] Implement `create_submission` (base64 → zip → storage → DB job).
- [ ] Implement worker cycle: claim job → bollard runner → write result.
- [ ] Integration test: full register → submit → eval → completed cycle.
- [ ] Test: Docker-only, no host Python involved.

### Phase 3: Public Read + Leaderboard (Week 3)
- [ ] All public GET routes.
- [ ] Leaderboard query + upsert logic.
- [ ] Discussion create/list.
- [ ] Port integration tests: public eval, leaderboard, discussions.

### Phase 4: Admin + Official Run (Week 4)
- [ ] Basic auth extractor.
- [ ] Admin routes: problem/version creation, rejudge, official run, hide, disable.
- [ ] Worker handles `official` eval type via bollard (same container flow, different `--mode`).
- [ ] Port `admin-official` integration test.

### Phase 5: Next.js Frontend (Week 5)
- [ ] Add deps: react-markdown, shiki, swr, zod, vitest, playwright.
- [ ] Port layout, theme, topbar.
- [ ] Port pages: catalog, problem detail, tabs, submissions, leaderboard, discussions.
- [ ] Port submission detail with code browser (Client Component).
- [ ] Playwright tests for all routes.

### Phase 6: Hardening (Week 6)
- [ ] Worker pre-pulls runner image on startup.
- [ ] Graceful shutdown handling for API and worker.
- [ ] Stuck-job reaper (SQL + scheduled task).
- [ ] Resource limits via bollard (`Memory`, `NanoCpus`).
- [ ] Container cleanup / leak detection by prefix.
- [ ] Rate limiting (Axum `tower::limit`).
- [ ] Observability: metrics endpoint, tracing spans.
- [ ] Load test: 100 concurrent submissions through Docker.

---

## 9. Open Design Questions & Recommendations

### Q1: Docker Image for Scorer
**Context**: The scorer is a Python script. It may need dependencies (e.g., `numpy`).

| Option | Image | Pros | Cons |
|--------|-------|------|------|
| A | `python:3.12-alpine` | Small (~50MB), fast pull | **musl libc** — binary wheels with C extensions often fail |
| B | `python:3.12-slim` (Debian bookworm) | **glibc**, good compatibility, pip available | Medium (~60MB), slightly slower pull than alpine |
| C | `ubuntu:24.04` + Python | Full glibc compatibility, apt for system deps | Large (~80MB+), slowest startup |
| D | Custom image with `uv` | Can run `uv run scorer/run.py` with deps | Requires building/maintaining custom image |

**Recommendation**: **Option B for v0** — `python:3.12-slim-bookworm`. It uses glibc (no musl issues), is only ~10MB larger than alpine, has `pip` if needed, and is the official Python team's recommended default. Ubuntu (Option C) is overkill for a scorer container. `runner_python_image` remains configurable for future custom images.

### Q2: Bollard Connection Strategy
**Context**: Bollard needs to talk to Docker daemon.

- **Linux**: Unix socket `/var/run/docker.sock` (bollard default).
- **macOS**: Docker Desktop socket at `~/.docker/run/docker.sock` or via `DOCKER_HOST`.
- **CI**: `DOCKER_HOST` env var or mounted socket.

**Recommendation**: Use `Docker::connect_with_defaults()` and allow `LLM_OJ_DOCKER_HOST` override. Document that macOS users may need to set `DOCKER_HOST`.

### Q3: Single Binary vs. Separate Binaries
**Context**: API and worker share most code.

- **Option A**: Single `server` binary with `--mode=api` / `--mode=worker` / `--mode=all`.
- **Option B**: Separate `api` and `worker` binaries.

**Recommendation**: **Option B** (separate binaries) for v0. Simpler process management, easier to scale workers independently. Both import the same `lib.rs` modules.

### Q4: How to Handle `uv` in Docker
**Context**: Current TypeScript worker uses `uv run python scorer/run.py` in local mode. With Docker-only, the container itself must provide the Python runtime.

**Recommendation**: The scorer contract stays the same: the container runs `python /problem/scorer/run.py ...`. If a problem needs `uv`, the custom Docker image (Option B from Q1) must have `uv` installed, and the `entrypoint` in `spec.json` would be adjusted. For v0, plain `python` is sufficient.

### Q5: ZIP Extraction Location
**Context**: Submission artifacts are ZIP files. The runner must extract them before mounting into Docker.

**Concern**: Extracting untrusted ZIP files directly onto the host filesystem risks path traversal and other exploits, even if the Rust `zip` crate is safe.

**Recommendation**: **Extract inside Docker** for defense in depth. Two approaches:

**Approach A: Extraction Container (Recommended for v0)**
1. Create a temporary Docker volume (`llm-oj-sub-{job_id}`).
2. Run a one-off extraction container:
   - Image: same runner image (`python:3.12-slim`)
   - Mount the ZIP file (read-only) + the temp volume (read-write)
   - Command: `python -m zipfile -e /input/submission.zip /submission`
3. Run the scorer container mounting the same volume as `/submission:ro`.
4. After evaluation, remove the volume.

**Approach B: Rust `zip` Crate on Host (Faster, less isolation)**
1. Validate every ZIP entry path (reject `..`, absolute paths, symlinks).
2. Extract to a dedicated host temp dir with strict permissions.
3. Mount the temp dir into the scorer container.
4. Clean up after container removal.

**v0 Recommendation**: Use **Approach B** for simplicity and speed, but with strict path validation (the Rust `zip` crate + manual path sanitization). Approach A can be added later as a hardening step if needed. The `zip` crate does not execute any code during extraction — it only writes files.

### Q6: Container Resource Limits
**Context**: Original has `time_limit_sec` and `memory_limit_mb` in `spec.json`.

**Recommendation**:
- **Timeout**: `tokio::time::timeout(Duration::from_secs(config.runner_timeout_sec), container_wait).await`.
- **Memory**: Map `memory_limit_mb` from spec to bollard `HostConfig.Memory` (bytes). If spec missing, use config default.
- **CPU**: Optional `cpu_limit` in config → `HostConfig.NanoCpus`.
- **Network**: Always `NetworkMode: "none"`.

### Q7: Runner Log Capture
**Context**: Current worker writes stdout+stderr to `runner.log`.

**Where are logs stored?**
Logs are **files on disk**, not blobs in the database. The database stores the **path** to the log file.

Flow:
1. Bollard `logs()` stream captures stdout + stderr from the scorer container.
2. The worker combines them and writes to disk via the `Storage` trait:
   ```
   {storage_root}/eval-artifacts/{job_id}/runner.log
   ```
3. The `evaluations` table has a `log_path` column that points to this file:
   ```sql
   log_path TEXT
   ```
4. When fetching a submission, the API returns `evaluation.log_path` in the JSON (or null if no log).
5. Admin can read logs directly from storage; they are not streamed through the API.

**Submission database?** Submissions, evaluations, and jobs are all in PostgreSQL. But the actual **artifact files** (ZIPs) and **log files** are in the filesystem (local storage). The database stores metadata and file paths. Future work can swap `LocalStorage` for S3/MinIO without schema changes.

---

## 9b. Additional Design Decisions (from review)

### D1: Pre-pull Runner Image on Worker Startup
The first evaluation job will be slow if the Docker image isn't cached. On startup, the worker should call `bollard::image::create_image_options()` to ensure `runner_python_image` is present locally. Fail fast with a clear error if the pull fails (e.g., no internet, bad image name).

### D2: Graceful Shutdown
Both binaries handle `SIGINT`/`SIGTERM`:
- **API**: Stop accepting new connections, finish in-flight requests, then exit. Axum's `with_graceful_shutdown` makes this straightforward.
- **Worker**: If mid-evaluation, kill the current bollard container and reset the job status back to `queued` in the database so another worker can pick it up. Then exit.

### D3: Database Connection Pool Sizing
Use different pool defaults per binary:
- **API**: `max_connections = 10` (handles concurrent HTTP requests).
- **Worker**: `max_connections = 2` (only needs 1 for claiming + 1 for writing results).
- Both share the same `shared::db::create_pool(config, overrides)` function.

### D4: Docker Container Naming
Name containers `llm-oj-{job_id}` via `CreateContainerOptions { name: Some(...) }`. This makes them easy to spot in `docker ps` and enables leak detection by prefix.

### D5: Stuck-Job Reaper
Jobs can get stuck in `running` if a worker crashes. Add a periodic reaper (either in the worker or a scheduled SQL job) that resets jobs stuck in `running` for longer than `runner_timeout_sec * 2` back to `queued`.

```sql
UPDATE evaluation_jobs
SET status = 'queued', worker_id = NULL, claimed_at = NULL
WHERE status = 'running'
  AND claimed_at < NOW() - INTERVAL '1 minute' * $1;
```

### D6: Next.js as a Separate Service
The Next.js frontend runs as its own process (`npm run dev` / `next start`), **not** served by the Rust API. The API only serves JSON. In development, the Next.js dev server proxies `/api/*` to `localhost:3000` via `next.config.ts` rewrites. In production, both services run behind a reverse proxy (e.g., nginx, Caddy, or Vercel).

This separation means:
- The `api-server` binary does **not** need `tower-http::fs` for static files.
- Frontend deployments can be independent of backend deployments.
- The API stays focused on JSON and auth.

### D7: SQLx Offline Query Cache
Since `shared` contains all SQL queries, run `cargo sqlx prepare` from the `backend/shared/` directory. Commit `backend/shared/.sqlx/query-*.json` to version control so CI and contributors can compile without a running database.

---

## 10. Risk & Mitigation

| Risk | Mitigation |
|------|------------|
| **Agent API breakage** | Lock JSON contracts first; use existing TS integration tests as oracle. |
| **Bollard / Docker daemon unavailable in CI** | GitHub Actions runners have Docker by default; use host socket (§6.4). Skip with `NO_DOCKER=1` if needed. |
| **sqlx compile fragility** | Commit `.sqlx/` JSON; `SQLX_OFFLINE=true` for CI builds. |
| **Container leaks on worker crash** | Label containers `llm-oj.job_id=<id>`; periodic cleanup sweep. |
| **Frontend hydration mismatches** | Only wrap interactive parts in Client Components; Server Components do all data fetching. |
| **Biome vs ESLint differences** | Follow `frontends/web/biome.json` rules; do not add ESLint. |

---

## 11. Acceptance Criteria

1. **Backend**
   - All integration tests pass against Rust API.
   - `cargo clippy --workspace -- -D warnings` is clean.
   - Worker evaluates submissions **exclusively through Docker** (bollard); no host Python invoked.
   - Agent can register, submit, poll, read results with identical HTTP contracts.

2. **Frontend**
   - All Playwright tests pass.
   - Biome lint/format clean.
   - Zero hydration errors in production build.

3. **Worker**
   - Public + official eval scores match TypeScript worker on `sample-sum` and `grid-routing`.
   - Containers are created, run, and removed cleanly per job.

4. **End-to-End**
   - `just test-all` passes locally.
   - CI passes (Postgres service, Docker daemon, Rust cache, Next.js build).

---

## 12. Appendix: File Mapping (Old → New)

| Old File (`llm-oj/`) | New Location | Notes |
|----------------------|--------------|-------|
| `apps/api/src/app.ts` | `backend/api-server/src/router.rs` | Axum router |
| `apps/api/src/routes/api-routes.ts` | `backend/api-server/src/handlers/*.rs` | Domain handlers |
| `apps/api/src/services.ts` | `backend/api-server/src/service.rs` | Orchestration |
| `apps/api/src/presenters.ts` | `backend/api-server/src/presenter.rs` | JSON mapping |
| `apps/api/src/http.ts` | `backend/api-server/src/extractors.rs` | Auth extractors |
| `apps/api/src/submission-artifact.ts` | `backend/shared/src/storage/zip_summary.rs` | ZIP metadata |
| `apps/worker/src/worker.ts` | `backend/worker/src/cycle.rs` | Job polling |
| `apps/worker/src/runner.ts` | `backend/shared/src/runner.rs` | **Bollard** container lifecycle (shared lib) |
| `packages/db/src/platform.ts` | `backend/shared/src/db/queries.rs` | sqlx functions |
| `packages/db/src/client.ts` | `backend/shared/src/db/pool.rs` | PgPool |
| `packages/shared/src/auth.ts` | `backend/shared/src/auth/mod.rs` | Token logic |
| `packages/shared/src/config.ts` | `backend/shared/src/config.rs` | **MOVED** from old `backend/src/` |
| `packages/shared/src/leaderboard.ts` | `backend/shared/src/leaderboard.rs` | Pure logic |
| `packages/shared/src/problem-bundle.ts` | `backend/shared/src/models/validation.rs` | Bundle validation |
| `packages/contracts/src/*.ts` | `backend/shared/src/models/*.rs` | Serde types |
| `apps/web/src/App.tsx` | `frontends/web/src/app/layout.tsx` | App Router root |
| `apps/web/src/api.ts` | `frontends/web/src/lib/api.ts` | Fetch wrapper |
| `apps/web/src/styles.css` | `frontends/web/src/app/globals.css` | **ALREADY EXISTS** |
| `tests/*.integration.test.ts` | `backend/integration-tests/tests/*.rs` | Rust integration tests |
