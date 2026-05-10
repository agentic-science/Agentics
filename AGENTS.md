# Instructions for Agents

## Conversation Requirements

- Always ask for more clarification if you are not sure about the specification of a task. You are encouraged to ask more questions before do the task.
- You are also encouraged to give your honest thoughts and suggestions on a task before doing it.
- Think proactively and provide suggestions/recommendations that might be helpful.

## Documentation

For your information:

- `docs/PRD/en.md` and `docs/PRD/zh.md` are the product requirements documents. They define the product scope, roadmap, roles, evaluation model, and Moltbook integration direction.
- `docs/milestones/en.md` and `docs/milestones/zh.md` are the actionable milestone plans. They must stay bidirectionally synced with the PRD at the feature level.
- `docs/api-json-contract/en.md` and `docs/api-json-contract/zh.md` document the API DTO JSON serialization policy and frontend schema-generation workflow. Response DTOs omit absent optional fields instead of emitting explicit `null`, and frontend Zod schemas are generated from shared Rust DTOs.
- `.agents/skills/agentics-cli-workflow/SKILL.md` is the agent-facing workflow guide for using the Agentics CLI to solve challenges. Keep it aligned with CLI command changes and README examples.
- `docs/versions/v0.0/` documents the implemented v0.0 baseline:
  - `README.md`: v0.0 product baseline.
  - `api.md`: v0.0 API contract and curl examples.
  - `challenge-bundles.md`: v0.0 bundle authoring contract.
  - `runner.md`: v0.0 worker and Docker runner behavior.
  - `observer-web.md`: v0.0 public web surface.
  - `release-checklist.md`: v0.0 local release and smoke-test checklist.
- `docs/versions/v0.1/challenge-authoring/en.md` and `docs/versions/v0.1/challenge-authoring/zh.md` document the v0.1 challenge authoring model, including validation, official evaluation, metric schema, ranking rules, and Moltbook metadata.
- `docs/versions/v0.1/admin-web/en.md` and `docs/versions/v0.1/admin-web/zh.md` document the v0.1 admin web console surface, including authentication, challenge publishing, solution submission operations, and worker heartbeat inspection.
- `docs/versions/v0.2/zip-project-protocol/en.md` and `docs/versions/v0.2/zip-project-protocol/zh.md` document the v0.2 `zip_project` solution manifest schema.
- `docs/versions/v0.2/benchmark-targets/en.md` and `docs/versions/v0.2/benchmark-targets/zh.md` document the v0.2 benchmark target schema, target-specific submission APIs, CLI behavior, worker behavior, and leaderboard behavior.
- `docs/versions/v0.2/cpu-base-image/en.md` and `docs/versions/v0.2/cpu-base-image/zh.md` document the first-party Agentics CPU base image definition, included toolchains, local build commands, and participant setup guidance.
- `docs/versions/v0.2.5/challenge-creation/en.md` and `docs/versions/v0.2.5/challenge-creation/zh.md` document the GitHub-backed challenge creation draft workflow, public manifest, repository layout, private asset upload boundary, and admin review lifecycle.
- `docs/versions/v0.2.5/deployment/en.md` and `docs/versions/v0.2.5/deployment/zh.md` document the MVP deployment baseline, Mac-local rehearsal assumptions, startup order, storage, backup, rollback, and DGX Spark follow-up boundary.
- `docs/versions/v0.2.5/operations/en.md` and `docs/versions/v0.2.5/operations/zh.md` document MVP health checks, quota policy, operational checks, logs, failure handling, and backup practices.
- `docs/versions/v0.2.5/hosted-cli-onboarding/en.md` and `docs/versions/v0.2.5/hosted-cli-onboarding/zh.md` document the hosted CLI smoke path for registration, challenge inspection, workspace initialization, validation, official submission, and polling.
- `docs/versions/v0.2.5/public-mvp-usage/en.md` and `docs/versions/v0.2.5/public-mvp-usage/zh.md` document concise public MVP usage for humans, agents, challenge creators, reviewers, and operators.
- `.agents/skills/challenge-authoring-workflow/SKILL.md` is the creator-facing workflow guide for preparing GitHub-backed challenge proposals and uploading private asset ZIP overlays.
- `.agents/skills/challenge-review-workflow/SKILL.md` is the admin/reviewer workflow guide for validating, approving, publishing, archiving, and cleaning up challenge drafts.

### Requirements

- If they have multi-lingual versions, always update all versions when one version is updated.
- When creating a new document, create a folder `<document_name>` in which you should create at least English and Chinese versions.
- When changing planned product scope, update both PRDs and both milestone documents in the same change set.
- When changing implemented behavior for a released version, update the matching `docs/versions/<version>/` documents and then update milestones if the implementation status changes.
- When changing Rust response DTOs consumed by the web frontend, derive `schemars::JsonSchema`, preserve the optional-field JSON contract, run `bun run generate:schemas` in `frontends/web/`, and keep `frontends/web/src/lib/schemas.ts` as a stable re-export facade.

## Coding Requirements

- Always prioritize code quality and avoid bad SWE practices
- Always group changes into logical commits and never commit changes of different features and purposes in one commit
- Do not commit changes automatically unless told (e.g., "do this and commit the changes").
- Don't rebuild the wheels: if there's a commonly used package/library for a feature or sub-feature, do not implement the functionalities yourself, unless the user explicitly ask you to rewrite or avoid external packages. If unsure, always ask for clarification.
- Keep track of file sizes. If a file has more then 1200 lines of code, propose a refactor to the user.
- Before v0.2.5-mvp, DO NOT consider any internal or external API compatibilities. If a new feature or a refactor needs to reasonably discard existing code, just do it. For example, if a backend change for a good reason breaks the APIs for the frontend, DO NOT add compatibility shims/layers/aliases. Instead, just fix the frontend.
- When fixing lint findings, preserving behavior is mandatory. In particular, replacing `unwrap`, `expect`, indexing, or other panic-prone code must not silently continue, skip work, substitute defaults, or weaken limits when the previous code would fail fast. Prefer eliminating impossible states by construction, for example by building the correctly typed value directly instead of constructing a generic value and then asserting its shape. If a failure can really happen at runtime, handle it with a clear domain error. If the old code represented an internal invariant that cannot be eliminated, convert it to a precise internal error, not a vague message such as "static value must be an object".

### Technical Requirements

- Always assume `uv` for managing Python environments and `bun` for JS/TS environments, unless you are explicitly told to use other tools.
- Only run lint, check and format tools (e.g., `cargo clippy`, `cargo check`, `cargo fmt`, `bunx biome`, `ruff`) before committing, not during iteration. Skip these when fixing bugs/issues to accelerate iteration speed.
- NO unsafe fixes should be applied even if a linter provides them. You should reason about the code to be fixed and come up appropriate fixes.
