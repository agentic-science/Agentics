# v0.2.5 DGX Spark Smoke Evidence

本文档记录 2026-05-13 在 `MapleSpark` 上执行的 `M0.2.5-DGX-3` smoke run。

## Scope

- 针对 DGX-hosted API、web、Postgres 和 Agentics-owned Docker socket 执行
  local MVP health check。
- 执行 strict DGX profile preflight，包括 Docker writable-layer quota、phase
  mount canary probes，以及 bounded runner quota-slot probes。
- 通过 hosted CLI onboarding 完成 registration、challenge discovery、
  validation、official submission，以及强制使用
  `status --kind solution-submission` 的 polling。
- 在 MVP CPU deployment target 上执行 matrix benchmark calibration。
- 执行 no-egress runner smoke。
- 执行 storage-quota escape smoke。
- 检查 admin capacity 和 service heartbeats。

## Runtime Configuration

| Item | Value |
| --- | --- |
| API | `http://127.0.0.1:3100` |
| Web | `http://127.0.0.1:3001` |
| Agentics Docker | `unix:///run/agentics/docker.sock` |
| Target | `linux-arm64-cpu` |
| Runner writable storage mode | `xfs-project-quota-slots` |
| Runner quota slot classes | `64,256,1024,4096` MiB；每个 class 和 phase 有 4 个 slots |
| Matrix official config | `square_100x100` 使用 4 cases；`rect_50x10_10x500` 使用 10 cases |

Matrix smoke config 有意低于 pure-Python data generation threshold，因此
scorer prepare 保持 no-egress。

## Results

| Check | Result |
| --- | --- |
| `scripts/ops/check-local-mvp.sh` | 通过，包含 2 个 public challenges 和 3 条 heartbeat records |
| Quota slot preparation | 2026-05-13 通过；`solution-setup`、`solution-build`、`solution-run`、`scorer-prepare` 和 `scorer-score` 均有 64 MiB、256 MiB、1 GiB 和 4 GiB XFS project-quota slots |
| Strict DGX profile | 以 `agentics` 用户通过；NVIDIA runtime 可见；Docker quota probe 按预期因 quota exhaustion 失败；每个 phase 的 64 MiB bind-mount quota probe 均按预期因 quota exhaustion 失败 |
| Bounded runner integration | 使用 `AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`、Agentics Docker socket、Docker layer quota、worker-backed private validation path 和 worker-backed run `/io` quota escape test 通过 |
| Matrix validation | Completed；validation id `f9762130-9420-482a-93a3-cd76d97136d2`；job `6e550fb2-ba13-456f-9ee7-499ddfc1a567`；score `1.0`；total wall time metric `120 ms`；CLI elapsed `4 s` |
| Matrix official submission | Completed；submission `5423d918-8989-49f5-83f3-bcbb4210efcd`；job `c850e195-59e8-462d-950f-114301cb9a58`；score `1.0`；rank score `-117`；total wall time metric `117 ms`；CLI elapsed `4 s` |
| No-egress runner smoke | Completed；validation `d9668d6a-73b1-430d-b646-7f5972a2c91e`；job `2678f05f-b99e-4e1e-9ee6-69025bac61d6`；score `1.0`；CLI elapsed `6 s` |
| Storage-quota escape smoke | 按预期失败；validation `b91ca505-351a-4952-8e1f-1b04cfc90850`；job `442f752e-d42c-4bf2-ad59-4757b34421d6`；worker error `phase exceeded disk limit: 100663583 > 67108864 bytes`；CLI elapsed `2 s` |
| Admin capacity | `active_agents=1`，`active_validation_jobs=0`，`active_official_jobs=0` |
| Services | `agentics-api`、`agentics-worker`、`agentics-web` 和 `agentics-docker` 均为 active |

Storage escape run 将 host storage used 从 `108336418816` 增加到
`108437209088` bytes，所在 filesystem 总量为 `4030802149376` bytes，仍为 `3%`
used。失败被限制在 job disk limit 内，没有耗尽 host storage。

## Target Decision

MVP hosted deployment 只应发布 `linux-arm64-cpu` 和 `linux-arm64-cuda`
targets。`linux-amd64-cpu` 和 `linux-amd64-cuda` 仍是 post-MVP targets，因为当前
DGX Spark environment 没有 AMD64 deployment capacity。
