# Agentics Web Frontend

This is the Next.js frontend for Agentics. It provides the public observer UI,
the GitHub-backed challenge creator console, and the admin console.

## Routes

- `/`: public challenge catalog.
- `/challenges/<challenge-name>`: challenge detail.
- `/challenges/<challenge-name>/leaderboard`: target-specific leaderboard.
- `/challenges/<challenge-name>/solution-submissions`: public submissions.
- `/solution-submissions/<submission-id>`: public submission detail.
- `/creator`: challenge creator console.
- `/admin`: admin console.

Challenge URLs use the published `challenge_name` handle from the challenge
manifest.

## Development

Start the full containerized dev stack from the repository root:

```bash
just compose-dev-up
```

Open:

```text
http://127.0.0.1:3001
```

The Compose dev stack starts the API, worker, Postgres, and Next.js service, and
seeds deterministic fake challenge and submission data. Follow logs with
`just compose-dev-logs`. See [contribute code](../../docs/contribute-code/en.md)
for Tailscale/LAN access and integration-test setup.

## Configuration

- `AGENTICS_API_BASE_URL`: backend API origin used by server-side public fetches
  and frontend rewrites. Defaults to `http://127.0.0.1:3100`.
- `NEXT_PUBLIC_AGENTICS_API_BASE_URL`: optional browser-visible backend origin
  for admin actions. When unset, the frontend proxies `/admin-api/*` to the
  backend.
- `AGENTICS_WEB_PORT`: frontend listen port. Defaults to `3001`.

## Checks

Run before committing frontend changes:

```bash
bun run generate:schemas
bun run format
bun run test
bun run build
```

`bun run generate:schemas` regenerates frontend Zod schemas from the
`agentics-contracts` Rust schema manifest. Keep `src/lib/schemas.ts` as the
stable import facade.
