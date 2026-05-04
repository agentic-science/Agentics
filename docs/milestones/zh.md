# Agentics 里程碑

本里程碑文档与 PRD 必须在功能层面保持双向同步。当 PRD 新增、删除、重命名或改变某项功能范围时，必须在同一个变更中更新本文档。当本文档新增、删除、调整优先级或实质性改变某个里程碑时，必须检查英文和中文 PRD，并在功能范围变化时同步更新。

下面的每个里程碑都应直接对应一个聚焦的 commit。一个 commit 可以包含该里程碑的实现、测试和文档，但不应混入无关 feature lane 的变更。

## 规划约定

- **Version：** PRD roadmap 中的发布目标。
- **Lane：** 主要产品或工程界面。
- **Milestone：** commit 粒度的工作单元。
- **Commit target：** 建议的 commit message 和 scope。
- **Test spec：** commit 前应新增或运行的测试与检查。
- **实现进度：** 每个 major version section 的末尾都包含一个 `### 实现进度` 小节，使用三列表格：里程碑、状态、附加说明。

进度状态取值：

- `已实现`：该里程碑已有满足 scope 的已合并或可工作的 artifact。
- `进行中`：实现已经开始，但里程碑尚未完成。
- `计划中`：该里程碑属于版本计划，但尚未开始实现。
- `阻塞中`：该里程碑需要先解决明确的依赖或决策。
- `已推迟`：该里程碑被明确移出当前版本。

代码里程碑的标准提交前检查：

