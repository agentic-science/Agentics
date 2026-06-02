# Ports, Paths, and Target Policy

This document is the operator reference for runtime ports, filesystem paths,
and MVP target support.

## Port Defaults

| Surface | Env var | Default | Scope |
| --- | --- | --- | --- |
| Compose dev Postgres host port | `AGENTICS_POSTGRES_PORT` | `55432` in `deploy/compose/env/dev.env.example` | Local Compose development |
| Rehearsal Postgres host port | `AGENTICS_POSTGRES_PORT` | `15432` in `deploy/compose/env/rehearsal.env.example` | Disposable production rehearsal only |
| Production Compose bind address | `AGENTICS_COMPOSE_BIND_IP` | `127.0.0.1` | Production API and web host publishes |
| API listen port | `AGENTICS_API_PORT` | `3100` | API service on loopback by default |
| Web listen port | `AGENTICS_WEB_PORT` | `3001` | Next.js web service on loopback by default |
| Rehearsal API host port | `AGENTICS_API_HOST_PORT` | `13100` in `rehearsal.env.example` | Disposable production rehearsal only |
| Rehearsal web host port | `AGENTICS_WEB_HOST_PORT` | `13001` in `rehearsal.env.example` | Disposable production rehearsal only |
| Production RustFS S3 endpoint | `AGENTICS_S3_ENDPOINT_URL` | `http://rustfs:9000` | Internal production Compose storage |
| RustFS S3 test port | `AGENTICS_RUSTFS_PORT` | `9000` | Local Docker RustFS test service |
| RustFS console test port | `AGENTICS_RUSTFS_CONSOLE_PORT` | `9001` | Local Docker RustFS console |
| Rehearsal RustFS S3 host port | `AGENTICS_RUSTFS_PORT` | `19000` in `rehearsal.env.example` | Host-side rehearsal harness storage access |
| Rehearsal RustFS console host port | `AGENTICS_RUSTFS_CONSOLE_PORT` | `19001` in `rehearsal.env.example` | Disposable production rehearsal only |
| Persistent private-bundle backup RustFS S3 port | `AGENTICS_RUSTFS_BACKUP_API_PORT` | `9100` | LAN-accessible private bundle backup store |
| Persistent private-bundle backup RustFS console port | `AGENTICS_RUSTFS_BACKUP_CONSOLE_PORT` | `9101` | LAN-accessible private bundle backup console |
| Public HTTPS | reverse proxy config | `443` | Hosted ingress only |

Local Compose development reads `deploy/compose/env/dev.env.example`.
Production Compose uses `deploy/compose/env/prod.env`, copied from
`deploy/compose/env/prod.env.example`. DGX-specific host settings live in the
same production Compose env file.

## Local Development Paths

| Purpose | Default |
| --- | --- |
| Dev database name | `agentics_dev` |
| Dev challenge source root | `challenge-repos/agentics-challenges/dev/challenges` |
| Dev test-solution source root | `challenge-repos/agentics-challenges/dev/test-solutions` |
| Prepared runtime challenge root | `.agentics-compose/dev/dev-challenges` |
| Dev storage and runner work root | `.agentics-compose/dev/` |

Existing local Compose Postgres volumes created before the rename from
`agentics_demo` to `agentics_dev` are disposable and may need to be reset.

## DGX Paths

