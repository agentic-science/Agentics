# Rust Modernization Reference

This guide summarizes Rust 1.80.0 through Rust 1.95.0 features that should
shape future Agentics code. It is a full-code-review skill reference for agents
working in this repository, not a complete Rust release changelog.

The Agentics workspace currently uses Rust 2024:

```toml
[workspace]
resolver = "3"

[workspace.package]
edition = "2024"
```

That means Rust 2024 edition behavior is already the local default. Before
using a feature below, still check the active toolchain and any future
`rust-version` policy in `Cargo.toml`.

## Prefer These Language Features

### Let chains in `if` and `while`

Use let chains when a validation path currently needs nested `if let` blocks.
This is useful in manifest validation, challenge bundle parsing, and response
normalization code where optional fields and shape checks are chained.

```rust
if let Some(path) = manifest.commands.setup.as_deref()
    && !is_safe_relative_path(path)
{
    return Err(ServiceError::Validation("invalid setup path".to_owned()));
}
```

This is clearer than nesting when each condition is part of the same guard.
Keep regular `match` when the branches have distinct behavior.

Official announcement: <https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/>

### `if let` guards in `match` arms

Use `if let` guards when a `match` arm should only apply after a secondary
fallible pattern match. This is useful for routing, status mapping, and
state-machine code that already uses `match`.

```rust
match job.status {
    EvaluationJobStatus::Running
        if let Some(claimed_at) = job.claimed_at
            && is_stale(claimed_at) =>
    {
        requeue(job).await?;
    }
    EvaluationJobStatus::Running => {}
    EvaluationJobStatus::Queued => claim(job).await?,
    EvaluationJobStatus::Completed | EvaluationJobStatus::Failed => {}
}
```

Do not rely on guard patterns for exhaustiveness. Rust does not count guard
conditions as proving that the overall `match` is exhaustive.

Official announcement: <https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/>

### Async closures

Use async closures for small async callbacks that need to borrow local state.
This can simplify tests, CLI workflows, and helper code that currently has to
wrap an async block in a regular closure.

```rust
let submit = async |path: &Path| {
    let artifact = package_solution(path).await?;
    client.submit_solution(artifact).await
};
```

Use a named `async fn` when the logic is non-trivial, reused, or needs a clear
error boundary.

Official announcement: <https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/>

### Precise `impl Trait` capture with `use<...>`

Use `+ use<...>` when returning `impl Trait` and the hidden type should capture
only specific lifetimes, type parameters, or const parameters. This replaces
older "Captures trick" style workarounds.

```rust
fn visible_cases<'a>(
    cases: &'a [ChallengeCase],
) -> impl Iterator<Item = &'a ChallengeCase> + use<'a> {
    cases.iter().filter(|case| case.public)
}
```

In Rust 2024, return-position `impl Trait` captures more lifetimes by default.
Use precise capture when the default makes the API too restrictive or obscures
what the returned value actually borrows.

Official announcements:

- <https://blog.rust-lang.org/2024/10/17/Rust-1.82.0/>
- <https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/>

### Trait upcasting

Use trait upcasting instead of adding manual `as_supertrait` methods when a
trait object needs to be viewed as one of its supertraits.

```rust
trait Storage: Send + Sync {
    // ...
}

trait ArtifactStorage: Storage {
    // ...
}

fn as_storage(storage: &dyn ArtifactStorage) -> &dyn Storage {
    storage
}
```

This is mainly useful if Agentics grows more trait-object based services. Do
not introduce trait objects only to use this feature; keep concrete generics
where they are simpler.

Official announcement: <https://blog.rust-lang.org/2025/04/03/Rust-1.86.0/>

### Native raw pointer syntax

Use `&raw const expr` and `&raw mut expr` in unsafe code that needs raw
pointers without first creating references. This matters for packed fields,
FFI, and low-level code.

```rust
let ptr = &raw const packed.not_aligned_field;
```

Agentics should rarely need this in normal backend, CLI, or database code. If
unsafe code appears in runner isolation, archive handling, or platform-specific
logic, prefer this syntax over `addr_of!` and document the safety invariant.

Official announcement: <https://blog.rust-lang.org/2024/10/17/Rust-1.82.0/>

### Rust 2024 unsafe boundaries

Rust 2024 tightened several safety-related defaults:

- `unsafe_op_in_unsafe_fn` warns by default.
- `extern` blocks should be written as `unsafe extern`.
- Unsafe attributes such as `no_mangle`, `link_section`, and `export_name`
  should be written as `#[unsafe(...)]`.
- References to `static mut` are denied by default.

Agentics should keep unsafe code scarce. When unsafe code is needed, make the
unsafe operation explicit inside an `unsafe {}` block even inside an
`unsafe fn`, and place a short `SAFETY:` comment on the invariant being relied
on.

