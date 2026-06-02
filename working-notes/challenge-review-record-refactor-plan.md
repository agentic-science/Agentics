# Challenge Review Record Naming Refactor Plan

## Summary

Rename the current "challenge draft" concept to **Challenge Review Record** across the codebase. The GitHub PR remains the actual challenge proposal; the Agentics record is the platform-side review record that binds PR metadata, private assets, validation state, audit history, and publication state.

No compatibility layer will be kept. Old routes, CLI commands, DTO names, env vars, schema exports, docs wording, and UI labels using the challenge-draft terminology will be removed.

This document is the source of truth for the implementation. Before considering the refactor complete, reread this plan and check every section against the diff.

## Canonical Naming

- `ChallengeDraft` -> `ChallengeReviewRecord`
- `ChallengeDraftId` -> `ChallengeReviewRecordId`
- `ChallengeDraftStatus` -> `ChallengeReviewRecordStatus`
- `ChallengeDraftValidationRecord` -> `ChallengeReviewValidationRecord`
- `ChallengeDraftValidationRecordId` -> `ChallengeReviewValidationRecordId`
- `ChallengeDraftAuditEventId` -> `ChallengeReviewAuditEventId`
- `ChallengeDraftPublishClaimId` -> `ChallengeReviewPublishClaimId`
- Initial status value: `draft` -> `pending_review`
- User-facing phrase: "challenge review record"
- Clear explanatory phrase: "register a challenge PR for review"

## Public Interfaces

- API routes:
  - `/creator/challenge-drafts` -> `/creator/challenge-review-records`
  - `/creator/challenge-drafts/{id}` -> `/creator/challenge-review-records/{review_record_id}`
  - `/creator/challenge-drafts/{id}/private-assets` -> `/creator/challenge-review-records/{review_record_id}/private-assets`
  - `/admin/challenge-drafts` -> `/admin/challenge-review-records`
  - All admin action routes keep the same action names: `validate`, `approve`, `reject`, `abandon`, `publish`, `cleanup`.
- JSON fields:
  - `draft_id` -> `review_record_id`
  - `challenge_draft_*` fields -> `challenge_review_record_*` where they refer to the platform record.
- DTOs:
  - `CreateChallengeDraftRequest` -> `CreateChallengeReviewRecordRequest`
  - `ValidateChallengeDraftRequest` -> `ValidateChallengeReviewRecordRequest`
  - `ReviewChallengeDraftRequest` -> `ChallengeReviewDecisionRequest`
  - `ChallengeDraftResponse` -> `ChallengeReviewRecordResponse`
  - Creator/admin variants follow the same rename.
- CLI:
  - Replace user-facing `draft` / `challenge-drafts` subcommands with `review-record` / `challenge-review-records`.
  - Replace arguments and output labels from `draft_id` / "Draft ID" to `review_record_id` / "Review record ID".
- Config/env:
  - `AGENTICS_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT` -> `AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_AGENT`
  - `AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT` -> `AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_REVIEW_RECORD`
  - `AGENTICS_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY` -> `AGENTICS_CHALLENGE_REVIEW_RECORD_VALIDATIONS_PER_DAY`
  - `AGENTICS_CHALLENGE_DRAFT_VALIDATION_TIMEOUT_MINUTES` -> `AGENTICS_CHALLENGE_REVIEW_RECORD_VALIDATION_TIMEOUT_MINUTES`
  - `AGENTICS_CHALLENGE_DRAFT_PUBLISH_TIMEOUT_MINUTES` -> `AGENTICS_CHALLENGE_REVIEW_RECORD_PUBLISH_TIMEOUT_MINUTES`
  - `AGENTICS_CHALLENGE_DRAFT_TTL_DAYS` -> `AGENTICS_CHALLENGE_REVIEW_RECORD_TTL_DAYS`

## Implementation Changes

- Domain layer:
  - Rename challenge-creation request/response/lifecycle types and status parsing.
  - Store lifecycle status as `pending_review`, `validated`, `approved`, `publishing`, `rejected`, `published`, `abandoned`.
  - Remove all old type aliases and schema names.
- Persistence and migrations:
  - Rename baseline schema objects because this is pre-MVP and no compatibility is required:
    - `challenge_drafts` -> `challenge_review_records`
    - `challenge_draft_validation_records` -> `challenge_review_validation_records`
    - `challenge_draft_audit_events` -> `challenge_review_audit_events`
    - `draft_id` columns -> `review_record_id`
  - Rename indexes, constraints, repository methods, SQL query aliases, and row structs.
  - Rename storage prefixes from `challenge-drafts/...` to `challenge-review-records/...`.
- Services and API:
  - Rename service modules, repository traits, handlers, route bindings, audit events, metrics/log messages, and error messages.
  - Preserve behavior: PR binding, validation, asset upload, approval, rejection, abandon, publish, cleanup.
- Frontend:
  - Regenerate Zod schemas after Rust DTO changes.
  - Update creator/admin API clients, components, tests, and EN/ZH messages.
  - Creator UX should say "Register PR for review", "Review record", and explain that the proposal itself lives in the GitHub PR.
- Docs and skills:
  - Update EN/ZH docs together: contribute-challenges, review-challenges, PRD, milestones, operations, deployment, ports/paths, API contract references if touched.
  - Update creator/reviewer agent skills so agents understand that a challenge review record is auxiliary platform state, not the challenge proposal itself.

## Verification

- Run schema generation in `frontends/web/`:
  - `bun install --frozen-lockfile`
  - `bun run generate:schemas`
- Run targeted Rust checks/tests for domain, persistence, services, API server, and CLI challenge-creator/admin flows.
- Run targeted frontend tests for creator console and admin panel.
- Run repo-standard checks before commit:
  - `just rust::clippy`
  - `just web::schema-check`
  - `just test-all-cpu` if the CPU test harness is available.
- Run a final terminology audit:
  - No remaining `ChallengeDraft`, `challenge_draft`, `challenge-drafts`, `draft_id`, or "challenge draft" in challenge-review workflow code/docs.
  - Allow unrelated prose only where "draft" means an actual article/document draft, not this workflow.

## Assumptions

- Because there is no compatibility requirement, old API routes, CLI commands, env vars, generated schema exports, and DB names are removed rather than deprecated.
- Existing local dev database/storage containing old challenge-draft tables or keys may need to be reset after the refactor.
- If a real production dataset unexpectedly exists, stop and convert the DB/storage rename into an explicit migration plan before applying changes.
