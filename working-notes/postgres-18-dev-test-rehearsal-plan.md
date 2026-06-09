# PostgreSQL 18 Dev/Test/Rehearsal Upgrade Plan

Date: 2026-06-10

## Summary

Dev, test, disposable production rehearsal, and production now run PostgreSQL
18 through `postgres:18-alpine`. The shared Compose base owns the PG18 image,
data mount, `io_method=io_uring`, and active `postgres_data_pg18` volume, so
the temporary DB override env vars are no longer part of normal operation.

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

Decision: use `io_uring` as the default for all Compose Postgres services and
set Postgres-only `security_opt: [seccomp=unconfined]` in the shared Compose
service.

## Implementation Checklist

- Make the shared Compose Postgres image and data mount target PG18 by default.
- Remove temporary PG image, volume, data-mount, and I/O-method env overrides.
- Add the shared Postgres command for `io_method=io_uring`.
- Add shared Postgres-only `seccomp=unconfined`.
- Update English and Chinese docs for the split Postgres version policy and
  disposable-volume reset requirements.
- Verify rendered Compose:
  - dev/test/rehearsal/production use `postgres:18-alpine`
  - dev/test/rehearsal/production mount `/var/lib/postgresql`
  - dev/test/rehearsal/production run `io_method=io_uring`
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

- After the production PG18 rollback window is closed, decide whether to delete
  the retained old `agentics-prod_postgres_data` volume and the cold PG16
  archive.
