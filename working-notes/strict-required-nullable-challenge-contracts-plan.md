# Strict Required-Nullable Challenge Contracts Plan

## Summary

Challenge-authored JSON should make absence explicit. For challenge bundle specs, run manifests, and piped session manifests, semantically optional fields must be present and use `null` when absent. Omitted optional keys should be rejected. This plan does not change the public API response DTO policy, where absent optional response fields may still be omitted.

## Product Decision

- `null` means "intentionally absent" for challenge-authored configuration.
- Missing fields mean "author forgot to specify the contract" and should fail validation.
- CPU target accelerator remains a special required-nullable semantic field: CPU targets must explicitly write `"accelerator": null`, GPU targets must write `"accelerator": "gpu"`, and omitted `accelerator` is invalid.
- Empty arrays should not be used to express semantic absence for fields such as no tie breakers or no input files. Use `null` for none, and use a non-empty array when entries exist.
- Mutually exclusive source arms such as `source_path`, `content`, and `content_json` are source-union alternatives, not ordinary optional metadata. Keep the existing exact-one validation rather than requiring the unused arms to be present as `null`.

## Implementation Plan

### Required-Nullable Helpers

- Add a small serde helper module in the domain contract layer with `required_nullable<T>()` for `Option<T>`.
- Add `required_nullable_non_empty_vec<T>()` for fields represented internally as `Vec<T>` where source JSON uses `null` for empty and a non-empty array for present values.
- Add matching serializers only where source contract serialization is used, so empty vectors serialize as `null` and `Option::None` serializes as `null`.
- Add schema helpers or manual schema annotations so generated schemas show fields as required and nullable, because `schemars` will not infer this from `deserialize_with`.
- Keep `TargetAccelerator` as a dedicated custom deserializer and schema implementation because `null` maps to `TargetAccelerator::None`, not to `Option<T>`.

### Challenge Bundle Spec Tightening

- Remove `#[serde(default)]` from `metric_schema` and reject specs that omit it.
- Make `ranking.tie_breaker_metric_names` required-nullable. `null` means no tie breakers, and a non-empty array lists tie breakers in priority order.
- Add `#[serde(deny_unknown_fields)]` to nested challenge-owned structs that currently accept residue, including `SolutionSpec`, `DatasetsSpec`, `PublicDatasetsSpec`, `MetricDefinitionSpec`, `RankingSpec`, `MetricSchemaSpec`, `HardwareProfileSpec`, `ChallengeRunManifest`, `ChallengeRunSpec`, and `ChallengeRunInputFile`.
- Convert semantic optionals in source challenge specs to required-nullable parsing, including `closes_at`, `validation_submission_limit`, `official_submission_limit`, `datasets.private_benchmark_dir`, `resource_profile.resource_description`, `resource_profile.hardware_metadata`, optional hardware fields, setup locators, setup specs, setup reproducibility notes, and metric unit/description.
- Preserve mode-dependent validation for `resource_profile.solution.run`: it is required for `separated_evaluator` and `piped_stdio`, forbidden for `coexecuted_benchmark`, and should be present as `null` only for coexecuted specs.

### Run And Session Manifest Tightening

- Require `ChallengeRunManifest.runs` to be present and non-empty.
- In every run spec, require `stdin_json`, `stdin_text`, `input_files`, and `output_files` keys.
- For `stdin_json` and `stdin_text`, use `null` when absent and rely on existing interface validation for legal combinations.
- For `input_files`, use `null` when there are no input files and a non-empty array when files exist.
- For `output_files`, use `null` when there are no expected output files and a non-empty array for file outputs. `file_system` runs should still require meaningful output files if the evaluator depends on platform-collected outputs.
- In every piped session manifest, require `input_files` and `metadata`; use `null` when absent.

### Challenge Corpus Cleanup

- Update every published and dev challenge bundle under `challenge-repos/agentics-challenges` to use explicit `null` for absent semantic values.
- Known cleanup from the current scan: 121 run entries omit `input_files` and need `input_files: null`.
- Known cleanup from the current scan: 7 piped session manifests omit `input_files` and need `input_files: null`.
- Existing specs currently specify `tie_breaker_metric_names` as arrays; convert empty arrays to `null` and keep non-empty arrays unchanged.
- Keep CPU target `"accelerator": null` entries exactly as they are.
- Replace the challenge repo Python validator with the `agentics` CLI as the validation entrypoint. The current CLI exposes remote review-record validation through `agentics admin review-record validate`, and the Rust contracts crate already has local bundle validation, but the CLI does not yet expose a local challenge repository checker.
- Add `agentics challenge-creator check <path>`. This command lives under `challenge-creator` because creators use it before opening or updating a PR, while admins can still use the same local checker for bulk review preparation without an admin token.
- `agentics challenge-creator check <path>` should auto-detect path shape. If `<path>/agentics.challenge.json` exists, check that one challenge proposal root. If `<path>/challenges/` exists, treat `<path>` as an Agentics challenge repository root and check every direct child under `<path>/challenges/*` that contains `agentics.challenge.json`. If `<path>` itself contains multiple direct children with `agentics.challenge.json`, treat it as a proposal collection directory and check each child. Otherwise fail with a clear message listing the accepted layouts.
- The command should report each checked challenge and aggregate success/failure counts. JSON output should include `checked_count`, `passed_count`, `failed_count`, and per-challenge results with path, challenge name when known, status, and error.
- The checker should call the Rust contract validation path used by publishing and also cover the Python validator's repository-level checks: `agentics.challenge.json`, challenge directory/name agreement, public statement presence, private asset declarations, private-file leakage, declared run/session manifests, public-bundle projection rules, private-benchmark locator policy, digest-pinned image policy when requested, and required-nullable field presence.
- Update challenge-repo hooks, docs, and skills to call `agentics challenge-creator check ...` instead of the Python validator. Remove the Python validator after the CLI replacement covers the same checks.

