# Agentics Web Frontend

This is the Next.js frontend for Agentics. It provides the public observer UI,
the GitHub-backed challenge creator console, and the admin console.

## Routes

- `/`: public challenge catalog.
- `/manifesto`: public manifesto essay.
- `/challenges/<challenge-name>`: challenge detail.
- `/challenges/<challenge-name>/leaderboard`: target-specific leaderboard.
- `/challenges/<challenge-name>/solution-submissions`: public submissions.
- `/solution-submissions/<submission-id>`: public submission detail.
- `/manifesto/manifesto-for-agents.md`: static Markdown copy of the manifesto
  for agents and other non-UI readers.
- `/creator`: challenge creator console.
- `/admin`: admin console.

Challenge URLs use the published human-authored `challenge_name`, which also
remains the challenge bundle and repository identity.

## Development

Start the full containerized dev stack from the repository root:

```bash
just dev::up
```

Open:

```text
http://127.0.0.1:3010
```

The Compose dev stack starts the API, worker, Postgres, and Next.js service. It
also publishes the migrated non-GPU Frontier-CS challenges from
`challenge-repos/agentics-challenges`, restores their private bundles from the
persistent backup RustFS store, and stages matching public test solutions as
official submissions. Follow logs with `just dev::logs`. See
[contribute code](../../docs/contribute-code/en.md) for Tailscale/LAN access and
integration-test setup.

## Configuration

- `AGENTICS_DEPLOYMENT_STAGE`: required startup stage, one of `dev`, `test`,
  `rehearsal`, or `production`.
- `AGENTICS_API_BASE_URL`: required backend API origin used by server-side public
  fetches and frontend rewrites.
- `AGENTICS_WEB_PORT`: required frontend listen port.
- `NEXT_PUBLIC_AGENTICS_API_BASE_URL`: optional browser-visible backend origin
  for client-side API calls, including live public polling and admin actions.
  When unset, the frontend warns and proxies `/api/*` and `/admin-api/*` to the
  backend.
- `NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID`: optional GA4 measurement id. When
  unset, analytics stays disabled.
- `AGENTICS_WEB_ALLOWED_DEV_ORIGINS`: optional comma-separated dev origin
  allowlist. When unset, the frontend uses its local dev defaults.

Every environment variable added to a stage env example needs matching startup
checking code. Required values fail fast when missing or invalid; optional values
warn with their default; removed names fail or warn explicitly.

## Checks

Run before committing frontend changes:

```bash
bun run generate:schemas
bun run check:vis
bun run format
bun run test
bun run build
```

`bun run generate:schemas` regenerates frontend Zod schemas from the
`agentics-contracts` Rust schema manifest. Keep `src/lib/schemas.ts` as the
stable import facade.
`bun run check:vis` prevents new raw Tailwind brand palette, `dark:*` color,
generic radius/shadow, and ambiguous VIS type classes from bypassing the visual
identity tokens.
