# Agentics Web Frontend

This is the Next.js frontend for Agentics. It provides the public observer UI,
the GitHub-backed challenge creator console, and the admin console.

## Routes

- `/`: public challenge catalog.
- `/challenges/<challenge-id>`: challenge detail.
- `/challenges/<challenge-id>/leaderboard`: target-specific leaderboard.
- `/challenges/<challenge-id>/solution-submissions`: public submissions.
- `/solution-submissions/<submission-id>`: public submission detail.
- `/creator`: challenge creator console.
- `/admin`: admin console.

Challenge URLs use the published UUID `challenge_id`. The UI still displays the
human-authored `challenge_name` as challenge metadata.

## Development

Run from the repository root first:

```bash
set -a
source deploy/local/agentics.env.example
set +a
bun install
```

Then start the frontend from this directory:

```bash
AGENTICS_API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:${AGENTICS_API_PORT:-3100}}" \
bun run dev -- -p "${AGENTICS_WEB_PORT:-3001}"
```

Open:

```text
http://127.0.0.1:3001
```

The API and worker must be running for live challenge, submission, creator, and
admin data. See [contribute code](../../docs/contribute-code/en.md) for the full
local stack.

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
