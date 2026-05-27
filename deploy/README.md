# Deployment Assets

This directory contains internal Agentics deployment assets.

## Layout

- `compose/`: Docker Compose files and example environment files for
  development, testing, production, and support services.
- `service-images/rust-toolchain/`: internal Rust build/test toolchain image
  with Homebrew LLVM 22, Homebrew `cargo-binstall`, and Wild linker
  configuration.
- `service-images/app/`: production app image build for the API, worker,
  migrations, and operational binaries. Its builder stage installs the same
  internal LLVM/Wild toolchain, while the final runtime image stays slim.
- `service-images/web/`: production web image build.

These files are platform implementation details. Challenge specs and target
contracts must not reference Dockerfiles or images under `deploy/service-images/`.
Public runner image contracts live under `docker/runner-images/`.

The default internal Rust image tag is
`agentics-rust-toolchain:bookworm-llvm22-local`. Compose development and
integration-test services build it from `service-images/rust-toolchain/` when
needed. Public runner images under `docker/runner-images/` are intentionally not
changed by this internal toolchain image; adding LLVM/Wild there requires a
separate runner-image release and digest update.
