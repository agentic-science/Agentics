# Agentics Milestones

This milestone document and the PRD must be bidirectionally synced at the feature level. When the PRD adds, removes, renames, or changes the scope of a feature, this document must be updated in the same change set. When this document adds, removes, reprioritizes, or materially changes a milestone, the English and Chinese PRDs must be checked and updated if the feature scope changes.

Each milestone below is intended to map to one focused commit. A commit may include implementation, tests, and documentation for that milestone, but should not mix unrelated feature lanes.

## Planning Conventions

- **Version:** release target from the PRD roadmap.
- **Lane:** major product or engineering surface.
- **Milestone:** commit-sized unit of work.
- **Commit target:** suggested commit message and scope.
- **Test spec:** tests or checks that should be added or run before the commit.
- **Implementation progress:** every major version section ends with a `### Implementation Progress` subsection containing a three-column table: milestone, status, and additional note.

Progress status values:

- `Implemented`: the milestone has a merged or working artifact that satisfies its scope.
- `In Progress`: implementation has started but the milestone is not complete.
- `Planned`: the milestone is part of the version plan but implementation has not started.
- `Blocked`: the milestone cannot proceed until an explicit dependency or decision is resolved.
- `Deferred`: the milestone was intentionally moved out of the version.

Standard pre-commit checks for code milestones:

- Rust: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and targeted `cargo test` or integration tests.
- Web: `bun run lint`, `bun run test`, and `bun run build` from `frontends/web` when UI or frontend data contracts change.
- CLI: `cargo test -p agentics-cli` plus command-level snapshot or golden-output tests when command output changes.
- Docs-only: structural review, link review for local links, and terminology sync with `docs/PRD/en.md` and `docs/PRD/zh.md`.

## v0.0 - Current Baseline Documentation

v0.0 is the already implemented baseline. Its historical version snapshot has been retired during MVP documentation cleanup. Current operational and contributor references start from `docs/README.md`.

### Product Documentation

- **M0.0-DOC-1: Document v0.0 product baseline**
  - Status: Implemented.
  - Commit target: `docs: document v0.0 platform baseline`
  - Scope: Add a v0.0 release baseline document that lists implemented backend, worker, web, admin API, artifact browsing, and challenge bundle capabilities.
  - Artifact: Historical version snapshot retired; current docs index is `docs/README.md`.
  - Test spec: Compare the baseline doc against current routes, README startup steps, and PRD current MVP scope.

- **M0.0-DOC-2: Add API usage examples**
  - Status: Implemented.
  - Commit target: `docs: add v0.0 API usage examples`
  - Scope: Document agent registration, challenge listing, solution submission creation, polling, public solution submission views, leaderboard reads, and admin rejudge or official-run APIs.
  - Artifact: Historical version snapshot retired; current docs index is `docs/README.md`.
  - Test spec: Run the documented curl examples against a local stack with seeded sample challenges.

- **M0.0-DOC-3: Add challenge bundle authoring reference**
  - Status: Implemented.
  - Commit target: `docs: add challenge bundle authoring guide`
  - Scope: Document bundle directory layout, `spec.json`, public data, private benchmark data, scorer contracts, result JSON, Docker image assumptions, validation rules, and common failure modes.
  - Artifact: Historical version snapshot retired; current challenge guidance starts at `docs/contribute-challenges/en.md`.
  - Test spec: Validate every documented field against the Rust bundle parser and the seeded example bundles.

- **M0.0-DOC-4: Add v0.0 release checklist**
  - Status: Implemented.
  - Commit target: `docs: add v0.0 release checklist`
  - Scope: Document local release verification for API startup, worker startup, sample solution submission execution, public visibility, leaderboard update, and admin actions.
  - Artifact: Historical version snapshot retired; current operations guidance starts at `docs/operations/en.md`.
  - Test spec: Complete the checklist on a clean Postgres volume and record any required environment variables.

### Backend and Worker

- **M0.0-BE-1: Capture current API contract**
  - Status: Implemented.
  - Commit target: `docs: capture v0.0 API contract`
  - Scope: Add a concise endpoint inventory for public, agent-authenticated, and admin routes. This is documentation only unless missing endpoint descriptions reveal a bug.
  - Artifact: Historical version snapshot retired; current docs index is `docs/README.md`.
  - Test spec: Cross-check endpoint inventory against the Axum router definitions and existing integration tests.

- **M0.0-WORKER-1: Capture runner behavior**
  - Status: Implemented.
  - Commit target: `docs: capture v0.0 runner behavior`
  - Scope: Document Docker execution, scorer image default, artifact mounting, timeout and resource limits, logs, job claiming, heartbeat behavior, and stale-job handling.
  - Artifact: Historical version snapshot retired; current operations guidance starts at `docs/operations/en.md`.
  - Test spec: Run a successful sample solution submission and one intentionally failing sample solution submission, then compare observed logs and persisted status with the document.

### Web

- **M0.0-WEB-1: Document current observer web surface**
  - Status: Implemented.
  - Commit target: `docs: document v0.0 observer web`
  - Scope: Document the current public pages for challenge list, challenge details, solution submissions, solution submission detail, artifact browser, and leaderboard.
  - Artifact: Historical version snapshot retired; observer usage is summarized in `README.md`.
  - Test spec: Start the frontend and inspect the listed pages against seeded sample data.

### Operations and Quality

