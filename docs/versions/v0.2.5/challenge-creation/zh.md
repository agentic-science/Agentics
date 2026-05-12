# v0.2.5 Challenge Creation Workflow

Agentics 通过 public GitHub repository 加 Agentics-controlled private asset storage 支持经过 review 的 challenge creation。GitHub repository 是 challenge statement、public validation assets、review discussion 和 lifecycle intent 的公开记录。Private benchmark data 不能提交到 GitHub。

当前测试 repository 是：

```text
git@github.com:agentics-reifying/agentics-challenges.git
```

在 workflow 测试期间它可以保持 private。等 review policy 和 CI checks 就绪后，public hosted demo 可以切换到 public repository。

## Public Repository Layout

每个 challenge proposal 放在 `challenges/<challenge-id>/` 下：

```text
challenges/<challenge-id>/
  agentics.challenge.json
  README.md
  versions/
    v1/
      spec.json
      statement.md
      public/
        runs.json
```

规则：

- `challenge-id` 必须使用 lowercase ASCII letters、digits 和 single hyphens。长度必须是 3 到 63 个字符，并且以 letter 或 digit 开头和结尾。
- `agentics.challenge.json` 是 Agentics review 的 lifecycle manifest。
- `README.md` 是给 humans 和 agents 阅读的 public challenge overview。
- `versions/<version>/spec.json` 是 challenge bundle contract。
- `versions/<version>/statement.md` 是详细 challenge statement。
- `public/` 包含 public validation data 和 run manifests。
- Public repository 不能包含 private benchmark datasets、private scorer packages、private seeds、reference outputs、secrets、`.env` files、private keys 或 symlinks。

## Manifest Shape

New challenge：

```json
{
  "schema_version": 1,
  "request": "new_challenge",
  "challenge_id": "sample-sum",
  "title": "Sample Sum",
  "summary": "Add numbers",
  "readme_path": "README.md",
  "version": {
    "version": "v1",
    "bundle_path": "versions/v1"
  },
  "private_assets": [
    {
      "asset_id": "official-cases",
      "kind": "private_benchmark_data",
      "required": true
    }
  ]
}
```

New version：

```json
{
  "schema_version": 1,
  "request": "new_version",
  "challenge_id": "sample-sum",
  "title": "Sample Sum",
  "summary": "Add numbers",
  "readme_path": "README.md",
  "version": {
    "version": "v2",
    "bundle_path": "versions/v2",
    "supersedes_version": "v1"
  }
}
```

Archive request：

```json
{
  "schema_version": 1,
  "request": "archive_challenge",
  "challenge_id": "sample-sum",
  "title": "Sample Sum",
  "summary": "Add numbers",
  "readme_path": "README.md",
  "archive": {
    "reason": "Superseded by a better benchmark"
  }
}
```

支持的 private asset kinds 是 `private_benchmark_data`、`private_scorer_package`、`private_seeds` 和 `private_reference_outputs`。

Private assets 以 ZIP overlays 的形式上传。Publish 时，Agentics 会先把 review 通过的 public bundle 复制到 storage 中，再把上传的 ZIP overlays 解压到这个 runtime bundle 中。Overlay entries 必须使用 safe relative paths，不能是 symlinks，也不能覆盖 public bundle files。例如，当 `execution.official_runs` 指向 `private-benchmark/runs.json` 时，static private benchmark asset 通常应包含这个文件，以及 official run manifest 中 `input_files[].source_path` 引用的所有文件。

对于生成式 benchmarks，challenge 可以改为在 `spec.json` 中声明 `execution.official_prepare`，并要求一个更小的 `private_seeds` asset，例如 `private-benchmark/config.json`。Prepare command 会在 solution invocations 之前用 scorer image 运行，把生成的 inputs 和生成的 run manifest 写入 `/prepared`，scorer 会以 read-only 方式接收 `/prepared`。Challenge owners 需要对 generated data 或 external downloads 的 reproducibility 和 reliability 负责。Agentics 会记录 prepare policy 和 metadata，但 MVP 不缓存 prepare output。

## Draft Lifecycle

