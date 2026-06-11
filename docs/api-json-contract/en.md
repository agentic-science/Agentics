# API JSON Contract

This document defines the JSON serialization policy for Agentics API DTOs.

## Response DTOs

Agentics response DTOs omit absent optional fields. Rust response structs should use:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub field: Option<T>,
```

The corresponding TypeScript/Zod shape is:

```ts
field?: T
```

Response DTOs should not emit explicit `null` for absent values.
This keeps the wire format compact, matches the relaxed JSON contract, and reduces ambiguity in generated schemas.

## Error Responses

All API handlers and extractors return the same nested error envelope:

```json
{
  "error": {
    "code": "bad_request",
    "message": "display_name must not be empty",
    "details": [
      { "field": "display_name", "message": "must not be empty" }
    ]
  }
}
```

`error.code` is the stable branching contract and is one of `bad_request`, `unauthorized`, `forbidden`, `not_found`, `conflict`, `too_many_requests`, `payload_too_large`, or `internal_error`.
`error.message` is safe for display but not stable for branching.
`error.details` is omitted when empty and is used only for structured request or field validation.
Internal failures always return `internal_error` with `internal server error`; sources and context stay in logs.

## Exceptions

Use explicit `null` only when the API must distinguish a field that is present but intentionally empty from a field that is not included in the response.
Any exception must be documented next to the Rust DTO field and covered by a contract fixture.
Current exception: `targets[].accelerator` is a required nullable field where `null` means no accelerator and `"gpu"` means GPU acceleration.

## Request DTOs

Request DTOs may accept omitted optional fields where that improves client ergonomics.
Request deserialization rules are separate from response serialization rules.

## Locator Naming

Use `*_key` only for canonical lookup values, not as a generic replacement for `id`, `name`, `path`, or `url`.

- `storage_key`, `artifact_key`, and `runner_log_storage_key` are opaque object-storage keys relative to the configured Agentics storage backend.
  They are not filesystem paths, URLs, or URIs, even when local development stores them on disk.
- `runner_log_storage_key` appears only on submitter-visible log responses or internal/admin DTOs where the caller may read that stored runner log.
  Public unauthenticated result surfaces must omit it.
- `repo_url` is the submitted GitHub remote and should be preserved for provenance and display.
- `repo_key` is the canonical GitHub repository identity used for duplicate detection and authorization.
  It normalizes accepted GitHub HTTPS and SSH remotes for the same repository into lowercase `owner/repo`.

Do not expose ambiguous fields such as `path` or `uri` when the value is really an object-storage key.
Do not expose `repo_key` as a replacement for `repo_url` when the original remote matters.

`SolutionSubmissionLogsResponse.availability` explains whether the logs endpoint returned content.
`available` means `runner_log_storage_key` and `content` may be present.
`not_persisted`, `redacted_private_official`, and `redacted_by_config` must not expose a runner log storage key or inline log content.

## Schema Generation

Frontend runtime schemas are generated from Rust DTOs:

```bash
cd frontends/web
bun install --frozen-lockfile
bun run generate:schemas
bun run generate:schemas:check
```

The command runs the `export-web-schemas` binary from `agentics-contracts`.
That binary uses the single Rust schema manifest in `agentics_contracts::validation::schemas`, converts the JSON Schemas into Zod, and writes `frontends/web/src/lib/generated/schemas.ts`.
The hand-written `frontends/web/src/lib/schemas.ts` module is only a stable re-export facade for frontend imports.

`bun run generate:schemas:check` is the non-mutating freshness check.
It should pass in normal verification and fail when Rust DTO changes have not been regenerated into the frontend schema facade.

The generator must preserve this mapping:

- `Option<T>` with `skip_serializing_if = "Option::is_none"` becomes `field?: T`.
- Explicit-null fields, if any are intentionally introduced, become `field: T | null` and require documentation.

When changing Rust response DTOs, update derives and serde attributes first, regenerate the frontend schemas, then update contract fixtures or rendering code only if the API contract intentionally changed.
Shared Rust and frontend contract fixtures must cover representative response DTOs.

Public result DTOs must stay redacted by projection rather than by frontend convention.
Public solution submission lists expose official result-of-record fields only; validation-only scores are not part of the public list contract.
