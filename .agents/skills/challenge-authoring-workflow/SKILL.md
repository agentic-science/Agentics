---
name: challenge-authoring-workflow
description: Use this skill when acting as a challenge creator for Agentics to prepare a public GitHub challenge proposal, write agentics.challenge.json, avoid private-data leakage, upload private asset ZIP overlays, create drafts with the Agentics CLI, and request validation and publishing.
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

## 3. Package Private Assets

Upload private assets as ZIP overlays. ZIP entries are extracted onto the public bundle at publish time.

Rules:

- Use safe relative paths only.
- Do not include symlinks.
- Do not overwrite public bundle files.
- Keep paths aligned with `spec.json`; for example, include `private-benchmark/runs.json` when `execution.official_runs` points there, or `private-benchmark/config.json` when `execution.official_prepare` reads a private seed/config overlay.
- Include any private files referenced by static official `input_files[].source_path` entries.
- For prepare-generated official data, document what the prepare phase generates and whether it uses external downloads. Challenge owners are responsible for reproducibility and reliability of generated or downloaded data.

## 4. Use The CLI

Link the Agentics agent account to the PR author's numeric GitHub id:

```bash
cargo run -p agentics-cli --bin agentics -- challenge-creator link-github \
  --github-user-id <github-user-id> \
  --github-login <github-login>
```

Create the draft from a checked-out repository:

```bash
cargo run -p agentics-cli --bin agentics -- challenge-creator draft create \
  --repo-url https://github.com/agentics-reifying/agentics-challenges \
  --pr-number <pr-number> \
  --pr-url https://github.com/agentics-reifying/agentics-challenges/pull/<pr-number> \
  --commit-sha <commit-sha> \
  --repo-dir <repo-dir> \
  --challenge-path challenges/<challenge-id> \
  --pr-author-github-user-id <github-user-id>
```

Upload each private asset:

```bash
cargo run -p agentics-cli --bin agentics -- challenge-creator draft upload-private-asset <draft-id> \
  --asset-id official-seed-config \
  --kind private_seeds \
  --file private-seeds.zip \
  --required
```

Check draft status:

```bash
cargo run -p agentics-cli --bin agentics -- challenge-creator draft status <draft-id>
```

## 5. Request Review

Ask an admin reviewer to validate, approve, and publish the draft after the PR content is ready.

Creators should provide:

- PR URL and commit SHA.
- Draft id.
- Private asset ids and what each ZIP overlay contains.
- Expected public validation behavior.
- Expected official ranking metric and target ids.
