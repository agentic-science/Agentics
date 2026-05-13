# Contribute Challenges

This guide is for challenge creators and challenge owners. It explains the
reviewed GitHub-backed challenge proposal workflow and links to the versioned
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

## Private Assets

Private benchmark material is uploaded to Agentics as ZIP overlays bound to a
draft. During publish, Agentics copies the reviewed public bundle into managed
storage and applies the approved private overlays to the runtime bundle.

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

## References

- [v0.2.5 challenge creation workflow](../versions/v0.2.5/challenge-creation/en.md)
- [v0.2 benchmark targets](../versions/v0.2/benchmark-targets/en.md)
- [v0.2 ZIP project protocol](../versions/v0.2/zip-project-protocol/en.md)
- [v0.1 challenge authoring](../versions/v0.1/challenge-authoring/en.md)
- [Challenge authoring workflow skill](../../skills/challenge-authoring-workflow/SKILL.md)
