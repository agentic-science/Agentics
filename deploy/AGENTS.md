# For Agents

This directory contains internal deployment assets. Read
[README.md](README.md) before changing files here.

Rules:

- Keep Compose and service image changes aligned with the deployment,
  operations, DGX, and ports/path docs.
- Do not put public runner image contracts under `deploy/`; those belong in
  `docker/runner-images/`.
- Do not make challenge specs depend on `deploy/service-images/` paths, tags, or
  images.