### Documentation Updates

- Update English and Chinese challenge authoring docs to state the source-contract rule: semantically optional challenge fields are required keys and use `null` when absent.
- Update solution protocol docs only where they describe run/session manifests, especially `input_files`, `output_files`, `stdin_json`, `stdin_text`, and piped session `metadata`.
- Update targets docs to preserve the special accelerator spelling: CPU targets explicitly use `"accelerator": null`; GPU targets use `"accelerator": "gpu"`.
- Update `skills/challenge-authoring-workflow/SKILL.md` and `.agents/skills/challenge-review-workflow/SKILL.md` so agents use `agentics challenge-creator check`.
- Update `challenge-repos/agentics-challenges/README.md` to replace `python3 scripts/validate_challenges.py` with `agentics challenge-creator check .`.
- Update CLI workflow docs to show both single-proposal and multi-proposal forms: `agentics challenge-creator check challenges/hello-world-rs` and `agentics challenge-creator check .`.
- Update docs without hard line breaks inside sentences.

## Tests And Verification

- Add contract unit tests for each required-nullable category: missing key fails, explicit `null` passes, valid value passes, and wrong type fails.
- Add focused tests proving `metric_schema` is required and `rank_score` remains rejected if that refactor is still active in the current branch.
- Add target tests for `accelerator`: explicit `null` passes for CPU, `"gpu"` passes for GPU, missing fails, `"cpu"` fails, and `"none"` fails.
- Add manifest tests for `input_files`: missing fails, `null` passes as empty, empty array fails, and non-empty array passes.
- Add unknown-field tests for tightened nested structs.
- Add CLI tests for `agentics challenge-creator check`: valid single challenge root passes, repository root checks multiple challenges, collection directory checks multiple proposals, missing required-nullable field fails, unsafe run/session paths fail, unknown nested fields fail, private-file leaks fail, and invalid path shape prints accepted layouts.
- Run challenge validation against every published and dev challenge bundle after corpus cleanup.
- Regenerate frontend schemas if any schema-exported contract types change.
- Run targeted Rust contract tests first, then `just test-all` after the strict contract and challenge corpus cleanup are complete.

## Commit Plan

- `fix(challenge-spec): require nullable challenge fields`
- `feat(cli): check challenge repositories locally`
- `fix(challenges): spell absent manifest fields as null`
- `test(challenge-spec): cover required nullable contracts`
- `docs(challenge-spec): document required nullable fields`

## Assumptions

- This is a breaking pre-MVP source-contract cleanup.
- Public API response DTO optional-field omission remains unchanged unless a response explicitly embeds the source challenge contract as-is.
- Empty array is rejected for required-nullable vector fields where the empty state is semantic absence.
- Source-union arms stay exact-one omitted alternatives in this pass.

## Implementation Check

- Done: added domain serde and schema helpers for required-nullable fields and required-nullable non-empty vectors.
- Done: tightened source challenge specs, run manifests, and piped session manifests while preserving source-union exact-one validation for input file content sources.
- Done: kept CPU targets as explicitly required `accelerator: null` and left `TargetAccelerator` as its dedicated null-or-`gpu` type.
- Done: updated the runner to provide a flattened evaluator-visible run manifest so existing challenge evaluators can keep reading metadata fields while source manifests store extra data under `metadata`.
- Done: added `agentics challenge-creator check <path>` with single-proposal, repository-root, and proposal-collection autodetection, then removed the Python validator from the challenge repository.
- Done: migrated published and dev challenge corpus files to explicit `null` fields and moved non-standard run metadata into `metadata`.
- Done: updated English and Chinese docs plus agent skills for required-nullable source contracts and the CLI checker.
- Done: regenerated web schemas and updated web fixtures for the embedded source-spec exception to the response optional-field omission policy.
- Verified: `cargo test -p agentics-domain --lib`.
- Verified: `cargo test -p agentics-contracts`.
- Verified: `cargo test -p agentics-runner`.
- Verified: `cargo test -p agentics --lib`.
- Verified: `target/debug/agentics challenge-creator check challenge-repos/agentics-challenges --json`.
- Verified: `target/debug/agentics challenge-creator check challenge-repos/agentics-challenges/dev/challenges --json`.
- Verified: `cd frontends/web && bun run generate:schemas:check`.
- Verified: `cd frontends/web && bunx biome check`.
- Verified: `cd frontends/web && bunx tsc --noEmit`.
- Verified: `cd frontends/web && bun test`.
- Verified: `cargo fmt --all -- --check`.
- Verified: `cargo clippy --workspace --all-targets -- -D warnings`.
- Deferred: `just test-all` remains for the combined end-to-end pass after the remaining GPU/submission plan lands.