- Rust：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`，以及有针对性的 `cargo test` 或 integration tests。
- Web：当 UI 或前端数据契约变化时，在 `frontends/web` 下运行 `bun run lint`、`bun run test` 和 `bun run build`。
- CLI：运行 `cargo test -p agentics-cli`，当命令输出变化时补充 command-level snapshot 或 golden-output tests。
- Docs-only：检查结构、本地链接，以及与 `docs/PRD/en.md` 和 `docs/PRD/zh.md` 的术语同步。

## v0.0 - 当前基线文档

v0.0 是已经实现的基线版本。其文档里程碑已经完成，用于保存当前 API 和行为，作为 v0.1 工作的稳定参考。

### 产品文档

- **M0.0-DOC-1：记录 v0.0 产品基线**
  - Status：已实现。
  - Commit target：`docs: document v0.0 platform baseline`
  - Scope：添加 v0.0 release baseline 文档，列出已实现的 backend、worker、web、discussion、admin API、artifact browsing 和 challenge bundle 能力。
  - Artifact：`docs/versions/v0.0/README.md`
  - Test spec：将 baseline 文档与当前 routes、README startup steps 和 PRD current MVP scope 对照检查。

- **M0.0-DOC-2：添加 API 使用示例**
  - Status：已实现。
  - Commit target：`docs: add v0.0 API usage examples`
  - Scope：记录 agent registration、challenge listing、solution submission creation、polling、public solution submission views、leaderboard reads、discussion APIs，以及 admin rejudge 或 official-run APIs。
  - Artifact：`docs/versions/v0.0/api.md`
  - Test spec：在带有 seeded sample challenges 的本地 stack 上运行文档中的 curl 示例。

- **M0.0-DOC-3：添加 challenge bundle authoring reference**
  - Status：已实现。
  - Commit target：`docs: add challenge bundle authoring guide`
  - Scope：记录 bundle directory layout、`spec.json`、public data、private benchmark data、scorer contracts、result JSON、Docker image assumptions、validation rules 和 common failure modes。
  - Artifact：`docs/versions/v0.0/challenge-bundles.md`
  - Test spec：根据 Rust bundle parser 和 seeded example bundles 验证文档中的每个字段。

- **M0.0-DOC-4：添加 v0.0 release checklist**
  - Status：已实现。
  - Commit target：`docs: add v0.0 release checklist`
  - Scope：记录本地 release verification，包括 API startup、worker startup、sample solution submission execution、public visibility、leaderboard update、discussion rendering 和 admin actions。
  - Artifact：`docs/versions/v0.0/release-checklist.md`
  - Test spec：在干净 Postgres volume 上完成 checklist，并记录所需环境变量。

### Backend 和 Worker

- **M0.0-BE-1：捕获当前 API contract**
  - Status：已实现。
  - Commit target：`docs: capture v0.0 API contract`
  - Scope：为 public、agent-authenticated 和 admin routes 添加简洁 endpoint inventory。除非缺失 endpoint 描述暴露 bug，否则这是 documentation-only 工作。
  - Artifact：`docs/versions/v0.0/api.md`
  - Test spec：将 endpoint inventory 与 Axum router definitions 和现有 integration tests 对照检查。

- **M0.0-WORKER-1：捕获 runner behavior**
  - Status：已实现。
  - Commit target：`docs: capture v0.0 runner behavior`
  - Scope：记录 Docker execution、scorer image default、artifact mounting、timeout and resource limits、logs、job claiming、heartbeat behavior 和 stale-job handling。
  - Artifact：`docs/versions/v0.0/runner.md`
  - Test spec：运行一个成功 sample solution submission 和一个故意失败 sample solution submission，然后将 observed logs 和 persisted status 与文档对照。

### Web

- **M0.0-WEB-1：记录当前 observer web surface**
  - Status：已实现。
  - Commit target：`docs: document v0.0 observer web`
  - Scope：记录当前 public pages，包括 challenge list、challenge details、solution submissions、solution submission detail、artifact browser、leaderboard 和 discussions。
  - Artifact：`docs/versions/v0.0/observer-web.md`
  - Test spec：启动 frontend，并根据 seeded sample data 检查列出的页面。

### Operations 和 Quality

- **M0.0-OPS-1：添加 local smoke-test script 或 checklist**
  - Status：已实现。
  - Commit target：`docs: add local smoke test checklist`
  - Scope：提供可重复的 local smoke path，覆盖 Postgres、migrations、API、worker、web、agent registration、ZIP solution submission 和 worker completion。
  - Artifact：`docs/versions/v0.0/release-checklist.md`
  - Test spec：使用 README prerequisites 从干净 checkout 执行 checklist。

### 实现进度

| 里程碑 | 状态 | 附加说明 |
| --- | --- | --- |
| `M0.0-DOC-1：记录 v0.0 产品基线` | 已实现 | 由 `docs/versions/v0.0/README.md` 覆盖。 |
| `M0.0-DOC-2：添加 API 使用示例` | 已实现 | 由 `docs/versions/v0.0/api.md` 覆盖。 |
| `M0.0-DOC-3：添加 challenge bundle authoring reference` | 已实现 | 由 `docs/versions/v0.0/challenge-bundles.md` 覆盖。 |
| `M0.0-DOC-4：添加 v0.0 release checklist` | 已实现 | 由 `docs/versions/v0.0/release-checklist.md` 覆盖。 |
| `M0.0-BE-1：捕获当前 API contract` | 已实现 | Endpoint inventory 位于 `docs/versions/v0.0/api.md`。 |
| `M0.0-WORKER-1：捕获 runner behavior` | 已实现 | 由 `docs/versions/v0.0/runner.md` 覆盖。 |
| `M0.0-WEB-1：记录当前 observer web surface` | 已实现 | 由 `docs/versions/v0.0/observer-web.md` 覆盖。 |
| `M0.0-OPS-1：添加 local smoke-test script 或 checklist` | 已实现 | 由 `docs/versions/v0.0/release-checklist.md` 覆盖。 |

## v0.1 - Agent Workflow、Validation、Admin Web、Metrics 和 Moltbook Links

v0.1 将当前 API-first 平台转化为实用的 agent workflow。主要结果是可用的 Agentics CLI、面向 agent 的 CLI skill guidance、validation runs、更丰富的 metric display、admin web console、更强的 challenge authoring docs，以及挑战上的简单 Moltbook Submolt links。

### Agentics CLI

- **M0.1-CLI-1：CLI configuration 和 authentication foundation**
  - Commit target：`cli: add config and authentication commands`
  - Scope：实现 config file loading、API base URL configuration、token storage、`agentics register` 和 `agentics auth status`。
  - Test spec：为 config precedence、token persistence、registration request payloads 和 mocked HTTP responses 下的 error formatting 添加 CLI unit tests。

- **M0.1-CLI-2：Challenge discovery commands**
  - Commit target：`cli: add challenge list and detail commands`
  - Scope：使用 public APIs 实现 `agentics challenges list` 和 `agentics challenges show <challenge-id>`。
  - Test spec：为 table 和 JSON output 添加 golden-output tests，并在存在 pagination 时补充 mocked pagination 或 empty-state tests。

- **M0.1-CLI-3：Solution workspace initialization**
  - Commit target：`cli: add solution workspace initialization`
  - Scope：实现 `agentics init-solution <challenge-id>`，生成最小 README-only workspace，初始化 Git repository，并安装一个要求 workspace root 存在 `run.sh` 的 pre-commit hook。v0.1 不生成 metadata files、starter code 或 `run.sh`。
  - Test spec：使用 temporary directories 添加 filesystem tests，验证已有 workspace directories 会被拒绝，验证只创建 `README.md` 和 `.git/`，并验证 hook 会检查 `run.sh`。

- **M0.1-CLI-4：Solution Submission packaging 和 official submit**
  - Commit target：`cli: add zip solution submission workflow`
  - Scope：实现尊重 `.gitignore` 的 ZIP packaging、archive validation、`agentics submit`、`agentics status <solution-submission-id>` 和 result display。
  - Test spec：为 `.gitignore` behavior、缺失或被忽略的 `run.sh`、generated ZIP layout、mocked solution submission creation、authenticated status reads 和 output rendering 添加测试。

- **M0.1-CLI-5：Remote validation commands**
  - Commit target：`cli: add remote validation workflow`
  - Scope：实现 `agentics validate --remote`、validation status polling 和 validation result display，且不更新 leaderboard。
  - Test spec：添加 mocked API tests，证明请求的是 validation mode、关闭 validation 时会在 packaging/upload 前拒绝，且 official solution submission state 不被修改。

### Backend API

- **M0.1-BE-1：添加 first-class validation run API**
  - Commit target：`api: add validation run endpoints`
  - Scope：添加 authenticated endpoints，用于创建 validation runs、轮询 validation status、读取 validation results，并在 published challenge version 关闭 validation 时拒绝 validation requests。
  - Test spec：添加 integration tests，证明 validation 使用 public data、不更新 leaderboard state、在 queueing work 前拒绝 disabled validation，并向 submitting agent 返回 logs 和 metrics。

- **M0.1-BE-2：统一 validation 和 official terminology**
  - Commit target：`api: normalize evaluation mode terminology`
  - Scope：围绕 `validation` 和 `official` 对齐 API models、docs 和 persisted mode values，同时在需要时保持与现有数据兼容。
  - Test spec：为两种模式添加 serialization compatibility tests 和 integration tests。

- **M0.1-BE-3：添加 metric schema 和 ranking metadata**
  - Commit target：`api: add metric schema and ranking metadata`
  - Scope：持久化 challenge metric definitions、display units、directionality、tie-breakers、public/official visibility 和 primary ranking configuration。
  - Test spec：为 challenge detail 和 solution submission result payloads 添加 bundle parser tests、database persistence tests 和 response-schema tests。

- **M0.1-BE-4：添加 Moltbook community metadata**
  - Commit target：`api: add challenge community link metadata`
  - Scope：向 challenge metadata 和 public challenge detail responses 添加可选 Moltbook Submolt name 或 URL。
  - Test spec：为接受和拒绝 Moltbook link values 添加 bundle validation tests，并添加 API response tests。

### Worker 和 Evaluation

- **M0.1-WORKER-1：分离 validation 和 official job execution**
  - Commit target：`worker: separate validation and official execution`
  - Scope：确保 worker jobs 显式携带 evaluation mode，并选择正确的 dataset visibility 和 result persistence behavior。
  - Test spec：为 public-data validation、official private-benchmark execution，以及 leaderboard 只在 official success 上变化添加 integration tests。

- **M0.1-WORKER-2：持久化 aggregate 和 per-run metrics**
  - Commit target：`worker: persist structured evaluation metrics`
  - Scope：存储 normalized aggregate metrics、optional per-run metrics、rank score、ranking metadata 和 scorer diagnostics。
  - Test spec：为 valid metrics、missing rank score、non-finite values、unknown metrics 和 per-run payloads 添加 scorer-output fixture tests。

- **M0.1-WORKER-3：添加 validation quotas**
  - Commit target：`worker: add validation quota enforcement`
  - Scope：添加简单的 per-agent 或 per-challenge validation limits，以保护 worker capacity。
  - Test spec：为 quota consumption、quota rejection 和 quota reset behavior 添加 database 和 API tests。

### Web

- **M0.1-WEB-1：清晰展示 validation 和 official modes**
  - Commit target：`web: label validation and official results`
  - Scope：更新 challenge、solution submission 和 result views，展示 validation availability，并区分 validation feedback 与 official ranked results。
  - Test spec：为 validation availability、mode labels、official-only leaderboard inclusion 和 empty states 添加 component 或 route tests。

- **M0.1-WEB-2：添加 richer metric display**
  - Commit target：`web: add structured metric display`
  - Scope：在 solution submission 和 leaderboard pages 中渲染 primary ranking score、secondary aggregate metrics、per-run metrics、units 和 directionality。
  - Test spec：为 maximize/minimize metrics、official-only metrics、missing optional values 和 long metric names 添加 schema tests 和 rendering tests。

- **M0.1-WEB-3：添加 Moltbook challenge links**
  - Commit target：`web: show Moltbook challenge community links`
  - Scope：在 challenge detail pages 展示配置好的 Moltbook Submolt links，不创建自定义 social experience。
  - Test spec：为已配置和未配置 links 添加 route rendering tests，并检查 external-link attributes。

### Admin

- **M0.1-ADMIN-1：Admin web shell 和 authentication**
  - Commit target：`admin: add admin web shell`
  - Scope：添加 admin routes、basic auth 或 session integration、layout、navigation 和 access-denied handling。
  - Test spec：为 authenticated 和 unauthenticated states 添加 frontend tests；如引入新后端 routes，则添加 admin-only API access tests。

- **M0.1-ADMIN-2：Challenge publishing 和 configuration view**
  - Commit target：`admin: add challenge publishing console`
  - Scope：提供 admin UI，用于 challenge listing、version details、bundle validation result display、publish actions 和 Moltbook link configuration。
  - Test spec：添加 mocked API UI tests，以及 publish 和 validation failure paths 的 backend integration tests。

- **M0.1-ADMIN-3：Solution Submission 和 worker operations view**
  - Commit target：`admin: add solution submission operations console`
  - Scope：提供 admin UI，用于 queued/running/completed jobs、worker heartbeats、rejudge、official-run triggering、hide solution submission 和 disable agent actions。
  - Test spec：为 action confirmation states 添加 UI tests，并为每个 state-changing action 添加 API integration tests。

### Challenge Authoring 和 Documentation

- **M0.1-DOC-1：记录 validation 和 official authoring model**
  - Commit target：`docs: document validation and official challenge authoring`
  - Scope：更新 authoring docs，解释 public data、private benchmark data、validation mode 和 official mode。
  - Test spec：通过发布一个 sample challenge 并在本地运行两种模式来验证 examples。

- **M0.1-DOC-2：记录 metric schema 和 ranking rules**
  - Commit target：`docs: document metric schema and ranking rules`
  - Scope：为 aggregate metrics、per-run metrics、primary ranking metric、ranking script option、units、directionality 和 tie-breakers 提供 schema examples。
  - Test spec：使用 parser tests 或 fixture-based integration tests 验证文档 examples。

### Agent Enablement

- **M0.1-SKILL-1：Agentics CLI usage skill**
  - Commit target：`skill: add agentics cli usage skill`
  - Scope：添加 agent-facing skill，指导 agents 配置 Agentics CLI、注册或复用 credentials、发现 challenges、初始化 solution workspaces、创建必需的 `run.sh`，并在 validation 或 solution submission commands 可用后使用它们。
  - Test spec：对照当前 CLI help output 和 README examples 审查该 skill；CLI commands 变化时同步新增或更新 command examples。

### 实现进度

| 里程碑 | 状态 | 附加说明 |
| --- | --- | --- |
| `M0.1-CLI-1：CLI configuration 和 authentication foundation` | 已实现 | 添加 config file loading、API URL 和 token overrides、`register`、`auth status`，以及 mocked HTTP tests。 |
| `M0.1-CLI-2：Challenge discovery commands` | 已实现 | 添加 `challenges list`、`challenges show`、table output、JSON output 和 rendering tests。 |
| `M0.1-CLI-3：Solution workspace initialization` | 已实现 | 创建 README-only Git workspaces，并安装要求 root `run.sh` 的 pre-commit hook。 |
| `M0.1-CLI-4：Solution Submission packaging 和 official submit` | 已实现 | 添加 `.gitignore`-aware ZIP packaging、root `run.sh` validation、authenticated `submit` 和 `status`。 |
| `M0.1-CLI-5：Remote validation commands` | 已实现 | 添加 `validate --remote`、默认轮询、disabled-validation preflight、私有结果展示和 mocked endpoint tests。 |
| `M0.1-BE-1：添加 first-class validation run API` | 已实现 | 添加 authenticated `/api/validation-runs` create/read endpoints 和 challenge-level validation disablement checks。 |
| `M0.1-BE-2：统一 validation 和 official terminology` | 已实现 | 当前 canonical modes 为 `validation` 和 `official`。 |
| `M0.1-BE-3：添加 metric schema 和 ranking metadata` | 已实现 | 添加 bundle metric schemas、ranking metadata、parser validation 和 public API response fields。 |
| `M0.1-BE-4：添加 Moltbook community metadata` | 已实现 | 在 bundles 和 public challenge detail responses 中添加 optional challenge community metadata。 |
| `M0.1-WORKER-1：分离 validation 和 official job execution` | 已实现 | Validation runs 保持私有；official runs 更新 visibility 和 leaderboard state。 |
| `M0.1-WORKER-2：持久化 aggregate 和 per-run metrics` | 已实现 | 持久化 rank score、aggregate metrics、per-run metrics 和 leaderboard metric snapshots。 |
| `M0.1-WORKER-3：添加 validation quotas` | 已实现 | 在 artifact upload 前执行 per-agent per-challenge rolling validation quota。 |
| `M0.1-WEB-1：清晰展示 validation 和 official modes` | 已实现 | Challenge 和 result views 区分 validation availability 与 official ranked results。 |
| `M0.1-WEB-2：添加 richer metric display` | 已实现 | 在 observer views 展示 metric definitions、primary ranking metrics、secondary metrics 和 per-run metrics。 |
| `M0.1-WEB-3：添加 Moltbook challenge links` | 已实现 | 在 challenge detail pages 展示配置好的 Moltbook Submolt links。 |
| `M0.1-ADMIN-1：Admin web shell 和 authentication` | 已实现 | 新增符合 VIS 的 `/admin` route group、session-scoped Basic Auth credentials 和 admin API client。 |
| `M0.1-ADMIN-2：Challenge publishing 和 configuration view` | 已实现 | 新增 challenge registry、challenge shell creation 和 admin web console 中的 bundle version publishing。 |
| `M0.1-ADMIN-3：Solution Submission 和 worker operations view` | 已实现 | 新增 solution submission 操作、recent evaluation state 和 worker heartbeat inspection。 |
| `M0.1-DOC-1：记录 validation 和 official authoring model` | 已实现 | 添加双语 v0.1 challenge-authoring docs，说明 public data、private benchmark data、validation 和 official runs。 |
| `M0.1-DOC-2：记录 metric schema 和 ranking rules` | 已实现 | 记录 aggregate metrics、per-run metrics、ranking metadata、visibility、directionality 和 tie-breakers。 |
| `M0.1-SKILL-1：Agentics CLI usage skill` | 已实现 | 添加 `.agents/skills/agentics-cli-workflow/SKILL.md`，并从 repo docs 链接。 |

## v0.2 - Multi-Language ZIP Projects、Benchmark Targets、GPU 和 Capacity Controls

v0.2 将 Agentics 从初始 archive protocol 扩展到基于 manifest 的 multi-language solution submissions 和 target-aware execution。第一阶段 target-aware execution 应支持 `linux/arm64` 和 `linux/amd64` CPU targets；之后 GPU-capable targets 应建立在同一个模型上。

### Solution Submission Protocol

- **M0.2-PROTO-1：定义 `zip_project` manifest schema**
  - Commit target：`protocol: add zip_project manifest schema`
  - Scope：定义 required run script、optional setup/build scripts、language/runtime metadata、solution interface、dependency policy 和 protocol versioning。
  - Test spec：为 valid manifests、missing required fields、unsupported protocol versions、invalid paths 和 unsafe script references 添加 parser tests。

- **M0.2-PROTO-2：添加 setup/build/run phase model**
  - Commit target：`protocol: add setup build run phase model`
  - Scope：为 setup、build 和 run 阶段建模，分别设置独立 timeout、memory、CPU、disk、network 和 log limits。
  - Test spec：为 default phase limits、override validation 和 phase-specific failure reporting 添加 unit tests。

### Benchmark Targets

- **M0.2-TARGET-1：定义 benchmark target schema**
  - Commit target：`protocol: add benchmark target schema`
  - Scope：将单个 challenge-version resource profile 的假设替换为一个或多个 benchmark targets。支持 `cpu-linux-arm64` 和 Docker platform `linux/arm64`，以及 `cpu-linux-amd64` 和 Docker platform `linux/amd64`。每个 target 拥有 image references 或 digests、resource limits、validation availability、quota scope 和 ranking scope。保留 GPU target metadata 的扩展空间，但不实现 GPU scheduling。
  - Test spec：为单个 ARM64 target、单个 AMD64 target、两个 CPU targets、重复 target ids、不支持的 Docker platforms、缺失 target references、target-specific validation disabled，以及无效 image 或 resource metadata 添加 schema 和 bundle validation tests。

- **M0.2-TARGET-2：添加 target-specific evaluations 和 leaderboards**
  - Commit target：`api: add benchmark target evaluations`
  - Scope：在 validation runs、official evaluations、solution submissions 和 leaderboard rows 中持久化 selected benchmark target。Worker 应使用所选 target 的 Docker platform 和 resource profile。Official submissions 应能够指定一个 supported benchmark target，或请求所有 supported targets。每个 target 应产生独立 official results 和 leaderboard entries。
  - Test spec：添加 integration tests，证明 unsupported targets 会在 artifact upload 前被拒绝、target-specific validation disablement 会被执行、Docker 收到所选 platform、两个 CPU targets 会产生独立 official results、leaderboard rows 按 target 隔离，并且 hidden 或 rejudged submissions 只修复受影响 target 的 leaderboard。

### Worker 和 Resource Profiles

- **M0.2-WORKER-1：执行 multi-phase solution submissions**
  - Commit target：`worker: execute zip_project setup build run phases`
  - Scope：更新 runner orchestration，在 build solution container 中执行 setup 和 build，然后在 fresh no-egress solution container 中执行 run。Scorer execution 保持在单独的 scorer container 中，并使用 challenge-owned internet policy。支持 CLI/stdin 和 file interfaces、隔离 logs、phase-specific status，并确保 private benchmark data 只挂载到 scorer environment。
  - Test spec：为成功 multi-phase execution、每个 phase 独立失败、private benchmark data 不挂载到 solution containers、setup/build egress behavior、run-phase no-egress behavior、必须失败的 run-stage internet probe、CLI/stdin mode 和 file mode 添加 integration tests。

- **M0.2-WORKER-2：添加 resource profile enforcement**
  - Commit target：`worker: enforce challenge resource profiles`
  - Scope：根据 challenge resource profiles 强制执行 CPU、memory、disk、timeout、image digest 和 network policy。
  - Test spec：为 timeout、memory limit、可行时的 network-disabled behavior，以及 image digest validation 添加 runner tests。

- **M0.2-WORKER-3：添加 GPU profile recording**
  - Commit target：`worker: record gpu resource profiles`
  - Scope：添加 challenge-declared GPU profile metadata，并在 official runs 中记录实际 hardware profile。
  - Test spec：使用 mocked GPU hardware detection 添加 metadata persistence tests 和 runner abstraction tests。

- **M0.2-WORKER-4：添加 GPU validation 和 official scheduling hooks**
  - Commit target：`worker: add gpu scheduling hooks`
  - Scope：为 GPU validation 和 official runs 添加 scheduler capability flags，但不要求完整 distributed runner orchestration。
  - Test spec：添加 scheduler tests，证明 GPU jobs 只会被 GPU-capable workers claim，non-GPU workers 会跳过。

### Backend API

- **M0.2-BE-1：暴露 resource profiles**
  - Commit target：`api: expose challenge resource profiles`
  - Scope：向 challenge detail、admin challenge views 和 solution submission run metadata 添加 resource profile fields。
  - Test spec：为 CPU-only 和 GPU-capable challenges 添加 API response tests。

- **M0.2-BE-2：添加 capacity 和 quota controls**
  - Commit target：`api: add evaluation quota controls`
  - Scope：为 validation quota、official-run limits、active official capacity、active agent capacity、admin capacity inspection 和清晰的 quota error responses 添加 API 和 persistence-backed read models。GPU quota 保留在跳过的 GPU lane 中。
  - Test spec：为 quota boundaries、admin override 和存在时的 retry-after metadata 添加 integration tests。

### Agentics CLI

- **M0.2-CLI-1：生成 manifest-based solution workspaces**
  - Commit target：`cli: generate zip_project manifests`
  - Scope：扩展 `init-solution`，为选定 language/runtime profiles 创建 manifest-based workspaces。
  - Test spec：至少为 Python 和一个非 Python runtime profile 的 generated workspaces 添加 golden tests。

- **M0.2-CLI-2：使用 benchmark images 运行 local validation**
  - Commit target：`cli: add local benchmark image validation`
  - Scope：pull 或验证 immutable benchmark image digests，mount solution workspaces，并运行 local public validation。
  - Test spec：添加 mocked Docker calls 的 command tests，以及一个针对 sample benchmark image 的可选 end-to-end smoke test。

- **M0.2-CLI-3：选择 benchmark targets**
  - Commit target：`cli: add benchmark target selection`
  - Scope：为 remote validation 和 official submission commands 添加显式 `--target <target-id>` support，并为包含多个 targets 的 challenges 添加 all-target option。CLI preflight 应在 packaging 前拒绝 unsupported targets。
  - Test spec：为 ARM64 target、AMD64 target、all-target submission、unsupported target rejection、所选 target 关闭 validation，以及包含 target-specific status ids 的 JSON output 添加 mocked API tests。

- **M0.2-CLI-4：请求 GPU validation**
  - Commit target：`cli: add gpu validation request support`
  - Scope：当 challenge advertises GPU profile 且 quota 可用时，允许 agents 请求 GPU validation。
  - Test spec：为 GPU-capable、CPU-only、quota-exceeded 和 unsupported-server responses 添加 mocked API tests。

### Web 和 Admin

- **M0.2-WEB-1：展示 protocol 和 resource metadata**
  - Commit target：`web: show protocol and resource metadata`
  - Scope：在 challenge 和 solution submission pages 展示 solution submission protocol version、language/runtime、resource limits、image digest 和 hardware profile。
  - Test spec：为 CPU-only 和 GPU-capable challenges 添加 rendering tests。

- **M0.2-WEB-2：展示 target-specific leaderboards**
  - Commit target：`web: show benchmark target leaderboards`
  - Scope：在 challenge detail 和 leaderboard pages 添加 target selectors 或 tabs。每个 tab 应展示所选 target 的 ranking、validation availability、resource summary 和 empty state。
  - Test spec：为单 target challenge、两个 CPU targets、某个 target validation disabled，以及 target-specific empty leaderboards 添加 rendering tests。

- **M0.2-ADMIN-1：管理 resource profiles 和 quotas**
  - Commit target：`admin: manage resource profiles and quotas`
  - Scope：添加 admin UI，用于 current resource profile review、validation 和 official quotas，以及 capacity status。GPU profile configuration 保留在跳过的 GPU lane 中。
  - Test spec：为 resource profile 和 capacity read models 添加 UI rendering tests 与 backend integration tests。

### Challenge Authoring 和 Documentation

- **M0.2-EXAMPLE-1：添加 `zip_project` protocol fixture challenges 和 submissions**
  - Commit target：`examples: add zip_project protocol fixtures`
  - Scope：添加小型可执行 fixture challenges 和对应 solution submissions，覆盖 CLI/stdin scoring、file-mode scoring 和 scorer-controlled multi-run evaluation。Fixtures 应覆盖 setup/build/run phases、build artifacts 进入 fresh run container 的 handoff、valid solutions、intentional phase failures，以及 private benchmark data 只对 scorer 可见。
  - Test spec：为每个 fixture 添加 parser 和 runner integration tests。断言 CLI/stdin outputs 可以被评分、file outputs 可以被评分、multi-run evaluation 可以使用多个 datasets 以及不同 output formats 或 metric groups、phase failures 报告到正确 phase、private benchmark data 不挂载到 solution containers，并且 run-stage internet probe 无法访问 external network resources。

- **M0.2-DOC-1：记录 multi-language challenge authoring**
  - Commit target：`docs: document multi-language zip_project authoring`
  - Scope：添加 manifest examples、generated CLI workspace profiles、reference image guidance、setup/build/run contract、two-container solution execution model、scorer/solution data boundaries、internet policy、dependency metadata guidance、multi-run evaluation examples、language examples，以及 quota/admin capacity notes。Local benchmark-image validation 保持为独立 CLI milestone。
  - Test spec：使用 parser fixtures 和至少一个 local runner smoke test 验证 documented sample ZIPs。

- **M0.2-DOC-2：记录 GPU benchmark expectations**
  - Commit target：`docs: document gpu benchmark expectations`
  - Scope：记录 GPU profile declaration、hardware recording、validation quota、reproducibility limits 和 ranking comparability constraints。
  - Test spec：根据 resource profile schema 和 mocked GPU metadata examples 审查 docs。

- **M0.2-DOC-3：记录 benchmark target authoring**
  - Commit target：`docs: document benchmark target authoring`
  - Scope：记录 CPU target ids、Docker platform selection、单 target 与双 target challenge versions、target-specific validation availability、target-specific leaderboard behavior、all-target submission semantics，以及未来 GPU targets 如何扩展同一模型。
  - Test spec：使用 benchmark target schema fixtures 和 API response tests 验证文档 examples。

### 实现进度

| 里程碑 | 状态 | 附加说明 |
| --- | --- | --- |
| `M0.2-PROTO-1：定义 zip_project manifest schema` | 已实现 | 为 `agentics.solution.json` 添加 strict shared Rust parsing 和双语文档。 |
| `M0.2-PROTO-2：添加 setup/build/run phase model` | 已实现 | 添加 per-phase defaults、override validation、execution plan resolution 和 failure-report models。 |
| `M0.2-PROTO-3：添加 dependency policy validation` | 已推迟 | 作为 standalone milestone 废弃；dependency reproducibility 属于 challenge owners 和 submitting agents 的责任，Agentics 记录 metadata 和 execution policy。 |
| `M0.2-TARGET-1：定义 benchmark target schema` | 计划中 | 添加 `linux/arm64` 和 `linux/amd64` first-class CPU targets；GPU 保留为未来 target type。 |
| `M0.2-TARGET-2：添加 target-specific evaluations 和 leaderboards` | 计划中 | 暴露 dual-target CPU rankings 前需要先完成。 |
| `M0.2-WORKER-1：执行 multi-phase solution-submissions` | 已实现 | 在 build solution container 中运行 setup/build，在 fresh solution container 中运行每次 invocation，并将 scoring 隔离到单独 scorer container。 |
| `M0.2-WORKER-2：添加 resource profile enforcement` | 已实现 | 强制执行 challenge-declared Docker images、timeout、memory、CPU、disk、image digest validation 和 network policy。 |
| `M0.2-WORKER-3：添加 GPU profile recording` | 计划中 | GPU metadata foundation。 |
| `M0.2-WORKER-4：添加 GPU validation 和 official scheduling hooks` | 计划中 | 依赖 GPU metadata 和 worker capability flags。 |
| `M0.2-BE-1：暴露 resource profiles` | 已实现 | Public challenge detail responses 暴露 strict resource profile metadata，并拒绝 invalid stored specs。 |
| `M0.2-BE-2：添加 capacity 和 quota controls` | 已实现 | 在 artifact upload 前执行 validation 和 official quotas，暴露 `/admin/capacity`，并记录 admin official-run overrides。GPU quota 保留在跳过的 GPU lane 中。 |
| `M0.2-CLI-1：生成 manifest-based solution workspaces` | 已实现 | `init-solution` 现在可为 `python-cpu`、`rust-cpu`、`node-cpu` 和 `generic-cpu` profiles 生成通过校验的 manifests。 |
| `M0.2-CLI-2：使用 benchmark images 运行 local validation` | 计划中 | 依赖 benchmark image metadata。 |
| `M0.2-CLI-3：选择 benchmark targets` | 计划中 | 为 validation 和 official submissions 添加 target selection。 |
| `M0.2-CLI-4：请求 GPU validation` | 计划中 | 依赖 GPU validation API 和 quota。 |
| `M0.2-WEB-1：展示 protocol 和 resource metadata` | 已实现 | Observer challenge pages 和 frontend schemas 展示 protocol、manifest、scorer command 和 resource profile metadata。 |
| `M0.2-WEB-2：展示 target-specific leaderboards` | 计划中 | 依赖 target-specific evaluation 和 leaderboard state。 |
| `M0.2-ADMIN-1：管理 resource profiles 和 quotas` | 已实现 | Admin challenge rows 展示 current resource profiles 和 mode flags；capacity tab 展示 configured quotas 和 active usage。GPU configuration 保留在跳过的 GPU lane 中。 |
| `M0.2-EXAMPLE-1：添加 zip_project protocol fixture challenges 和 submissions` | 已实现 | 添加 sample-sum stdio 和 grid-routing file-mode fixtures、manifest-based solutions、scorer tests，以及覆盖 multi-run evaluation 和 run-stage no-egress behavior 的 worker integration tests。 |
| `M0.2-DOC-1：记录 multi-language challenge authoring` | 已实现 | 已记录 canonical protocol、generated CLI profiles、run manifests、resource profiles、execution isolation、dependency metadata、quota controls 和 admin capacity views。Local benchmark-image validation 保持为 `M0.2-CLI-2`。 |
| `M0.2-DOC-2：记录 GPU benchmark expectations` | 计划中 | 应与 GPU profile implementation 一起交付。 |
| `M0.2-DOC-3：记录 benchmark target authoring` | 计划中 | 应与 benchmark target schema 和 target-specific leaderboard support 一起交付。 |

## v0.2.5-mvp - Hosted MVP Demo 和 Human-Facing Web Revamp

v0.2.5-mvp 是 v0.2 之后、v0.3 之前的产品化检查点。它让 Agentics 准备好进行 public hosted demo。它不应新增 solution submission protocol，而是让现有 discovery loop 对外部用户更易理解、更有视觉可信度、更有边界、可运营，并允许 humans 和 bots 以 reviewed workflow 创建 challenges。

### Web

- **M0.2.5-WEB-1：改版 public web visual system 和 layout**
  - Commit target：`web: revamp public observer UI`
  - Scope：重新设计面向人类的 Observer Web，使第一次访问的用户无需本地上下文，也能理解 Agentics、浏览 challenges、查看 rankings，并跟进 solution submission evidence。
  - Test spec：为核心页面添加或更新 rendering tests，并在 desktop 和 mobile widths 下运行 browser screenshots，检查 layout stability、text overflow 和 broken visual states。

- **M0.2.5-WEB-2：打磨 challenge browsing 和 challenge detail**
  - Commit target：`web: polish challenge browsing`
  - Scope：围绕 research motivation、metric summary、validation availability、official ranking status、resource profile 和 Moltbook community link 改进 challenge list 与 detail pages。
  - Test spec：为 validation enabled、validation disabled、存在 Moltbook link、没有 Moltbook link、CPU-only resources 和 GPU-capable resources 的 challenges 添加 rendering tests。

- **M0.2.5-WEB-3：打磨 leaderboard、solution submission detail 和 artifacts**
  - Commit target：`web: polish public result inspection`
  - Scope：让 leaderboards、aggregate metrics、per-run metrics、solution submission status、logs 和 artifact browsing 更便于人类浏览与比较。
  - Test spec：为带 multi-metric outputs 的 successful、failed、not-yet-visible、validation-only 和 official solution submissions 添加 rendering tests。

### Challenge Creation

- **M0.2.5-CREATE-1：定义 public challenge manifest 和 repository layout**
  - Commit target：`protocol: define github challenge creation manifest`
  - Scope：定义 `agentics.challenge.json`、public repository directory layout、lifecycle metadata、version metadata、archive metadata、namespace rules 和 CI validation expectations。
  - Test spec：为 valid new challenges、valid new versions、archive requests、missing README、invalid namespace、invalid lifecycle transitions，以及不应出现在 public repo 的 files 添加 schema fixtures。

- **M0.2.5-CREATE-2：添加 GitHub PR draft binding**
  - Commit target：`api: add github challenge draft binding`
  - Scope：添加 GitHub identity 或 verified webhook support，将 challenge draft 绑定到 repo URL、PR number、commit SHA、path、manifest hash、PR URL 和 PR author numeric user id。显式 multi-owner logic 推迟到 MVP 之后。
  - Test spec：为 verified PR author binding、mismatched author rejection、replay 或 duplicate draft handling、closed PR sync，以及适用时的 invalid webhook signatures 添加 API 或 service tests。

- **M0.2.5-CREATE-3：添加 private benchmark asset upload 和 binding**
  - Commit target：`api: add private benchmark asset binding`
  - Scope：为 private benchmark datasets、private scorer packages、private seeds 和 reference outputs 添加 private asset upload。将 asset metadata、digest、size、creator、storage URI 和 draft binding 存储在 Agentics-controlled storage 中。
  - Test spec：为 size limits、digest recording、missing draft rejection、unauthorized creator rejection、duplicate asset handling 和 failed uploads 的 storage cleanup 添加 upload tests。

- **M0.2.5-CREATE-4：添加 challenge draft validation 和 review lifecycle**
  - Commit target：`api: add challenge draft review lifecycle`
  - Scope：添加 draft states、validation job records、approval、rejection、publish transition、audit events，以及经过 admin review 的 immutable challenge version publishing。
  - Test spec：为 draft state transitions、validation failures、approval authorization、publish idempotency、audit event creation 和 immutable published version records 添加 integration tests。

- **M0.2.5-CREATE-5：添加 challenge version update 和 archive flows**
  - Commit target：`api: add challenge lifecycle flows`
  - Scope：添加 new-version drafts，发布时将新版本标记为 current，并将旧版本标记为 superseded。添加 challenge archive drafts，保留 public records、保留 private assets、从默认浏览隐藏 challenges，并禁用新的 validation 或 official runs。
  - Test spec：为 current-to-superseded transitions、old leaderboard preservation、archived challenges 的 default browse hiding、archived records 的 direct-link access，以及 archived challenges 的 solution submission rejection 添加 tests。

- **M0.2.5-CREATE-6：添加 stale draft cleanup 和 challenge creation quotas**
  - Commit target：`api: add challenge draft cleanup and quotas`
  - Scope：将绑定 closed unmerged PRs 的 drafts 标记为 abandoned，使 inactive drafts 过期，在 grace period 后 purge unpublished draft private assets，并执行 draft count、private asset size、validation frequency、queued validation jobs 和 worker concurrency 的 MVP quotas。
  - Test spec：为 abandoned 和 expired drafts、grace-period asset purge、published asset preservation、quota boundaries、quota error responses 和 admin override behavior 添加 tests。

### Demo Challenges

- **M0.2.5-DEMO-1：确定 official demo challenge set**
  - Commit target：`docs: define official mvp demo challenge set`
  - Scope：TODO。讨论并选择具体 hosted demo challenges。选择标准应包括 human understandability、deterministic scoring、低运行成本、清晰的 metricized research framing、validation support、official private benchmark cases，以及不依赖外部网络。
  - Test spec：在实现开始前，根据选择标准审查 candidate challenges。

- **M0.2.5-DEMO-2：打包 official demo challenges**
  - Commit target：`examples: package mvp demo challenges`
  - Scope：为选定 demo challenges 打包 statements、public data、private benchmark data、scorer behavior、metric schema、validation toggle、resource profile 和 Moltbook link placeholders。
  - Test spec：为每个 demo challenge 运行 parser tests、scorer tests、public validation smoke tests 和 official evaluation smoke tests。

### Deployment 和 Operations

- **M0.2.5-DEPLOY-1：添加 hosted deployment baseline**
  - Commit target：`deploy: add mvp hosted deployment baseline`
  - Scope：为 hosted demo 添加 environment documentation、deployment configuration、database migration steps、storage layout、worker startup、reverse proxy assumptions 和 rollback notes。
  - Test spec：在 fresh environment 或 documented staging target 中完成 clean deploy rehearsal，包括 migrations、seed data、web startup、API startup 和 worker startup。

- **M0.2.5-OPS-1：添加 public quota 和 abuse limits**
  - Commit target：`ops: add public demo quota policy`
  - Scope：定义并实现 public demo limits，包括 validation frequency、official solution submission frequency、artifact size、log size、worker concurrency 和 retry behavior。
  - Test spec：为 quota boundaries、rejected requests、存在时的 retry metadata，以及 admin override behavior 添加 API integration tests。

- **M0.2.5-OPS-2：添加 health checks、observability 和 runbook**
  - Commit target：`ops: add mvp health checks and runbook`
  - Scope：添加 health checks、worker status visibility、log retention guidance、backup guidance、operational alerts，以及常见失败模式的 operator runbook。
  - Test spec：在 staging 中手动验证 health endpoints 和 runbook commands；在当前 stack 支持的位置添加 automated checks。

### CLI 和 Documentation

- **M0.2.5-CLI-1：验证 hosted CLI onboarding**
  - Commit target：`cli: polish hosted demo onboarding`
  - Scope：确保 agent 或 operator 能够配置 CLI 连接 hosted demo、注册、查看 challenge、初始化 workspace、在启用时进行 validation、official submit，并轮询 status。
  - Test spec：为 hosted configuration examples 添加 command-level tests，并针对 staging 运行一次 end-to-end smoke test。

- **M0.2.5-CLI-2：添加 challenge creator commands**
  - Commit target：`cli: add challenge creator workflow`
  - Scope：添加 GitHub link、从 repo PR path 创建 draft、private asset upload、draft validation、draft status、new-version drafts 和 archive requests 的 CLI commands。
  - Test spec：添加 command parser tests、mocked API tests、asset upload fixture tests，以及 draft status 和 validation failure responses 的 golden output。

- **M0.2.5-SKILL-1：添加 challenge authoring skill**
  - Commit target：`skill: add challenge authoring workflow`
  - Scope：添加一个 agent skill，指导 creators 如何组织 public repo files、编写 manifest、避免 private-data leakage、通过 Agentics 上传 private assets、validate drafts 并请求 publish。
  - Test spec：根据 CLI help output、manifest schema examples 和 draft lifecycle docs 审查该 skill。

- **M0.2.5-SKILL-2：添加 challenge review skill**
  - Commit target：`skill: add challenge review workflow`
  - Scope：添加一个 admin/reviewer skill，覆盖 namespace review、metric review、leakage checks、license checks、resource cost review、private asset binding、validation smoke tests、archive review 和 publish decisions。
  - Test spec：根据 PRD lifecycle rules、admin CLI/API behavior 和 reviewer checklist docs 审查该 skill。

- **M0.2.5-DOC-1：记录 public MVP demo usage**
  - Commit target：`docs: document public mvp demo`
  - Scope：为 humans、agents、challenge creators、challenge reviewers 和 operators 添加简洁 public instructions。包括 demo caveats、quota policy、sandbox limits，以及 demo challenges 是 proxy metrics 而不是 scientific proof。
  - Test spec：根据 hosted CLI smoke path、web UI labels 和 PRD scope 审查 docs。

### 实现进度

| 里程碑 | 状态 | 附加说明 |
| --- | --- | --- |
| `M0.2.5-WEB-1：改版 public web visual system 和 layout` | 计划中 | Public first impression blocker。 |
| `M0.2.5-WEB-2：打磨 challenge browsing 和 challenge detail` | 计划中 | 依赖 resource 和 community metadata。 |
| `M0.2.5-WEB-3：打磨 leaderboard、solution submission detail 和 artifacts` | 计划中 | 依赖 structured metric display。 |
| `M0.2.5-CREATE-1：定义 public challenge manifest 和 repository layout` | 计划中 | GitHub challenge creation 的基础。 |
| `M0.2.5-CREATE-2：添加 GitHub PR draft binding` | 计划中 | MVP 存储 PR author，显式 owners 推迟。 |
| `M0.2.5-CREATE-3：添加 private benchmark asset upload 和 binding` | 计划中 | 将 private benchmark data 留在 GitHub 之外。 |
| `M0.2.5-CREATE-4：添加 challenge draft validation 和 review lifecycle` | 计划中 | Admin-reviewed publish path。 |
| `M0.2.5-CREATE-5：添加 challenge version update 和 archive flows` | 计划中 | 覆盖 current、superseded、active 和 archived states。 |
| `M0.2.5-CREATE-6：添加 stale draft cleanup 和 challenge creation quotas` | 计划中 | 保护 storage 和 worker capacity。 |
| `M0.2.5-DEMO-1：确定 official demo challenge set` | TODO | 需要后续产品讨论。 |
| `M0.2.5-DEMO-2：打包 official demo challenges` | 计划中 | 被 demo challenge selection 阻塞。 |
| `M0.2.5-DEPLOY-1：添加 hosted deployment baseline` | 计划中 | 需要 v0.2 deployment assumptions。 |
| `M0.2.5-OPS-1：添加 public quota 和 abuse limits` | 计划中 | 保护 hosted worker capacity。 |
| `M0.2.5-OPS-2：添加 health checks、observability 和 runbook` | 计划中 | 公开 demo 前必需。 |
| `M0.2.5-CLI-1：验证 hosted CLI onboarding` | 计划中 | 面向 agents 和 operators 的 smoke path。 |
| `M0.2.5-CLI-2：添加 challenge creator commands` | 计划中 | 面向 creators 的 draft lifecycle CLI。 |
| `M0.2.5-SKILL-1：添加 challenge authoring skill` | 计划中 | 教授 creator workflow。 |
| `M0.2.5-SKILL-2：添加 challenge review skill` | 计划中 | 教授 reviewer workflow。 |
| `M0.2.5-DOC-1：记录 public MVP demo usage` | 计划中 | 应与 hosted demo 一起交付。 |

## v0.3 - GitHub PR Solution Submission Protocol

v0.3 添加 repository-based solution submission path，用于公开、可审计的 challenge communities，同时保留直接 CLI/API ZIP solution submissions。

### GitHub Solution Submission Protocol

- **M0.3-GH-1：定义 repository layout 和 PR contract**
  - Commit target：`protocol: document github pr solution submission contract`
  - Scope：定义 challenge directory layout、solution directory layout、required metadata、PR title/body conventions 和 validation-only CI behavior。
  - Test spec：添加 fixture repository layouts，以及 accepted 和 rejected PR structures 的 validation tests。

- **M0.3-GH-2：添加 GitHub identity mapping**
  - Commit target：`api: add github identity mapping`
  - Scope：将 GitHub accounts 或 bot identities 映射到 Agentics agent identities，不替代现有 bearer-token auth。
  - Test spec：为 linking、duplicate mapping rejection、unlinking 和 unauthorized access 添加 API tests。

- **M0.3-GH-3：添加 trusted validation result ingestion**
  - Commit target：`api: add trusted github result ingestion`
  - Scope：从 trusted callbacks、signed artifacts 或 platform polling 摄取 validation results。
  - Test spec：添加 signature 或 artifact verification tests、replay rejection tests 和 malformed payload tests。

- **M0.3-GH-4：添加 official-run handoff**
  - Commit target：`api: add github official run handoff`
  - Scope：允许 trusted repository workflows 或 admin actions 在 validation 后触发 Agentics-controlled official runs。
  - Test spec：添加 integration tests，证明 private benchmark data 永不离开 Agentics-controlled runners，leaderboard 只在 official success 后更新。

### Worker 和 CI Integration

- **M0.3-WORKER-1：添加 repository artifact fetch support**
  - Commit target：`worker: fetch trusted repository artifacts`
  - Scope：获取 trusted solution artifacts 或 checked-out refs 以用于 official runs，不依赖 untrusted fork CI 处理 private benchmark data。
  - Test spec：添加 mocked GitHub artifact/ref fetch tests，以及 missing、expired 或 oversized artifacts 的 failure-mode tests。

- **M0.3-CI-1：添加 validation workflow templates**
  - Commit target：`ci: add github validation workflow templates`
  - Scope：为 forks 或 PRs 上的 public validation runs 提供 reusable workflow templates。
  - Test spec：为 workflow YAML 添加 static validation，并在可行时添加 dry-run style fixture test。

### Web 和 Admin

- **M0.3-WEB-1：展示 GitHub-linked solution submissions**
  - Commit target：`web: show github-linked solution-submissions`
  - Scope：在 solution submission pages 展示 PR URL、commit SHA、validation status、official-run handoff status 和 trusted artifact metadata。
  - Test spec：为 direct ZIP solution submissions 和 GitHub PR solution submissions 添加 rendering tests。

- **M0.3-ADMIN-1：添加 PR moderation 和 official-run controls**
  - Commit target：`admin: add github pr moderation controls`
  - Scope：添加 admin tools，用于 approving official-run handoff、blocking abusive PR-linked solution submissions 和 inspecting trusted ingestion metadata。
  - Test spec：添加 UI action tests 和 backend authorization tests。

### Agentics CLI

- **M0.3-CLI-1：添加 GitHub workflow helper commands**
  - Commit target：`cli: add github solution submission helpers`
  - Scope：添加 helpers，用于初始化 challenge directories、验证 local repository layout 和打印 PR instructions。
  - Test spec：添加 filesystem fixture tests 和 generated instructions 的 golden-output tests。

### Documentation

- **M0.3-DOC-1：记录 GitHub solution submission security model**
  - Commit target：`docs: document github solution submission security model`
  - Scope：解释 private benchmark data handling、trusted runners、result ingestion、identity mapping、PR spam controls、CI hardware limits 和 GPU limitations。
  - Test spec：根据 implementation behavior 和 PRD GitHub Solution Submission Concerns 审查。

### 实现进度

| 里程碑 | 状态 | 附加说明 |
| --- | --- | --- |
| `M0.3-GH-1：定义 repository layout 和 PR contract` | 计划中 | 定义 public repository contract。 |
| `M0.3-GH-2：添加 GitHub identity mapping` | 计划中 | PR solution submissions 的身份前置条件。 |
| `M0.3-GH-3：添加 trusted validation result ingestion` | 计划中 | 需要具体 trust model。 |
| `M0.3-GH-4：添加 official-run handoff` | 计划中 | 依赖 trusted ingestion 和 official runners。 |
| `M0.3-WORKER-1：添加 repository artifact fetch support` | 计划中 | Repository artifacts official runs 所需。 |
| `M0.3-CI-1：添加 validation workflow templates` | 计划中 | 提供 validation-only templates。 |
| `M0.3-WEB-1：展示 GitHub-linked solution-submissions` | 计划中 | 依赖 PR metadata ingestion。 |
| `M0.3-ADMIN-1：添加 PR moderation 和 official-run controls` | 计划中 | PR workflow 的 admin control。 |
| `M0.3-CLI-1：添加 GitHub workflow helper commands` | 计划中 | Helper layer，不是 CI ingestion 的必要条件。 |
| `M0.3-DOC-1：记录 GitHub solution submission security model` | 计划中 | 应在公开 GitHub workflow 前交付。 |

## Cross-Version Backlog

这些事项跨版本存在，应在它们成为当前 release 阻塞项时再排期。

- **BACKLOG-QA-1：添加 end-to-end smoke harness**
  - Commit target：`test: add local e2e smoke harness`
  - Scope：自动化本地路径，从 migrations 到 agent registration、sample solution submission、worker completion、leaderboard update 和 web read。
  - Test spec：该 harness 本身就是测试。它应能在本地运行，并在 Docker 可用时进入 CI。

- **BACKLOG-DOC-1：保持英文和中文文档一致**
  - Commit target：`docs: sync english and chinese product docs`
  - Scope：每当产品文档变化时，在功能层面保持 `docs/PRD/en.md`、`docs/PRD/zh.md` 和 milestone docs 对齐。
  - Test spec：每次 docs commit 前手动比较 headings 和 feature lists。

- **BACKLOG-OBS-1：改进 operational observability**
  - Commit target：`ops: improve worker and evaluation observability`
  - Scope：根据 worker 和 admin milestones 的需要，添加 structured logs、job lifecycle traces 和 failed evaluations diagnostics。
  - Test spec：在可行处为 emitted state transitions 添加 unit 或 integration tests，并在 smoke tests 中验证 logs。
