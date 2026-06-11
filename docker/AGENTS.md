# For Agents

This directory contains public runner image assets.
Read [README.md](README.md) before changing files here.

Rules:

- Keep runner image behavior documented in the colocated runner image README.
- Do not add platform service image Dockerfiles under `docker/`; those belong in `deploy/service-images/`.
- Challenge specs may reference runner images and their published registry tags or digests, but must never reference internal service images.
