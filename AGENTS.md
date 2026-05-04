# Instructions for Agents

## Conversation Requirements

- Always ask for more clarification if you are not sure about the specification of a task. You are encouraged to ask more questions before do the task.
- You are also encouraged to give your honest thoughts and suggestions on a task before doing it.
- Think proactively and provide suggestions/recommendations that might be helpful.

## Documentation

For your information:

- `docs/PRD/en.md` and `docs/PRD/zh.md` are the product requirements documents. They define the product scope, roadmap, roles, evaluation model, and Moltbook integration direction.
- `docs/milestones/en.md` and `docs/milestones/zh.md` are the actionable milestone plans. They must stay bidirectionally synced with the PRD at the feature level.
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

### Requirements

- If they have multi-lingual versions, always update all versions when one version is updated.
- When creating a new document, create a folder `<document_name>` in which you should create at least English and Chinese versions.
- When changing planned product scope, update both PRDs and both milestone documents in the same change set.
- When changing implemented behavior for a released version, update the matching `docs/versions/<version>/` documents and then update milestones if the implementation status changes.

## Coding Requirements

- Always prioritize code quality and avoid bad SWE practices
- Always group changes into logical commits and never commit changes of different features and purposes in one commit
- Do not commit changes automatically unless told (e.g., "do this and commit the changes").
- Don't rebuild the wheels: if there's a commonly used package/library for a feature or sub-feature, do not implement the functionalities yourself, unless the user explicitly ask you to rewrite or avoid external packages. If unsure, always ask for clarification.
- Keep track of file sizes. If a file has more then 1200 lines of code, propose a refactor to the user.
- Before v0.2.5-mvp, DO NOT consider any internal or external API compatibilities. If a new feature or a refactor needs to reasonably discard existing code, just do it. For example, if a backend change for a good reason breaks the APIs for the frontend, DO NOT add compatibility shims/layers/aliases. Instead, just fix the frontend.

### Technical Requirements

- Always assume `uv` for managing Python environments and `bun` for JS/TS environments, unless you are explicitly told to use other tools.
- Only run lint, check and format tools (e.g., `cargo clippy`, `cargo check`, `cargo fmt`, `bunx biome`, `ruff`) before committing, not during iteration. Skip these when fixing bugs/issues to accelerate iteration speed.
- NO unsafe fixes should be applied even if a linter provides them. You should reason about the code to be fixed and come up appropriate fixes.