Official announcement: <https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/>

### Exclusive range patterns

Use exclusive range patterns for adjacent numeric ranges in validation and
classification code.

```rust
match timeout_sec {
    0 => Err("timeout must be positive"),
    1..30 => Ok(TimeoutClass::Short),
    30..300 => Ok(TimeoutClass::Normal),
    _ => Ok(TimeoutClass::Long),
}
```

This avoids off-by-one constants and makes boundary ownership visible.

Official announcement: <https://blog.rust-lang.org/2024/07/25/Rust-1.80.0/>

### `_` inference for const generics

Use `_` for const generic arguments in expression contexts when the compiler
can infer the value from the surrounding type.

```rust
let row: [bool; 4] = [false; _];
```

This will not be common in Agentics today, but it can reduce noise in tests and
fixed-size validation helpers.

Official announcement: <https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/>

### `cfg_select!` and boolean `cfg`

Use `cfg_select!` when platform-specific code has multiple mutually exclusive
branches. Agentics already has Unix and non-Unix code in the CLI; `cfg_select!`
can make future platform dispatch easier to read.

```rust
let default_config_dir = cfg_select! {
    unix => unix_config_dir(),
    windows => windows_config_dir(),
    _ => fallback_config_dir(),
};
```

Use `cfg(true)` or `cfg(false)` when a generated or macro-heavy path needs an
explicit always-on or always-off predicate. For normal code, avoid clever cfg
expressions.

Official announcements:

- <https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/>
- <https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/>

## Useful Stabilized APIs

### `LazyLock` and `LazyCell`

Use `std::sync::LazyLock` for process-wide static data that is expensive or
awkward to initialize at compile time, such as static regexes or lookup tables.
Prefer it over adding a crate such as `lazy_static` or `once_cell`.

```rust
static RESERVED_NAMES: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
    std::sync::LazyLock::new(|| std::collections::HashSet::from(["CON", "NUL", "AUX"]));
```

Do not use global lazy state for request-scoped configuration, database pools,
or test state that should remain explicit.

Official announcement: <https://blog.rust-lang.org/2024/07/25/Rust-1.80.0/>

### `Option::take_if`

Use `Option::take_if` when validation or state transitions need to remove a
value only if it satisfies a predicate.

```rust
let expired_claim = job.claim.take_if(|claim| claim.is_stale(now));
```

This can be clearer than a separate `if option.as_ref().is_some_and(...)`
followed by `take()`.

Official announcement: <https://blog.rust-lang.org/2024/07/25/Rust-1.80.0/>

### `std::fs::exists`

Use `std::fs::exists` in synchronous CLI or test code when the only question is
whether a path exists. It avoids the common `metadata(...).is_ok()` idiom.

```rust
if !std::fs::exists(&manifest_path)? {
    return Err(anyhow::anyhow!("missing manifest: {}", manifest_path.display()));
}
```

In async backend paths, prefer `tokio::fs` and keep blocking filesystem calls
out of request handlers and worker tasks.

Official announcement: <https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/>

### `HashMap::get_disjoint_mut` and slice `get_disjoint_mut`

Use these APIs when code needs multiple mutable references from the same map or
slice and the keys or indices are known to be distinct.

```rust
let [validation, official] =
    summaries.get_disjoint_mut([&ScoringMode::Validation, &ScoringMode::Official]);

if let (Some(validation), Some(official)) = (validation, official) {
    normalize_pair(validation, official);
}
```

This is preferable to cloning, temporarily removing entries, or using
interior mutability just to satisfy the borrow checker.

Official announcement: <https://blog.rust-lang.org/2025/04/03/Rust-1.86.0/>

### `std::io::pipe`

Use `std::io::pipe` for local child-process workflows that need to combine or
redirect output without temporary files. It may be useful for future CLI-side
validation helpers.

The current Docker runner mostly consumes logs through Bollard streams, so do
not refactor runner logging just to use this API.

Official announcement: <https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/>

### Collection filtering APIs

Use collection-native extraction APIs when removing and processing selected
items:

- `Vec::extract_if`
- `LinkedList::extract_if`
- `BTreeMap::extract_if`
- `BTreeSet::extract_if`
- `VecDeque::pop_front_if`
- `VecDeque::pop_back_if`

These are good fits for job queues, test fixtures, challenge case selection,
and leaderboard maintenance code where matching entries need to be removed and
processed.

Official announcements:

- <https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/>
- <https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/>
- <https://blog.rust-lang.org/2026/01/22/Rust-1.93.0/>

### Path and string quality-of-life APIs

Prefer the newer path helpers when they express intent directly:

