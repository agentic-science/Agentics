# Move Internal Rust/App Images From Bookworm To Trixie

## Summary

Update Agentics internal Rust build/test/deployment images from Debian Bookworm
to Debian Trixie. Keep the existing three-role separation:

- internal Rust toolchain image for dev/test,
- app builder stage for production/rehearsal Rust binaries,
- app runtime stage for production/rehearsal execution.

Do not change public runner images, RustFS, web Bun images, or
challenge-facing image contracts in this change.

Follow-up scope added during implementation: remove the temporary
Postgres-version env override path now that production has been cut over. All
Compose stages use the shared PG18 service definition, active PG18 volume, and
`io_uring` setting.

## Key Changes

- Update the internal Rust toolchain image.
  - Change `deploy/service-images/rust-toolchain/Dockerfile` from
    `rust:1-bookworm@...` to digest-pinned `rust:1-trixie@...`.
  - Rename the local image tag from
    `agentics-rust-toolchain:bookworm-llvm22-local` to
    `agentics-rust-toolchain:trixie-llvm22-local`.
  - Keep Homebrew LLVM 22, `cargo-binstall`, Wild `0.9.0`,
    `/opt/cargo/config.toml`, and smoke checks unchanged.
  - Update Compose dev/test defaults and `justfiles/common.just` to use the new
    Trixie tag.

- Update the app image.
  - Change the app builder base in `deploy/service-images/app/Dockerfile` from
    `rust:1-bookworm@...` to the same digest-pinned `rust:1-trixie@...`.
  - Change the runtime stage from `debian:bookworm-slim` to digest-pinned
    `debian:trixie-slim`.
  - Keep runtime slim: do not copy Rust, Cargo, LLVM, Homebrew, or Wild into the
    final image.
  - Keep runtime packages limited to current needs: `ca-certificates`, `git`,
    and `xfsprogs`, unless Trixie package names force a minimal adjustment.

- Update docs and env examples.
  - Replace `bookworm-llvm22-local` with `trixie-llvm22-local` in dev/test env
    examples, deploy docs, operations docs, and README/deploy README references.
  - Update English and Chinese docs together.
  - Mention that public runner images remain unchanged and are a separate
    challenge-facing contract.

- Collapse temporary PG18 env overrides.
  - Remove `AGENTICS_POSTGRES_IMAGE`, `AGENTICS_POSTGRES_VOLUME`,
    `AGENTICS_POSTGRES_DATA_MOUNT`, and `AGENTICS_POSTGRES_IO_METHOD` from
    tracked env examples and ignored live dev/rehearsal/production env files.
  - Make the shared Compose Postgres service use `postgres:18-alpine`,
    `postgres_data_pg18`, `/var/lib/postgresql`, and `io_method=io_uring`.
  - Keep the old PG16 volume as rollback data until the cleanup window, but do
    not route any stage through PG16 by default.

## Version And Digest Handling

- Use current upstream Trixie image tags:
  - Rust builder/toolchain:
    `rust:1-trixie@sha256:fb328f0f58becb23ba1719940a2c94ece8b0b48afa837d05b79ef64bc1e18f6e`.
  - Runtime:
    `debian:trixie-slim@sha256:b6e2a152f22a40ff69d92cb397223c906017e1391a73c952b588e51af8883bf8`.
- Resolve digests before editing:
  - `docker buildx imagetools inspect rust:1-trixie`.
  - `docker buildx imagetools inspect debian:trixie-slim`.
- Record the chosen digests in Dockerfile `ARG` and `FROM` lines.

## Verification

- Build and smoke the internal toolchain image:

  ```bash
  docker build --network host -t agentics-rust-toolchain:trixie-llvm22-local \
    deploy/service-images/rust-toolchain
  docker run --rm --network none agentics-rust-toolchain:trixie-llvm22-local \
    /opt/agentics/smoke-rust-toolchain.sh
  ```

- Build the app image:

  ```bash
  just prod::build
  ```

- Render Compose and confirm:
  - dev/test configs use `agentics-rust-toolchain:trixie-llvm22-local`,
  - rehearsal/prod app builds use Trixie bases,
  - no public runner image references change.

- Run canonical checks:

  ```bash
  just test-env-status-cpu
  just test-all-cpu
  just test-env-status
  just test-all
  ```

- Live environment checks:
  - bring dev services up and back down;
  - bring rehearsal services up, restore private bundles, check, and bring them
    back down;
  - restart production services and verify production `SHOW server_version;`
    returns PG18 and `SHOW io_method;` returns `io_uring`.

## Assumptions

- `rust:1-trixie` and `debian:trixie-slim` are available for the host
  architecture used by dev/test/prod builds.
- Homebrew Linux, LLVM 22, `cargo-binstall`, and Wild work on Trixie without
  installer changes.
- The app runtime still only needs `ca-certificates`, `git`, and `xfsprogs`.
- Public runner images under `docker/runner-images/` are not part of this
  change.
