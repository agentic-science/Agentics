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
- `docs/README.md` is the documentation index.
- `docs/contribute-code/en.md` and `docs/contribute-code/zh.md` are the role-facing setup and workflow guides for code contributors.
- `docs/contribute-challenges/en.md` and `docs/contribute-challenges/zh.md` are the role-facing guides for challenge creators and owners.
- `docs/review-challenges/en.md` and `docs/review-challenges/zh.md` are the role-facing guides for challenge reviewers.
- `docs/operate-platform/en.md` and `docs/operate-platform/zh.md` are the role-facing guides for platform operators.
- `docs/solution-protocol/en.md` and `docs/solution-protocol/zh.md` document the current `zip_project` solution manifest and runner contract.
- `docs/benchmark-targets/en.md` and `docs/benchmark-targets/zh.md` document benchmark target schema, target-specific submission APIs, CLI behavior, worker behavior, and leaderboard behavior.
- `docs/deployment/en.md` and `docs/deployment/zh.md` document the MVP Mac-local deployment rehearsal, startup order, storage, backup, rollback, and DGX hosted profile handoff.
- `docs/dgx-spark/en.md` and `docs/dgx-spark/zh.md` document the DGX Spark hosted profile, host inventory summary, storage quotas, systemd startup, checks, and smoke evidence.
- `docs/operations/en.md` and `docs/operations/zh.md` document MVP health checks, quota policy, operational checks, logs, failure handling, and backup practices.
- `docs/ports-and-paths/en.md` and `docs/ports-and-paths/zh.md` document runtime ports, filesystem paths, and MVP target support.
- `docs/visual-identity-system/en.md` and `docs/visual-identity-system/zh.md` are the UI contribution reference for visual style, layout, and frontend polish.
- `docs/new-rust-features-apis/en.md` is the agent-facing Rust modernization reference used by full code review.
- `skills/agentics-cli-workflow/SKILL.md` is the agent-facing workflow guide for using the Agentics CLI to solve challenges. Keep it aligned with CLI command changes and README examples.
- `skills/challenge-authoring-workflow/SKILL.md` is the creator-facing workflow guide for preparing GitHub-backed challenge proposals and uploading private asset ZIP overlays.
- `.agents/skills/challenge-review-workflow/SKILL.md` is the admin/reviewer workflow guide for validating, approving, publishing, archiving, and cleaning up challenge drafts.

### Requirements

- If they have multi-lingual versions, always update all versions when one version is updated.
- When creating a new document, create a folder `<document_name>` in which you should create at least English and Chinese versions.
- When changing planned product scope, update both PRDs and both milestone documents in the same change set.
- When changing implemented behavior, update the matching current docs and then update milestones if the implementation status changes.
- When changing Rust response DTOs consumed by the web frontend, derive `schemars::JsonSchema`, preserve the optional-field JSON contract, run `bun run generate:schemas` in `frontends/web/`, and keep `frontends/web/src/lib/schemas.ts` as a stable re-export facade.

## Coding Requirements

- Always prioritize code quality and avoid bad SWE practices
- Always group changes into logical commits and never commit changes of different features and purposes in one commit
- Do not commit changes automatically unless told (e.g., "do this and commit the changes").
- Don't rebuild the wheels: if there's a commonly used package/library for a feature or sub-feature, do not implement the functionalities yourself, unless the user explicitly ask you to rewrite or avoid external packages. If unsure, always ask for clarification.
- Do not write trivial or low-value tests. Tests must protect meaningful behavior, contracts, regressions, security properties, or user-visible workflows. Avoid tests that only restate constants, assert freshly constructed struct fields, check library serialization mechanics, or verify static labels without exercising behavior.
- Keep track of file sizes. If a file has more then 1200 lines of code, propose a refactor to the user.
- Before the public MVP release, DO NOT consider any internal or external API compatibilities. If a new feature or a refactor needs to reasonably discard existing code, just do it. For example, if a backend change for a good reason breaks the APIs for the frontend, DO NOT add compatibility shims/layers/aliases. Instead, just fix the frontend.
- When fixing lint findings, preserving behavior is mandatory. In particular, replacing `unwrap`, `expect`, indexing, or other panic-prone code must not silently continue, skip work, substitute defaults, or weaken limits when the previous code would fail fast. Prefer eliminating impossible states by construction, for example by building the correctly typed value directly instead of constructing a generic value and then asserting its shape. If a failure can really happen at runtime, handle it with a clear domain error. If the old code represented an internal invariant that cannot be eliminated, convert it to a precise internal error, not a vague message such as "static value must be an object".

### Technical Requirements

- Always assume `uv` for managing Python environments and `bun` for JS/TS environments, unless you are explicitly told to use other tools.
- Only run lint, check and format tools (e.g., `cargo clippy`, `cargo check`, `cargo fmt`, `bunx biome`, `ruff`) before committing, not during iteration. Skip these when fixing bugs/issues to accelerate iteration speed.
- NO unsafe fixes should be applied even if a linter provides them. You should reason about the code to be fixed and come up appropriate fixes.
