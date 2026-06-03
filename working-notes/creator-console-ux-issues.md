# Creator Console UX Issues

## Context

The creator console currently suffers from a confusing mental model. Challenge
proposals should live as PRs in the public `agentics-challenges` repository.
The Agentics creator console should not feel like the place where a creator
authors the challenge itself. Its role is to register the PR for review, bind
private assets, and expose validation, review, and publication state.

The rename from "challenge draft" to "challenge review record" fixes the
language at the data-model level, but the creator console still needs a
workflow-oriented UX pass.

## Problems To Fix

1. The old "challenge draft" framing made the wrong object feel primary.
   It suggested that Agentics owned the challenge proposal, when the actual
   proposal is the GitHub PR. The Agentics-side object is auxiliary review
   state: PR metadata, private assets, validation records, audit history,
   approval, and publish state.

2. The flow starts from the wrong user action.
   Creators should first prepare a challenge proposal in the challenge repo,
   then open a PR, then register that PR with Agentics for review. The console
   should make that sequence obvious.

3. The page mixes several concepts under one surface.
   PR registration, private asset upload, validation, review feedback,
   approval, rejection, abandonment, and publication are different workflow
   stages. The UI should separate them enough that creators understand where
   they are and what action is currently available.

4. The purpose of the creator console is under-explained.
   A creator uses the console to bind PR metadata, upload private benchmark
   assets that must not go into Git, and track review/publish state. That value
   proposition should be visible without reading backend terminology.

5. Creator and reviewer responsibilities are blurred.
   Creators can register PRs, upload assets, inspect status, and respond to
   feedback. Admins/reviewers validate, approve, reject, abandon, publish, and
   run cleanup. The UI should avoid presenting reviewer-only actions as part of
   the creator flow.

6. The experience is form-heavy instead of workflow-first.
   A creator needs a guided sequence: prepare PR, register PR, upload private
   assets, wait for validation/review, respond to feedback, and publish after
   approval. The page should behave more like a workflow tracker than a loose
   set of forms.

## Design Direction

- Use "challenge proposal" for the GitHub PR and "review record" for the
  Agentics-side state.
- Lead with a step-by-step flow, not with raw forms.
- Make the active step, blocking condition, and next action visually obvious.
- Keep private asset upload tied to the review record, but explain that assets
  stay outside Git.
- Keep creator-facing copy focused on what creators can do, and reserve
  reviewer/admin actions for status messages or read-only timeline entries.
