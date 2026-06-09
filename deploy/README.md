# Deployment Assets

This directory contains internal Agentics deployment assets.

## Layout

- `compose/`: Docker Compose files and example environment files for
  development, testing, production, disposable production rehearsals, and
  support services.
- `service-images/rust-toolchain/`: internal Rust build/test toolchain image
  with Homebrew LLVM 22, Homebrew `cargo-binstall`, and Wild linker
  configuration.
- `service-images/app/`: production app image build for the API, worker,
  migrations, and operational binaries. Its builder stage installs the same
  internal LLVM/Wild toolchain, while the final runtime image stays slim.
- `service-images/web/`: production web image build.

The app image includes the `agentics-rehearse-production` ops binary used by
`just rehearsal::run` and `just rehearsal::run-cpu` for the disposable
`agentics-rehearsal` environment. It also includes the public migrated challenge
catalog from `challenge-repos/agentics-challenges/challenges` at
`/app/challenges`; production and rehearsal startup seeding expect private
benchmark ZIP overlays to have been restored from the backup RustFS into the
runtime RustFS namespace before the API starts. Copy
`compose/env/rehearsal.env.example` to ignored `compose/env/rehearsal.env`,
replace placeholders, prepare `/srv/agentics-rehearsal`, then use the
`just rehearsal::*` lifecycle commands. The rehearsal Compose override exposes
Postgres and RustFS only on loopback so the host-side rehearsal harness can
exercise disposable fixture workflows without touching production namespaces.
The committed source of truth is `compose/env/rehearsal.env.example` plus
`compose/compose.rehearsal.yml`; `compose/env/rehearsal.env` is ignored.

Dev, test, and disposable rehearsal Compose environments opt in to
`postgres:18-alpine`, mount Postgres data at `/var/lib/postgresql`, and run
`io_method=io_uring`. Their Postgres services use `seccomp=unconfined` because
the current Docker default seccomp profile blocks PG 18 `io_uring`. Production
keeps the base `postgres:16-alpine` default until the operator performs the
documented dump/restore cutover. The PG 18 production cutover uses
`AGENTICS_POSTGRES_VOLUME=postgres_data_pg18`, so the old PG 16 `postgres_data`
volume remains available for rollback. The production Compose wrapper refuses
to run PG 18 against the old default volume.

These files are platform implementation details. Challenge specs and target
contracts must not reference Dockerfiles or images under `deploy/service-images/`.
Public runner image contracts live under `docker/runner-images/`.

The default internal Rust image tag is
`agentics-rust-toolchain:bookworm-llvm22-local`. Compose development and
integration-test services build it from `service-images/rust-toolchain/` when
needed. Public runner images under `docker/runner-images/` are intentionally not
changed by this internal toolchain image; adding LLVM/Wild there requires a
separate runner-image release and digest update.
