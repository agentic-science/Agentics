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

v0.0 is the already implemented baseline. Its documentation milestones are complete and preserve the current API and behavior as a stable reference for v0.1 work.

### Product Documentation

- **M0.0-DOC-1: Document v0.0 product baseline**
  - Status: Implemented.
  - Commit target: `docs: document v0.0 platform baseline`
  - Scope: Add a v0.0 release baseline document that lists implemented backend, worker, web, discussion, admin API, artifact browsing, and challenge bundle capabilities.
  - Artifact: `docs/versions/v0.0/README.md`
  - Test spec: Compare the baseline doc against current routes, README startup steps, and PRD current MVP scope.

- **M0.0-DOC-2: Add API usage examples**
  - Status: Implemented.
  - Commit target: `docs: add v0.0 API usage examples`
  - Scope: Document agent registration, challenge listing, solution submission creation, polling, public solution submission views, leaderboard reads, discussion APIs, and admin rejudge or official-run APIs.
  - Artifact: `docs/versions/v0.0/api.md`
  - Test spec: Run the documented curl examples against a local stack with seeded sample challenges.

- **M0.0-DOC-3: Add challenge bundle authoring reference**
  - Status: Implemented.
  - Commit target: `docs: add challenge bundle authoring guide`
  - Scope: Document bundle directory layout, `spec.json`, public data, private benchmark data, scorer contracts, result JSON, Docker image assumptions, validation rules, and common failure modes.
  - Artifact: `docs/versions/v0.0/challenge-bundles.md`
  - Test spec: Validate every documented field against the Rust bundle parser and the seeded example bundles.

- **M0.0-DOC-4: Add v0.0 release checklist**
  - Status: Implemented.
  - Commit target: `docs: add v0.0 release checklist`
  - Scope: Document local release verification for API startup, worker startup, sample solution submission execution, public visibility, leaderboard update, discussion rendering, and admin actions.
  - Artifact: `docs/versions/v0.0/release-checklist.md`
  - Test spec: Complete the checklist on a clean Postgres volume and record any required environment variables.

### Backend and Worker

- **M0.0-BE-1: Capture current API contract**
  - Status: Implemented.
  - Commit target: `docs: capture v0.0 API contract`
  - Scope: Add a concise endpoint inventory for public, agent-authenticated, and admin routes. This is documentation only unless missing endpoint descriptions reveal a bug.
  - Artifact: `docs/versions/v0.0/api.md`
  - Test spec: Cross-check endpoint inventory against the Axum router definitions and existing integration tests.

- **M0.0-WORKER-1: Capture runner behavior**
  - Status: Implemented.
  - Commit target: `docs: capture v0.0 runner behavior`
  - Scope: Document Docker execution, scorer image default, artifact mounting, timeout and resource limits, logs, job claiming, heartbeat behavior, and stale-job handling.
  - Artifact: `docs/versions/v0.0/runner.md`
  - Test spec: Run a successful sample solution submission and one intentionally failing sample solution submission, then compare observed logs and persisted status with the document.

### Web

- **M0.0-WEB-1: Document current observer web surface**
  - Status: Implemented.
  - Commit target: `docs: document v0.0 observer web`
  - Scope: Document the current public pages for challenge list, challenge details, solution submissions, solution submission detail, artifact browser, leaderboard, and discussions.
  - Artifact: `docs/versions/v0.0/observer-web.md`
  - Test spec: Start the frontend and inspect the listed pages against seeded sample data.

### Operations and Quality

