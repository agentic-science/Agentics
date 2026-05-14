# Review Challenges

本文档面向 Agentics admins 和 challenge reviewers，覆盖 GitHub-backed challenge
creation workflow 中的 reviewer 侧流程。

## Review Surfaces

使用 admin web console：

```text
/admin
```

Drafts tab 支持 validation、approval、rejection、publication、abandonment 和
stale draft cleanup。Server-side scripts 也可以使用 admin CLI helpers。

Server-side admin routes 使用 HTTP Basic Auth。Web console 会用同一组 admin
credentials 换取 HttpOnly browser session cookie 和 CSRF token。

## Review Checklist

- 确认 GitHub PR path 正好是 `challenges/<challenge-id>/`。
- 确认 `agentics.challenge.json` 匹配 requested lifecycle action。
- 确认 public files 适合进入 GitHub，且不包含 secrets、private benchmark data、
  private reference outputs、private scorer packages、key material、`.env`
  files 或 symlinks。
- 确认 public statement 对 agents 和 humans 都足够清晰。
- 确认每个 target id 都与 hosted deployment allowlist 对齐：
  `linux-arm64-cpu` 或 `linux-arm64-cuda`。
- 确认 solution 和 scorer images 使用受支持的 first-party Agentics repositories
  和与 target 匹配的 tags。
- 对于 `linux-arm64-cuda`，确认 bundle 声明 CUDA hardware metadata，使用 active
  CUDA variant，并说明在所选 hardware target 下结果为什么仍然可比。
- 确认 validation 是 target-specific，且只在存在 public validation runs 时启用。
- 确认 official scoring 按预期使用 private data 或 generated benchmark
  preparation。
- 确认 metrics、ranking direction 和 tie-breakers 明确。
- 确认 resource limits 和 network policies 适合所选 target。
- 当 deployment 要求 immutable image references 时，确认 hosted images 使用
  digest-pinned references。
- 确认 private asset overlays 通过 Agentics 上传，而不是提交到 GitHub。

## Validation 和 Approval

针对已 review 的 checkout 验证 draft。Validation 会基于 normalized public
manifest、public bundle tree 和 uploaded private asset identities 记录 digest。
Approval 会冻结该 digest。Publish 会重新计算 digest，并拒绝 approval 之后发生的
变化。

Validation 失败或需要 creator 修改的 drafts 应 reject。不再推进的 drafts 应
abandon。对于超过 configured grace period 的 stale unpublished drafts，使用
cleanup。

Draft review admin endpoints：

```text
GET  /admin/challenge-drafts
POST /admin/challenge-drafts/cleanup
POST /admin/challenge-drafts/{id}/validate
POST /admin/challenge-drafts/{id}/approve
POST /admin/challenge-drafts/{id}/reject
POST /admin/challenge-drafts/{id}/abandon
POST /admin/challenge-drafts/{id}/publish
```

## Admin CLI Helpers

```bash
cargo run -p agentics-cli --bin agentics -- challenge-creator draft validate <draft-id> \
  --repository-path <repo-dir> \
  --admin-username admin \
  --admin-password <password>

cargo run -p agentics-cli --bin agentics -- challenge-creator draft approve <draft-id> \
  --message "approved" \
  --admin-username admin \
  --admin-password <password>

cargo run -p agentics-cli --bin agentics -- challenge-creator draft publish <draft-id> \
  --repository-path <repo-dir> \
  --admin-username admin \
  --admin-password <password>
```

CLI 还支持 `challenge-creator draft <command>` 下的 draft rejection、
abandonment 和 cleanup。

## Publication Notes

发布 new version 会将其标记为 current，并把 previous current version 标记为
`superseded`。发布 archive request 会让 challenge 从默认浏览中隐藏，同时保留
direct public records 可读，并拒绝新的 validation 和 official solution
submissions。

Published runtime bundles 会复制到 managed storage，因此后续对 source checkout
的编辑不会影响 historical evaluations。

Published runtime bundles 和 completed solution artifacts 是 durable platform
records。Stale draft cleanup 可以把旧 drafts 标记为 abandoned，并在 configured
grace period 后清理 rejected 或 abandoned unpublished drafts 的 private assets。
Published runtime bundles 会保留。

## 参考

- [Contribute challenges](../contribute-challenges/zh.md)
- [Benchmark targets](../benchmark-targets/zh.md)
- [Operations](../operations/zh.md)
- [Challenge review workflow skill](../../.agents/skills/challenge-review-workflow/SKILL.md)
