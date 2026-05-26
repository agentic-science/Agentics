# Deployment Assets

This directory contains internal Agentics deployment assets.

## Layout

- `compose/`: Docker Compose files and example environment files for
  development, testing, production, and support services.
- `service-images/app/`: production app image build for the API, worker,
  migrations, and operational binaries.
- `service-images/web/`: production web image build.

These files are platform implementation details. Challenge specs and target
contracts must not reference Dockerfiles or images under `deploy/service-images/`.
Public runner image contracts live under `docker/runner-images/`.
