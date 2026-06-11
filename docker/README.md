# Docker Assets

This directory is for public runner image assets that are part of the Agentics challenge execution contract.

## Layout

- `runner-images/linux-arm64-cpu/`: first-party CPU runner image source for the `linux-arm64-cpu` target.
- `runner-images/linux-arm64-cuda/`: first-party CUDA runner image source for the `linux-arm64-cuda` target.

Runner images may be referenced by challenge specs, target documentation, smoke tests, and published registry tags or digests.
Keep their READMEs focused on participant/evaluator image behavior, dependency tools, target compatibility, tagging, digest policy, and smoke validation.

Internal platform service images do not live here.
API, worker, ops, migration, and web image builds are deployment implementation details under `deploy/service-images/`.