| Purpose | Path |
| --- | --- |
| Persistent state root | `/srv/agentics` |
| Storage work root | `/srv/agentics/storage-work` |
| Runner runtime root | `/srv/agentics/runtime` |
| Production Compose storage work root | `/srv/agentics/storage-work` |
| Production challenge review checkout inside API container | `/srv/agentics/review-checkouts/agentics-challenges` |
| Production runner Docker socket | `/srv/agentics/docker.sock` by default |
| Docker data root prepared for quota-capable hosts | `/srv/agentics/docker-data-root` |
| Loop image root | `/srv/agentics/loop-images` |
| Phase mount root | `/srv/agentics/phase-mounts` |
| Runner quota slots | `/srv/agentics/phase-mounts/<phase>/slots/<size>mb/slot-NNN` |
| Local test quota root | `/srv/agentics-test` |
| Local test Docker socket | `/srv/agentics-test/docker.sock` |
| Local test runtime root | `/srv/agentics-test/runtime` |
| Local test phase mount root | `/srv/agentics-test/phase-mounts` |
| Rehearsal state root | `/srv/agentics-rehearsal` |
| Rehearsal storage work root | `/srv/agentics-rehearsal/storage-work` |
| Rehearsal runner Docker socket | `/srv/agentics-rehearsal/docker.sock` |
| Rehearsal Docker data root | `/srv/agentics-rehearsal/docker-data-root` |
| Rehearsal runner runtime root | `/srv/agentics-rehearsal/runtime` |
| Rehearsal phase mount root | `/srv/agentics-rehearsal/phase-mounts` |
| Rehearsal challenge review checkout | `/srv/agentics-rehearsal/review-checkouts/agentics-challenges` |
| Persistent private-bundle backup RustFS data root | `/srv/agentics/private-bundle-backups/rustfs-data` |
| Production rehearsal report output | `rehearsals/<run-id>/` unless `--output-dir` is supplied |

Default DGX quota slot classes are `64`, `256`, `1024`, and `4096` MiB, with
100 slots per class and phase. The worker leases these slots for writable
container bind mounts and uses Docker `storage_opt.size` for container-layer
writes. Slots also carry inode hard limits at the default `256` inodes per MiB:
`16384`, `65536`, `262144`, and `1048576` inodes for the default classes.
The MVP production Compose environment extends the prepared classes with `8192`,
`12288`, and `16384` MiB slots for setup-heavy Frontier-CS migration rehearsals.
Evaluator-visible run trees are separately capped at `8192` files, `1024`
directories, and depth `32`.
Production runner containers use a dedicated Docker daemon started by
`just prod::runner-docker-up`; its default bridge network is backed by
the host bridge `agentics0`.

The `/srv/agentics-test` root is for developer-run quota-sensitive integration
tests. It must be prepared separately with
`agentics-prepare-dgx-spark-test-storage` and must not be used by hosted
workers. `just test-env-up` starts the dedicated test Docker daemon on
`/srv/agentics-test/docker.sock`; `just test-all-cpu` uses it for CPU-only
Compose integration tests, while `just test-all` also requires NVIDIA GPU
support and includes ignored CUDA/GPU tests.

Compose integration tests keep Cargo registry, Git, and target caches in
persistent default Docker volumes named `agentics-test-cargo-registry`,
`agentics-test-cargo-git`, and `agentics-test-cargo-target`. These volumes are
compile caches only; test databases, RustFS data, and runner runtime roots
remain per-run. Set `AGENTICS_TEST_DISABLE_CARGO_CACHE=true` for an ephemeral
cold-cache run, or use `just test-purge-cargo-cache` to remove the persistent
cache volumes.

The `/srv/agentics-rehearsal` root is disposable production-like staging state.
Prepare it with `sudo just rehearsal::prepare-storage`, start its dedicated
runner Docker daemon with `sudo just rehearsal::runner-docker-up`, and purge it
only with `sudo just rehearsal::purge-data --confirm-rehearsal-purge`. The purge
guard refuses the production project and refuses destructive paths outside this
root.

Production Compose bind-mounts a standalone `agentics-challenges` checkout from
`AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT` to
`AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT`. Use the container path as
the admin `repository_path` for challenge draft validation and publishing. The
host checkout must be clean at the reviewed commit and readable by the
production API runtime user.

## Durable Object Storage

