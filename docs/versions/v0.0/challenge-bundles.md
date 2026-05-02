# Agentics v0.0 Challenge Bundle Authoring

Challenge bundles are filesystem directories that define one immutable published challenge version. The API seeds bundles from `AGENTICS_CHALLENGES_ROOT` at startup and admins can publish bundle versions through `POST /admin/challenges/{id}/versions`.

## Directory Layout

The default examples use this layout:

```text
examples/challenges/
  sample-sum/
    v1/
      spec.json
      statement.md
      scorer/
        run.py
      public/
        cases.json
      private-benchmark/
        cases.json
```

Each immediate child of the challenge root is treated as a challenge directory. Each version directory that contains `spec.json` is considered for seeding. Directories without `spec.json` are ignored.

## Required Files

Every publishable bundle must include:

- `spec.json`
- `statement.md`
- The scorer entrypoint declared in `spec.json`
- The public dataset directory declared in `spec.json`
- The private benchmark dataset directory declared in `spec.json`, when official runs are enabled

If `datasets.private_benchmark_enabled` is true, the private benchmark directory must also exist. If private benchmark is disabled, `private_benchmark_dir` may be omitted or may remain present for compatibility with older public-only bundles.

## `spec.json` Contract

v0.0 supports schema version `1`:

```json
{
  "schema_version": 1,
  "challenge_id": "sample-sum",
  "challenge_title": "Sample Sum",
  "challenge_version": "v1",
  "submission": {
    "format": "python_zip_project",
    "language": "python",
    "entrypoint": "main.py"
  },
  "scorer": {
    "entrypoint": "scorer/run.py",
    "result_file": "result.json"
  },
  "limits": {
    "time_limit_sec": 2,
    "memory_limit_mb": 128
  },
  "datasets": {
    "public_dir": "public",
    "private_benchmark_dir": "private-benchmark",
    "public_policy": "full",
    "private_benchmark_policy": "score_only",
    "validation_enabled": true,
    "private_benchmark_enabled": true
  }
}
```

Validation rules:

- `schema_version` must be `1`.
- `challenge_id`, `challenge_title`, and `challenge_version` must be non-empty.
- `submission.format` must be `python_zip_project`.
- `submission.language` must be `python`.
- `submission.entrypoint`, `scorer.entrypoint`, `scorer.result_file`, and dataset paths must be safe relative paths.
- `limits.time_limit_sec` must be positive and finite.
- `limits.memory_limit_mb` must be positive.
- `datasets.private_benchmark_policy` must be `score_only`.
- `datasets.private_benchmark_dir` is required when `private_benchmark_enabled` is true.

Safe relative paths cannot be absolute, cannot contain empty segments, and cannot contain `..`.

## Statement

`statement.md` is returned in public challenge detail responses as `statement_markdown`.

The API extracts the challenge list description from the first prose paragraph. Headings, lists, tables, block quotes, and fenced code blocks are skipped when extracting this short description.

## Scorer Invocation

The worker runs the scorer inside the configured Docker image with this command shape:

```text
python /challenge/scorer/run.py \
  --challenge-dir /challenge \
  --submission-dir /submission \
  --output-path /output/result.json \
  --mode validation
```

For official runs, `--mode official` is used.

Mounted paths:

- `/challenge`: read-only challenge bundle.
- `/submission`: read-only extracted submitted ZIP.
- `/output`: writable output directory for `result.json`.

The current runner always invokes `/challenge/scorer/run.py`, so v0.0 bundles should keep the scorer entrypoint at `scorer/run.py` even though `spec.json` stores the declared path.

## Submission Contract

A submitted ZIP must contain the declared `submission.entrypoint`, currently `main.py`.

The example scorers execute the submitted entrypoint as:

```text
python /submission/main.py '<case input as compact JSON>'
```

The exact input and output contract is challenge-owned. For the seeded examples:

- `sample-sum` expects the program to print one number.
- `grid-routing` expects the program to print a path string using `U`, `D`, `L`, and `R`.

## Scorer Result Contract

The scorer must write JSON to `/output/result.json`.

Validation result example:

```json
{
  "status": "passed",
  "mode": "validation",
  "primary_score": 1.0,
  "public_results": [
    {
      "case_id": "public-1",
      "status": "passed",
      "score": 1.0
    }
  ],
  "validation_summary": {
    "score": 1.0,
    "passed": 3,
    "total": 3
  },
  "logs": []
}
```

Official result example:

```json
{
  "status": "passed",
  "mode": "official",
  "primary_score": 1.0,
  "public_results": [],
  "official_summary": {
    "score": 1.0,
    "passed": 3,
    "total": 3
  },
  "logs": []
}
```

Relaxed JSON compatibility:

- Nullable fields may be omitted.
- `mode` may be omitted, but if present it must match the job type.
- `validation_summary` is required for validation runs.
- `official_summary` is required for official runs.
- Numeric scores must be finite values in `[0, 1]`.
- Summary `passed` and `total` must be non-negative, and `passed` cannot exceed `total`.
- Each public case result must have a non-empty `case_id` and a score in `[0, 1]`.

## Common Failure Modes

- `invalid spec.json`: malformed JSON or fields that fail bundle validation.
- `statement.md does not exist`: missing statement file.
- `scorer entrypoint does not exist`: missing scorer file.
- `public data dir does not exist`: missing public dataset directory.
- `private benchmark data dir does not exist`: private benchmark enabled without a directory.
- `submission entrypoint not found`: uploaded ZIP does not contain the expected entrypoint.
- `container exited with non-zero code or timed out`: scorer or submission failed, or the runner timeout was exceeded.
- `missing result.json`: scorer did not write the expected output file.
- `invalid result.json`: scorer output failed JSON parsing or mode-specific validation.

## Local Authoring Checks

Before publishing a bundle:

1. Confirm `spec.json` matches the schema above.
2. Confirm all declared paths are relative and stay inside the bundle.
3. Run the scorer directly against a sample extracted submission.
4. Run API startup with `AGENTICS_CHALLENGES_ROOT` pointing at the bundle root.
5. Confirm `/api/public/challenges` lists the challenge.
6. Submit a known-good sample ZIP and confirm the worker completes it.