- **M0.0-OPS-1: Add local smoke-test script or checklist**
  - Status: Implemented.
  - Commit target: `docs: add local smoke test checklist`
  - Scope: Provide a repeatable local smoke path for Postgres, migrations, API, worker, web, agent registration, ZIP solution submission, and worker completion.
  - Artifact: Historical version snapshot retired; current operations guidance starts at `docs/operations/en.md`.
  - Test spec: Execute the checklist from a clean checkout using the README prerequisites.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.0-DOC-1: Document v0.0 product baseline` | Implemented | Historical snapshot retired during MVP docs cleanup; use `docs/README.md` for current references. |
| `M0.0-DOC-2: Add API usage examples` | Implemented | Historical snapshot retired during MVP docs cleanup; use `docs/README.md` for current references. |
| `M0.0-DOC-3: Add challenge bundle authoring reference` | Implemented | Historical snapshot retired; current creator guidance starts at `docs/contribute-challenges/en.md`. |
| `M0.0-DOC-4: Add v0.0 release checklist` | Implemented | Historical snapshot retired; current operations guidance starts at `docs/operations/en.md`. |
| `M0.0-BE-1: Capture current API contract` | Implemented | Historical endpoint snapshot retired during MVP docs cleanup. |
| `M0.0-WORKER-1: Capture runner behavior` | Implemented | Historical runner snapshot retired; current operations guidance starts at `docs/operations/en.md`. |
| `M0.0-WEB-1: Document current observer web surface` | Implemented | Historical observer snapshot retired; current observer usage is summarized in `README.md`. |
| `M0.0-OPS-1: Add local smoke-test script or checklist` | Implemented | Historical checklist retired; current operations guidance starts at `docs/operations/en.md`. |

## v0.1 - Agent Workflow, Validation, Admin Web, Metrics, and Collaboration Guidance

v0.1 turns the current API-first platform into a practical agent workflow. The main outcomes are a usable Agentics CLI, agent-facing CLI skill guidance, validation runs, richer metric display, an admin web console, stronger challenge authoring docs, and manual Moltbook collaboration guidance without challenge metadata links.

### Agentics CLI

- **M0.1-CLI-1: CLI configuration and authentication foundation**
  - Commit target: `cli: add config and authentication commands`
  - Scope: Implement config file loading, API base URL configuration, token storage, `agentics register`, and `agentics auth status`.
  - Test spec: Add CLI unit tests for config precedence, token persistence, registration request payloads, and error formatting with mocked HTTP responses.

- **M0.1-CLI-2: Challenge discovery commands**
  - Commit target: `cli: add challenge list and detail commands`
  - Scope: Implement `agentics challenges list` and `agentics challenges show <challenge-name>` using public APIs.
  - Test spec: Add golden-output tests for table and JSON output, plus mocked pagination or empty-state tests if pagination exists.

- **M0.1-CLI-3: Solution workspace initialization**
  - Commit target: `cli: add solution workspace initialization`
  - Scope: Implement `agentics init-solution <challenge-name>` with a minimal README-only workspace, Git repository initialization, and a pre-commit hook that requires `run.sh` at the workspace root. Do not generate metadata files, starter code, or `run.sh` in v0.1.
  - Test spec: Add filesystem tests using temporary directories, verify existing workspace directories are rejected, verify only `README.md` and `.git/` are created, and verify the hook checks for `run.sh`.

- **M0.1-CLI-4: Solution Submission packaging and official submit**
  - Commit target: `cli: add zip solution submission workflow`
- Scope: Implement ZIP packaging that respects `.gitignore`, archive validation, `agentics submit <challenge-name> --target <target>`, `agentics submissions show|status|wait|logs|rank`, and result display.
  - Test spec: Add tests for `.gitignore` behavior, missing or ignored `run.sh`, generated ZIP layout, mocked solution submission creation, authenticated submission reads, and output rendering.

- **M0.1-CLI-5: Remote validation commands**
  - Commit target: `cli: add remote validation workflow`
  - Scope: Implement `agentics validate --remote <challenge-name> --target <target>`, validation status polling, and validation result display without leaderboard updates.
  - Test spec: Add mocked API tests proving validation mode is requested, disabled validation is rejected before packaging/upload, and official solution submission state is not mutated.

### Backend API

- **M0.1-BE-1: Add first-class validation run API**
  - Commit target: `api: add validation run endpoints`
  - Scope: Add authenticated endpoints for creating validation runs, polling validation status, reading validation results, and rejecting validation requests when the selected target disables validation.
  - Test spec: Add integration tests proving validation uses public data, does not update leaderboard state, rejects disabled validation before queueing work, and returns logs and metrics to the submitting agent.

- **M0.1-BE-2: Normalize validation and official terminology**
  - Commit target: `api: normalize evaluation mode terminology`
  - Scope: Align API models, docs, and persisted mode values around `validation` and `official`, while preserving compatibility with existing data where needed.
  - Test spec: Add serialization compatibility tests and integration tests for both modes.

- **M0.1-BE-3: Add metric schema and ranking metadata**
  - Commit target: `api: add metric schema and ranking metadata`
  - Scope: Persist challenge metric definitions, display units, directionality, tie-breakers, public/official visibility, and primary ranking configuration.
  - Test spec: Add bundle parser tests, database persistence tests, and response-schema tests for challenge detail and solution submission result payloads.

- **M0.1-BE-4: Defer Moltbook community metadata**
  - Commit target: `api: remove challenge community link metadata`
  - Scope: Keep Moltbook links out of challenge metadata and public challenge detail responses for the MVP. Canonical Moltbook posts are manual external records until automation exists.
  - Test spec: Add bundle and contract tests proving legacy community fields are rejected or absent from public response DTOs.

### Worker and Evaluation

- **M0.1-WORKER-1: Separate validation and official job execution**
  - Commit target: `worker: separate validation and official execution`
  - Scope: Ensure worker jobs carry evaluation mode explicitly and select the correct dataset visibility and result persistence behavior.
  - Test spec: Add integration tests for public-data validation, official private-benchmark execution, and leaderboard mutation only on official success.

- **M0.1-WORKER-2: Persist aggregate and per-run metrics**
  - Commit target: `worker: persist structured evaluation metrics`
  - Scope: Store normalized aggregate metrics, optional per-run metrics, rank score, ranking metadata, and scorer diagnostics.
  - Test spec: Add scorer-output fixture tests for valid metrics, missing rank score, non-finite values, unknown metrics, and per-run payloads.

- **M0.1-WORKER-3: Add validation quotas**
  - Commit target: `worker: add validation quota enforcement`
  - Scope: Add simple per-agent or per-challenge validation limits to protect worker capacity.
  - Test spec: Add database and API tests for quota consumption, quota rejection, and quota reset behavior.

### Web

- **M0.1-WEB-1: Display validation and official modes clearly**
  - Commit target: `web: label validation and official results`
  - Scope: Update challenge, solution submission, and result views to show validation availability and distinguish validation feedback from official ranked results.
  - Test spec: Add component or route tests for validation availability, mode labels, official-only leaderboard inclusion, and empty states.

- **M0.1-WEB-2: Add richer metric display**
  - Commit target: `web: add structured metric display`
  - Scope: Render primary ranking score, secondary aggregate metrics, per-run metrics, units, and directionality on solution submission and leaderboard pages.
  - Test spec: Add schema tests and rendering tests for maximize/minimize metrics, official-only metrics, missing optional values, and long metric names.

- **M0.1-WEB-3: Defer Moltbook challenge links**
  - Commit target: `web: remove Moltbook challenge community links`
  - Scope: Keep Observer Web focused on challenges, metrics, targets, rankings, solution submissions, and artifacts. Per-challenge Moltbook links remain future automation work.
  - Test spec: Regenerate frontend schemas and remove rendering tests for configured Moltbook links.

### Admin

- **M0.1-ADMIN-1: Admin web shell and authentication**
  - Commit target: `admin: add admin web shell`
  - Scope: Add admin routes, basic auth or session integration, layout, navigation, and access-denied handling.
  - Test spec: Add frontend tests for authenticated and unauthenticated states, plus backend tests for admin-only API access if new routes are introduced.

- **M0.1-ADMIN-2: Challenge publishing and configuration view**
  - Commit target: `admin: add challenge publishing console`
  - Scope: Provide admin UI for challenge listing, version details, bundle validation result display, and publish actions.
  - Test spec: Add mocked API UI tests and backend integration tests for publish and validation failure paths.

- **M0.1-ADMIN-3: Solution Submission and worker operations view**
  - Commit target: `admin: add solution submission operations console`
  - Scope: Provide admin UI for queued/running/completed jobs, worker heartbeats, rejudge, official-run triggering, hide solution submission, and disable agent actions.
  - Test spec: Add UI tests for action confirmation states and API integration tests for each state-changing action.

### Challenge Authoring and Documentation

- **M0.1-DOC-1: Document validation and official authoring model**
  - Commit target: `docs: document validation and official challenge authoring`
  - Scope: Update authoring docs to explain public data, private benchmark data, validation mode, and official mode.
  - Test spec: Verify examples by publishing a sample challenge and running both modes locally.

- **M0.1-DOC-2: Document metric schema and ranking rules**
  - Commit target: `docs: document metric schema and ranking rules`
  - Scope: Provide schema examples for aggregate metrics, per-run metrics, primary ranking metric, ranking script option, units, directionality, and tie-breakers.
  - Test spec: Validate documented examples with parser tests or fixture-based integration tests.

### Agent Enablement

- **M0.1-SKILL-1: Agentics CLI usage skill**
  - Commit target: `skill: add agentics cli usage skill`
  - Scope: Add an agent-facing skill that teaches agents how to configure the Agentics CLI, register or reuse credentials, discover challenges, initialize solution workspaces, create the required `run.sh`, and use validation or solution submission commands as they become available.
  - Test spec: Review the skill against current CLI help output and README examples, and add or update command examples whenever CLI commands change.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.1-CLI-1: CLI configuration and authentication foundation` | Implemented | Adds config file loading, API URL and token overrides, `register`, `auth status`, and mocked HTTP tests. |
