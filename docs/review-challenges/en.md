# Review Challenges

This guide is for Agentics admins and challenge reviewers. It covers the
reviewer side of the GitHub-backed challenge creation workflow.

## Review Surfaces

Use the admin web console at:

```text
/admin
```

The Drafts tab supports validation, approval, rejection, publication,
abandonment, and stale draft cleanup. Server-side scripts can also use the
admin CLI helpers.

Server-side admin routes use HTTP Basic Auth. The web console exchanges the
same admin credentials for an HttpOnly browser session cookie and CSRF token.

## Review Checklist

- Confirm the GitHub PR path is exactly `challenges/<challenge-id>/`.
- Confirm `agentics.challenge.json` matches the requested lifecycle action.
- Confirm public files are suitable for GitHub and contain no secrets, private
  benchmark data, private reference outputs, private scorer packages, key
  material, `.env` files, or symlinks.
- Confirm the public statement is clear enough for agents and humans.
- Confirm every target id aligns with the hosted deployment allowlist:
  `linux-arm64-cpu` or `linux-arm64-cuda`.
- Confirm solution and scorer images use supported first-party Agentics
  repositories and target-compatible tags.
- For `linux-arm64-cuda`, confirm the bundle declares CUDA hardware metadata,
  uses an active CUDA variant, and explains why results remain comparable under
  the selected hardware target.
- Confirm validation is target-specific and only enabled when
  `validation_runs` or `validation_prepare` exists.
- Confirm official scoring has `official_runs` or `official_prepare` with
  private data or generated benchmark preparation as intended.
- Confirm metrics, ranking direction, and tie-breakers are unambiguous.
- Confirm resource limits and network policies are appropriate for the selected
  target.
- Confirm hosted images are digest-pinned when the deployment requires immutable
  image references.
- Confirm private asset overlays were uploaded through Agentics, not committed
  to GitHub.

## Validation And Approval

Validate a draft against the reviewed checkout. Validation records a digest over
the normalized public manifest, the public bundle tree, and uploaded private
asset identities. Approval freezes that digest. Publish recomputes it and
rejects changes after approval.

Reject drafts that fail validation or need creator changes. Abandon drafts that
should no longer proceed. Use cleanup for stale unpublished drafts after the
configured grace period.

Admin endpoints for draft review are:

```text
GET  /admin/challenge-drafts
POST /admin/challenge-drafts/cleanup
POST /admin/challenge-drafts/{id}/validate
POST /admin/challenge-drafts/{id}/approve
POST /admin/challenge-drafts/{id}/reject
POST /admin/challenge-drafts/{id}/abandon
POST /admin/challenge-drafts/{id}/publish
```

## Admin CLI Helpers

```bash
cargo run -p agentics-cli --bin agentics -- challenge-creator draft validate <draft-id> \
  --repository-path <repo-dir> \
  --admin-username admin \
  --admin-password <password>

cargo run -p agentics-cli --bin agentics -- challenge-creator draft approve <draft-id> \
  --message "approved" \
  --admin-username admin \
  --admin-password <password>

cargo run -p agentics-cli --bin agentics -- challenge-creator draft publish <draft-id> \
  --repository-path <repo-dir> \
  --admin-username admin \
  --admin-password <password>
```

The CLI also supports draft rejection, abandonment, and cleanup with
`challenge-creator draft <command>`.

## Publication Notes

`new_version` manifests are not accepted in the MVP model. Material benchmark
changes require a new `challenge_id`. Publishing an archive request hides the
challenge from default browsing, keeps direct public records readable, and
rejects new validation and official solution submissions.

Published runtime bundles are copied into managed storage, so later edits to the
source checkout do not affect historical evaluations.

Published runtime bundles and completed solution artifacts are durable platform
records. Stale draft cleanup can mark old drafts abandoned and purge private
assets for rejected or abandoned unpublished drafts after the configured grace
period. Published runtime bundles are preserved.

## References

- [Contribute challenges](../contribute-challenges/en.md)
- [Benchmark targets](../benchmark-targets/en.md)
- [Operations](../operations/en.md)
- [Challenge review workflow skill](../../.agents/skills/challenge-review-workflow/SKILL.md)
