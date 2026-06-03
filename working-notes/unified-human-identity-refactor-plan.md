# Unified Human Identity Refactor Plan

## Summary

Replace separate creator GitHub OAuth and admin username/password authentication
with one human identity system. Humans authenticate through GitHub OAuth and
receive roles. Agents continue to authenticate with agent bearer tokens.
Non-browser admin automation uses admin service tokens.

No compatibility layer will be kept. Remove admin username/password login,
HTTP Basic admin access, `x-agentics-admin-automation`, shadow creator-agent
rows for humans, and old admin login/session DTOs.

This document is the source of truth for the implementation. Keep referring
back to it during the refactor. Before considering the work complete, reread
this plan and check each section against the diff.

## Implementation Order And Commit Boundaries

Use focused commits. Do not mix unrelated cleanup with this refactor.

1. Plan and terminology notes.
   Commit only this source-of-truth plan and other already-requested working
   notes.
2. Backend domain, config, migrations, and service model.
   Introduce human identity, roles, human sessions, admin service tokens, and
   generalized pioneer-code use records. Remove password admin auth and shadow
   creator-agent identity. Keep API handlers compiling only after the matching
   route/extractor changes land.
3. Backend routes and authorization.
   Replace creator/admin browser auth with human sessions, add service-token
   admin auth, update challenge review and challenge-owner flows to use
   humans, and update audit attribution.
4. Generated schemas and frontend auth flows.
   Regenerate frontend schemas, replace admin password login with GitHub OAuth,
   require roles in creator/admin surfaces, and add admin service-token and
   human-role UI.
5. CLI and non-browser admin automation.
   Replace admin username/password options with admin service-token options and
   environment settings.
6. Documentation and skills.
   Update all affected English and Chinese docs, public agent docs, and local
   skills in the same behavior change set.
7. Verification and final audit.
   Run targeted checks first, then required format/check/lint commands before
   final commits. Reread this plan before the last implementation commit.

## Canonical Model

- Human authentication:
  - GitHub OAuth browser flow.
  - Cookie-backed human sessions with CSRF for unsafe browser requests.
  - Roles: `creator`, `admin`.
- Agent authentication:
  - Existing agent bearer token flow.
  - Agent identities remain separate from humans.
- Admin automation:
  - Opaque `agentics_admin_...` service tokens.
  - Stored hashed, returned once, revocable, auditable.
- Pioneer codes:
  - Gate first-time human creation and first-time agent registration.
  - Existing humans do not need a pioneer code to sign in again.
  - First bootstrap admin may bypass pioneer code only while no active admin
    exists and their numeric GitHub user id is configured for bootstrap.

## Pioneer-Code Interaction

Pioneer codes remain invitation gates, not ongoing login credentials.

- First-time human OAuth in pioneer-code mode:
  - A new GitHub identity must provide a valid pioneer code unless the
    bootstrap-admin rule applies.
  - The code is validated before GitHub redirect only for syntax and stored as
    a hash in OAuth state.
  - The code is consumed only in the callback transaction after GitHub identity
    is known.
  - The resulting use record is `subject_kind = human` and
    `registration_kind = human_github_oauth`.
- Returning human OAuth:
  - No pioneer code is required.
  - A provided pioneer code is ignored for account creation because the human
    already exists, and no new use record is written.
- First bootstrap admin:
  - Allowed only if no active human has the `admin` role.
  - The numeric GitHub user id must be present in
    `AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS`.
  - The created human receives both `creator` and `admin` roles.
  - No pioneer-code use record is written for bootstrap.
- Agent registration:
  - Existing agent registration keeps using pioneer codes.
  - The resulting use record is `subject_kind = agent` and
    `registration_kind = agent_api`.
- Pioneer-code revoke:
  - Disables humans created through that code and deletes their active human
    sessions.
  - Disables agents created through that code and revokes their active agent
    tokens.
  - Does not delete historical submissions, review records, audit events, or
    ownership records.

