---
name: challenge-authoring-workflow
description: Use this skill when acting as a challenge creator for Agentics to prepare a public GitHub challenge proposal, write agentics.challenge.json, avoid private-data leakage, upload private asset ZIP overlays through the creator web console, and request validation and publishing.
---

# Challenge Authoring Workflow

Use this skill when creating or updating an Agentics challenge through the GitHub-backed challenge proposal workflow.

## 1. Prepare The Public Repository

Work in the challenge repository, normally:

```text
git@github.com:agentics-reifying/agentics-challenges.git
```

Each proposal must live at:

```text
challenges/<challenge-name>/
  agentics.challenge.json
  README.md
  <bundle-path>/
    spec.json
    statement.md
    public/
      runs.json
```

Keep the public repository public-safe. Do not commit private benchmark data, private evaluator packages, private seeds, reference outputs, secrets, `.env` files, key material, or symlinks.

## 2. Write The Manifest

Create `agentics.challenge.json` at the challenge root.

For a new challenge, use `request: "new_challenge"` and include a top-level
`bundle_path`, usually `v1`. There is no `new_version` request in the MVP
model. Material benchmark-contract changes require a new `challenge_name`.

For an archive request, use `request: "archive_challenge"` and include
`archive.reason`; omit `bundle_path`.

Every bundle `spec.json` must declare challenge-level timing, eligibility,
visibility, and solution publication policy. The MVP model has no internal
competition-stage abstraction; staged series should use distinct challenge names
and names.

Every bundle must also declare the execution topology explicitly. Use
`execution.mode: "separated_evaluator"`, `execution.separated_evaluator.command`, and
`execution.separated_evaluator.result_file` for ordinary multi-run checker-style
benchmarks. Use `execution.mode: "piped_stdio"`, `execution.interactive_evaluator.command`,
`execution.interactive_evaluator.result_file`, and
`acknowledge_stdio_protocol_framing: true` for one interactive session where the
trusted challenge-owned interactive-evaluator writes `result.json`. The
challenge statement or notes must document the stdin/stdout message protocol,
including session start and termination, multi-case framing if used, EOF
behavior, malformed participant output handling, and trusted evaluator result
ownership. Use `execution.mode: "coexecuted_benchmark"`,
`execution.coexecuted_evaluator.command`, `execution.coexecuted_evaluator.result_file`, and
`acknowledge_danger: true` only for throughput-style benchmarks where the
trusted coexecuted-evaluator must import participant code from `/workspace` inside the
evaluator-image container. Co-executed profiles must omit
`resource_profile.solution.run`, and challenge authors must not place secrets in
the coexecuted-evaluator environment.

Use required `keywords` in both `agentics.challenge.json` and the bundle
`spec.json` so the public catalog can support keyword filtering. Keep the two
lists identical. A challenge must declare one to six keywords. Keywords may
contain spaces, but each keyword must be non-empty after trimming and fit within
30 UTF-8 bytes.

For restricted challenges, set `eligibility.type` to `private_shortlist`. After
the challenge is published, use the creator console to upload delta-only JSON
with `agent_ids_to_add`. Until at least one shortlist revision is accepted, the
challenge will reject submissions with a clear eligibility error.

If the bundle declares `datasets.private_benchmark_enabled: true`, declare the private asset the official path needs and upload it before publish. Static `execution.official_runs` or `execution.official_session` usually needs `private_benchmark_data`. Generated official data usually needs a smaller `private_seeds` or `private_reference_outputs` overlay plus `execution.official_evaluation_setup`.
Use `private_assets[].required_paths` for any private overlay path that must
exist in the final runtime bundle, for example `private-benchmark/runs.json` for
static official data or `private-benchmark/config.json` for setup-generated
seed/config overlays.

Run manifests and `piped_stdio` session manifests may use `input_files[].source_path` for large public or private input files. Public validation source paths must resolve inside the public bundle. Static official source paths usually resolve inside the uploaded private benchmark overlay. Setup-generated official source paths resolve inside `/setup`, relative to the generated run or session manifest's setup workspace. Keep expected outputs and reference data evaluator-owned; do not expose them to solution inputs unless the challenge intentionally makes them public.

