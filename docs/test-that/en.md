# `test-that` Crate Summary

This document summarizes [`test-that`](https://crates.io/crates/test-that), based on the [release announcement](https://hovinen.me/announcements/2026/06/24/introducing-test-that.html), the [docs.rs API reference](https://docs.rs/test-that/latest/test_that/), and the published crate metadata observed on 2026-07-04.

## Snapshot

- Package: `test-that`
- Current observed version: `0.5.2`
- Rust version: `1.81.0`
- License: `Apache-2.0`
- Repository: [`github.com/hovinen/test-that`](https://github.com/hovinen/test-that)
- Description: “A rich assertion and matcher library based on GoogleTest”

## Executive Summary

`test-that` is a Rust assertion library for writing tests as matcher expressions instead of low-level boolean checks.
Its central promise is better test intent and better failure diagnostics: the assertion should say what property the value should satisfy, and the failure should explain which part did not satisfy it.
The crate is a fork of `googletest` for Rust, created because the author wanted the expressive matcher model of older GoogleTest Rust while avoiding newer API friction around references and pattern matching.
It is not a test runner, property-testing framework, mocking framework, or snapshot-testing tool.
It is a library of assertion macros and matchers that can be used inside normal Rust tests.

## Core Idea

The basic shape is:

```rust
use test_that::prelude::*;

#[test]
fn example() -> TestResult {
    let values = vec![5, 123, -4];

    verify_that!(values, each(gt(0)))
}
```

When that test fails, the library reports the asserted expression, the expected matcher description, and a structured explanation of where the actual value diverged.
The release announcement uses the vector example above to show that the failure points to the offending element rather than only saying that a boolean expression was false.

## Assertion Macros

`test-that` exposes assertion macros that evaluate a value against a matcher.
The announcement emphasizes `verify_that!`, which returns a test result and therefore fits naturally with tests that return `TestResult`.
The crate also provides assertion-style macros for tests that should fail immediately on mismatch.
The important conceptual split is that the macro captures the actual value and the matcher describes the expected property.

This is different from fluent assertion libraries where the API reads like `assert_that!(value).is_equal_to(expected)`.
In `test-that`, the idiom is matcher-based: `verify_that!(value, eq(expected))`, `verify_that!(values, each(gt(0)))`, or a richer matcher expression.

## Matcher Surface

The matcher vocabulary is broad enough for ordinary unit tests and structured data assertions.
The release announcement and API reference highlight these areas:

- Equality and comparison matchers such as `eq` and `gt`.
- Container matchers such as `each`, `contains_exactly!`, `contains_each!`, and `is_contained_in!`.
- Ordered and unordered collection checks, including `in_order()` for collection matchers that can enforce ordering.
- Pattern matching through `matches_pattern!`, including matching fields, tuple-struct fields, and method-returned properties.
- Map matching with `contains_exactly!` and key-value syntax using `=>`.
- Regex-backed matchers when the `regex` feature is enabled.
- Numeric matchers when the numeric feature support is enabled.
- Non-fatal assertions when the `non-fatal-assertions` feature is enabled.

The core API is designed so matchers are composable values.
That makes it possible to build nested checks, for example matching a struct while also matching one field as a collection and another field as a scalar predicate.

## Design Motivation

The announcement says `test-that` was forked from GoogleTest Rust because later GoogleTest Rust versions introduced reference-related API friction.
The examples show cases where a matcher inside `matches_pattern!` requires a reference in one context but rejects that reference in another context, creating a mental model that is hard to apply consistently.
The author argues that unit-test assertions should be low-friction: developers should be able to state the intended property and let the matcher machinery produce useful diagnostics.

The fork changes the internal matcher design in three important ways:

- The `Matcher` trait takes the actual value type as a type parameter again, which lets the same matcher participate in more lifetime and reference contexts.
- Matcher constructors no longer need to be parameterized by the actual type; that type is inferred from where the matcher is used.
- Concrete matcher structs are part of the public API instead of being hidden behind opaque `impl Matcher` return types.

The practical result is better composition for pattern matching, methods returning slices or string slices, values containing references, and containers that yield owned values.

## API Differences From GoogleTest Rust

The announcement calls out several user-facing differences from GoogleTest Rust:

- The API is intended to be simpler than recent GoogleTest Rust releases.
- The `Result` alias was renamed to `TestResult` to avoid colliding with ordinary `Result` types in test modules.
- `unordered_elements_are!` was renamed to `contains_exactly!`.
- Ordered collection matching is expressed by adding `.in_order()` to collection matchers, which makes the stronger ordering constraint explicit.
- `contains_each!` and `is_contained_in!` can also use `.in_order()` when the relative ordering matters.
- `HashMap` matching inside `contains_exactly!` uses `key_matcher => value_matcher` syntax instead of pair syntax.
- The crate includes compatibility and migration features for users porting existing GoogleTest Rust assertions.

For Agentics, the migration detail matters mostly if we ever import existing GoogleTest Rust style tests from another codebase.
For new tests, the better path is to write directly against the `test-that` names and avoid compatibility aliases unless they are needed for a mechanical migration.

## Feature Flags

The observed `0.5.2` crate metadata reports these features:

- `default`: enables `regex`, `num-traits`, `non-fatal-assertions`, and `std`.
- `std`: enables standard-library support.
- `regex`: enables regex-backed matchers through the `regex` dependency.
- `num-traits`: enables numeric matcher support through `num-traits`.
- `non-fatal-assertions`: enables non-fatal assertion support and depends on `test-that-macro` plus `std`.
- `test-that-macro`: enables the procedural macro dependency directly.
- `anyhow`: enables integration with `anyhow`.
- `proptest`: enables integration with `proptest`.
- `googletest-compat`: enables compatibility names for GoogleTest Rust migration.
- `googletest-migrate`: enables migration helpers and includes `googletest-compat`.

The announcement says the crate can be built with all features disabled, in which case dependency-backed capabilities such as non-fatal assertions, regexes, and some numeric matchers are unavailable.
That is useful for projects that care about keeping test dependency trees small.

## Strengths

`test-that` is strongest when a test failure should explain a mismatch inside a structured value.
For example, it can report which collection element failed a predicate or which field in a pattern did not match.
This is often more useful than a plain `assert!(...)` failure and can be more readable than a long hand-written `assert_eq!` chain.

The matcher style also scales well for nested assertions.
Instead of first extracting intermediate values into local variables, a test can describe the intended shape of the whole object.
That can make tests read more like contract examples and less like imperative inspection scripts.

The crate also has a practical migration story for users of GoogleTest Rust.
It is close enough to that ecosystem to feel familiar, but it intentionally cleans up names and matcher composition.

## Tradeoffs

The matcher DSL is another test vocabulary for contributors to learn.
For a very simple equality check, `assert_eq!(actual, expected)` may still be clearer and has no extra dependency.
The crate is more attractive when tests involve nested structs, collections, partial object matching, regexes, or diagnostics where a single boolean failure is not enough.

The matcher style can also become too dense if a single assertion tries to describe too much.
Agentics tests should still prefer clear setup, named fixtures, and small assertions over one enormous matcher expression.
The goal is better failure messages, not clever one-liners.

Because `test-that` is an assertion library, it does not replace integration-test orchestration, database fixtures, Docker-backed runner tests, snapshot tests, fuzzing, or property testing.
It can combine with those tools, but it is not a substitute for them.

## Fit For Agentics

`test-that` could be useful in Agentics where tests currently inspect structured Rust values and would benefit from richer mismatch diagnostics.
Good candidate areas include:

- Challenge bundle validation tests, especially when asserting nested execution specs, metric schemas, resource profiles, and required-nullable fields.
- Config parsing tests where many inputs differ only by one invalid field.
- CLI rendering tests where a parsed JSON response or command result has a nested expected shape.
- Service-layer unit tests where domain records have many fields and the test only cares about selected fields.
- Storage and archive tests where the failure should identify a specific missing or unexpected entry.

Less compelling areas include full API integration tests that already assert HTTP status and persisted database state, runner tests dominated by Docker side effects, and tests where ordinary `assert_eq!` already gives an excellent diff.

## Agentics Adoption Policy

Agentics uses `test-that` opportunistically rather than through a mass rewrite.
Add it as a dev-dependency only in crates where it improves meaningful tests.
Use it for nested structural assertions, matcher composition, and diagnostics-heavy tests.
Keep `assert_eq!`, `assert!`, and direct pattern matching where they are simpler.
Avoid enabling compatibility features unless migrating GoogleTest Rust tests.

The current adopted areas are contract validation, storage/config/runner unit tests, CLI workflow tests, ops Compose tests, and selected integration assertions where structured mismatch output is useful.
Those tests encode many shape and policy rules, and more expressive failure messages make broad schema refactors easier to debug.

## Bottom Line

`test-that` is a serious Rust assertion library with a GoogleTest-style matcher model, polished diagnostics, and a cleaner API direction than the GoogleTest Rust version it forked from.
It is worth considering for Agentics test code that asserts rich structured contracts, but it should not replace simple standard assertions or higher-level integration tests.