## Public Interfaces

- Keep and generalize:
  - `POST /api/auth/github/login`
  - `POST /api/auth/github/callback`
- Add:
  - `GET /api/auth/session`
  - `POST /api/auth/logout`
  - `GET /admin/humans`
  - `POST /admin/humans/{human_id}/roles/admin/grant`
  - `POST /admin/humans/{human_id}/roles/admin/revoke`
  - `GET /admin/admin-service-tokens`
  - `POST /admin/admin-service-tokens`
  - `POST /admin/admin-service-tokens/{token_id}/revoke`
- Remove:
  - `POST /api/auth/admin/login`
  - `GET /api/auth/admin/session`
  - `POST /api/auth/admin/logout`
  - `GET /api/creator/session`
  - `GET /api/creator/me`
  - Admin HTTP Basic auth on `/admin/*`
- Request/response DTOs:
  - Remove `AdminLoginRequest` and `AdminSessionResponse`.
  - Replace creator-only session/me DTOs with `HumanSessionResponse`.
  - `HumanSessionResponse` includes:
    - `human_id`
    - `github_user_id`
    - `github_login`
    - `roles`
    - `csrf_token`
    - `expires_at`
  - `GithubOauthLoginRequest` keeps optional `pioneer_code` and adds optional
    `return_to`.
  - `GithubOauthLoginResponse` keeps `authorization_url`.
  - `GithubOauthCallbackResponse` returns `HumanSessionResponse` plus
    `return_to`.
- CLI:
  - Replace admin username/password args with `--admin-service-token`.
  - Also read `AGENTICS_ADMIN_SERVICE_TOKEN`.
  - Send `Authorization: Bearer agentics_admin_...`.

## Database And Domain Changes

- Add human identity tables:
  - `humans(id UUID PRIMARY KEY, status TEXT, created_at, disabled_at)`
  - `human_external_identities(human_id, provider, provider_user_id, provider_login, updated_at)`
  - `human_roles(human_id, role, granted_by_human_id, granted_at, revoked_at)`
  - `human_sessions(id, session_token_hash, csrf_token_hash, human_id, expires_at, created_at, last_used_at)`
- Replace admin session storage:
  - Remove `web_sessions.role = 'admin'`, `admin_username`, and admin password
    session creation.
  - Use `human_sessions` for all browser humans.
- Add admin service token tables:
  - `admin_service_tokens(id, token_hash, label, status, created_by_human_id, created_at, last_used_at, expires_at, revoked_at)`
  - Service tokens initially have full `admin` scope only.
- Generalize pioneer-code use tracking:
  - Replace `agent_pioneer_code_uses` with `pioneer_code_uses`.
  - Fields: `pioneer_code_id`, `subject_kind`, `human_id`, `agent_id`,
    `registration_kind`, `used_at`.
  - Valid `subject_kind`: `human`, `agent`.
  - Valid `registration_kind`: `human_github_oauth`, `agent_api`.
  - Revoking a pioneer code disables agents created by it, revokes their agent
    tokens, disables humans created by it, and deletes those humans' sessions.
- Human-owned workflow state:
  - Change challenge review records from `creator_agent_id` to
    `creator_human_id`.
  - Change challenge owners from `agent_id` to `human_id`.
  - Change owner-visible challenge stats, participants, and shortlist mutation
    authorization to use human owners.
  - Keep shortlist entries as agent IDs because the shortlist is a list of
    participant agents.
- Audit fields:
  - Replace `actor_admin_username` with `actor_human_id` and
    `actor_admin_service_token_id`.
  - Store a display snapshot where needed for admin UI, such as GitHub login or
    service-token label.
- Config/env:
  - Add `AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS`.
  - Remove `AGENTICS_ADMIN_USERNAME`, `AGENTICS_ADMIN_PASSWORD`, and insecure
    default admin credential validation.

## Backend Behavior