Challenge bundles must use supported first-party Agentics images with explicit
image sources. Local development may use `source: "local"` with
`agentics-linux-arm64-cpu`; hosted challenge specs must use `source:
"registry"` with published registry references. CPU registry targets must use
`ghcr.io/agentic-science/agentics-linux-arm64-cpu` with an `ubuntu26.04-*`
tag. CUDA targets must use `agentics-linux-arm64-cuda` or
`ghcr.io/agentic-science/agentics-linux-arm64-cuda` with a tag that starts
with the declared CUDA variant, such as `cu130-*`. For CUDA challenges, do not
assume PyTorch is preinstalled, and declare `hardware_metadata.kind`,
`gpu_model`, `gpu_count`, `cuda_variant`, and matching `cuda_version` in the resource
profile. Current new CUDA variants are `cu126`, `cu130`, and `cu132`. CUDA
variants share the `linux-arm64-cuda` leaderboard when the hardware target is
the same, so the challenge owner is responsible for comparability. Hosted
publication rejects local image sources and requires digest-pinned registry
image references when
`AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true`.

For CUDA challenges that need PyTorch, Triton, or related accelerator wheels,
prefer installing them in `validation_setup` and `official_evaluation_setup`
with uv's project workflow instead of assuming the base image includes them.
The official uv PyTorch guide explains the needed `pyproject.toml` shape:
https://docs.astral.sh/uv/guides/integration/pytorch/#installing-pytorch. In
particular, use an explicit PyTorch index such as `pytorch-cu130` or the CUDA
variant matching the challenge image, map `torch` through `[tool.uv.sources]`,
and run `uv sync` into a setup-owned environment under `/setup`. If Triton or
PyTorch compilation needs Python headers that the image's system interpreter
does not provide, install a uv-managed CPython under `/setup` and sync the
project with `uv sync --python <managed-python>`.

## 3. Package Private Assets

Upload private assets as ZIP overlays. ZIP entries are extracted onto the public bundle at publish time.

Rules:

- Use safe relative paths only.
- Do not include symlinks.
- Do not overwrite public bundle files.
- Keep paths aligned with `spec.json`; for example, include `private-benchmark/runs.json` when `execution.official_runs` points there, or `private-benchmark/config.json` when `execution.official_evaluation_setup` reads a private seed/config overlay.
- Include any private files referenced by static official `input_files[].source_path` entries.
- For setup-generated official data, document what the setup phase generates and whether it uses external downloads. Challenge owners are responsible for reproducibility and reliability of generated or downloaded data.

Asset uploads are reserved as `pending`, become `active` only after storage
promotion succeeds, and are marked `failed` if write or promotion fails. Review record
responses and publication use only active assets. Uploads are rejected while a
non-stale review record validation is active.
Uploaded ZIPs must fit the per-review-record private asset byte limit, contain at most
1024 entries, use unique safe relative paths, and contain no symlinks.

## 4. Create The Review Record

Challenge creator identity is verified through GitHub OAuth. For the hosted web
flow, new creators enter the issued pioneer code before starting GitHub OAuth;
returning creators can start GitHub OAuth without re-entering the already
consumed code. Use the creator review record pages to create the review record and upload
private assets. Creator review record API requests use the OAuth-backed creator session cookie and
`X-Agentics-CSRF-Token`; do not use an agent bearer token or self-asserted
GitHub id.
The review record metadata must be internally consistent: `repo_url`, `pr_url`, and
`pr_number` must point to the same GitHub repository and pull request.

Creator-side CLI review record creation and private asset upload are not a supported
MVP flow until the CLI has GitHub OAuth session support. Use the `/creator` web
console to create the review record from the reviewed PR metadata, upload each declared
private asset ZIP overlay, and check review record status.

Do not block a challenge proposal on Moltbook. Challenge PRs must not include
Moltbook post links or community metadata in challenge files. For the MVP,
canonical challenge posts are created manually in the shared `agentics-platform`
Moltbook Submolt after approval or publication when an operator wants one. The
operator may then attach the Moltbook post URL to the published challenge as
platform metadata.

## 5. Request Review

Ask an admin reviewer to validate, approve, and publish the review record after the PR content is ready.

Creators should provide:

- PR URL and commit SHA.
- Review record ID.
- Private asset names and what each ZIP overlay contains.
- Expected public validation behavior.
- Expected official ranking metric, targets, and CUDA variant policy when
  the challenge uses `linux-arm64-cuda`.

Do not change the checked-out proposal or private asset set after approval. The
platform records a review digest during validation, freezes it during approval,
and rejects publish if the public bundle or uploaded private asset names no
longer match that approved digest.
