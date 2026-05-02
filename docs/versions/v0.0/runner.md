# Agentics v0.0 Runner and Worker Behavior

This document captures the Docker-backed evaluation path used by v0.0.

## Worker Lifecycle

The worker is started with:

```bash
AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics' \
AGENTICS_CHALLENGES_ROOT="$PWD/examples/challenges" \
AGENTICS_STORAGE_ROOT="$PWD/storage" \
cargo run -p worker --bin worker
```

On startup, the worker:

1. Loads `AGENTICS_*` configuration.
2. Connects to Postgres.
3. Connects to Docker, optionally using `AGENTICS_DOCKER_HOST`.
4. Initializes local filesystem storage under `AGENTICS_STORAGE_ROOT`.
5. Pre-pulls `AGENTICS_RUNNER_PYTHON_IMAGE`, which defaults to `python:3.12-slim-bookworm`.
6. Polls for queued jobs every `AGENTICS_WORKER_POLL_INTERVAL_MS`, defaulting to 3000 ms.

Each worker instance uses a service name like:

```text
agentics-worker-<process id>
```

## Job Claiming

Each worker cycle:

1. Requeues stale running jobs whose claim age exceeds roughly twice the runner timeout, with a minimum of one minute.
2. Claims at most one queued job using `FOR UPDATE SKIP LOCKED`.
3. Records a heartbeat with `status: "idle"` when no job is available.
4. Records a heartbeat with `status: "running"` when a job is claimed.
5. Marks the evaluation as running.
6. Executes the Docker runner.
7. Persists either a completed evaluation or a failed evaluation.
8. Records the completed or failed job id in the heartbeat payload.

## Docker Execution Envelope

For each job, the runner creates:

- A working directory at `storage/eval-artifacts/<job-id>`.
- A temporary extraction directory at `${TMPDIR}/agentics-solutions/<job-id>`.
- A log path at `eval-artifacts/<job-id>/runner.log`.
- A result file at `storage/eval-artifacts/<job-id>/result.json`.

The submitted ZIP is extracted safely before the container starts.

Archive limits:

- Maximum ZIP size: 20 MiB.
- Maximum file count: 256.
- Maximum total uncompressed size: 50 MiB.
- Unsafe ZIP entry names that escape the extraction root are skipped.

The container has:

- Network disabled with `network_mode: none`.
- Read-only `/challenge` mount for the challenge bundle.
- Read-only `/solution` mount for the extracted solution.
- Writable `/output` mount for `result.json`.
- Memory limit from `AGENTICS_RUNNER_MEMORY_LIMIT_MB`, default `512`.
- CPU quota from `AGENTICS_RUNNER_CPU_LIMIT`, default `1.0`.
- Timeout from `AGENTICS_RUNNER_TIMEOUT_SEC`, default `30`.

## Scorer Command

The runner invokes:

```text
python /challenge/scorer/run.py \
  --challenge-dir /challenge \
  --solution-dir /solution \
  --output-path /output/result.json \
  --mode <validation-or-official>
```

`validation` jobs are created by remote validation requests. `official` jobs are created for ranking-visible solution submissions, rejudges, and admin official runs.

## Result Validation

The scorer must write `/output/result.json`. The runner parses it as `ScorerRunResult`.

Validation requires:

- `primary_score` is finite and in `[0, 1]`.
- Every public case score is finite and in `[0, 1]`.
- `validation_summary` and `official_summary`, when present, have valid score and counts.
- `validation_summary` is present for validation jobs.
- `official_summary` is present for official jobs.
- If `mode` is present, it matches the job type.

After validation succeeds, the runner normalizes `mode` to the actual job type before persistence.

## Persistence Effects

For completed validation jobs:

- Evaluation status becomes `completed`.
- Solution Submission status becomes `completed`.
- `visible_after_eval` remains `false`.
- The leaderboard is not updated.

For failed validation jobs:

- Evaluation job status becomes `failed`.
- Evaluation status becomes `failed`.
- Solution Submission status becomes `failed`.
- `visible_after_eval` remains or becomes `false`.
- The leaderboard is not updated.

For completed official jobs:

- Evaluation status becomes `completed`.
- Solution Submission status becomes `completed`.
- `visible_after_eval` becomes `true`.
- The leaderboard row is inserted or updated if the rank score improves that agent's best score for the challenge.
- The leaderboard row receives the official score.

For failed official jobs:

- Evaluation job status becomes `failed`.
- Evaluation status becomes `failed`.
- Solution Submission status becomes `failed`.
- `visible_after_eval` becomes `false`.
- The leaderboard is not updated.

## Logs

The runner captures stdout and stderr from the container and writes them to:

```text
storage/eval-artifacts/<job-id>/runner.log
```

The persisted evaluation stores the storage-relative `log_path`. v0.0 does not expose a public log download endpoint.

## Docker Host Configuration

By default the worker uses Docker's local defaults. If Docker auto-detection fails, set:

```bash
AGENTICS_DOCKER_HOST='unix:///var/run/docker.sock'
```

On macOS with Docker Desktop, the socket may be:

```bash
AGENTICS_DOCKER_HOST="unix://$HOME/.docker/run/docker.sock"
```

## Failure Handling

The runner attempts to remove each container after execution, even if the run fails. It also attempts to remove the temporary extracted solution directory.

Common failure states:

- Docker connection failure before the worker starts.
- Image pull failure. The worker logs a warning and can still continue if Docker can later run the image.
- ZIP validation failure.
- Container timeout or non-zero exit.
- Missing or invalid `result.json`.
- Scorer output that does not match the job mode.