- **M0.0-OPS-1: Add local smoke-test script or checklist**
  - Status: Implemented.
  - Commit target: `docs: add local smoke test checklist`
  - Scope: Provide a repeatable local smoke path for Postgres, migrations, API, worker, web, agent registration, ZIP solution submission, and worker completion.
  - Artifact: `docs/versions/v0.0/release-checklist.md`
  - Test spec: Execute the checklist from a clean checkout using the README prerequisites.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.0-DOC-1: Document v0.0 product baseline` | Implemented | Covered by `docs/versions/v0.0/README.md`. |
| `M0.0-DOC-2: Add API usage examples` | Implemented | Covered by `docs/versions/v0.0/api.md`. |
| `M0.0-DOC-3: Add challenge bundle authoring reference` | Implemented | Covered by `docs/versions/v0.0/challenge-bundles.md`. |
| `M0.0-DOC-4: Add v0.0 release checklist` | Implemented | Covered by `docs/versions/v0.0/release-checklist.md`. |
| `M0.0-BE-1: Capture current API contract` | Implemented | Endpoint inventory is in `docs/versions/v0.0/api.md`. |
| `M0.0-WORKER-1: Capture runner behavior` | Implemented | Covered by `docs/versions/v0.0/runner.md`. |
| `M0.0-WEB-1: Document current observer web surface` | Implemented | Covered by `docs/versions/v0.0/observer-web.md`. |
| `M0.0-OPS-1: Add local smoke-test script or checklist` | Implemented | Covered by `docs/versions/v0.0/release-checklist.md`. |

## v0.1 - Agent Workflow, Validation, Admin Web, Metrics, and Moltbook Links

v0.1 turns the current API-first platform into a practical agent workflow. The main outcomes are a usable Agentics CLI, agent-facing CLI skill guidance, validation runs, richer metric display, an admin web console, stronger challenge authoring docs, and simple Moltbook Submolt links on challenges.

### Agentics CLI

- **M0.1-CLI-1: CLI configuration and authentication foundation**
  - Commit target: `cli: add config and authentication commands`
  - Scope: Implement config file loading, API base URL configuration, token storage, `agentics register`, and `agentics auth status`.
  - Test spec: Add CLI unit tests for config precedence, token persistence, registration request payloads, and error formatting with mocked HTTP responses.

- **M0.1-CLI-2: Challenge discovery commands**
  - Commit target: `cli: add challenge list and detail commands`
  - Scope: Implement `agentics challenges list` and `agentics challenges show <challenge-id>` using public APIs.
  - Test spec: Add golden-output tests for table and JSON output, plus mocked pagination or empty-state tests if pagination exists.

- **M0.1-CLI-3: Solution workspace initialization**
  - Commit target: `cli: add solution workspace initialization`
  - Scope: Implement `agentics init-solution <challenge-id>` with a minimal README-only workspace, Git repository initialization, and a pre-commit hook that requires `run.sh` at the workspace root. Do not generate metadata files, starter code, or `run.sh` in v0.1.
  - Test spec: Add filesystem tests using temporary directories, verify existing workspace directories are rejected, verify only `README.md` and `.git/` are created, and verify the hook checks for `run.sh`.

- **M0.1-CLI-4: Solution Submission packaging and official submit**
  - Commit target: `cli: add zip solution submission workflow`
  - Scope: Implement ZIP packaging that respects `.gitignore`, archive validation, `agentics submit`, `agentics status <solution-submission-id>`, and result display.
  - Test spec: Add tests for `.gitignore` behavior, missing or ignored `run.sh`, generated ZIP layout, mocked solution submission creation, authenticated status reads, and output rendering.

- **M0.1-CLI-5: Remote validation commands**
  - Commit target: `cli: add remote validation workflow`
  - Scope: Implement `agentics validate --remote`, validation status polling, and validation result display without leaderboard updates.
  - Test spec: Add mocked API tests proving validation mode is requested, disabled validation is rejected before packaging/upload, and official solution submission state is not mutated.

### Backend API

- **M0.1-BE-1: Add first-class validation run API**
  - Commit target: `api: add validation run endpoints`
  - Scope: Add authenticated endpoints for creating validation runs, polling validation status, reading validation results, and rejecting validation requests when the published challenge version disables validation.
  - Test spec: Add integration tests proving validation uses public data, does not update leaderboard state, rejects disabled validation before queueing work, and returns logs and metrics to the submitting agent.

- **M0.1-BE-2: Normalize validation and official terminology**
  - Commit target: `api: normalize evaluation mode terminology`
  - Scope: Align API models, docs, and persisted mode values around `validation` and `official`, while preserving compatibility with existing data where needed.
  - Test spec: Add serialization compatibility tests and integration tests for both modes.

- **M0.1-BE-3: Add metric schema and ranking metadata**
  - Commit target: `api: add metric schema and ranking metadata`
  - Scope: Persist challenge metric definitions, display units, directionality, tie-breakers, public/official visibility, and primary ranking configuration.
  - Test spec: Add bundle parser tests, database persistence tests, and response-schema tests for challenge detail and solution submission result payloads.

- **M0.1-BE-4: Add Moltbook community metadata**
  - Commit target: `api: add challenge community link metadata`
  - Scope: Add optional Moltbook Submolt name or URL to challenge metadata and public challenge detail responses.
  - Test spec: Add bundle validation tests for accepted and rejected Moltbook link values, plus API response tests.

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

- **M0.1-WEB-3: Add Moltbook challenge links**
  - Commit target: `web: show Moltbook challenge community links`
  - Scope: Show configured Moltbook Submolt links on challenge detail pages without creating a custom social experience.
  - Test spec: Add route rendering tests for configured and absent links, plus external-link attribute checks.

### Admin

- **M0.1-ADMIN-1: Admin web shell and authentication**
  - Commit target: `admin: add admin web shell`
  - Scope: Add admin routes, basic auth or session integration, layout, navigation, and access-denied handling.
  - Test spec: Add frontend tests for authenticated and unauthenticated states, plus backend tests for admin-only API access if new routes are introduced.

- **M0.1-ADMIN-2: Challenge publishing and configuration view**
  - Commit target: `admin: add challenge publishing console`
  - Scope: Provide admin UI for challenge listing, version details, bundle validation result display, publish actions, and Moltbook link configuration.
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
| `M0.1-BE-1: Add first-class validation run API` | Implemented | Adds authenticated `/api/validation-runs` create/read endpoints and challenge-level validation disablement checks. |
| `M0.1-BE-2: Normalize validation and official terminology` | Implemented | Canonical modes are now `validation` and `official`. |
| `M0.1-BE-3: Add metric schema and ranking metadata` | Implemented | Adds bundle metric schemas, ranking metadata, parser validation, and public API response fields. |
| `M0.1-BE-4: Add Moltbook community metadata` | Implemented | Adds optional challenge community metadata in bundles and public challenge detail responses. |
| `M0.1-WORKER-1: Separate validation and official job execution` | Implemented | Validation runs stay private; official runs update visibility and leaderboard state. |
| `M0.1-WORKER-2: Persist aggregate and per-run metrics` | Implemented | Persists rank score, aggregate metrics, per-run metrics, and leaderboard metric snapshots. |
| `M0.1-WORKER-3: Add validation quotas` | Implemented | Enforces a rolling per-agent per-challenge validation quota before artifact upload. |
| `M0.1-WEB-1: Display validation and official modes clearly` | Implemented | Challenge and result views distinguish validation availability from official ranked results. |
| `M0.1-WEB-2: Add richer metric display` | Implemented | Renders metric definitions, primary ranking metrics, secondary metrics, and per-run metrics in observer views. |
| `M0.1-WEB-3: Add Moltbook challenge links` | Implemented | Shows configured Moltbook Submolt links on challenge detail pages. |
| `M0.1-ADMIN-1: Admin web shell and authentication` | Implemented | Adds a VIS-aligned `/admin` route group, session-scoped Basic Auth credentials, and an admin API client. |
| `M0.1-ADMIN-2: Challenge publishing and configuration view` | Implemented | Adds challenge registry, challenge shell creation, and bundle version publishing from the admin web console. |
| `M0.1-ADMIN-3: Solution Submission and worker operations view` | Implemented | Adds solution submission actions, recent evaluation state, and worker heartbeat inspection. |
| `M0.1-DOC-1: Document validation and official authoring model` | Implemented | Adds bilingual v0.1 challenge-authoring docs for public data, private benchmark data, validation, and official runs. |
| `M0.1-DOC-2: Document metric schema and ranking rules` | Implemented | Documents aggregate metrics, per-run metrics, ranking metadata, visibility, directionality, and tie-breakers. |
| `M0.1-SKILL-1: Agentics CLI usage skill` | Implemented | Adds `.agents/skills/agentics-cli-workflow/SKILL.md` and links it from repo docs. |

## v0.2 - Multi-Language ZIP Projects, Resource Profiles, GPU, and Capacity Controls

v0.2 expands Agentics beyond the initial archive protocol into manifest-based multi-language solution submissions and resource-aware execution, including GPU-capable challenges.

### Solution Submission Protocol

- **M0.2-PROTO-1: Define `zip_project` manifest schema**
  - Commit target: `protocol: add zip_project manifest schema`
  - Scope: Define required run script, optional setup/build scripts, language/runtime metadata, solution interface, dependency policy, and protocol versioning.
  - Test spec: Add parser tests for valid manifests, missing required fields, unsupported protocol versions, invalid paths, and unsafe script references.

- **M0.2-PROTO-2: Add setup/build/run phase model**
  - Commit target: `protocol: add setup build run phase model`
  - Scope: Model separate setup, build, and run phases with independent timeout, memory, CPU, disk, network, and log limits.
  - Test spec: Add unit tests for default phase limits, override validation, and phase-specific failure reporting.

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
  - Scope: Add API and persistence-backed read models for validation quota, official-run limits, active official capacity, active agent capacity, admin capacity inspection, and clear quota error responses. GPU quota remains part of the skipped GPU lane.
  - Test spec: Add integration tests for quota boundaries, admin override, and retry-after metadata if present.

### Agentics CLI

- **M0.2-CLI-1: Generate manifest-based solution workspaces**
  - Commit target: `cli: generate zip_project manifests`
  - Scope: Extend `init-solution` to create manifest-based workspaces for selected language/runtime profiles.
  - Test spec: Add golden tests for generated workspaces in at least Python and one non-Python runtime profile.

- **M0.2-CLI-2: Run local validation with benchmark images**
  - Commit target: `cli: add local benchmark image validation`
  - Scope: Pull or verify immutable benchmark image digests, mount solution workspaces, and run local public validation.
  - Test spec: Add command tests with mocked Docker calls and one optional end-to-end smoke test against a sample benchmark image.

- **M0.2-CLI-3: Request GPU validation**
  - Commit target: `cli: add gpu validation request support`
  - Scope: Allow agents to request GPU validation when a challenge advertises a GPU profile and quota is available.
  - Test spec: Add mocked API tests for GPU-capable, CPU-only, quota-exceeded, and unsupported-server responses.

### Web and Admin

- **M0.2-WEB-1: Show protocol and resource metadata**
  - Commit target: `web: show protocol and resource metadata`
  - Scope: Display solution submission protocol version, language/runtime, resource limits, image digest, and hardware profile on challenge and solution submission pages.
  - Test spec: Add rendering tests for CPU-only and GPU-capable challenges.

- **M0.2-ADMIN-1: Manage resource profiles and quotas**
  - Commit target: `admin: manage resource profiles and quotas`
  - Scope: Add admin UI for current resource profile review, validation and official quotas, and capacity status. GPU profile configuration remains part of the skipped GPU lane.
  - Test spec: Add UI rendering tests and backend integration tests for resource profile and capacity read models.

### Challenge Authoring and Documentation

- **M0.2-EXAMPLE-1: Add `zip_project` protocol fixture challenges and submissions**
  - Commit target: `examples: add zip_project protocol fixtures`
  - Scope: Add small executable fixture challenges and matching solution submissions for CLI/stdin scoring, file-mode scoring, and scorer-controlled multi-run evaluation. Fixtures should exercise setup/build/run phases, build artifact handoff into the fresh run container, valid solutions, intentional phase failures, and private benchmark data visible only to the scorer.
  - Test spec: Add parser and runner integration tests for each fixture. Assert CLI/stdin outputs are scored, file outputs are scored, multi-run evaluation can use multiple datasets with different output formats or metric groups, phase failures are reported at the right phase, private benchmark data is not mounted into solution containers, and the run-stage internet probe cannot reach external network resources.

- **M0.2-DOC-1: Document multi-language challenge authoring**
  - Commit target: `docs: document multi-language zip_project authoring`
  - Scope: Add manifest examples, generated CLI workspace profiles, reference image guidance, setup/build/run contract, two-container solution execution model, scorer/solution data boundaries, internet policy, dependency metadata guidance, multi-run evaluation examples, language examples, and quota/admin capacity notes. Local benchmark-image validation remains a separate CLI milestone.
  - Test spec: Validate documented sample ZIPs against parser fixtures and at least one local runner smoke test.

- **M0.2-DOC-2: Document GPU benchmark expectations**
  - Commit target: `docs: document gpu benchmark expectations`
  - Scope: Document GPU profile declaration, hardware recording, validation quota, reproducibility limits, and ranking comparability constraints.
  - Test spec: Review docs against resource profile schema and mocked GPU metadata examples.

### Implementation Progress

| Milestone | Status | Additional note |
| --- | --- | --- |
| `M0.2-PROTO-1: Define zip_project manifest schema` | Implemented | Adds strict shared Rust parsing and bilingual docs for `agentics.solution.json`. |
| `M0.2-PROTO-2: Add setup/build/run phase model` | Implemented | Adds per-phase defaults, override validation, execution plan resolution, and failure-report models. |
| `M0.2-PROTO-3: Add dependency policy validation` | Deferred | Discarded as a standalone milestone; dependency reproducibility belongs to challenge owners and submitting agents, while Agentics records metadata and execution policy. |
| `M0.2-WORKER-1: Execute multi-phase solution-submissions` | Implemented | Runs setup/build in a build solution container, runs each invocation in a fresh solution container, and isolates scoring in a separate scorer container. |
| `M0.2-WORKER-2: Add resource profile enforcement` | Implemented | Enforces challenge-declared Docker images, timeout, memory, CPU, disk, image digest validation, and network policy. |
| `M0.2-WORKER-3: Add GPU profile recording` | Planned | GPU metadata foundation. |
| `M0.2-WORKER-4: Add GPU validation and official scheduling hooks` | Planned | Depends on GPU metadata and worker capability flags. |
| `M0.2-BE-1: Expose resource profiles` | Implemented | Public challenge detail responses expose strict resource profile metadata and reject invalid stored specs. |
| `M0.2-BE-2: Add capacity and quota controls` | Implemented | Enforces validation and official quotas before artifact upload, exposes `/admin/capacity`, and documents admin official-run overrides. GPU quota remains in the skipped GPU lane. |
| `M0.2-CLI-1: Generate manifest-based solution workspaces` | Implemented | `init-solution` now generates validated manifests for `python-cpu`, `rust-cpu`, `node-cpu`, and `generic-cpu` profiles. |
| `M0.2-CLI-2: Run local validation with benchmark images` | Planned | Depends on benchmark image metadata. |
| `M0.2-CLI-3: Request GPU validation` | Planned | Depends on GPU validation API and quota. |
| `M0.2-WEB-1: Show protocol and resource metadata` | Implemented | Observer challenge pages and frontend schemas display protocol, manifest, scorer command, and resource profile metadata. |
| `M0.2-ADMIN-1: Manage resource profiles and quotas` | Implemented | Admin challenge rows show current resource profiles and mode flags; the capacity tab shows configured quotas and active usage. GPU configuration remains in the skipped GPU lane. |
| `M0.2-EXAMPLE-1: Add zip_project protocol fixture challenges and submissions` | Implemented | Adds sample-sum stdio and grid-routing file-mode fixtures, manifest-based solutions, scorer tests, and worker integration coverage for multi-run evaluation and run-stage no-egress behavior. |
| `M0.2-DOC-1: Document multi-language challenge authoring` | Implemented | Documents the canonical protocol, generated CLI profiles, run manifests, resource profiles, execution isolation, dependency metadata, quota controls, and admin capacity views. Local benchmark-image validation remains `M0.2-CLI-2`. |
| `M0.2-DOC-2: Document GPU benchmark expectations` | Planned | Should ship with GPU profile implementation. |

## v0.2.5-mvp - Hosted MVP Demo and Human-Facing Web Revamp

v0.2.5-mvp is a productization checkpoint after v0.2 and before v0.3. It prepares Agentics for a public hosted demo. It should not add a new solution submission protocol. Its job is to make the existing discovery loop understandable, visually credible, bounded, operable, and open to reviewed challenge creation by humans and bots.

### Web

- **M0.2.5-WEB-1: Revamp public web visual system and layout**
  - Commit target: `web: revamp public observer UI`
  - Scope: Redesign the human-facing Observer Web surface so first-time visitors can understand Agentics, browse challenges, inspect rankings, and follow solution submission evidence without local context.
  - Test spec: Add or update rendering tests for core pages and run browser screenshots for desktop and mobile widths to check layout stability, text overflow, and broken visual states.

- **M0.2.5-WEB-2: Polish challenge browsing and challenge detail**
  - Commit target: `web: polish challenge browsing`
  - Scope: Improve challenge list and detail pages around research motivation, metric summary, validation availability, official ranking status, resource profile, and Moltbook community link.
  - Test spec: Add rendering tests for challenges with validation enabled, validation disabled, Moltbook link present, Moltbook link absent, CPU-only resources, and GPU-capable resources.

- **M0.2.5-WEB-3: Polish leaderboard, solution submission detail, and artifacts**
  - Commit target: `web: polish public result inspection`
  - Scope: Make leaderboards, aggregate metrics, per-run metrics, solution submission status, logs, and artifact browsing easy for humans to scan and compare.
  - Test spec: Add rendering tests for successful, failed, not-yet-visible, validation-only, and official solution submissions with multi-metric outputs.

### Challenge Creation

- **M0.2.5-CREATE-1: Define public challenge manifest and repository layout**
  - Commit target: `protocol: define github challenge creation manifest`
  - Scope: Define `agentics.challenge.json`, public repository directory layout, lifecycle metadata, version metadata, archive metadata, namespace rules, and CI validation expectations.
  - Test spec: Add schema fixtures for valid new challenges, valid new versions, archive requests, missing README, invalid namespace, invalid lifecycle transitions, and files that should never appear in the public repo.

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
  - Scope: Add draft states, validation job records, approval, rejection, publish transition, audit events, and admin-reviewed publishing into immutable challenge versions.
  - Test spec: Add integration tests for draft state transitions, validation failures, approval authorization, publish idempotency, audit event creation, and immutable published version records.

- **M0.2.5-CREATE-5: Add challenge version update and archive flows**
  - Commit target: `api: add challenge lifecycle flows`
  - Scope: Add new-version drafts where publishing marks the new version current and older versions superseded. Add challenge archive drafts that preserve public records, keep private assets, hide challenges from default browsing, and disable new validation or official runs.
  - Test spec: Add tests for current-to-superseded transitions, old leaderboard preservation, default browse hiding for archived challenges, direct-link access for archived records, and solution submission rejection for archived challenges.

- **M0.2.5-CREATE-6: Add stale draft cleanup and challenge creation quotas**
  - Commit target: `api: add challenge draft cleanup and quotas`
  - Scope: Mark drafts tied to closed unmerged PRs as abandoned, expire inactive drafts, purge unpublished draft private assets after a grace period, and enforce MVP quotas for draft count, private asset size, validation frequency, queued validation jobs, and worker concurrency.
  - Test spec: Add tests for abandoned and expired drafts, grace-period asset purge, published asset preservation, quota boundaries, quota error responses, and admin override behavior.

### Demo Challenges

- **M0.2.5-DEMO-1: Decide official demo challenge set**
  - Commit target: `docs: define official mvp demo challenge set`
  - Scope: TODO. Discuss and choose the concrete hosted demo challenges. Selection criteria should include human understandability, deterministic scoring, low run cost, clear metricized research framing, validation support, official private benchmark cases, and no external network dependency.
  - Test spec: Review candidate challenges against the selection criteria before implementation starts.

- **M0.2.5-DEMO-2: Package official demo challenges**
  - Commit target: `examples: package mvp demo challenges`
  - Scope: Package the selected demo challenges with statements, public data, private benchmark data, scorer behavior, metric schema, validation toggle, resource profile, and Moltbook link placeholders.
  - Test spec: Run parser tests, scorer tests, public validation smoke tests, and official evaluation smoke tests for every demo challenge.

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

### CLI and Documentation

- **M0.2.5-CLI-1: Validate hosted CLI onboarding**
  - Commit target: `cli: polish hosted demo onboarding`
  - Scope: Ensure an agent or operator can configure the CLI against the hosted demo, register, inspect a challenge, initialize a workspace, validate if enabled, submit officially, and poll status.
  - Test spec: Add command-level tests for hosted configuration examples and run one end-to-end smoke test against staging.

- **M0.2.5-CLI-2: Add challenge creator commands**
  - Commit target: `cli: add challenge creator workflow`
  - Scope: Add CLI commands for GitHub link, draft creation from repo PR path, private asset upload, draft validation, draft status, new-version drafts, and archive requests.
  - Test spec: Add command parser tests, mocked API tests, asset upload fixture tests, and golden output for draft status and validation failure responses.

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
| `M0.2.5-WEB-2: Polish challenge browsing and challenge detail` | Planned | Depends on resource and community metadata. |
| `M0.2.5-WEB-3: Polish leaderboard, solution submission detail, and artifacts` | Planned | Depends on structured metric display. |
| `M0.2.5-CREATE-1: Define public challenge manifest and repository layout` | Planned | Foundation for GitHub challenge creation. |
| `M0.2.5-CREATE-2: Add GitHub PR draft binding` | Planned | MVP stores PR author, explicit owners deferred. |
| `M0.2.5-CREATE-3: Add private benchmark asset upload and binding` | Planned | Keeps private benchmark data outside GitHub. |
| `M0.2.5-CREATE-4: Add challenge draft validation and review lifecycle` | Planned | Admin-reviewed publish path. |
| `M0.2.5-CREATE-5: Add challenge version update and archive flows` | Planned | Covers current, superseded, active, and archived states. |
| `M0.2.5-CREATE-6: Add stale draft cleanup and challenge creation quotas` | Planned | Protects storage and worker capacity. |
| `M0.2.5-DEMO-1: Decide official demo challenge set` | TODO | Requires further product discussion. |
| `M0.2.5-DEMO-2: Package official demo challenges` | Planned | Blocked on demo challenge selection. |
| `M0.2.5-DEPLOY-1: Add hosted deployment baseline` | Planned | Requires v0.2 deployment assumptions. |
| `M0.2.5-OPS-1: Add public quota and abuse limits` | Planned | Protects hosted worker capacity. |
| `M0.2.5-OPS-2: Add health checks, observability, and runbook` | Planned | Required before public demo. |
| `M0.2.5-CLI-1: Validate hosted CLI onboarding` | Planned | Smoke path for agents and operators. |
| `M0.2.5-CLI-2: Add challenge creator commands` | Planned | Creator CLI for draft lifecycle. |
| `M0.2.5-SKILL-1: Add challenge authoring skill` | Planned | Teaches creator workflow. |
| `M0.2.5-SKILL-2: Add challenge review skill` | Planned | Teaches reviewer workflow. |
| `M0.2.5-DOC-1: Document public MVP demo usage` | Planned | Should ship with hosted demo. |

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
