# `test-that` Crate 总结

本文根据 [`test-that`](https://crates.io/crates/test-that) 的 [release announcement](https://hovinen.me/announcements/2026/06/24/introducing-test-that.html)、[docs.rs API reference](https://docs.rs/test-that/latest/test_that/) 以及 2026-07-04 观察到的已发布 crate 元数据整理。

## 基本信息

- Package：`test-that`
- 当前观察到的版本：`0.5.2`
- Rust version：`1.81.0`
- License：`Apache-2.0`
- Repository：[`github.com/hovinen/test-that`](https://github.com/hovinen/test-that)
- Description：“A rich assertion and matcher library based on GoogleTest”

## 核心总结

`test-that` 是一个 Rust assertion library，用 matcher expression 来表达测试断言，而不是只写底层的 boolean check。
它的核心目标是让测试更清楚地表达意图，并在失败时给出更有信息量的诊断：断言应该说明这个值需要满足什么性质，失败信息应该指出实际值的哪个部分不满足。
这个 crate 是 Rust 版 `googletest` 的 fork，作者希望保留旧版 GoogleTest Rust 中表达力强的 matcher model，同时避开新版 API 在 reference 和 pattern matching 上带来的心智负担。
它不是 test runner、property-testing framework、mocking framework 或 snapshot-testing tool。
它是一组可以在普通 Rust tests 里使用的 assertion macros 和 matchers。

## 核心用法

基本形态如下：

```rust
use test_that::prelude::*;

#[test]
fn example() -> TestResult {
    let values = vec![5, 123, -4];

    verify_that!(values, each(gt(0)))
}
```

当这个测试失败时，库会报告被断言的表达式、期望的 matcher 描述，以及实际值在哪里偏离了期望。
release announcement 用上面的 vector 示例说明，失败信息会指出具体哪个元素违反了 predicate，而不只是说某个 boolean expression 是 false。

## Assertion Macros

`test-that` 暴露了一组 assertion macros，用 matcher 检查实际值。
announcement 重点展示了 `verify_that!`，它返回 test result，因此很适合返回 `TestResult` 的测试函数。
这个 crate 也提供了在 mismatch 时立即失败的 assertion-style macros。
关键概念是 macro 捕获 actual value，而 matcher 描述 expected property。

这和 fluent assertion library 不同，后者的 API 通常像 `assert_that!(value).is_equal_to(expected)`。
在 `test-that` 中，惯用写法是 matcher-based：`verify_that!(value, eq(expected))`、`verify_that!(values, each(gt(0)))`，或者更丰富的 matcher expression。

## Matcher 能力范围

它的 matcher vocabulary 足以覆盖普通 unit tests 和结构化数据断言。
release announcement 和 API reference 重点体现了以下能力：

- Equality 和 comparison matchers，例如 `eq` 和 `gt`。
- Container matchers，例如 `each`、`contains_exactly!`、`contains_each!` 和 `is_contained_in!`。
- Ordered 和 unordered collection checks，包括 collection matchers 上用于强制顺序的 `in_order()`。
- 通过 `matches_pattern!` 进行 pattern matching，包括匹配字段、tuple struct 字段以及 method-returned properties。
- 使用 `contains_exactly!` 和 `=>` key-value syntax 匹配 map。
- 启用 `regex` feature 后使用 regex-backed matchers。
- 启用 numeric feature support 后使用 numeric matchers。
- 启用 `non-fatal-assertions` feature 后使用 non-fatal assertions。

核心 API 把 matchers 设计为可以组合的值。
这使得嵌套检查成为可能，例如匹配一个 struct，同时把一个字段作为 collection 来匹配，把另一个字段作为 scalar predicate 来匹配。

## 设计动机

announcement 解释说，`test-that` 从 GoogleTest Rust fork 出来，是因为后来的 GoogleTest Rust 版本引入了和 reference 相关的 API 摩擦。
示例展示了 `matches_pattern!` 中的 matcher 在某些上下文里必须传 reference，但在另一些上下文里又不能传 reference，这让一致使用 API 的心智模型变得困难。
作者认为 unit-test assertions 应该低摩擦：开发者应该能直接表达想检查的性质，然后让 matcher machinery 生成有用的诊断信息。

这个 fork 在内部 matcher 设计上做了三项关键改变：

- `Matcher` trait 再次把 actual value type 作为 type parameter，因此同一个 matcher 可以参与更多 lifetime 和 reference contexts。
- Matcher constructors 不再需要由 actual type 参数化；actual type 由 matcher 的使用位置推断。
- Concrete matcher structs 成为 public API 的一部分，而不是隐藏在 opaque `impl Matcher` return types 后面。

实际效果是更好的 pattern matching 组合能力，尤其是 methods returning slices or string slices、values containing references，以及 containers yielding owned values 这些场景。

## 与 GoogleTest Rust 的 API 差异

announcement 提到了一些用户可见的差异：

- API 目标是比近期 GoogleTest Rust releases 更简单。
- `Result` alias 改名为 `TestResult`，避免和 test modules 里的普通 `Result` 类型冲突。
- `unordered_elements_are!` 改名为 `contains_exactly!`。
- Ordered collection matching 通过给 collection matcher 添加 `.in_order()` 来表达，让更强的 ordering constraint 更显式。
- `contains_each!` 和 `is_contained_in!` 在相对顺序重要时也可以使用 `.in_order()`。
- `contains_exactly!` 里的 `HashMap` matching 使用 `key_matcher => value_matcher` syntax，而不是 pair syntax。
- crate 包含 compatibility 和 migration features，方便从已有 GoogleTest Rust assertions 迁移。

对 Agentics 来说，迁移细节主要在我们从别的代码库引入 GoogleTest Rust 风格测试时才重要。
对于新测试，更好的方式是直接使用 `test-that` 的名称，除非需要机械迁移，否则不要引入 compatibility aliases。

## Feature Flags

观察到的 `0.5.2` crate metadata 报告了这些 features：

- `default`：启用 `regex`、`num-traits`、`non-fatal-assertions` 和 `std`。
- `std`：启用 standard-library support。
- `regex`：通过 `regex` dependency 启用 regex-backed matchers。
- `num-traits`：通过 `num-traits` 启用 numeric matcher support。
- `non-fatal-assertions`：启用 non-fatal assertion support，并依赖 `test-that-macro` 和 `std`。
- `test-that-macro`：直接启用 procedural macro dependency。
- `anyhow`：启用 `anyhow` integration。
- `proptest`：启用 `proptest` integration。
- `googletest-compat`：启用 GoogleTest Rust migration 兼容名称。
- `googletest-migrate`：启用 migration helpers，并包含 `googletest-compat`。

announcement 说明，这个 crate 可以在关闭所有 features 的情况下构建，此时依赖外部 crate 的能力不可用，例如 non-fatal assertions、regexes 和部分 numeric matchers。
这对想控制 test dependency tree 大小的项目有价值。

## 优点

当测试失败需要解释结构化值内部哪个部分不匹配时，`test-that` 最有价值。
例如，它可以报告 collection 中哪个元素没有满足 predicate，或者 pattern 中哪个字段没有匹配。
这通常比普通 `assert!(...)` 失败更有用，也可能比一串手写 `assert_eq!` 更可读。

Matcher 风格也适合嵌套断言。
测试不一定要先把中间值提取到本地变量里，而是可以直接描述整个对象应该满足的形状。
这能让测试更像 contract examples，而不是 imperative inspection scripts。

这个 crate 也为 GoogleTest Rust users 提供了实际迁移路径。
它和那个生态足够接近，因此容易理解，同时又有意清理了名称和 matcher composition。

## 取舍

Matcher DSL 是贡献者需要学习的另一套测试词汇。
对于很简单的 equality check，`assert_eq!(actual, expected)` 可能仍然更清晰，而且没有额外依赖。
当测试涉及 nested structs、collections、partial object matching、regexes，或者普通 boolean failure 不够说明问题时，这个 crate 更有吸引力。

Matcher 风格如果过度压缩，也可能让单个 assertion 变得太密。
Agentics tests 仍然应该偏好清晰的 setup、命名 fixtures 和小断言，而不是一个巨大的 matcher expression。
目标是更好的 failure messages，不是聪明的一行代码。

因为 `test-that` 是 assertion library，它不会替代 integration-test orchestration、database fixtures、Docker-backed runner tests、snapshot tests、fuzzing 或 property testing。
它可以和这些工具组合使用，但不是它们的替代品。

## 对 Agentics 的适配度

在 Agentics 中，`test-that` 适合那些检查结构化 Rust values，并且会从更丰富 mismatch diagnostics 中受益的测试。
比较合适的领域包括：

- Challenge bundle validation tests，尤其是断言 nested execution specs、metric schemas、resource profiles 和 required-nullable fields 时。
- Config parsing tests，尤其是很多输入只差一个 invalid field 时。
- CLI rendering tests，尤其是 parsed JSON response 或 command result 有嵌套 expected shape 时。
- Service-layer unit tests，尤其是 domain records 字段很多但测试只关心部分字段时。
- Storage 和 archive tests，尤其是失败时应该指出某个 missing 或 unexpected entry 时。

不太值得使用的领域包括已经断言 HTTP status 和 persisted database state 的 full API integration tests、主要由 Docker side effects 支配的 runner tests，以及普通 `assert_eq!` 已经能给出很好 diff 的测试。

## Agentics 采用策略

Agentics 对 `test-that` 采用渐进引入，而不是大规模重写。
只在能改善有意义测试的 crates 中把它作为 dev-dependency。
把它用于 nested structural assertions、matcher composition 和 diagnostics-heavy tests。
在更简单的地方继续使用 `assert_eq!`、`assert!` 和直接 pattern matching。
除非迁移 GoogleTest Rust tests，否则不要启用 compatibility features。

当前已经采用的区域包括 contract validation、storage/config/runner unit tests、CLI workflow tests、ops Compose tests，以及一些 structured mismatch output 有价值的 integration assertions。
这些测试已经编码了很多 shape 和 policy rules，更有表达力的 failure messages 会让广泛 schema refactors 更容易调试。

## 结论

`test-that` 是一个严肃的 Rust assertion library，采用 GoogleTest 风格的 matcher model，提供更细致的 diagnostics，并且比它 fork 自的 GoogleTest Rust 版本走向了更清晰的 API。
它值得用于 Agentics 中断言复杂结构化 contract 的测试代码，但不应该替代简单的标准库 assertions 或更高层的 integration tests。