| `M0.1-CLI-2: Challenge discovery commands` | Implemented | Adds `challenges list`, `challenges show`, table output, JSON output, and rendering tests. |
| `M0.1-CLI-3: Solution workspace initialization` | Implemented | Creates README-only Git workspaces with a pre-commit hook requiring root `run.sh`. |
| `M0.1-CLI-4: Solution Submission packaging and official submit` | Implemented | Adds `.gitignore`-aware ZIP packaging, root `run.sh` validation, authenticated `submit`, and `status`. |
| `M0.1-CLI-5: Remote validation commands` | Implemented | Adds `validate --remote`, default polling, disabled-validation preflight, private result display, and mocked endpoint tests. |
| `M0.1-BE-1: Add first-class validation run API` | Implemented | Adds authenticated `/api/agent/validation-runs` create/read endpoints and challenge-level validation disablement checks. |
| `M0.1-BE-2: Normalize validation and official terminology` | Implemented | Canonical modes are now `validation` and `official`. |
| `M0.1-BE-3: Add metric schema and ranking metadata` | Implemented | Adds bundle metric schemas, ranking metadata, parser validation, and public API response fields. |
| `M0.1-BE-4: Defer Moltbook community metadata` | Deferred | Removed optional Moltbook metadata from bundles and public challenge detail responses. Canonical posts stay manual and external until automation exists. |
| `M0.1-WORKER-1: Separate validation and official job execution` | Implemented | Validation runs stay private; official runs update visibility and leaderboard state. |
| `M0.1-WORKER-2: Persist aggregate and per-run metrics` | Implemented | Persists rank score, aggregate metrics, per-run metrics, and leaderboard metric snapshots. |
| `M0.1-WORKER-3: Add validation quotas` | Implemented | Enforces rolling per-agent, per-challenge, per-target validation quotas before artifact upload. |
| `M0.1-WEB-1: Display validation and official modes clearly` | Implemented | Challenge and result views distinguish validation availability from official ranked results. |
| `M0.1-WEB-2: Add richer metric display` | Implemented | Renders metric definitions, primary ranking metrics, secondary metrics, and per-run metrics in observer views. |
| `M0.1-WEB-3: Defer Moltbook challenge links` | Deferred | Observer Web no longer renders per-challenge Moltbook links in the MVP. |
| `M0.1-ADMIN-1: Admin web shell and authentication` | Implemented | Adds a VIS-aligned `/admin` route group, cookie-backed admin sessions for the web console, Basic Auth for server-side tools, and an admin API client. |
| `M0.1-ADMIN-2: Challenge publishing and configuration view` | Implemented | Adds challenge registry, challenge shell creation, and bundle version publishing from the admin web console. |
| `M0.1-ADMIN-3: Solution Submission and worker operations view` | Implemented | Adds solution submission actions, recent evaluation state, and worker heartbeat inspection. |
| `M0.1-DOC-1: Document validation and official authoring model` | Implemented | Adds bilingual v0.1 challenge-authoring docs for public data, private benchmark data, validation, and official runs. |
| `M0.1-DOC-2: Document metric schema and ranking rules` | Implemented | Documents aggregate metrics, per-run metrics, ranking metadata, visibility, directionality, and tie-breakers. |
| `M0.1-SKILL-1: Agentics CLI usage skill` | Implemented | Adds `skills/agentics-cli-workflow/SKILL.md` and links it from repo docs. |

## v0.2 - Multi-Language ZIP Projects, Targets, GPU, and Capacity Controls

v0.2 expands Agentics beyond the initial archive protocol into manifest-based multi-language solution submissions and target-aware execution. For the hosted MVP, target-aware execution is DGX-first: `linux-arm64-cpu` and `linux-arm64-cuda` run on `linux/arm64`, local platform development may use `macos-arm64-cpu` foreground rehearsal, and `linux-amd64-cpu` plus `linux-amd64-cuda` are post-MVP expansion targets.

### Solution Submission Protocol

- **M0.2-PROTO-1: Define `zip_project` manifest schema**
  - Commit target: `protocol: add zip_project manifest schema`
  - Scope: Define protocol metadata, optional public note, required run script, optional setup/build scripts, and protocol versioning. Runtime, interface, dependency, and execution-limit policy are not participant-controlled manifest fields.
  - Test spec: Add parser tests for valid manifests, missing required fields, unsupported protocol versions, invalid paths, unsafe script references, old-field rejection, note length, and note control-character validation.

- **M0.2-PROTO-2: Add setup/build/run phase model**
  - Commit target: `protocol: add setup build run phase model`
  - Scope: Model setup, build, and run phases from manifest-declared scripts while deriving timeout, memory, CPU, disk, and network policy from challenge-owned resource profiles. Container log capture is platform-owned runner configuration, not solution manifest data.
  - Test spec: Add unit tests for script-to-phase resolution, profile-owned limit selection, platform log caps, and phase-specific failure reporting.

- **M0.2-PROTO-4: Add scorer-owned prepare phase**
  - Commit target: `worker: add challenge prepare phase`
  - Scope: Let challenge bundles declare `validation_prepare` or `official_prepare` commands that run in the scorer image before solution invocations, write generated inputs and a generated run manifest under `/prepared`, and keep private prepared data out of the public challenge repository. Record prepare network policy and reproducibility metadata without enforcing a universal data reproducibility scheme.
  - Test spec: Add bundle parser tests for static versus prepared run modes, runner integration tests for prepare-generated `source_path` inputs, scorer access to `/prepared`, official publish with private seed assets, and successful solution scoring through a prepared run manifest.

### Targets

- **M0.2-TARGET-1: Define target schema**
  - Commit target: `protocol: add target schema`
  - Scope: Replace the single challenge resource profile assumption with one or more targets. For MVP, support `linux-arm64-cpu` and `linux-arm64-cuda` on Docker platform `linux/arm64`; reserve `linux-amd64-cpu` and `linux-amd64-cuda` for post-MVP deployment expansion. Each target owns image references or digests, resource limits, validation availability, quota scope, and ranking scope.
  - Test spec: Add schema and bundle validation tests for ARM64 CPU target, ARM64 CUDA target, AMD64 target rejection, duplicate targets, unsupported Docker platforms, missing target references, target-specific validation disabled, CUDA hardware metadata, and invalid image or resource metadata.

- **M0.2-TARGET-2: Add target-specific evaluation and leaderboards**
  - Commit target: `api: add target evaluations`
  - Scope: Persist the selected target on validation runs, official evaluations, solution submissions, and leaderboard rows. The worker should use the selected target's Docker platform and resource profile. Official submissions should be able to target one supported target or all supported targets. Each target should produce independent official results and leaderboard entries.
  - Test spec: Add integration tests proving unsupported targets are rejected before artifact upload, target-specific validation disablement is enforced, Docker receives the selected platform and accelerator policy, multiple supported targets produce separate official results, leaderboard rows are scoped by target, and hidden or rejudged submissions repair only the affected target leaderboard.

### Base Images

- **M0.2-IMAGE-1: Define first-party CPU base image**
  - Commit target: `docker: add agentics cpu base image`
  - Scope: Add a source-defined Agentics CPU base image for solution and scorer containers. For MVP, publish and smoke `linux/arm64`; reserve `linux/amd64` publication for post-MVP capacity. Use Ubuntu 26.04, run setup/build/run as root for MVP simplicity, install shell/core utilities, network tools, build tools, `apt-fast` with `aria2`, `uv`, `fnm`, Node, Bun, rustup, `jq`, `file`, editor/debugging basics, `time`, and `tini`. Add image metadata, a smoke script, local build instructions, participant guidance, and validation requiring CPU targets to use supported `agentics-linux-arm64-cpu` repositories with `ubuntu26.04-*` tags.
  - Test spec: Run shell syntax checks for image scripts and, when network is stable, build `linux/arm64` with Docker Buildx and run `/opt/agentics/smoke.sh` on that supported MVP platform. Add bundle-validation tests for supported and unsupported CPU image repositories and tags.

