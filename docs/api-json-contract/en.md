# API JSON Contract

This document defines the JSON serialization policy for Agentics API DTOs.

## Response DTOs

Agentics response DTOs omit absent optional fields. Rust response structs should
use:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub field: Option<T>,
```

The corresponding TypeScript/Zod shape is:

```ts
field?: T
```

Response DTOs should not emit explicit `null` for absent values. This keeps the
wire format compact, matches the relaxed JSON contract, and reduces ambiguity
when generated schemas are introduced.

## Exceptions

Use explicit `null` only when the API must distinguish a field that is present
but intentionally empty from a field that is not included in the response. Any
exception must be documented next to the Rust DTO field and covered by a
contract fixture.

## Request DTOs

Request DTOs may accept omitted optional fields where that improves client
ergonomics. Request deserialization rules are separate from response
serialization rules.

## Schema Generation

When Agentics adopts generated TypeScript or Zod schemas, the generator must
preserve this mapping:

- `Option<T>` with `skip_serializing_if = "Option::is_none"` becomes
  `field?: T`.
- Explicit-null fields, if any are intentionally introduced, become
  `field: T | null` and require documentation.

Until schema generation is adopted, shared Rust and frontend contract fixtures
must cover representative response DTOs.
