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
紧凑，符合 relaxed JSON contract，并减少未来引入 generated schemas 时的歧义。

## Exceptions

只有当 API 必须区分“字段存在但有意为空”和“字段未包含在 response 中”时，
才应使用显式 `null`。任何例外都必须在 Rust DTO field 旁边注明，并由
contract fixture 覆盖。

## Request DTOs

Request DTOs 可以在有助于 client ergonomics 时接受省略的 optional fields。
Request deserialization rules 与 response serialization rules 分开处理。

## Schema Generation

当 Agentics 引入 generated TypeScript 或 Zod schemas 时，generator 必须保留
以下映射：

- 带有 `skip_serializing_if = "Option::is_none"` 的 `Option<T>` 映射为
  `field?: T`。
- 如果未来有意引入 explicit-null fields，则映射为 `field: T | null`，
  并且必须有文档说明。

在引入 schema generation 之前，shared Rust 与 frontend contract fixtures
必须覆盖有代表性的 response DTOs。