- **M0.2-IMAGE-2: Define first-party CUDA devel base images**
  - Commit target: `docker: add agentics cuda base images`
  - Scope: Add target-named `linux-arm64-cuda` image sources based on NVIDIA CUDA devel Ubuntu 24.04 images. Maintain active variants for CUDA versions supported by the latest stable PyTorch release, subject to NVIDIA `linux/arm64` image availability and DGX smoke validation. Do not bundle PyTorch. Record CUDA variant, CUDA version, NVIDIA base image, Ubuntu version, and Agentics image version in labels and `/opt/agentics/image-info.json`. Validate that CUDA targets use supported `agentics-linux-arm64-cuda` repositories and tags that start with the declared CUDA variant.
  - Test spec: Verify the selected NVIDIA base image manifests include `linux/arm64`; run shell syntax checks for image scripts; build each active variant with Docker Buildx when network is stable; run `/opt/agentics/smoke.sh` with `AGENTICS_GPU_SMOKE_REQUIRE_DEVICE=1` on DGX before publication. Add bundle-validation tests for CUDA image variant/tag alignment.

### Worker and Resource Profiles

- **M0.2-WORKER-1: Execute multi-phase solution submissions**
  - Commit target: `worker: execute zip_project setup build run phases`
  - Scope: Update runner orchestration to execute setup and build in a build solution container, then execute run in a fresh no-egress solution container. Keep scorer execution in a separate scorer container with challenge-owned internet policy. Support CLI/stdin and file interfaces, isolated logs, phase-specific status, and private benchmark data mounted only into the scorer environment.
  - Test spec: Add integration tests for successful multi-phase execution, each phase failing independently, no private benchmark data mounted into solution containers, setup/build egress behavior, run-phase no-egress behavior, a defensive run-stage internet probe that must fail, CLI/stdin mode, and file mode.

- **M0.2-WORKER-2: Add resource profile enforcement**
  - Commit target: `worker: enforce challenge resource profiles`
  - Scope: Enforce CPU, memory, disk, timeout, image digest, and network policy from challenge resource profiles.
  - Test spec: Add runner tests for timeout, memory limit, network-disabled behavior where feasible, and image digest validation.

- **M0.2-WORKER-3: Add GPU profile recording**
  - Commit target: `worker: record gpu resource profiles`
  - Scope: Add challenge-declared GPU profile metadata and official-run recording of actual hardware profile.
  - Test spec: Add metadata persistence tests and runner abstraction tests using mocked GPU hardware detection.

- **M0.2-WORKER-4: Add GPU validation and official scheduling hooks**
  - Commit target: `worker: add gpu scheduling hooks`
  - Scope: Add scheduler capability flags for GPU validation and official runs without requiring full distributed runner orchestration.
  - Test spec: Add scheduler tests proving GPU jobs are only claimed by GPU-capable workers and non-GPU workers skip them.

### Backend API

- **M0.2-BE-1: Expose resource profiles**
  - Commit target: `api: expose challenge resource profiles`
  - Scope: Add resource profile fields to challenge detail, admin challenge views, and solution submission run metadata.
  - Test spec: Add API response tests for CPU-only and GPU-capable challenges.

- **M0.2-BE-2: Add capacity and quota controls**
  - Commit target: `api: add evaluation quota controls`
  - Scope: Add API and persistence-backed read models for validation quota, official-run limits, active official capacity, active agent capacity, admin capacity inspection, and clear quota error responses. Heterogeneous GPU quota remains part of the future GPU lane.
  - Test spec: Add integration tests for quota boundaries, admin override, and retry-after metadata if present.

### Agentics CLI

- **M0.2-CLI-1: Generate manifest-based solution workspaces**
  - Commit target: `cli: generate zip_project manifests`
  - Scope: Extend `init-solution` to create manifest-based workspaces with protocol metadata, empty public note, and a default run script path. Runtime/profile and interface choices remain README scaffolding hints only.
  - Test spec: Add golden tests for generated workspaces in at least Python and one non-Python README-hint profile.

- **M0.2-CLI-2: Run local validation with benchmark images**
  - Commit target: `cli: add local benchmark image validation`
  - Scope: Run local public validation from a checked-out challenge bundle by packaging the solution workspace and reusing the production Docker runner path for the selected target.
  - Test spec: Add command tests for local bundle preflight and one optional end-to-end smoke test against a sample benchmark image.

- **M0.2-CLI-3: Select targets**
  - Commit target: `cli: add target selection`
  - Scope: Add explicit `--target <target>` support to remote validation and official submission commands, plus an all-target option for challenges that advertise more than one target. CLI preflight should reject unsupported targets before packaging.
  - Test spec: Add mocked API tests for ARM64 CPU target, ARM64 CUDA target metadata, all-target submission, unsupported target rejection, disabled validation on a selected target, and JSON output containing target-specific status ids.

- **M0.2-CLI-4: Request GPU validation**
  - Commit target: `cli: add gpu validation request support`
  - Scope: Allow agents to request GPU validation when a challenge advertises a GPU profile and quota is available.
  - Test spec: Add mocked API tests for GPU-capable, CPU-only, quota-exceeded, and unsupported-server responses.

### Web and Admin

- **M0.2-WEB-1: Show protocol and resource metadata**
  - Commit target: `web: show protocol and resource metadata`
  - Scope: Display solution submission notes, target-owned resource limits, image digest, and hardware profile on challenge and solution submission pages.
  - Test spec: Add rendering tests for CPU-only and GPU-capable challenges.

- **M0.2-WEB-2: Show target-specific leaderboards**
  - Commit target: `web: show target leaderboards`
  - Scope: Add target selectors or tabs on challenge detail and leaderboard pages. Each tab should show the selected target's ranking, validation availability, resource summary, and empty state.
  - Test spec: Add rendering tests for challenges with one target, CPU and CUDA targets, disabled validation on one target, and target-specific empty leaderboards.

- **M0.2-ADMIN-1: Manage resource profiles and quotas**
  - Commit target: `admin: manage resource profiles and quotas`
  - Scope: Add admin UI for current resource profile review, validation and official quotas, and capacity status. Heterogeneous GPU profile configuration remains part of the future GPU lane.
  - Test spec: Add UI rendering tests and backend integration tests for resource profile and capacity read models.

### Challenge Authoring and Documentation

- **M0.2-EXAMPLE-1: Add `zip_project` protocol fixture challenges and submissions**
  - Commit target: `examples: add zip_project protocol fixtures`
  - Scope: Add small executable fixture challenges and matching solution submissions for CLI/stdin scoring, file-mode scoring, and scorer-controlled multi-run evaluation. Fixtures should exercise setup/build/run phases, build artifact handoff into the fresh run container, valid solutions, intentional phase failures, and private benchmark data visible only to the scorer.
  - Test spec: Add parser and runner integration tests for each fixture. Assert CLI/stdin outputs are scored, file outputs are scored, multi-run evaluation can use multiple datasets with different output formats or metric groups, phase failures are reported at the right phase, private benchmark data is not mounted into solution containers, and the run-stage internet probe cannot reach external network resources.

- **M0.2-DOC-1: Document multi-language challenge authoring**
  - Commit target: `docs: document multi-language zip_project authoring`
  - Scope: Add manifest examples, generated CLI workspace hints, reference image guidance, setup/build/run contract, two-container solution execution model, scorer/solution data boundaries, internet policy, dependency guidance, multi-run evaluation examples, language examples, and quota/admin capacity notes. Local benchmark-image validation remains a separate CLI milestone.
  - Test spec: Validate documented sample ZIPs against parser fixtures and at least one local runner smoke test.

- **M0.2-DOC-2: Document GPU benchmark expectations**
  - Commit target: `docs: document gpu benchmark expectations`
  - Scope: Document GPU profile declaration, hardware recording, validation quota, reproducibility limits, and ranking comparability constraints.
  - Test spec: Review docs against resource profile schema and mocked GPU metadata examples.

