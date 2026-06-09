# PostgreSQL 18 Dev/Test/Rehearsal Upgrade Plan

Date: 2026-06-10

## Summary

Dev, test, and disposable production rehearsal should run PostgreSQL 18 through
`postgres:18-alpine`. Production stays on PostgreSQL 16 until a separate
production migration is planned and rehearsed.

The shared Compose base stays production-safe by default:

- default image: `postgres:16-alpine`
- default data mount: `/var/lib/postgresql/data`

Disposable environments opt in with env vars:

- `AGENTICS_POSTGRES_IMAGE=postgres:18-alpine`
- `AGENTICS_POSTGRES_DATA_MOUNT=/var/lib/postgresql`
- `AGENTICS_POSTGRES_IO_METHOD=io_uring`

## PG 18 Docker Mount Note

The PostgreSQL 18 official image stores its default data directory under
`/var/lib/postgresql/18/docker`. For PG 18 Compose volumes, mount the named
volume at `/var/lib/postgresql`, not `/var/lib/postgresql/data`.

## `io_uring` Probe Result

Host:

- kernel: `Linux MapleSpark 6.17.0-1021-nvidia aarch64`
- `/proc/sys/kernel/io_uring_disabled`: `0`
- Docker server: `29.2.1`

Probe without seccomp relaxation:

- image: `postgres:18-alpine`
- mount: named volume at `/var/lib/postgresql`
- command: `postgres -c io_method=io_uring`
- result: failed
- Postgres log:
  - `FATAL: could not setup io_uring queue: Operation not permitted`
  - `HINT: Check if io_uring is disabled via /proc/sys/kernel/io_uring_disabled.`

Probe with `--security-opt seccomp=unconfined`:

- image: `postgres:18-alpine`
- mount: named volume at `/var/lib/postgresql`
- command: `postgres -c io_method=io_uring`
- result: succeeded
- runtime values:
  - `SHOW server_version;` -> `18.4`
  - `SHOW io_method;` -> `io_uring`
  - `SHOW effective_io_concurrency;` -> `16`

Decision: use `io_uring` as the default for dev/test/rehearsal Postgres and add
`security_opt: [seccomp=unconfined]` only to those disposable Postgres service
overlays. Production remains on PostgreSQL 16 until its documented
dump/restore cutover is executed.

## Implementation Checklist

- Make the shared Compose Postgres image and data mount target environment
  controlled.
- Add PG 18 env overrides to dev/test/rehearsal env examples.
- Add dev/test/rehearsal-only Postgres commands for `io_method`.
- Add dev/test/rehearsal-only Postgres `seccomp=unconfined`.
- Add the temporary env names to env-policy known optional values.
- Update English and Chinese docs for the split Postgres version policy and
  disposable-volume reset requirements.
- Verify rendered Compose:
  - dev/test/rehearsal use `postgres:18-alpine`
  - dev/test/rehearsal mount `/var/lib/postgresql`
  - dev/test/rehearsal run `io_method=io_uring`
  - production defaults remain `postgres:16-alpine` and
    `/var/lib/postgresql/data`
- Verify runtime after reset:
  - `SHOW server_version;`
  - `SHOW io_method;`
  - `SHOW effective_io_concurrency;`
- Run `just test-env-status-cpu`, `just test-all-cpu`, `just test-env-status`,
  and `just test-all` when GPU verification is available.

## Verification Result

Completed on 2026-06-10:

- `just test-env-status-cpu` passed.
- `just test-all-cpu` passed, including non-ignored Compose integration tests
  against PostgreSQL 18.
- `just test-env-status` passed, including NVIDIA GPU visibility, CUDA runner
  image availability, and Docker GPU device-request smoke.
- `just test-all` passed, including the ignored CUDA smoke integration test.

The full GPU suite confirmed `dgx_cuda_smoke_completes_official_result_and_leaderboard`
passes with the disposable PostgreSQL 18 test environment.

## Follow-Up TODOs

- Execute the production PostgreSQL 18 dump/restore cutover from
  `docs/deployment/en.md` and `docs/deployment/zh.md`.
- After production is migrated and the rollback window is closed, delete the
  temporary DB-related env vars and return the Compose base to a single PG 18
  configuration path.