- GitHub login start:
  - Validate optional pioneer code syntax only.
  - Store hashed OAuth state, browser nonce hash, optional pioneer-code hash,
    optional `return_to`, and TTL.
  - Do not put pioneer code in URLs.
- GitHub callback:
  - Consume state once by state hash and browser nonce hash.
  - Exchange code for GitHub access token and fetch numeric GitHub user id.
  - If GitHub identity maps to an active human, issue a human session.
  - If no human exists:
    - If no active admin exists and GitHub id is in bootstrap config, create
      active human, grant `creator` and `admin`, and issue a session.
    - Else if registration mode is `pioneer_code`, require and consume a valid
      pioneer code, create active human, grant `creator`, and issue a session.
    - Else if registration mode is `public`, create active human, grant
      `creator`, and issue a session.
  - Disabled humans cannot sign in.
- Auth extractors:
  - Add `HumanAuth`.
  - Add `CreatorAuth` as a thin wrapper requiring `creator` role.
  - Replace `AdminAuth` with admin role/session or admin service token auth.
  - Browser unsafe requests require CSRF.
  - Admin service token requests do not use CSRF.
- Admin service tokens:
  - Only human admins can create/revoke/list them.
  - Raw token is returned only on creation.
  - Token hashes are compared using the existing opaque-token hash helper.

## Frontend Behavior

- Shared GitHub sign-in:
  - Creator and admin surfaces use the same OAuth functions and session hook.
  - Returning humans may leave pioneer code blank.
  - New invited humans provide pioneer code before GitHub OAuth.
  - Bootstrap admin signs in without pioneer code when configured.
- Creator console:
  - Uses `GET /api/auth/session`.
  - Requires `creator` role.
  - Sends returned CSRF token for mutations.
- Admin console:
  - Remove username/password form.
  - Shows GitHub sign-in when not authenticated.
  - Shows a forbidden state when authenticated human lacks `admin`.
  - Adds admin service token management.
  - Adds human admin role management.
- Admin API client:
  - Uses human session cookies and CSRF in browser.
  - CLI uses admin service token bearer auth.

## Docs And Skills

- Update English and Chinese docs together:
  - PRD
  - milestones
  - architecture
  - operations
  - deployment
  - contribute-challenges
  - review-challenges
  - ports-and-paths if env names are listed
- Update agent skills:
  - challenge authoring workflow
  - challenge review workflow
  - full code review references if they mention admin auth.
- Update frontend and public `skill.md` docs for the new human/agent auth split.
- Document first-admin bootstrap and admin service token creation.

## Verification

- Run codegen after Rust DTO changes:
  - `bun run generate:schemas` in `frontends/web/`.
- Targeted backend tests:
  - GitHub OAuth new human with pioneer code.
  - GitHub OAuth existing human without pioneer code.
  - Bootstrap first admin without pioneer code.
  - Bootstrap ignored after admin exists.
  - Non-admin human rejected from admin routes.
  - Admin service token authenticates admin routes.
  - Revoked/expired admin service token rejected.
  - Pioneer-code revoke disables derived agents and humans.
  - Review record create/read/upload uses `creator_human_id`.
  - Challenge owner stats/participants/shortlist authorize by human owner.
- Targeted frontend tests:
  - Creator sign-in and session restore.
  - Admin sign-in, non-admin forbidden state, and admin service token UI.
  - Admin API client no longer sends Basic auth.
- Required checks before final commits:
  - `cargo fmt --all`
  - `cargo check --workspace --all-targets`
  - `cargo test -p agentics-cli challenge_creator`
  - targeted integration tests for auth/admin/pioneer codes/challenge creation
  - targeted web tests for admin and creator auth
  - `bun run generate:schemas:check`
  - `just rust::clippy`
  - `just web::schema-check`
  - `just test-all-cpu` on Linux if available.