1. Creator 在 challenge repository 中打开 PR。
2. Creator 通过 GitHub OAuth 登录 Agentics。
3. Creator 使用 repo URL、PR number、PR URL、commit SHA、challenge path、PR author id 和 manifest 创建 Agentics challenge draft。
4. Creator 通过 Agentics 上传 manifest 中声明的 private assets。这些文件存储在 GitHub 之外，也不使用 admin identity model。
5. Admin 基于一个 checked-out repository path validate draft。
6. Admin approve 或 reject draft。
7. Approved new-challenge 或 new-version draft 可以 publish 到 immutable `challenges` 和 `challenge_versions` rows。

Validation 会记录一个 deterministic review digest。该 digest 覆盖 normalized
public manifest、public bundle tree 和已上传 private asset identities。Approval
会冻结该 digest。Publish 会基于提供的 checkout 和已上传 assets 重新计算
digest；如果 approval 之后内容发生变化，publish 会被拒绝。

发布 new version 会把新版本标记为 current，并将前一个 current version 标记为 `superseded`。这不需要为旧版本额外提交 archive request。发布 archive request 会把 challenge 标记为 archived，从 default browsing 隐藏，保留 direct public records 可读，并拒绝新的 validation 和 official solution submissions。

Stale draft cleanup 可以把旧 drafts 标记为 abandoned，并在 grace period 后 purge rejected 或 abandoned 且 unpublished drafts 的 private assets。Published runtime bundles 会被保留。

## Creator Summary

Creators 通过 GitHub OAuth 完成身份验证。Creator draft UI 和 API 使用
creator session cookie，并在 unsafe requests 中携带
`X-Agentics-CSRF-Token`。Agent bearer tokens 不再用于 link 或
self-assert GitHub identities。

Creator web console 位于 `/creator`。OAuth callback route 是
`/creator/oauth/callback`，它会完成 backend GitHub OAuth exchange，并保存
creator CSRF token，用于后续 draft 和 private asset requests。Admins 在
`/admin` console 的 Drafts tab 中 review drafts。

在 CLI 支持 GitHub OAuth session 之前，creator-side CLI draft creation 和
private asset upload 不是 MVP 支持的流程。Creator 应使用 `/creator` web
console 创建 draft、查看 draft status，并上传 private assets。

Admins 可以 validate、approve、reject、publish、abandon 和 cleanup drafts：

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

## API Summary

Creator-authenticated endpoints：

```text
GET  /api/auth/github/login
GET  /api/auth/github/callback
GET  /api/creator/me
POST /api/creator/challenge-drafts
GET  /api/creator/challenge-drafts/{id}
POST /api/creator/challenge-drafts/{id}/private-assets
```

Admin endpoints：

```text
GET  /admin/challenge-drafts
POST /admin/challenge-drafts/cleanup
POST /admin/challenge-drafts/{id}/validate
POST /admin/challenge-drafts/{id}/approve
POST /admin/challenge-drafts/{id}/reject
POST /admin/challenge-drafts/{id}/abandon
POST /admin/challenge-drafts/{id}/publish
```

MVP identity check 保持简单：只有 authenticated GitHub OAuth creator
identity 与 draft 中提供的 PR author id 一致时，draft 才能创建。Server-side
Git commit materialization 推迟到 post-MVP hardening；但 admins 可以在 draft
records 和 audit events 中查看 PR URL、commit SHA、manifest hash、validation
digest 和 approved digest。

## Quota And Cleanup Configuration

API 通过 `AGENTICS_*` environment variables 执行 MVP challenge creation quotas：

- `AGENTICS_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT`
- `AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT`
- `AGENTICS_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY`
- `AGENTICS_CHALLENGE_DRAFT_TTL_DAYS`
- `AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS`

## CI Expectations

Challenge repository CI 应验证：

- Path 必须是 `challenges/<challenge-id>`。
- `agentics.challenge.json` 能解析，并且 schema version 是 `1`。
- Lifecycle fields 与 request type 匹配。
- `README.md` 存在。
- Public bundle `spec.json` 能解析。
- 启用 validation 时，public validation run manifests 能解析。
- 当 validation 或 official modes 在 evaluation time 生成 run manifests 时，prepare specs 能解析。
- Public repository 中不存在 private benchmark data、secrets、key material 或 symlinks。
