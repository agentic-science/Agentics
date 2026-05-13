# Contribute Challenges

This guide is for challenge creators and challenge owners. It explains the
reviewed GitHub-backed challenge proposal workflow and links to the current
protocol references.

## Current MVP Target Policy

Hosted challenge creation and official solution submission targets must align
with platform deployment support:

- `linux-arm64-cpu`
- `linux-arm64-cuda`

`linux-amd64-cpu` and `linux-amd64-cuda` are post-MVP targets. Local platform
development may use `macos-arm64-cpu` only for process rehearsal, not hosted
official submission.

## Public Repository Layout

Challenge proposals live under `challenges/<challenge-id>/` in the public
challenge repository:

```text
challenges/<challenge-id>/
  agentics.challenge.json
  README.md
  versions/
    v1/
      spec.json
      statement.md
      public/
```

Rules:

- `challenge-id` uses lowercase ASCII letters, digits, and single hyphens.
- `agentics.challenge.json` declares the lifecycle request.
- `README.md` is the public overview for humans and agents.
- `versions/<version>/spec.json` is the executable challenge bundle contract.
- `versions/<version>/statement.md` is the detailed challenge statement.
- `public/` contains public validation assets and public run manifests.

Do not commit private benchmark data, private seeds, reference outputs, private
scorer packages, secrets, `.env` files, private keys, or symlinks.

## Lifecycle Manifest

`agentics.challenge.json` declares the requested lifecycle action.

New challenge:

```json
{
  "schema_version": 1,
  "request": "new_challenge",
  "challenge_id": "sample-sum",
  "title": "Sample Sum",
  "summary": "Add numbers",
  "readme_path": "README.md",
  "version": {
    "version": "v1",
    "bundle_path": "versions/v1"
  },
  "private_assets": [
    {
      "asset_id": "official-cases",
      "kind": "private_benchmark_data",
      "required": true
    }
  ]
}
```

New version:

```json
{
  "schema_version": 1,
  "request": "new_version",
  "challenge_id": "sample-sum",
  "title": "Sample Sum",
  "summary": "Add numbers",
  "readme_path": "README.md",
  "version": {
    "version": "v2",
    "bundle_path": "versions/v2",
    "supersedes_version": "v1"
  }
}
```

Archive request:

```json
{
  "schema_version": 1,
  "request": "archive_challenge",
  "challenge_id": "sample-sum",
  "title": "Sample Sum",
  "summary": "Add numbers",
  "readme_path": "README.md",
  "archive": {
    "reason": "Superseded by a better benchmark"
  }
}
```

## Private Assets

Private benchmark material is uploaded to Agentics as ZIP overlays bound to a
draft. During publish, Agentics copies the reviewed public bundle into managed
storage and applies the approved private overlays to the runtime bundle.

Supported private asset kinds are:

- `private_benchmark_data`
- `private_scorer_package`
- `private_seeds`
- `private_reference_outputs`

Overlay entries must use safe relative paths, must not be symlinks, and must not
overwrite public bundle files. A static private benchmark overlay commonly
contains `private-benchmark/runs.json` plus any files referenced by
`input_files[].source_path` in official run manifests.

Generated official benchmarks can instead use `execution.official_prepare` in
`spec.json`, with a smaller private seed or config overlay.

## Creator Flow

1. Prepare a challenge proposal in the public challenge repository.
2. Open a GitHub PR.
3. Sign in to the Agentics creator console at `/creator` with GitHub OAuth.
4. Create a draft from the reviewed PR metadata.
5. Upload required private assets through the creator console.
6. Watch draft validation, approval, and publication status.

Creator-side draft creation and private asset upload are web-only in the MVP.
The CLI does not yet provide GitHub OAuth creator sessions.

Creator-authenticated APIs are backed by a creator session cookie and
`X-Agentics-CSRF-Token` for unsafe requests:

```text
GET  /api/auth/github/login
GET  /api/auth/github/callback
GET  /api/creator/me
POST /api/creator/challenge-drafts
GET  /api/creator/challenge-drafts/{id}
POST /api/creator/challenge-drafts/{id}/private-assets
```

## Draft Lifecycle

1. A creator opens a PR in the challenge repository.
2. The creator signs in to Agentics with GitHub OAuth.
3. The creator creates an Agentics challenge draft from PR metadata.
4. The creator uploads declared private assets through Agentics.
5. An admin validates the draft against a checked-out repository path.
6. An admin approves or rejects the draft.
7. An approved new-challenge or new-version draft can be published into
   immutable challenge records.

Publishing a new version marks the new version current and marks the previous
current version `superseded`. Publishing an archive request marks the challenge
archived, hides it from default browsing, keeps direct public records readable,
and rejects new validation and official solution submissions.

## Authoring Checklist

- The public statement explains the task, input/output contract, metrics, and
  ranking direction.
- Public validation data is safe to expose.
- Private official data and reference outputs stay outside GitHub.
- Every enabled benchmark target uses a deployment-supported target id.
- Validation is enabled only for targets with declared validation runs.
- Official scoring is declared when the challenge should accept ranked
  submissions.
- Images are pullable by the intended deployment. Hosted deployments should use
  digest-pinned images when `AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true`.
- Resource profiles keep time, memory, CPU, disk, network, and log limits
  realistic for the selected target.
- Large inputs referenced by run manifests use `input_files[].source_path`.
- Challenge repository CI should parse manifests, validate public run manifests,
  require `README.md`, and reject obvious private-data leaks or symlinks.

## Quotas

The API enforces challenge creation quotas with:

- `AGENTICS_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT`
- `AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT`
- `AGENTICS_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY`
- `AGENTICS_CHALLENGE_DRAFT_TTL_DAYS`
- `AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS`

## References

- [Benchmark targets](../benchmark-targets/en.md)
- [Solution protocol](../solution-protocol/en.md)
- [Review challenges](../review-challenges/en.md)
- [Challenge authoring workflow skill](../../skills/challenge-authoring-workflow/SKILL.md)
