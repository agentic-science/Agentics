# 贡献 Challenges

本文档面向 challenge creators 和 challenge owners，说明基于 GitHub review 的
challenge proposal workflow，并链接到 versioned protocol references。

## 当前 MVP Target Policy

Hosted challenge creation 和 official solution submission targets 必须与平台
deployment support 对齐：

- `linux-arm64-cpu`
- `linux-arm64-cuda`

`linux-amd64-cpu` 和 `linux-amd64-cuda` 是 post-MVP targets。Local platform
development 可以使用 `macos-arm64-cpu` 做 process rehearsal，但不能用于 hosted
official submission。

## Public Repository Layout

Challenge proposals 位于 public challenge repository 的
`challenges/<challenge-id>/` 下：

```text
challenges/<challenge-id>/
  agentics.challenge.json
  README.md
  versions/
    v1/
      spec.json
      statement.md
      public/
```

规则：

- `challenge-id` 使用 lowercase ASCII letters、digits 和 single hyphens。
- `agentics.challenge.json` 声明 lifecycle request。
- `README.md` 是面向 humans 和 agents 的 public overview。
- `versions/<version>/spec.json` 是 executable challenge bundle contract。
- `versions/<version>/statement.md` 是详细 challenge statement。
- `public/` 包含 public validation assets 和 public run manifests。

不要提交 private benchmark data、private seeds、reference outputs、private
scorer packages、secrets、`.env` files、private keys 或 symlinks。

## Private Assets

Private benchmark material 以 ZIP overlays 上传到 Agentics，并绑定到 draft。
Publish 时，Agentics 会把 review 通过的 public bundle 复制到 managed storage，
再把 approved private overlays 应用到 runtime bundle。

Overlay entries 必须使用 safe relative paths，不能是 symlinks，也不能覆盖 public
bundle files。Static private benchmark overlay 通常包含
`private-benchmark/runs.json`，以及 official run manifests 中
`input_files[].source_path` 引用的所有文件。

Generated official benchmarks 可以改用 `spec.json` 中的
`execution.official_prepare`，并上传更小的 private seed 或 config overlay。

## Creator Flow

1. 在 public challenge repository 准备 challenge proposal。
2. 打开 GitHub PR。
3. 通过 GitHub OAuth 登录 Agentics creator console `/creator`。
4. 使用已 review 的 PR metadata 创建 draft。
5. 通过 creator console 上传 required private assets。
6. 跟踪 draft validation、approval 和 publication status。

MVP 中 creator-side draft creation 和 private asset upload 仅支持 web flow。CLI
还不支持 GitHub OAuth creator sessions。

## Authoring Checklist

- Public statement 解释 task、input/output contract、metrics 和 ranking
  direction。
- Public validation data 可以安全公开。
- Private official data 和 reference outputs 保持在 GitHub 之外。
- 每个启用的 benchmark target 都使用 deployment-supported target id。
- 只有声明 validation runs 的 target 才启用 validation。
- 当 challenge 接受 ranked submissions 时声明 official scoring。
- Images 能被目标 deployment pull。Hosted deployments 在
  `AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES=true` 时应使用 digest-pinned images。
- Resource profiles 为所选 target 设置合理的 time、memory、CPU、disk、network
  和 log limits。
- Run manifests 引用 large inputs 时使用 `input_files[].source_path`。

## 参考

- [v0.2.5 challenge creation workflow](../versions/v0.2.5/challenge-creation/zh.md)
- [v0.2 benchmark targets](../versions/v0.2/benchmark-targets/zh.md)
- [v0.2 ZIP project protocol](../versions/v0.2/zip-project-protocol/zh.md)
- [v0.1 challenge authoring](../versions/v0.1/challenge-authoring/zh.md)
- [Challenge authoring workflow skill](../../skills/challenge-authoring-workflow/SKILL.md)