- **M0.2-DOC-3: Document target authoring**
  - Commit target: `docs: document target authoring`
  - Scope: Document CPU targets, Docker platform selection, one-target versus two-target challenges, target-specific validation availability, challenge-and-target-specific leaderboard behavior, all-target submission semantics, and how future GPU targets extend the same model.
  - Test spec: Validate documented examples against target schema fixtures and API response tests.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.2-PROTO-1: Define zip_project manifest schema` | Implemented | Adds strict shared Rust parsing and bilingual docs for a smaller `agentics.solution.json` containing protocol metadata, a public note, and setup/build/run script paths. |
| `M0.2-PROTO-2: Add setup/build/run phase model` | Implemented | Resolves setup/build/run phases from script paths while deriving execution limits from challenge-owned resource profiles and platform-owned log capture settings. |
| `M0.2-PROTO-3: Add dependency policy validation` | Deferred | Discarded as a standalone milestone; dependency reproducibility belongs to challenge owners and submitting agents and is not a participant-controlled manifest policy. |
| `M0.2-PROTO-4: Add scorer-owned prepare phase` | Implemented | Challenge bundles can generate validation or official run manifests and source-backed inputs in a scorer-owned `/prepared` workspace before solution invocations. |
| `M0.2-TARGET-1: Define target schema` | Implemented | Challenge bundles now declare `targets` with canonical ARM64 CPU/CUDA targets, Docker platform, required nullable accelerator, validation flag, and target-owned resource profile. CUDA targets require `hardware_metadata` with hardware model, GPU count, CUDA variant, and matching CUDA version metadata. AMD64 Linux targets are rejected until post-MVP deployment capacity exists. |
| `M0.2-TARGET-2: Add target-specific evaluation and leaderboards` | Implemented | Solution submissions, jobs, evaluations, quotas, workers, API DTOs, and leaderboard rows now carry `target`; HTTP submissions validate targets before artifact decode. |
| `M0.2-IMAGE-1: Define first-party CPU base image` | Implemented | Adds source-defined Ubuntu 26.04 CPU base image files, smoke checks, local build docs, participant guidance, and validation requiring supported CPU image repositories and `ubuntu26.04-*` tags. Publishing and digest rollout are intentionally deferred. |
| `M0.2-IMAGE-2: Define first-party CUDA devel base images` | Implemented | Adds target-named `linux-arm64-cuda` image sources, active CUDA 12.6/13.0/13.2 variant policy, NVIDIA manifest digests, metadata labels, smoke checks, DGX publication guidance, and validation requiring CUDA image tags to match the declared variant. Publishing and DGX runtime smoke remain deferred. |
| `M0.2-WORKER-1: Execute multi-phase solution-submissions` | Implemented | Runs setup/build in a build solution container, runs each invocation in a fresh solution container, supports source-backed run inputs, records per-invocation metadata, and isolates scoring in a separate scorer container. |
| `M0.2-WORKER-2: Add resource profile enforcement` | Implemented | Enforces challenge-declared Docker images, timeout, memory, CPU, disk, image digest validation, and network policy. |
| `M0.2-WORKER-3: Add GPU profile recording` | Implemented | Targets record accelerator and CUDA hardware metadata, including CUDA variant and version, for the DGX MVP. |
| `M0.2-WORKER-4: Add GPU validation and official scheduling hooks` | Planned | Single-DGX CUDA execution uses target accelerator metadata; heterogeneous worker capability flags and GPU-specific scheduling remain planned. |
| `M0.2-BE-1: Expose resource profiles` | Implemented | Public challenge detail responses expose strict target and resource profile metadata and reject invalid stored specs. |
| `M0.2-BE-2: Add capacity and quota controls` | Implemented | Enforces validation and official quotas before artifact upload, exposes `/admin/capacity`, and documents admin official-run overrides. Heterogeneous GPU quota remains in the future GPU lane. |
| `M0.2-CLI-1: Generate manifest-based solution workspaces` | Implemented | `init-solution` now generates smaller manifests with an empty public note and records `python-cpu`, `rust-cpu`, `node-cpu`, and `generic-cpu` as README hints only. |
| `M0.2-CLI-2: Run local validation with benchmark images` | Implemented | `validate <challenge-name> --bundle-dir <path> --target <target>` runs local validation through the shared Docker runner path, stores local logs in the CLI cache by default, supports `--all-targets`, and preflights target-disabled validation before packaging. |
| `M0.2-CLI-3: Select targets` | Implemented | `submit` and `validate --remote` support `--target` and `--all-targets`; CLI preflight rejects unsupported targets and target-disabled validation before packaging. |
| `M0.2-CLI-4: Request GPU validation` | Planned | Dedicated GPU quota UX remains planned; the current CLI can select a CUDA target through `--target`. |
| `M0.2-WEB-1: Show protocol and resource metadata` | Implemented | Observer challenge pages and frontend schemas display submission notes, scorer command, targets, and resource profile metadata. |
| `M0.2-WEB-2: Show target-specific leaderboards` | Implemented | Observer leaderboard fetches and displays the selected target, with target tabs for multi-target challenges. |
| `M0.2-ADMIN-1: Manage resource profiles and quotas` | Implemented | Admin challenge rows show current targets and mode flags; the capacity tab shows configured quotas and active usage. Heterogeneous GPU configuration remains in the future GPU lane. |
| `M0.2-EXAMPLE-1: Add zip_project protocol fixture challenges and submissions` | Implemented | Adds sample-sum stdio, grid-routing file-mode, and matrix-multiplication multi-invocation fixtures, manifest-based solutions, scorer tests, and worker integration coverage for timing metadata, private source-backed inputs, and run-stage no-egress behavior. |
| `M0.2-DOC-1: Document multi-language challenge authoring` | Implemented | Documents the canonical protocol, generated CLI hints, run manifests, resource profiles, execution isolation, dependency guidance, quota controls, admin capacity views, and local benchmark-image validation. |
| `M0.2-DOC-2: Document GPU benchmark expectations` | Implemented | MVP CUDA target policy documents required hardware metadata, active CUDA variants, shared leaderboard behavior under `linux-arm64-cuda`, and challenge-owner comparability responsibility. Heterogeneous GPU scheduling docs remain future work. |
| `M0.2-DOC-3: Document target authoring` | Implemented | Adds bilingual v0.2 target docs covering targets, Docker platforms, validation flags, target-aware APIs, CLI behavior, worker behavior, and leaderboards. |

## v0.2.5-mvp - Hosted MVP Demo and Human-Facing Web Revamp

v0.2.5-mvp is a productization checkpoint after v0.2 and before v0.3. It prepares Agentics for a public hosted demo. It should not add a new solution submission protocol. Its job is to make the existing discovery loop understandable, visually credible, bounded, operable, and open to reviewed challenge creation by humans and bots.

### Web

- **M0.2.5-WEB-1: Revamp public web visual system and layout**
  - Commit target: `web: revamp public observer UI`
  - Scope: Redesign the human-facing Observer Web surface so first-time visitors can understand Agentics, browse challenges, inspect rankings, and follow solution submission evidence without local context.
  - Test spec: Add or update rendering tests for core pages and run browser screenshots for desktop and mobile widths to check layout stability, text overflow, and broken visual states.

- **M0.2.5-WEB-2: Polish challenge browsing and challenge detail**
  - Commit target: `web: polish challenge browsing`
  - Scope: Improve challenge list and detail pages around research motivation, metric summary, validation availability, official ranking status, and resource profile.
  - Test spec: Add rendering tests for challenges with validation enabled, validation disabled, CPU-only resources, and GPU-capable resources.

- **M0.2.5-WEB-3: Polish leaderboard, solution submission detail, and artifacts**
  - Commit target: `web: polish public result inspection`
  - Scope: Make leaderboards, aggregate metrics, per-run metrics, solution submission status, logs, and artifact browsing easy for humans to scan and compare.
  - Test spec: Add rendering tests for successful, failed, not-yet-visible, validation-only, and official solution submissions with multi-metric outputs.

- **M0.2.5-WEB-4: Add creator and draft review web surfaces**
  - Commit target: `web: add creator challenge draft console`
  - Scope: Add a GitHub OAuth-backed creator route for draft creation, private asset upload, and draft status inspection. Add an admin draft review tab for validation, approval, rejection, publish, abandon, and stale cleanup. Creator pages may share the web app, but must not use the admin identity model.
  - Test spec: Add rendering tests for the creator console and admin draft tab, and verify that unsafe creator requests use a creator CSRF token rather than admin credentials.

### Challenge Creation

- **M0.2.5-CREATE-1: Define public challenge manifest and repository layout**
  - Commit target: `protocol: define github challenge creation manifest`
  - Scope: Define `agentics.challenge.json`, public repository directory layout, lifecycle metadata, archive metadata, namespace rules, required challenge-level eligibility/timing policy, and CI validation expectations.
  - Test spec: Add schema fixtures for valid new challenges, archive requests, rejected `new_version` manifests, missing README, invalid namespace, invalid lifecycle transitions, and files that should never appear in the public repo.

- **M0.2.5-CREATE-2: Add GitHub PR draft binding**
  - Commit target: `api: add github challenge draft binding`
  - Scope: Add GitHub identity or verified webhook support needed to bind a challenge draft to repo URL, PR number, commit SHA, path, manifest hash, PR URL, and PR author numeric user id. Explicit multi-owner logic is deferred until after MVP.
  - Test spec: Add API or service tests for verified PR author binding, mismatched author rejection, replay or duplicate draft handling, closed PR sync, and invalid webhook signatures where applicable.

- **M0.2.5-CREATE-3: Add private benchmark asset upload and binding**
  - Commit target: `api: add private benchmark asset binding`
  - Scope: Add private asset upload for private benchmark datasets, private scorer packages, private seeds, and reference outputs. Store asset metadata, digest, size, creator, storage URI, and draft binding in Agentics-controlled storage.
  - Test spec: Add upload tests for size limits, digest recording, missing draft rejection, unauthorized creator rejection, duplicate asset handling, and storage cleanup on failed uploads.

- **M0.2.5-CREATE-4: Add challenge draft validation and review lifecycle**
  - Commit target: `api: add challenge draft review lifecycle`
  - Scope: Add draft states, validation job records, approval, rejection, publish transition, audit events, and admin-reviewed publishing into immutable challenge contracts.
  - Test spec: Add integration tests for draft state transitions, validation failures, approval authorization, publish idempotency, audit event creation, and immutable published challenge contract records.

- **M0.2.5-CREATE-5: Add challenge archive flow and reject version updates**
  - Commit target: `api: add challenge lifecycle flows`
  - Scope: Reject `new_version` drafts because material benchmark changes require a new challenge name. Add challenge archive drafts that preserve public records, keep private assets, hide challenges from default browsing, and disable new validation or official runs.
  - Test spec: Add tests for `new_version` manifest rejection, default browse hiding for archived challenges, archived records' direct-link access, and solution submission rejection for archived challenges.

- **M0.2.5-CREATE-6: Add stale draft cleanup and challenge creation quotas**
  - Commit target: `api: add challenge draft cleanup and quotas`
  - Scope: Mark drafts tied to closed unmerged PRs as abandoned, expire inactive drafts, purge unpublished draft private assets after a grace period, and enforce MVP quotas for draft count, private asset size, validation frequency, queued validation jobs, and worker concurrency.
  - Test spec: Add tests for abandoned and expired drafts, grace-period asset purge, published asset preservation, quota boundaries, quota error responses, and admin override behavior.

### Demo Challenges

- **M0.2.5-DEMO-1: Decide official demo challenge set**
  - Commit target: `docs: define official mvp demo challenge set`
  - Scope: Use matrix multiplication throughput as the first MVP demo challenge. Keep the broader hosted demo challenge set as a TODO for later product discussion. Selection criteria should include human understandability, deterministic scoring, low run cost, clear metricized research framing, validation support, and official private benchmark cases.
  - Test spec: Review candidate challenges against the selection criteria before implementation starts.

- **M0.2.5-DEMO-2: Package official demo challenges**
  - Commit target: `examples: package mvp demo challenges`
  - Scope: Package the matrix multiplication demo with statements, public data, private seed/config overlay, scorer prepare behavior, scorer behavior, metric schema, validation toggle, resource profile, targets, and challenge repository CI.
  - Test spec: Run parser tests, challenge repository CI validation, scorer tests, public validation smoke tests, and official evaluation smoke tests for the demo challenge.

### Deployment and Operations

- **M0.2.5-DEPLOY-1: Add hosted deployment baseline**
  - Commit target: `deploy: add mvp hosted deployment baseline`
  - Scope: Add environment documentation, deployment configuration, database migration steps, storage layout, worker startup, reverse proxy assumptions, and rollback notes for the hosted demo.
  - Test spec: Run a clean deploy rehearsal in a fresh environment or documented staging target, including migrations, seed data, web startup, API startup, and worker startup.

- **M0.2.5-OPS-1: Add public quota and abuse limits**
  - Commit target: `ops: add public demo quota policy`
  - Scope: Define and implement public demo limits for validation frequency, official solution submission frequency, artifact size, log size, worker concurrency, and retry behavior.
  - Test spec: Add API integration tests for quota boundaries, rejected requests, retry metadata where present, and admin override behavior.

- **M0.2.5-OPS-2: Add health checks, observability, and runbook**
  - Commit target: `ops: add mvp health checks and runbook`
  - Scope: Add health checks, worker status visibility, log retention guidance, backup guidance, operational alerts, and an operator runbook for common failure modes.
  - Test spec: Manually verify health endpoints and runbook commands in staging; add automated checks where the current stack supports them.

- **M0.2.5-DGX-1: Inventory DGX Spark host and container runtime**
  - Commit target: `ops: document dgx spark host inventory`
  - Scope: Before moving the MVP to DGX Spark, record OS image, architecture, Docker version, Docker storage driver, loopback XFS and project-quota support, NVIDIA driver, CUDA visibility, NVIDIA container runtime, persistent storage mount, ingress path, and operator access model. Decide the Agentics-owned Docker daemon socket and data-root location.
  - Test spec: On the DGX Spark host, capture `uname -a`, `docker info`, `findmnt` or equivalent mount evidence for the loopback XFS image, `nvidia-smi`, and an NVIDIA container runtime Docker smoke command, then attach the results to the deployment checklist.

- **M0.2.5-DGX-2: Add DGX Spark deployment profile**
  - Commit target: `deploy: add dgx spark mvp profile`
  - Scope: Define DGX-specific environment values, persistent storage layout, reverse proxy and TLS assumptions, Docker runtime settings, service supervision, backup locations, and release artifact paths. Include an Agentics-owned Docker daemon backed by a loopback XFS data-root image with project quotas, `AGENTICS_HOST_PROBE_MODE=require`, Docker writable-layer quota probes, and root-prepared XFS project-quota slots under per-phase loopback filesystem images for all solution setup/build/run writable mounts and scorer prepare/score writable mounts. Keep GPU solution execution disabled until the GPU milestone lane is implemented.
  - Test spec: Dry-run migrations, API startup, worker startup, web startup, health checks, Docker writable-layer quota probe, per-phase loop-image writable-mount probe, and per-phase quota-slot exhaustion probe on DGX Spark with persistent storage and non-default admin credentials.

- **M0.2.5-DGX-3: Run DGX Spark end-to-end smoke and benchmark calibration**
  - Commit target: `ops: add dgx spark smoke checklist`
  - Scope: Run hosted CLI onboarding, matrix official submission on supported CPU targets, no-egress runner smoke, storage-quota escape smoke, worker heartbeat inspection, capacity inspection, and initial runtime calibration on DGX Spark. Record that the hosted MVP deployment supports `linux-arm64-cpu` and `linux-arm64-cuda`, while `linux-amd64-cpu` and `linux-amd64-cuda` remain post-MVP targets until AMD64 deployment capacity exists.
  - Test spec: Capture terminal status for a sample official submission, `/admin/capacity`, `/admin/service-heartbeats`, runner logs, matrix benchmark timing baselines, and proof that a job writing beyond Docker writable-layer or writable-mount limits fails without exhausting host disk.

### CLI and Documentation

- **M0.2.5-CLI-1: Validate hosted CLI onboarding**
  - Commit target: `cli: polish hosted demo onboarding`
  - Scope: Ensure an agent or operator can configure the CLI against the hosted demo, register, inspect a challenge, initialize a workspace, validate if enabled, submit officially, and poll status.
  - Test spec: Add command-level tests for hosted configuration examples and run one end-to-end smoke test against staging.

- **M0.2.5-CLI-2: Add challenge draft reviewer commands**
  - Commit target: `cli: add challenge draft reviewer workflow`
  - Scope: Add CLI helpers for admin validation, approval, rejection, publish, abandon, and cleanup using Basic Auth. Creator-side draft creation, draft status, and private asset upload remain web-only until the CLI supports GitHub OAuth creator sessions.
  - Test spec: Add command parser tests, mocked admin API tests, and golden output for validation failure responses.

- **M0.2.5-CLI-3: Add agent result exploration commands**
  - Commit target: `cli: add agent result exploration commands`
  - Scope: Add `agentics challenges stats <challenge-name> --target <target>`, `agentics submissions list <challenge-name> --target <target>`, and `agentics submissions report <solution-submission-id>`. `challenges stats` should display challenge status, timing, eligibility, ranking metric, ranked-agent count, visible-submission count, best/mean/median/p90 summary for the selected metric, and a small top-leaderboard table. `submissions list` should default to `--limit 20`, be capped by a server-side maximum, default to newest visible submissions, and display fields needed to chain follow-up commands: submission id, agent display name, target, status, rank score, official score when visible, and creation time. `submissions report` should show the submission's challenge, target, agent, status, timestamps, validation and official scores when visible, aggregate metrics, ranking context, and a logs command hint when authenticated logs are available.
  - Test spec: Add CLI parser and mocked API tests for the three commands, including default limit 20, server-limit error rendering, public fallback for result reports without a token, authenticated result reports with ranking context, hidden/redacted visibility states, and table plus JSON output.

- **M0.2.5-CLI-4: Replace output-format flag with global JSON convention**
  - Commit target: `cli: add global json output`
  - Scope: Replace the current `--output json` command style with a global `--json` flag before MVP. Every command that emits structured information should support `--json`, including registration, auth/config inspection, challenge discovery and stats, solution initialization, validation, official submission, submission list/show/wait/report/logs/rank, leaderboard reads, metric distributions, and admin/reviewer helpers. Plain table or log-friendly text remains the default.
  - Test spec: Add command parser tests proving `--json` is accepted globally, old `--output json` is rejected before MVP, and representative commands produce complete machine-readable responses rather than table-shaped JSON.

- **M0.2.5-SKILL-1: Add challenge authoring skill**
  - Commit target: `skill: add challenge authoring workflow`
  - Scope: Add an agent skill that teaches creators how to structure the public repo files, write the manifest, avoid private-data leakage, upload private assets through Agentics, validate drafts, and request publishing.
  - Test spec: Review the skill against CLI help output, manifest schema examples, and the draft lifecycle docs.

- **M0.2.5-SKILL-2: Add challenge review skill**
  - Commit target: `skill: add challenge review workflow`
  - Scope: Add an admin/reviewer skill covering namespace review, metric review, leakage checks, license checks, resource cost review, private asset binding, validation smoke tests, archive review, and publish decisions.
  - Test spec: Review the skill against PRD lifecycle rules, admin CLI/API behavior, and reviewer checklist docs.

- **M0.2.5-DOC-1: Document public MVP demo usage**
  - Commit target: `docs: document public mvp demo`
  - Scope: Add concise public instructions for humans, agents, challenge creators, challenge reviewers, and operators. Include demo caveats, quota policy, sandbox limits, and the fact that demo challenges are proxy metrics rather than scientific proof.
  - Test spec: Review docs against the hosted CLI smoke path, web UI labels, and PRD scope.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.2.5-WEB-1: Revamp public web visual system and layout` | Planned | Public first impression blocker. |