- `Path::file_prefix` for archive names where only the first extension should
  be stripped.
- `PathBuf::add_extension` and `PathBuf::with_added_extension` for appending
  extensions without string formatting.
- `OsStr::display`, `OsString::display`, and `os_str::Display` for user-facing
  path-like values.

These are useful in the CLI package builder, artifact naming, and error
messages for uploaded files.

Official announcements:

- <https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/>
- <https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/>

### `array_windows`

Use slice `array_windows` when scanning fixed-width windows. It avoids manual
indexing and gives the closure an array reference with a known length.

```rust
fn has_parent_dir_marker(bytes: &[u8]) -> bool {
    bytes.array_windows().any(|window: &[u8; 3]| window == b"../")
}
```

This can help in parsers, path validation, and compact test assertions. Keep
normal iterator code when the window size is dynamic.

Official announcement: <https://blog.rust-lang.org/2026/03/05/Rust-1.94.0/>

### `Duration` constructors

Use `Duration::from_mins`, `Duration::from_hours`, and
`Duration::from_nanos_u128` when they describe configuration defaults more
clearly than manual multiplication.

```rust
const STALE_CLAIM_GRACE: std::time::Duration = std::time::Duration::from_mins(1);
```

This is useful in worker polling, stale job requeue windows, and timeout
defaults.

Official announcements:

- <https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/>
- <https://blog.rust-lang.org/2026/01/22/Rust-1.93.0/>

### `bool: TryFrom<{integer}>`

Use `bool::try_from(value)` when decoding integer-backed booleans from external
formats, instead of accepting any non-zero value implicitly.

```rust
let visible = bool::try_from(raw_visible)
    .map_err(|_| ServiceError::Validation("visible must be 0 or 1".to_owned()))?;
```

This is useful for strict protocol or database import paths.

Official announcement: <https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/>

## Lints and Diagnostics to Respect

### `mismatched_lifetime_syntaxes`

This lint warns when a function signature hides a lifetime in one position
while showing or eliding it differently elsewhere.

Prefer spelling `'_` in return types when it makes a borrowed result obvious:

```rust
fn cases(spec: &ChallengeBundleSpec) -> std::slice::Iter<'_, ChallengeCase> {
    spec.cases.iter()
}
```

Official announcement: <https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/>

### Dangling raw pointer lint

Do not return raw pointers to local variables. If unsafe code needs a pointer,
make ownership explicit and keep the pointee alive for the required duration.

Official announcement: <https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/>

### Never-type fallback lints

If never-type future-compatibility lints fire, fix the type inference rather
than allowing the lint. Add explicit types around `?`, `return`, `panic!`, or
diverging closures when needed.

Official announcement: <https://blog.rust-lang.org/2025/12/11/Rust-1.92.0/>

## Project Guidance

- Prefer these features when they remove real nesting, cloning, temporary
  variables, or unsafe-code ambiguity.
- Do not refactor working code solely to demonstrate a new Rust feature.
- When touching CLI platform-specific code, consider `cfg_select!` before
  duplicating `#[cfg(unix)]` and `#[cfg(not(unix))]` helper functions.
- When touching validation code, consider let chains, `Option::take_if`,
  exclusive range patterns, and direct path APIs.
- When touching queue or leaderboard logic, consider collection extraction APIs
  and `get_disjoint_mut` before reaching for clones or interior mutability.
- When adding unsafe code, follow Rust 2024 unsafe-boundary style and keep the
  safety invariant local to the unsafe operation.

## Release Announcement Links

- Rust 1.80.0: <https://blog.rust-lang.org/2024/07/25/Rust-1.80.0/>
- Rust 1.81.0: <https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/>
- Rust 1.82.0: <https://blog.rust-lang.org/2024/10/17/Rust-1.82.0/>
- Rust 1.83.0: <https://blog.rust-lang.org/2024/11/28/Rust-1.83.0/>
- Rust 1.84.0: <https://blog.rust-lang.org/2025/01/09/Rust-1.84.0/>
- Rust 1.85.0: <https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/>
- Rust 1.86.0: <https://blog.rust-lang.org/2025/04/03/Rust-1.86.0/>
- Rust 1.87.0: <https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/>
- Rust 1.88.0: <https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/>
- Rust 1.89.0: <https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/>
- Rust 1.90.0: <https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/>
- Rust 1.91.0: <https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/>
- Rust 1.92.0: <https://blog.rust-lang.org/2025/12/11/Rust-1.92.0/>
- Rust 1.93.0: <https://blog.rust-lang.org/2026/01/22/Rust-1.93.0/>
- Rust 1.94.0: <https://blog.rust-lang.org/2026/03/05/Rust-1.94.0/>
- Rust 1.95.0: <https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/>
