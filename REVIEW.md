# Full Code Review Log

This file is the index for dated Agentics full-code-review logs. Detailed review
notes are split by review date and reviewed commit head under `reviews/`.

## Review Logs

- [2026-05-20, `4a47753`](reviews/2026-05-20-4a47753.md)
  - Scope: backend API, database state machines, worker and Docker runner, DGX
    hosted profile, CLI, web frontend, schemas, tests, documentation alignment,
    and web frontend component reuse.
  - Current notable open items: DGX Linux-only bind-mount runtime root, retained
    quota-backed runner storage, CUDA GPU-count enforcement, split admin/creator
    draft projections, active validation terminal-state races, public web count
    accuracy, red web tests, generated request-schema coverage, public zh i18n,
    docs/schema naming drift, CLI fixture optional-field contract drift, and
    draft audit transaction semantics.
  - Accepted MVP scope decisions in this pass: client-asserted GitHub PR
    ownership, pioneer-code re-exposure, and placeholder creator/owner CLI
    commands.
- [2026-05-19, `ebf4c9a`](reviews/2026-05-19-ebf4c9a.md)
  - Scope: backend API, worker and Docker runner, challenge draft lifecycle,
    CLI, frontend, schemas, tests, documentation alignment, and validation logic
    centralization planning.
  - Status: previous findings are recorded as resolved in that review log.

## Notes

- New full-review passes should create `reviews/<date>-<commit>.md` and add a
  short entry here.
- Accepted MVP risks should be called out explicitly in the relevant dated log,
  not silently removed from the review record.