| `M0.2.5-WEB-2: Polish challenge browsing and challenge detail` | Planned | Depends on resource metadata and structured challenge summaries. |
| `M0.2.5-WEB-3: Polish leaderboard, solution submission detail, and artifacts` | Planned | Depends on structured metric display. |
| `M0.2.5-WEB-4: Add creator and draft review web surfaces` | Implemented | `/creator` uses GitHub OAuth creator sessions for draft creation and asset upload; `/admin` includes a Drafts tab for reviewer lifecycle actions. |
| `M0.2.5-CREATE-1: Define public challenge manifest and repository layout` | Implemented | Public manifest, repo layout validation, namespace rules, and leakage checks are implemented and documented. |
| `M0.2.5-CREATE-2: Add GitHub PR draft binding` | Implemented | Drafts bind repo URL, PR number, commit SHA, path, manifest hash, PR URL, and linked PR author id. |
| `M0.2.5-CREATE-3: Add private benchmark asset upload and binding` | Implemented | Private asset upload stores digest, size, storage URI, uploader, and draft binding outside GitHub. |
| `M0.2.5-CREATE-4: Add challenge draft validation and review lifecycle` | Implemented | Draft validation records, approval, rejection, publish transition, and audit events are implemented. |
| `M0.2.5-CREATE-5: Add challenge archive flow and reject version updates` | Implemented | `new_version` manifests are rejected; archive drafts hide challenges while preserving direct records. |
| `M0.2.5-CREATE-6: Add stale draft cleanup and challenge creation quotas` | Implemented | Active draft limits, private asset byte limits, validation-frequency limits, stale draft abandonment, and unpublished asset purge are implemented. |
| `M0.2.5-DEMO-1: Decide official demo challenge set` | Implemented | Matrix multiplication throughput is the first MVP demo challenge; broader hosted demo set remains a TODO. |
| `M0.2.5-DEMO-2: Package official demo challenges` | Implemented | Matrix demo lives in the challenge repository, uses private seed/config plus prepare-generated official data, and passed the local GitHub draft/publish/submit smoke path. |
| `M0.2.5-DEPLOY-1: Add hosted deployment baseline` | Implemented | Mac-local MVP deployment rehearsal is documented; DGX Spark hosted profile is now covered separately by DGX-1 and DGX-2. |
| `M0.2.5-OPS-1: Add public quota and abuse limits` | Implemented | Backend-enforced quotas and pioneer-code gated registration are documented with recommended Mac-local MVP values and Cloudflare edge controls. |
| `M0.2.5-OPS-2: Add health checks, observability, and runbook` | Implemented | Operations runbook and `scripts/ops/check-local-mvp.sh` cover health, capacity, heartbeat, logs, failures, and backups. |
| `M0.2.5-DGX-1: Inventory DGX Spark host and container runtime` | Implemented | Linux host, GPU, NVIDIA toolkit, storage, XFS tooling, loopback tooling, default Docker server/storage driver, and NVIDIA Docker smoke evidence are summarized in `docs/dgx-spark/en.md`. |
| `M0.2.5-DGX-2: Add DGX Spark deployment profile` | Implemented | Profile docs, env template, systemd units, Agentics-owned Docker config, Linux-gated storage/profile scripts, loopback XFS mounts with `/etc/fstab` entries, root-prepared runner quota slots, enabled Agentics-owned Docker daemon, and strict profile verification are in place. |
| `M0.2.5-DGX-3: Run DGX Spark end-to-end smoke and benchmark calibration` | Implemented | DGX smoke evidence is summarized in `docs/dgx-spark/en.md`, including hosted CLI onboarding, matrix validation and official submission on `linux-arm64-cpu`, no-egress runner smoke, storage-quota escape smoke, capacity, heartbeats, and the MVP target decision. |
| `M0.2.5-CLI-1: Validate hosted CLI onboarding` | Implemented | Hosted CLI smoke path is documented for registration, challenge inspection, workspace initialization, validation, official submission, and polling. |
| `M0.2.5-CLI-2: Add challenge draft reviewer commands` | Implemented | CLI covers admin validation, review, publish, abandon, and cleanup helpers; creator-side GitHub OAuth CLI support remains deferred in favor of the `/creator` web flow. |
| `M0.2.5-CLI-3: Add agent result exploration commands` | Implemented | Adds challenge stats, visible solution submission listing with default limit 20, detailed submission reports, public/authenticated report fallback, and target-scoped API support. |
| `M0.2.5-CLI-4: Replace output-format flag with global JSON convention` | Implemented | Replaces `--output json` with global `--json` and keeps JSON complete for agent automation. |
| `M0.2.5-SKILL-1: Add challenge authoring skill` | Implemented | `skills/challenge-authoring-workflow/SKILL.md` documents creator workflow, `/creator` web usage, and private asset ZIP overlays. |
| `M0.2.5-SKILL-2: Add challenge review skill` | Implemented | `.agents/skills/challenge-review-workflow/SKILL.md` documents reviewer checks, admin web inspection, and admin CLI operations. |
| `M0.2.5-DOC-1: Document public MVP demo usage` | Implemented | Public MVP usage docs now cover humans, agents, creators, reviewers, operators, quotas, sandbox limits, demo caveats, and local smoke evidence. |

