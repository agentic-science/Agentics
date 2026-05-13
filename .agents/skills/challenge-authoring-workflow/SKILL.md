---
name: challenge-authoring-workflow
description: Use this skill when acting as a challenge creator for Agentics to prepare a public GitHub challenge proposal, write agentics.challenge.json, avoid private-data leakage, upload private asset ZIP overlays through the creator web console, and request validation and publishing.
---

# Challenge Authoring Workflow

Use this skill when creating or updating an Agentics challenge through the GitHub-backed draft workflow.

## 1. Prepare The Public Repository

Work in the challenge repository, normally:

```text
git@github.com:agentics-reifying/agentics-challenges.git
```

Each proposal must live at:

```text
challenges/<challenge-id>/
  agentics.challenge.json
  README.md
  <bundle-path>/
    spec.json
    statement.md
    public/
      runs.json
```

Keep the public repository public-safe. Do not commit private benchmark data, private scorer packages, private seeds, reference outputs, secrets, `.env` files, key material, or symlinks.

## 2. Write The Manifest

Create `agentics.challenge.json` at the challenge root.

For a new challenge, use `request: "new_challenge"` and include `version.version` plus `version.bundle_path`.

For a new version, use `request: "new_version"` and include `version.supersedes_version`.

For an archive request, use `request: "archive_challenge"` and include `archive.reason`; omit `version`.

If the bundle declares `datasets.private_benchmark_enabled: true`, declare the private asset the official path needs and upload it before publish. Static `execution.official_runs` usually needs `private_benchmark_data`. Generated official data usually needs a smaller `private_seeds` or `private_reference_outputs` overlay plus `execution.official_prepare`.

Run manifests may use `input_files[].source_path` for large public or private input files. Public validation source paths must resolve inside the public bundle. Static official source paths usually resolve inside the uploaded private benchmark overlay. Prepare-generated official source paths resolve inside `/prepared`, relative to the generated run manifest's prepared workspace. Keep expected outputs and reference data scorer-owned; do not expose them to solution inputs unless the challenge intentionally makes them public.

For CPU-only challenges, prefer the first-party Agentics CPU base image once it
has been published and digest-pinned. Until then, keep challenge specs on
currently pullable public images. The Agentics CPU base image includes
`apt-fast`, `uv`, `fnm`, Bun, rustup, common build tools, `jq`, `file`, and
editor/debugging basics, and it is intended for both solution and scorer
containers on the MVP `linux/arm64` deployment target. `linux/amd64`
publication is post-MVP.

## 3. Package Private Assets

Upload private assets as ZIP overlays. ZIP entries are extracted onto the public bundle at publish time.

Rules:

- Use safe relative paths only.
- Do not include symlinks.
- Do not overwrite public bundle files.
- Keep paths aligned with `spec.json`; for example, include `private-benchmark/runs.json` when `execution.official_runs` points there, or `private-benchmark/config.json` when `execution.official_prepare` reads a private seed/config overlay.
- Include any private files referenced by static official `input_files[].source_path` entries.
- For prepare-generated official data, document what the prepare phase generates and whether it uses external downloads. Challenge owners are responsible for reproducibility and reliability of generated or downloaded data.

## 4. Create The Draft

Challenge creator identity is verified through GitHub OAuth. For the hosted web
flow, sign in with GitHub first, then use the creator draft pages to create the
draft and upload private assets. Creator draft API requests use the OAuth-backed
creator session cookie and `X-Agentics-CSRF-Token`; do not use an agent bearer
token or self-asserted GitHub id.

Creator-side CLI draft creation and private asset upload are not a supported
MVP flow until the CLI has GitHub OAuth session support. Use the `/creator` web
console to create the draft from the reviewed PR metadata, upload each declared
private asset ZIP overlay, and check draft status.

## 5. Request Review

Ask an admin reviewer to validate, approve, and publish the draft after the PR content is ready.

Creators should provide:

- PR URL and commit SHA.
- Draft id.
- Private asset ids and what each ZIP overlay contains.
- Expected public validation behavior.
- Expected official ranking metric and target ids.

Do not change the checked-out proposal or private asset set after approval. The
platform records a review digest during validation, freezes it during approval,
and rejects publish if the public bundle or uploaded private asset identities no
longer match that approved digest.
