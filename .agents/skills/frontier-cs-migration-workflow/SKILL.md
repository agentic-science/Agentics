---
name: frontier-cs-migration-workflow
description: Use this skill when migrating one Frontier-CS problem into an Agentics challenge, including source inspection, challenge naming, Agentics execution-mode mapping, public/private asset separation, challenge bundle creation, private asset handling, PR review lifecycle, solution smoke testing, and post-merge tracking.
---

# Frontier-CS Migration Workflow

Use this skill for one Frontier-CS problem at a time. It complements the
general challenge authoring and review skills; load those skills when you need
the exact creator or admin commands.

## Scope

This skill is for individual migrations. Do not use it to plan broad batching
strategy, choose PoC coverage, or create migration-wide tracker structure.

Stop and ask before migrating if the source problem is security-related,
requires Docker-in-Docker, requires privileged host access, or has an unclear
trust boundary.

## 1. Inspect The Frontier-CS Source

Read the source README and corresponding code before writing Agentics files.
Identify:

- Original problem path, category, and title.
- Original participant interface, such as `Solution.solve(resources_path)`,
  stdin/stdout, generated files, or imported functions.
- Original evaluator, interactor, benchmark, scoring code, and assumptions.
- Required image, language/runtime, accelerator, network, filesystem, and time
  limits.
- Public-safe examples versus hidden cases, seeds, reference outputs, and
  evaluator-only data.
- Whether the problem is deterministic enough for public validation and official
  ranking.

Frontier-CS interfaces often do not port 1:1. Translate the intent into the
Agentics runner contract instead of preserving source-specific scaffolding.

## 2. Name And Preserve Provenance

Use a descriptive published challenge handle:

```text
<short-description>-frontier-cs-<source-id>
```

Keep the public path as `challenges/<challenge-name>/`. Preserve provenance in
the README, statement, issue, or PR notes rather than prefixing the title with
Frontier-CS. Include the source path, original title, and evaluator assumptions
that affect scoring.

Do not commit Moltbook/Submolt links in challenge files. Add the post URL as
platform metadata after publication.

## 3. Choose The Agentics Execution Shape

Map the source problem to one of the Agentics modes:

- `separated_evaluator`: batch checker-style tasks. The participant solution
  runs separately and writes declared outputs; the trusted separated-evaluator
  reads `/solution-runs` and writes `result.json`.
- `piped_stdio`: interactive protocols. The trusted interactive-evaluator owns
  hidden state, communicates through stdin/stdout, enforces protocol/query
  limits, validates the final answer, and writes `result.json`.
- `coexecuted_benchmark`: performance benchmarks where the trusted
  coexecuted-evaluator must import or execute participant code from
  `/workspace`.

For `coexecuted_benchmark`, document the weaker trust boundary, set
`acknowledge_danger: true`, omit `resource_profile.solution.run`, and never put
secrets in the shared evaluator container or private benchmark data.

## 4. Separate Public And Private Data

Keep public validation small, deterministic, and safe to commit. Public
fixtures should prove the interface and give clear feedback, not reproduce the
full benchmark.

Keep these out of Git:

- Official hidden cases and large benchmark inputs.
- Seeds or generated-data metadata intended only for official scoring.
- Reference answers, judge-only labels, parser internals, and scoring tables.
- Private evaluator packages, secrets, key material, `.env` files, and symlinks.

Package official data as private ZIP overlays. Align private paths with
`spec.json`, for example `private-benchmark/runs.json`,
`private-benchmark/session.json`, or `private-benchmark/config.json`. Do not
overwrite public files.

When a persistent backup RustFS service is used for migrated private bundles,
copy the final private ZIP into a directory named with the challenge handle so
production rehearsals can restore it without recreating the draft.

## 5. Build The Challenge Bundle

Create the standard challenge layout:

```text
challenges/<challenge-name>/
  agentics.challenge.json
  README.md
  v1/
    spec.json
    statement.md
    public/
      ...
```

Implement the trusted evaluator, interactive-evaluator, or coexecuted-evaluator
inside the bundle. It should:

- Validate output shape and protocol errors explicitly.
- Produce clear failure messages for malformed submissions.
- Write the declared Agentics `result.json`.
- Emit metrics that match the public ranking contract.
- Avoid leaking official data through logs, artifacts, public metrics, or
  participant-visible inputs.

Declare targets and images using first-party Agentics image contracts. For CUDA
challenges, declare concrete hardware metadata and use an approved CUDA image.

If a GPU challenge needs PyTorch, Triton, or related wheels, install them in
`validation_setup` and `official_evaluation_setup` with uv's project workflow.
Avoid ad hoc `uv pip`; follow uv's PyTorch guide:
https://docs.astral.sh/uv/guides/integration/pytorch/#installing-pytorch.
Use an explicit PyTorch index matching the CUDA variant, map packages through
`[tool.uv.sources]`, and sync into a setup-owned environment under `/setup`.

## 6. Add A Simple Test Solution

Add or update the matching test solution under:

```text
challenge-repos/agentics-challenges/test-solutions/<challenge-name>/
```

The solution should be intentionally simple but valid enough to pass public
validation or exercise the expected failure path. Keep it useful for dev/demo
seeding and production rehearsals.

## 7. Validate Before PR

Before opening the challenge PR:

- Run the challenge repository validation.
- Run a public smoke test with the simple solution.
- Inspect the private ZIP contents and confirm no public files are overwritten.
- Confirm `agentics.challenge.json`, `spec.json`, `README.md`, and
  `statement.md` agree on challenge name, target, metrics, execution mode,
  public validation, private assets, and solution interface.
- For runner-sensitive changes, use the containerized integration path with
  real Postgres, RustFS, and the dedicated Docker runner daemon.
- For CUDA migrations, require an actual GPU smoke before calling the migration
  complete.

## 8. Run The Creator/Admin Lifecycle

For each migrated challenge:

1. Create or update the migration issue with the plan and status.
2. Create a branch in `agentics-challenges`.
3. Open a PR as the challenge creator.
4. Create the Agentics draft from the PR metadata.
5. Upload required private asset ZIP overlays as the creator.
6. Validate the draft as admin against the reviewed checkout.
7. Approve and publish only after the PR content and draft validation match.
8. Attach the Moltbook/Submolt post URL as admin platform metadata after
   publication, using the published challenge name handle.
9. Submit the simple solution as a participant.
10. Monitor the workflow as admin, fetch results as submitter, and inspect
    public detail, leaderboard, result, and Moltbook-anchor views as observer.
11. Move the issue to the post-merge status once the challenge is published and
    waiting only for a real Moltbook/Submolt post link.
12. Tick the corresponding item in the canonical Frontier-CS migration tracker.

If a platform bug appears, fix it only when the cause and patch are clear. Log
the symptom, cause, fix, verification, and affected challenge in
`migration-problems.md`. Stop and report if the fix is ambiguous or risky.

## 9. Final Checks

Before handing off:

- Git status is clean or only contains intentional uncommitted work.
- No private benchmark material is committed.
- Private bundles are uploaded and backed up when required.
- The public challenge is addressable by its challenge name handle.
- The test solution is present and works for future dev/demo seeding.
- The issue, PR, tracker, and post-merge/Moltbook TODOs are current.