## v0.3 - GitHub PR Solution Submission Protocol

v0.3 adds a repository-based solution submission path for public, auditable challenge communities while preserving direct CLI/API ZIP solution submissions.

### GitHub Solution Submission Protocol

- **M0.3-GH-1: Define repository layout and PR contract**
  - Commit target: `protocol: document github pr solution submission contract`
  - Scope: Define challenge directory layout, solution directory layout, required metadata, PR title/body conventions, and validation-only CI behavior.
  - Test spec: Add fixture repository layouts and validation tests for accepted and rejected PR structures.

- **M0.3-GH-2: Add GitHub identity mapping**
  - Commit target: `api: add github identity mapping`
  - Scope: Map GitHub accounts or bot identities to Agentics agent identities without replacing existing bearer-token auth.
  - Test spec: Add API tests for linking, duplicate mapping rejection, unlinking, and unauthorized access.

- **M0.3-GH-3: Add trusted validation result ingestion**
  - Commit target: `api: add trusted github result ingestion`
  - Scope: Ingest validation results from trusted callbacks, signed artifacts, or platform polling.
  - Test spec: Add signature or artifact verification tests, replay rejection tests, and malformed payload tests.

- **M0.3-GH-4: Add official-run handoff**
  - Commit target: `api: add github official run handoff`
  - Scope: Allow trusted repository workflows or admin actions to trigger Agentics-controlled official runs after validation.
  - Test spec: Add integration tests proving private benchmark data never leaves Agentics-controlled runners and leaderboard updates only after official success.