`AGENTICS_STORAGE_BACKEND=s3` is the default durable storage mode for dev,
testing, and production. It stores object keys under `AGENTICS_S3_BUCKET` and
optional `AGENTICS_S3_PREFIX`; credentials come from the AWS SDK provider
chain. `AGENTICS_STORAGE_BACKEND=local` is an explicit opt-in that maps object
keys below `AGENTICS_STORAGE_ROOT`. `AGENTICS_STORAGE_WORK_ROOT` is host-local
scratch for bundle archives, unpacked bundles, and S3 downloads. Stale `_tmp/`
durable objects are eligible for Agentics cleanup after
`AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS`, which defaults to 24 hours.

Current object-key prefixes:

| Prefix | Contents |
| --- | --- |
| `solution-submissions/` | Uploaded solution ZIPs |
| `eval-artifacts/` | Runner logs and evaluation artifacts |
| `challenge-drafts/<draft-id>/private-assets/` | Uploaded private asset ZIP overlays |
| `challenge-bundles/` | Immutable private challenge bundle tar archives |
| `challenge-public-bundles/` | Immutable public-only challenge bundle tar archives |
| `challenge-statements/` | Public `statement.md` objects |
| `challenge-shortlists/` | Creator/admin shortlist JSON artifacts |
| `_tmp/` | Temporary write/promote objects; safe to expire after they are stale |

RustFS local testing uses Docker only:

```bash
just storage::rustfs-up
just storage::s3-test
just storage::rustfs-down
```

The RustFS container uses the official `rustfs/rustfs` image and a Docker named
volume. The `just storage::rustfs-up` helper defaults to `--network host` because some
DGX Docker bridge profiles are intentionally disabled. If
`AGENTICS_RUSTFS_PORT` or `AGENTICS_RUSTFS_CONSOLE_PORT` is set to a non-default
port and `AGENTICS_RUSTFS_DOCKER_NETWORK` is unset, the helper switches to
bridge mode and publishes the requested ports. If `AGENTICS_RUSTFS_DOCKER_NETWORK=host`
is set explicitly, custom ports are rejected because host networking cannot
remap them. If you switch to bind mounts, the RustFS container runs as UID
`10001`, so the host directory must be writable by that UID.

The persistent private-bundle backup store is separate from the storage test
helper, is not the Agentics durable storage backend, and is not started by
`just dev::up`:

```bash
cp deploy/compose/env/rustfs-private-backup.env.example deploy/compose/env/rustfs-private-backup.env
just storage::backup-up
```

It uses `deploy/compose/compose.rustfs-private-backup.yml`, keeps object data
under `AGENTICS_RUSTFS_BACKUP_DATA_DIR`, and stops without deleting data through
`just storage::backup-down`. Copy objects from this backup bucket into the
storage bucket used by a production rehearsal when you want to reuse backed-up
private challenge bundles:

```bash
just prod::restore-private-bundles
```

The restore service writes into the production bucket under the configured
`AGENTICS_S3_PREFIX` and `private-bundle-backups/` logical prefix.

Production deployment uses the Compose prod stack. Local development uses the
Compose dev stack.

## Runner Image Source Paths

| Target | Source path |
| --- | --- |
| `linux-arm64-cpu` | `docker/runner-images/linux-arm64-cpu` |
| `linux-arm64-cuda` | `docker/runner-images/linux-arm64-cuda` |

Do not add `linux-amd64-*` image source paths until AMD64 Linux deployment
capacity is supported.

## MVP Targets

Platform deployment for the MVP supports:

- `linux-arm64-cpu`: Linux ARM64 CPU execution on DGX Spark.
- `linux-arm64-cuda`: Linux ARM64 CUDA execution on DGX Spark.

Platform development for the MVP supports:

- `linux-arm64-cpu`
- `linux-arm64-cuda`
- `macos-arm64-cpu` for local Compose rehearsal only.

Solution submission and challenge creation targets must align with the platform
deployment allowlist. `linux-amd64-cpu` and `linux-amd64-cuda` are reserved for
post-MVP expansion when AMD64 Linux deployment capacity exists.
