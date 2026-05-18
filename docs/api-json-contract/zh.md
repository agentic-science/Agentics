# API JSON Contract

本文档定义 Agentics API DTO 的 JSON 序列化规则。

## Response DTOs

Agentics response DTOs 应省略不存在的 optional fields。Rust response structs
应使用：

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub field: Option<T>,
```

对应的 TypeScript/Zod 形态是：

```ts
field?: T
```

Response DTOs 不应为缺失值输出显式 `null`。这样可以保持 wire format
紧凑，符合 relaxed JSON contract，并减少 generated schemas 中的歧义。

## Exceptions

只有当 API 必须区分“字段存在但有意为空”和“字段未包含在 response 中”时，
才应使用显式 `null`。任何例外都必须在 Rust DTO field 旁边注明，并由
contract fixture 覆盖。当前例外：`targets[].accelerator` 是 required nullable
field，`null` 表示没有 accelerator，`"gpu"` 表示 GPU acceleration。

## Request DTOs

Request DTOs 可以在有助于 client ergonomics 时接受省略的 optional fields。
Request deserialization rules 与 response serialization rules 分开处理。

## Locator Naming

只有 canonical lookup values 才使用 `*_key`，不要把 `key` 当作 `id`、
`name`、`path` 或 `url` 的通用替代词。

- `storage_key`、`artifact_key` 和 `log_key` 是相对于 Agentics storage
  backend 的 opaque object-storage keys。它们不是 filesystem paths、URLs 或
  URIs，即使 local development 会把它们映射到磁盘文件。
- `repo_url` 是 contributor 提交的 GitHub remote，应保留用于 provenance
  和 display。
- `repo_key` 是用于 duplicate detection 和 authorization 的 canonical
  GitHub repository identity。它会把同一个 repository 的 GitHub HTTPS 与
  SSH remotes 规范化成小写的 `owner/repo`。

如果一个值本质上是 object-storage key，不要暴露含糊的 `path` 或 `uri`
字段。如果需要保留原始 remote，不要用 `repo_key` 替代 `repo_url`。

## Schema Generation

Frontend runtime schemas 从 Rust DTOs 生成：

```bash
cd frontends/web
bun run generate:schemas
bun run generate:schemas:check
```

该命令会运行 `backend/shared` 的 `export_web_schemas` binary，将 JSON
Schemas 转成 Zod，并写入 `frontends/web/src/lib/generated/schemas.ts`。
手写的 `frontends/web/src/lib/schemas.ts` 只作为 frontend imports 的稳定
re-export facade。

`bun run generate:schemas:check` 是非写入的 freshness check。常规验证中应通过；
如果 Rust DTO 改动没有重新生成到 frontend schema facade，它应失败。

Generator 必须保留以下映射：

- 带有 `skip_serializing_if = "Option::is_none"` 的 `Option<T>` 映射为
  `field?: T`。
- 如果未来有意引入 explicit-null fields，则映射为 `field: T | null`，
  并且必须有文档说明。

修改 Rust response DTOs 时，应先更新 derives 和 serde attributes，再重新生成
frontend schemas。只有 API contract 有意变化时，才更新 contract fixtures 或
rendering code。shared Rust 与 frontend contract fixtures 必须覆盖有代表性的
response DTOs。

Public result DTOs 必须通过 projection 保持 redaction，而不是依赖 frontend
约定。Public solution submission lists 只暴露 official result-of-record fields；
validation-only scores 不属于 public list contract。