### Worker and CI Integration

- **M0.3-WORKER-1: Add repository artifact fetch support**
  - Commit target: `worker: fetch trusted repository artifacts`
  - Scope: Fetch trusted solution artifacts or checked-out refs for official runs without relying on untrusted fork CI for private benchmark data.
  - Test spec: Add mocked GitHub artifact/ref fetch tests and failure-mode tests for missing, expired, or oversized artifacts.

- **M0.3-CI-1: Add validation workflow templates**
  - Commit target: `ci: add github validation workflow templates`
  - Scope: Provide reusable workflow templates for public validation runs on forks or PRs.
  - Test spec: Add static validation for workflow YAML and a dry-run style fixture test if available.

### Web and Admin

- **M0.3-WEB-1: Show GitHub-linked solution submissions**
  - Commit target: `web: show github-linked solution-submissions`
  - Scope: Display PR URL, commit SHA, validation status, official-run handoff status, and trusted artifact metadata on solution submission pages.
  - Test spec: Add rendering tests for direct ZIP solution submissions and GitHub PR solution submissions.

- **M0.3-ADMIN-1: Add PR moderation and official-run controls**
  - Commit target: `admin: add github pr moderation controls`
  - Scope: Add admin tools for approving official-run handoff, blocking abusive PR-linked solution submissions, and inspecting trusted ingestion metadata.
  - Test spec: Add UI action tests and backend authorization tests.

### Agentics CLI

- **M0.3-CLI-1: Add GitHub workflow helper commands**
  - Commit target: `cli: add github solution submission helpers`
  - Scope: Add helpers to initialize challenge directories, validate local repository layout, and print PR instructions.
  - Test spec: Add filesystem fixture tests and golden-output tests for generated instructions.

### Documentation

- **M0.3-DOC-1: Document GitHub solution submission security model**
  - Commit target: `docs: document github solution submission security model`
  - Scope: Explain private benchmark data handling, trusted runners, result ingestion, identity mapping, PR spam controls, CI hardware limits, and GPU limitations.
  - Test spec: Review against implementation behavior and PRD GitHub Solution Submission Concerns.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.3-GH-1: Define repository layout and PR contract` | Planned | Defines the public repository contract. |
| `M0.3-GH-2: Add GitHub identity mapping` | Planned | Identity prerequisite for PR solution submissions. |
| `M0.3-GH-3: Add trusted validation result ingestion` | Planned | Requires a concrete trust model. |
| `M0.3-GH-4: Add official-run handoff` | Planned | Depends on trusted ingestion and official runners. |
| `M0.3-WORKER-1: Add repository artifact fetch support` | Planned | Required for official runs from repository artifacts. |
| `M0.3-CI-1: Add validation workflow templates` | Planned | Provides validation-only templates. |
| `M0.3-WEB-1: Show GitHub-linked solution-submissions` | Planned | Depends on PR metadata ingestion. |
| `M0.3-ADMIN-1: Add PR moderation and official-run controls` | Planned | Admin control for PR workflow. |
| `M0.3-CLI-1: Add GitHub workflow helper commands` | Planned | Helper layer, not required for CI ingestion. |
| `M0.3-DOC-1: Document GitHub solution submission security model` | Planned | Should ship before public GitHub workflow. |

## Cross-Version Backlog

These items cut across versions and should be scheduled when they become blockers for the release in progress.

- **BACKLOG-QA-1: Add end-to-end smoke harness**
  - Commit target: `test: add local e2e smoke harness`
  - Scope: Automate the local path from migrations through agent registration, sample solution submission, worker completion, leaderboard update, and web read.
  - Test spec: The harness is the test. It should be runnable locally and in CI when Docker is available.

- **BACKLOG-DOC-1: Keep English and Chinese docs aligned**
  - Commit target: `docs: sync english and chinese product docs`
  - Scope: Whenever product docs change, keep `docs/PRD/en.md`, `docs/PRD/zh.md`, and milestone docs aligned at the feature level.
  - Test spec: Manual heading and feature-list comparison before each docs commit.

- **BACKLOG-OBS-1: Improve operational observability**
  - Commit target: `ops: improve worker and evaluation observability`
  - Scope: Add structured logs, job lifecycle traces, and diagnostics for failed evaluations as needed by worker and admin milestones.
  - Test spec: Add unit or integration tests for emitted state transitions where practical, and verify logs during smoke tests.
