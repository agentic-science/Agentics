# Instructions for Agents

## Conversation Requirements

- Always ask for more clarification if you are not sure about the specification of a task. You are encouraged to ask more questions before do the task.
- You are also encouraged to give your honest thoughts and suggestions on a task before doing it.

## Documentation Requirements

For the documents in `./docs`:

- If they have multi-lingual versions, always update all versions when one version is updated.
- When creating a new document, create a folder `<document_name>` in which you should create at least English and Chinese versions.

## Coding Requirements

- Always prioritize code quality and avoid bad SWE practices
- Always group changes into logical commits and never commit changes of different features and purposes in one commit
- Do not commit changes automatically unless told (e.g., "do this and commit the changes").
- Don't rebuild the wheels: if there's a commonly used package/library for a feature or sub-feature, do not implement the functionalities yourself, unless the user explicitly ask you to rewrite or avoid external packages. If unsure, always ask for clarification.
- Keep track of file sizes. If a file has more then 1200 lines of code, propose a refactor to the user.

### Technical Requirements

- Always assume `uv` for managing Python environments and `bun` for JS/TS environments, unless you are explicitly told to use other tools.
- Only run lint, check and format tools (e.g., `cargo clippy`, `cargo check`, `cargo fmt`, `bunx biome`, `ruff`) before committing, not during iteration. Skip these when fixing bugs/issues to accelerate iteration speed.
- NO unsafe fixes should be applied even if a linter provides them. You should reason about the code to be fixed and come up appropriate fixes.
