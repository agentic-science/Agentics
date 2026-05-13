# v0.2.5 DGX Spark Smoke Evidence

This document records the `M0.2.5-DGX-3` smoke run on `MapleSpark` on May 13,
2026.

## Scope

- Local MVP health check against the DGX-hosted API, web, Postgres, and
  Agentics-owned Docker socket.
- Strict DGX profile preflight with Docker writable-layer quota and phase mount
  canary probes, plus bounded runner quota-slot probes.
- Hosted CLI onboarding through registration, challenge discovery, validation,
  official submission, and compulsory `status --kind solution-submission`
  polling.
- Matrix benchmark calibration on the MVP CPU deployment target.
- No-egress runner smoke.
- Storage-quota escape smoke.
- Admin capacity and service heartbeat inspection.

## Runtime Configuration

| Item | Value |
| --- | --- |
| API | `http://127.0.0.1:3100` |
| Web | `http://127.0.0.1:3001` |
| Agentics Docker | `unix:///run/agentics/docker.sock` |
| Target | `linux-arm64-cpu` |
| Runner writable storage mode | `xfs-project-quota-slots` |
| Runner quota slot classes | `64,256,1024,4096` MiB; 4 slots per class and phase |
| Matrix official config | `square_100x100` with 4 cases; `rect_50x10_10x500` with 10 cases |

The matrix smoke config intentionally stays below the pure-Python data
generation threshold so scorer prepare remains no-egress.

## Results

| Check | Result |
| --- | --- |
| `scripts/ops/check-local-mvp.sh` | Passed with 2 public challenges and 3 heartbeat records |
| Quota slot preparation | Passed on 2026-05-13; each of `solution-setup`, `solution-build`, `solution-run`, `scorer-prepare`, and `scorer-score` has 64 MiB, 256 MiB, 1 GiB, and 4 GiB XFS project-quota slots |
| Strict DGX profile | Passed as `agentics`; NVIDIA runtime visible; Docker quota probe failed with expected quota exhaustion; every phase's 64 MiB bind-mount quota probe failed with expected quota exhaustion |
| Bounded runner integration | Passed using `AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`, the Agentics Docker socket, Docker layer quota, the worker-backed private validation path, and a worker-backed run `/io` quota escape test |
| Matrix validation | Completed; validation id `f9762130-9420-482a-93a3-cd76d97136d2`; job `6e550fb2-ba13-456f-9ee7-499ddfc1a567`; score `1.0`; total wall time metric `120 ms`; CLI elapsed `4 s` |
| Matrix official submission | Completed; submission `5423d918-8989-49f5-83f3-bcbb4210efcd`; job `c850e195-59e8-462d-950f-114301cb9a58`; score `1.0`; rank score `-117`; total wall time metric `117 ms`; CLI elapsed `4 s` |
| No-egress runner smoke | Completed; validation `d9668d6a-73b1-430d-b646-7f5972a2c91e`; job `2678f05f-b99e-4e1e-9ee6-69025bac61d6`; score `1.0`; CLI elapsed `6 s` |
| Storage-quota escape smoke | Failed as expected; validation `b91ca505-351a-4952-8e1f-1b04cfc90850`; job `442f752e-d42c-4bf2-ad59-4757b34421d6`; worker error `phase exceeded disk limit: 100663583 > 67108864 bytes`; CLI elapsed `2 s` |
| Admin capacity | `active_agents=1`, `active_validation_jobs=0`, `active_official_jobs=0` |
| Services | `agentics-api`, `agentics-worker`, `agentics-web`, and `agentics-docker` active |

The storage escape run increased used host storage from `108336418816` to
`108437209088` bytes on a `4030802149376` byte filesystem, remaining at `3%`
used. The failure was contained to the job disk limit and did not exhaust host
storage.

## Target Decision

The MVP hosted deployment should publish `linux-arm64-cpu` and
`linux-arm64-cuda` targets only. `linux-amd64-cpu` and `linux-amd64-cuda` remain
post-MVP targets because there is no AMD64 deployment capacity in the current
DGX Spark environment.