- Final audit:
  - No `AdminLoginRequest`, `AdminSessionResponse`, `admin_password`,
    `admin_username`, `parse_basic_auth`, `x-agentics-admin-automation`, or
    `creator_agent_id` in live code/docs except historical notes.

## Linux Handoff After Review Fixes

This section is for the Linux agent picking up the environment-dependent
verification after the full code-review pass.

Current reviewed commits:

- `1c4b9fb4 fix(identity): harden admin revocation paths`
- `c733bce6 fix(cli): remove argv admin token input`
- `98a3be00 docs(review): log identity code review`

Review log:

- `reviews/2026-06-03-beb005c2.md`

What changed after review:

- Human admin role revocation is guarded so identity management cannot revoke
  the final active human admin.
- Pioneer-code revocation now revokes active admin service tokens created by
  humans derived from that code.
- Human role and admin service-token revocations record the revoking human.
- Hosted bootstrap/admin operation now requires GitHub OAuth config when needed.
- Admin and creator browser consoles share one human-session SWR cache key.
- Admin service tokens can no longer be passed through CLI argv. Use
  `AGENTICS_ADMIN_SERVICE_TOKEN` or `--admin-service-token-stdin`.

Accepted MVP tradeoffs, not blockers:

- Public observer stats may expose aggregate total attempt counts.
- Pioneer codes remain visible to admins after creation.
- Agent-facing registration examples may pass pioneer codes through argv.

Mac verification already completed:

- `cargo fmt --all`
- `bunx biome check --write src messages`
- `cargo test -p integration-tests --no-run`
- `cargo check --workspace --all-targets`
- `bun run generate:schemas:check`
- `bun run lint`
- `bun x tsc --noEmit --pretty false`
- `cargo test -p agentics-cli admin_service_token_argv_flag_is_removed`
- `cargo test -p agentics-config github_oauth`
- `cargo test -p agentics-config hosted_bind_requires_secure_cookies_and_invited_registration`
- `bun run test -- src/components/admin/AdminConsole.test.tsx src/components/creator/CreatorConsole.test.tsx src/lib/creatorApi.test.ts src/lib/adminData.test.ts src/lib/creatorData.test.ts src/lib/schemas.test.ts`
- `just rust::clippy`
- `just web::schema-check`
- `git diff --check`

Linux work to do:

1. Check the dedicated test environment:

   ```bash
   just test-env-status-cpu
   ```

2. If storage or the dedicated Docker daemon is missing, prepare/start it with
   the documented Linux flow:

   ```bash
   sudo AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage \
     agentics-prepare-dgx-spark-test-storage
   sudo env AGENTICS_TEST_ROOT=/srv/agentics-test just test-env-up
   just test-env-status-cpu
   ```

3. Run the canonical CPU suite:

   ```bash
   just test-all-cpu
   ```

4. If the full suite fails and the failure appears localized, first isolate the
   new identity regressions:

   ```bash
   cargo test -p integration-tests final_human_admin_role_cannot_be_revoked
   cargo test -p integration-tests pioneer_code_revoke_revokes_derived_human_admin_service_tokens
   ```

   These direct SQLx commands require a reachable test Postgres and
   `DATABASE_URL` in the shell. Prefer `just test-all-cpu` when the Compose
   harness is available.

5. If Linux verification requires a fix, keep commits focused:
   - Backend/test fixes in one `fix(identity): ...` or `test(identity): ...`
     commit.
   - Docs-only updates in a separate `docs(...)` commit.
   - Do not change the accepted MVP tradeoffs unless product requirements
     change.

## Assumptions

- No `reviewer` role in this refactor.
- GitHub OAuth App remains the provider for now. GitHub App PR verification is
  a separate future task.
- Admin service tokens start with one full-admin scope, `admin`.
- Existing dev databases can be reset because there is no compatibility
  requirement before MVP.
- Published challenge ownership is human-owned after this refactor; participant
  agents remain separate and continue to use agent IDs.
