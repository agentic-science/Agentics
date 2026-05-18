#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${AGENTICS_DEMO_ENV_FILE:-${REPO_ROOT}/deploy/local/agentics.env.example}"
RUNTIME_ROOT="${AGENTICS_DEMO_RUNTIME_ROOT:-${REPO_ROOT}/.agentics-demo}"
PID_DIR="${RUNTIME_ROOT}/pids"
LOG_DIR="${RUNTIME_ROOT}/logs"
DEMO_ADMIN_PASSWORD_FILE="${RUNTIME_ROOT}/admin-password"
COMPOSE_FILE="${REPO_ROOT}/docker/platform-db/docker-compose.yml"
DEFAULT_DEMO_API_HOST="127.0.0.1"
DEFAULT_DEMO_WEB_HOST="127.0.0.1"
DEFAULT_DEMO_API_PORT="13100"
DEFAULT_DEMO_WEB_PORT="13001"

usage() {
  cat <<'EOF'
Usage:
  scripts/dev/local-demo.sh up [--lan]
  scripts/dev/local-demo.sh down [--db] [--purge-data]
  scripts/dev/local-demo.sh seed
  scripts/dev/local-demo.sh status
  scripts/dev/local-demo.sh logs

The demo profile starts local Postgres, runs migrations, seeds a 12-challenge
catalog plus fake completed results, and launches the API plus Next.js frontend
for visual inspection.
It does not start the worker because results are seeded directly.

Demo defaults intentionally differ from foreground development:
  AGENTICS_DEMO_API_HOST=127.0.0.1
  AGENTICS_DEMO_WEB_HOST=127.0.0.1
  AGENTICS_DEMO_API_PORT=13100
  AGENTICS_DEMO_WEB_PORT=13001

Use `up --lan` to bind the API and web frontend to `0.0.0.0` for inspection
from another machine on the same network.

Use `down --purge-data` to remove generated demo logs, PID files, seeded
artifacts, and the local Postgres volume.
EOF
}

log() {
  printf '[agentics-demo] %s\n' "$*"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '[agentics-demo] missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

detect_lan_host() {
  local host=""
  if command -v hostname >/dev/null 2>&1; then
    host="$(hostname -I 2>/dev/null | tr ' ' '\n' | sed -n '/^[0-9]/p' | sed '/^127\./d' | head -n 1 || true)"
  fi
  if [ -z "$host" ] && command -v ip >/dev/null 2>&1; then
    host="$(ip route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p' | head -n 1 || true)"
  fi
  printf '%s\n' "$host"
}

generate_demo_admin_password() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 24
    return
  fi
  if command -v uuidgen >/dev/null 2>&1; then
    printf 'local-demo-%s\n' "$(uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '-')"
    return
  fi
  printf 'local-demo-%s-%s\n' "$$" "$(date +%s)"
}

configure_demo_admin_password() {
  case "${AGENTICS_ADMIN_PASSWORD:-}" in
    ""|"agentics-admin"|"change-me")
      mkdir -p "$RUNTIME_ROOT"
      if [ ! -f "$DEMO_ADMIN_PASSWORD_FILE" ]; then
        generate_demo_admin_password >"$DEMO_ADMIN_PASSWORD_FILE"
        chmod 600 "$DEMO_ADMIN_PASSWORD_FILE" >/dev/null 2>&1 || true
      fi
      export AGENTICS_ADMIN_PASSWORD
      AGENTICS_ADMIN_PASSWORD="$(cat "$DEMO_ADMIN_PASSWORD_FILE")"
      ;;
  esac
}

demo_cors_allowed_origins() {
  local origins="http://127.0.0.1:${AGENTICS_WEB_PORT},http://localhost:${AGENTICS_WEB_PORT}"
  local lan_host="${AGENTICS_DEMO_PUBLIC_HOST:-}"
  if ! host_is_loopback "$AGENTICS_WEB_HOST" && [ -n "$lan_host" ] && [ "$lan_host" != "127.0.0.1" ] && [ "$lan_host" != "localhost" ]; then
    origins="${origins},http://${lan_host}:${AGENTICS_WEB_PORT}"
  fi
  printf '%s\n' "$origins"
}

demo_allowed_dev_origins() {
  local origins="127.0.0.1,localhost"
  local lan_host="${AGENTICS_DEMO_PUBLIC_HOST:-}"
  if ! host_is_loopback "$AGENTICS_WEB_HOST" && [ -n "$lan_host" ] && [ "$lan_host" != "127.0.0.1" ] && [ "$lan_host" != "localhost" ]; then
    origins="${origins},${lan_host}"
  fi
  printf '%s\n' "$origins"
}

demo_network_web_url() {
  if ! host_is_loopback "$AGENTICS_WEB_HOST" && [ -n "${AGENTICS_DEMO_PUBLIC_HOST:-}" ]; then
    printf 'http://%s:%s\n' "$AGENTICS_DEMO_PUBLIC_HOST" "$AGENTICS_WEB_PORT"
  fi
}

host_is_loopback() {
  local host="$1"
  case "$host" in
    localhost|127.*|::1) return 0 ;;
    *) return 1 ;;
  esac
}

load_env() {
  if [ ! -f "$ENV_FILE" ]; then
    printf '[agentics-demo] missing env file: %s\n' "$ENV_FILE" >&2
    exit 1
  fi
  local requested_api_host="${AGENTICS_API_HOST:-}"
  local requested_api_port="${AGENTICS_API_PORT:-}"
  local requested_web_host="${AGENTICS_WEB_HOST:-}"
  local requested_web_port="${AGENTICS_WEB_PORT:-}"
  local requested_api_base_url="${AGENTICS_API_BASE_URL:-}"
  local requested_web_base_url="${AGENTICS_WEB_BASE_URL:-}"
  local requested_cors_origins="${AGENTICS_CORS_ALLOWED_ORIGINS:-}"
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
  export AGENTICS_DATABASE_URL="${AGENTICS_DATABASE_URL:-postgres://agentics:agentics@127.0.0.1:5432/agentics}"
  export AGENTICS_API_HOST="${AGENTICS_DEMO_API_HOST:-${requested_api_host:-$DEFAULT_DEMO_API_HOST}}"
  export AGENTICS_API_PORT="${AGENTICS_DEMO_API_PORT:-${requested_api_port:-$DEFAULT_DEMO_API_PORT}}"
  export AGENTICS_WEB_HOST="${AGENTICS_DEMO_WEB_HOST:-${requested_web_host:-$DEFAULT_DEMO_WEB_HOST}}"
  export AGENTICS_WEB_PORT="${AGENTICS_DEMO_WEB_PORT:-${requested_web_port:-$DEFAULT_DEMO_WEB_PORT}}"
  export AGENTICS_DEMO_DATABASE_NAME="${AGENTICS_DEMO_DATABASE_NAME:-agentics_demo}"
  export AGENTICS_DEMO_PUBLIC_HOST="${AGENTICS_DEMO_PUBLIC_HOST:-$(detect_lan_host)}"
  case "$AGENTICS_DEMO_DATABASE_NAME" in
    ''|*[!A-Za-z0-9_]*)
      printf '[agentics-demo] AGENTICS_DEMO_DATABASE_NAME must contain only letters, digits, and underscores\n' >&2
      exit 1
      ;;
  esac
  export AGENTICS_DATABASE_URL="${AGENTICS_DEMO_DATABASE_URL:-postgres://agentics:agentics@127.0.0.1:${AGENTICS_POSTGRES_PORT:-5432}/${AGENTICS_DEMO_DATABASE_NAME}}"
  export AGENTICS_API_BASE_URL="${AGENTICS_DEMO_API_BASE_URL:-${requested_api_base_url:-http://127.0.0.1:${AGENTICS_API_PORT}}}"
  export AGENTICS_WEB_BASE_URL="${AGENTICS_DEMO_WEB_BASE_URL:-${requested_web_base_url:-http://127.0.0.1:${AGENTICS_WEB_PORT}}}"
  export AGENTICS_CORS_ALLOWED_ORIGINS="${AGENTICS_DEMO_CORS_ALLOWED_ORIGINS:-${requested_cors_origins:-$(demo_cors_allowed_origins)}}"
  export AGENTICS_WEB_ALLOWED_DEV_ORIGINS="${AGENTICS_DEMO_WEB_ALLOWED_DEV_ORIGINS:-${AGENTICS_WEB_ALLOWED_DEV_ORIGINS:-$(demo_allowed_dev_origins)}}"
  export AGENTICS_STORAGE_ROOT="${AGENTICS_STORAGE_ROOT:-storage}"
  export AGENTICS_CHALLENGES_ROOT="${AGENTICS_CHALLENGES_ROOT:-examples/challenges}"
  if ! host_is_loopback "$AGENTICS_API_HOST"; then
    export AGENTICS_WEB_SESSION_COOKIE_SECURE="${AGENTICS_DEMO_WEB_SESSION_COOKIE_SECURE:-true}"
  fi
  configure_demo_admin_password
}

compose() {
  AGENTICS_POSTGRES_PORT="${AGENTICS_POSTGRES_PORT:-5432}" docker compose -f "$COMPOSE_FILE" "$@"
}

storage_root_abs() {
  case "$AGENTICS_STORAGE_ROOT" in
    /*) printf '%s\n' "$AGENTICS_STORAGE_ROOT" ;;
    *) printf '%s/%s\n' "$REPO_ROOT" "$AGENTICS_STORAGE_ROOT" ;;
  esac
}

safe_remove_demo_tree() {
  local label="$1"
  local path="$2"
  case "$path" in
    ""|"/"|"$REPO_ROOT")
      printf '[agentics-demo] refusing to remove unsafe %s path: %s\n' "$label" "$path" >&2
      exit 1
      ;;
    "$REPO_ROOT/.agentics-demo"|"$REPO_ROOT/.agentics-demo/"*|"$REPO_ROOT/storage"|"$REPO_ROOT/storage/"*)
      if [ -e "$path" ]; then
        log "removing ${label}: ${path}"
        rm -rf "$path"
      fi
      ;;
    *)
      printf '[agentics-demo] refusing to remove %s outside demo-owned paths: %s\n' "$label" "$path" >&2
      printf '[agentics-demo] remove it manually if this custom path is intentionally disposable.\n' >&2
      ;;
  esac
}

purge_demo_files() {
  safe_remove_demo_tree "runtime root" "$RUNTIME_ROOT"
  safe_remove_demo_tree "storage root" "$(storage_root_abs)"
}

wait_for_db() {
  log "waiting for Postgres"
  for _ in $(seq 1 60); do
    if compose exec -T platform-db pg_isready -U agentics -d agentics >/dev/null 2>&1; then
      return
    fi
    sleep 1
  done
  printf '[agentics-demo] timed out waiting for Postgres\n' >&2
  exit 1
}

wait_for_http() {
  local label="$1"
  local url="$2"
  log "waiting for ${label}: ${url}"
  for _ in $(seq 1 180); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return
    fi
    sleep 1
  done
  printf '[agentics-demo] timed out waiting for %s at %s\n' "$label" "$url" >&2
  exit 1
}

pid_is_running() {
  local pid_file="$1"
  if [ ! -f "$pid_file" ]; then
    return 1
  fi
  local pid
  pid="$(cat "$pid_file")"
  [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1
}

start_process() {
  local name="$1"
  local cwd="$2"
  shift 2
  local pid_file="${PID_DIR}/${name}.pid"
  local log_file="${LOG_DIR}/${name}.log"

  if pid_is_running "$pid_file"; then
    log "${name} already running with pid $(cat "$pid_file")"
    return
  fi

  mkdir -p "$PID_DIR" "$LOG_DIR"
  log "starting ${name}; log: ${log_file}"
  (
    cd "$cwd"
    if command -v setsid >/dev/null 2>&1; then
      setsid "$@" >"$log_file" 2>&1 < /dev/null &
    else
      nohup "$@" >"$log_file" 2>&1 < /dev/null &
    fi
    printf '%s\n' "$!" >"$pid_file"
  )
}

stop_process() {
  local name="$1"
  local pid_file="${PID_DIR}/${name}.pid"
  if ! pid_is_running "$pid_file"; then
    rm -f "$pid_file"
    return
  fi
  local pid
  pid="$(cat "$pid_file")"
  log "stopping ${name} pid ${pid}"
  kill -- "-$pid" >/dev/null 2>&1 || kill "$pid" >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      rm -f "$pid_file"
      return
    fi
    sleep 0.2
  done
  kill -9 -- "-$pid" >/dev/null 2>&1 || kill -9 "$pid" >/dev/null 2>&1 || true
  rm -f "$pid_file"
}

write_demo_artifact() {
  local submission_id="$1"
  local storage_root
  storage_root="$(storage_root_abs)"
  local artifact_dir="${storage_root}/solution-submissions"
  local artifact_path="${artifact_dir}/${submission_id}.zip"
  local temp_dir
  temp_dir="$(mktemp -d)"
  mkdir -p "$artifact_dir"

  cat >"${temp_dir}/agentics.solution.json" <<'JSON'
{
  "protocol": "zip_project",
  "protocol_version": 1,
  "note": "Local demo artifact for frontend inspection.",
  "commands": {
    "setup": "setup.sh",
    "run": "run.sh"
  }
}
JSON
  cat >"${temp_dir}/README.md" <<'EOF'
# Local Demo Submission

This ZIP is generated by `scripts/dev/local-demo.sh` so the frontend artifact
browser has a realistic public solution to render.
EOF
  cat >"${temp_dir}/setup.sh" <<'EOF'
#!/usr/bin/env sh
set -eu
echo "demo setup"
EOF
  cat >"${temp_dir}/run.sh" <<'EOF'
#!/usr/bin/env sh
set -eu
python main.py
EOF
  cat >"${temp_dir}/main.py" <<'EOF'
import json
import sys

payload = json.load(sys.stdin)
print(json.dumps({"answer": payload.get("a", 0) + payload.get("b", 0)}))
EOF
  chmod +x "${temp_dir}/setup.sh" "${temp_dir}/run.sh"
  (
    cd "$temp_dir"
    zip -qr "$artifact_path" .
  )
  rm -rf "$temp_dir"
}

write_demo_artifacts() {
  local ids=(
    20000000-0000-4000-8000-000000000001
    20000000-0000-4000-8000-000000000002
    20000000-0000-4000-8000-000000000003
    20000000-0000-4000-8000-000000000101
    20000000-0000-4000-8000-000000000102
    20000000-0000-4000-8000-000000000103
  )
  for id in "${ids[@]}"; do
    write_demo_artifact "$id"
  done
}

psql_demo() {
  compose exec -T platform-db psql -v ON_ERROR_STOP=1 -U agentics -d "$AGENTICS_DEMO_DATABASE_NAME" "$@"
}

reset_demo_database() {
  log "resetting demo database ${AGENTICS_DEMO_DATABASE_NAME}"
  compose exec -T platform-db psql -v ON_ERROR_STOP=1 -U agentics -d postgres \
    -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${AGENTICS_DEMO_DATABASE_NAME}' AND pid <> pg_backend_pid();" >/dev/null
  compose exec -T platform-db dropdb -U agentics --if-exists "$AGENTICS_DEMO_DATABASE_NAME"
  compose exec -T platform-db createdb -U agentics "$AGENTICS_DEMO_DATABASE_NAME"
}

seed_demo_results() {
  write_demo_artifacts
  log "seeding fake challenge catalog and public results"
  psql_demo <<'SQL'
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM challenges WHERE name = 'sample-sum') THEN
    RAISE EXCEPTION 'sample-sum challenge was not seeded; start the API before seeding demo results';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM challenges WHERE name = 'grid-routing') THEN
    RAISE EXCEPTION 'grid-routing challenge was not seeded; start the API before seeding demo results';
  END IF;
END $$;

DELETE FROM challenges
WHERE name LIKE 'demo-ui-%';

UPDATE challenges
SET created_at = NOW(), updated_at = NOW()
WHERE name = 'sample-sum';

UPDATE challenges
SET created_at = NOW() - INTERVAL '1 second', updated_at = NOW()
WHERE name = 'grid-routing';

WITH source AS (
  SELECT *
  FROM challenges
  WHERE name = 'sample-sum'
),
fake(name, title, summary, ordinal) AS (
  VALUES
    ('demo-ui-alpha', 'Orbital Protein Folding', 'Predict compact protein conformations under synthetic orbital constraints.', 1),
    ('demo-ui-beta', 'Catalyst Search', 'Find reaction pathways that maximize yield while minimizing unsafe intermediates.', 2),
    ('demo-ui-gamma', 'Cellular Maze', 'Route signaling molecules through a noisy cellular grid without crossing blocked regions.', 3),
    ('demo-ui-delta', 'Climate Patch', 'Select localized interventions that reduce simulated heat stress under budget limits.', 4),
    ('demo-ui-epsilon', 'Lab Scheduler', 'Optimize robotic wet-lab batches while preserving reagent and timing constraints.', 5),
    ('demo-ui-zeta', 'Spectra Denoising', 'Recover clean spectral peaks from corrupted instrument traces.', 6),
    ('demo-ui-eta', 'Genome Primer', 'Design primer sets that cover target regions while avoiding off-target matches.', 7),
    ('demo-ui-theta', 'Graph Molecules', 'Generate candidate molecules that satisfy graph constraints and scoring rules.', 8),
    ('demo-ui-iota', 'Signal Forecast', 'Forecast sparse experimental signals with uncertainty-aware ranking.', 9),
    ('demo-ui-kappa', 'Microscopy Segment', 'Segment cell boundaries from noisy microscopy tiles with hidden labels.', 10)
)
INSERT INTO challenges (
  name, title, summary, bundle_path, statement_path, spec_json,
  starts_at, closes_at, eligibility_policy_json, validation_submission_limit,
  official_submission_limit, leaderboard_visibility, score_distribution_visibility,
  result_detail_visibility, solution_publication_policy, status, created_at, updated_at
)
SELECT
  fake.name,
  fake.title,
  fake.summary,
  source.bundle_path,
  source.statement_path,
  jsonb_set(
    jsonb_set(
      jsonb_set(source.spec_json, '{challenge_name}', to_jsonb(fake.name)),
      '{challenge_title}', to_jsonb(fake.title)
    ),
    '{challenge_summary}', to_jsonb(fake.summary)
  ),
  source.starts_at,
  source.closes_at,
  source.eligibility_policy_json,
  source.validation_submission_limit,
  source.official_submission_limit,
  source.leaderboard_visibility,
  source.score_distribution_visibility,
  source.result_detail_visibility,
  source.solution_publication_policy,
  'active',
  NOW() - ((fake.ordinal + 2) || ' seconds')::interval,
  NOW()
FROM fake
CROSS JOIN source;

DELETE FROM leaderboard_entries
WHERE agent_id IN (
  '10000000-0000-4000-8000-000000000001'::uuid,
  '10000000-0000-4000-8000-000000000002'::uuid,
  '10000000-0000-4000-8000-000000000003'::uuid,
  '10000000-0000-4000-8000-000000000004'::uuid
);

DELETE FROM solution_submissions
WHERE id IN (
  '20000000-0000-4000-8000-000000000001'::uuid,
  '20000000-0000-4000-8000-000000000002'::uuid,
  '20000000-0000-4000-8000-000000000003'::uuid,
  '20000000-0000-4000-8000-000000000101'::uuid,
  '20000000-0000-4000-8000-000000000102'::uuid,
  '20000000-0000-4000-8000-000000000103'::uuid
);

DELETE FROM agent_tokens
WHERE agent_id IN (
  '10000000-0000-4000-8000-000000000001'::uuid,
  '10000000-0000-4000-8000-000000000002'::uuid,
  '10000000-0000-4000-8000-000000000003'::uuid,
  '10000000-0000-4000-8000-000000000004'::uuid
);

DELETE FROM agents
WHERE id IN (
  '10000000-0000-4000-8000-000000000001'::uuid,
  '10000000-0000-4000-8000-000000000002'::uuid,
  '10000000-0000-4000-8000-000000000003'::uuid,
  '10000000-0000-4000-8000-000000000004'::uuid
);

INSERT INTO agents (id, display_name, agent_description, owner, model_info, status, created_at)
VALUES
  ('10000000-0000-4000-8000-000000000001', 'Maple Baseline', 'Deterministic reference implementation for local demo data.', 'Agentics Demo', '{"model":"baseline","profile":"demo"}', 'active', NOW() - INTERVAL '5 days'),
  ('10000000-0000-4000-8000-000000000002', 'Vector Alchemist', 'Optimized vectorized solution with strong private benchmark results.', 'Agentics Demo', '{"model":"demo-optimizer","profile":"demo"}', 'active', NOW() - INTERVAL '4 days'),
  ('10000000-0000-4000-8000-000000000003', 'Careful Optimizer', 'Conservative solution with lower variance across cases.', 'Agentics Demo', '{"model":"careful-demo","profile":"demo"}', 'active', NOW() - INTERVAL '3 days'),
  ('10000000-0000-4000-8000-000000000004', 'Experimental Draft', 'Fresh demo participant used for pending UI states.', 'Agentics Demo', '{"model":"experimental","profile":"demo"}', 'active', NOW() - INTERVAL '2 days');

WITH demo_submissions(id, challenge_name, target, agent_id, note, explanation, score, passed, total, age_hours) AS (
  VALUES
    ('20000000-0000-4000-8000-000000000001'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 'Reference arithmetic implementation.', 'Straightforward parser with exact integer arithmetic.', 1.0000::double precision, 16, 16, 72),
    ('20000000-0000-4000-8000-000000000002'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 'Fast path for compact JSON inputs.', 'Vectorized decode path and minimal allocation.', 0.9375::double precision, 15, 16, 48),
    ('20000000-0000-4000-8000-000000000003'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 'Handles edge cases but misses overflow probe.', 'Careful implementation that intentionally leaves one case unresolved.', 0.8125::double precision, 13, 16, 24),
    ('20000000-0000-4000-8000-000000000101'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 'Shortest-path routing with deterministic tie breaking.', 'A* style route search tuned for narrow corridors.', 0.9167::double precision, 11, 12, 60),
    ('20000000-0000-4000-8000-000000000102'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 'Conservative BFS route planner.', 'Prioritizes valid paths over path length.', 0.8333::double precision, 10, 12, 36),
    ('20000000-0000-4000-8000-000000000103'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 'Baseline Manhattan fallback.', 'Simple fallback route planner with obstacle checks.', 0.6667::double precision, 8, 12, 12)
)
INSERT INTO solution_submissions (
  id, challenge_name, target, agent_id, artifact_key, note, status, explanation,
  credit_text, visible_after_eval, created_at, updated_at
)
SELECT
  id,
  challenge_name,
  target,
  agent_id,
  'solution-submissions/' || id::text || '.zip',
  note,
  'completed',
  explanation,
  'Seeded by scripts/dev/local-demo.sh',
  TRUE,
  NOW() - (age_hours || ' hours')::interval,
  NOW() - (age_hours || ' hours')::interval + INTERVAL '8 minutes'
FROM demo_submissions;

WITH demo_submissions(id, challenge_name, target, agent_id, score, passed, total, age_hours) AS (
  VALUES
    ('20000000-0000-4000-8000-000000000001'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 1.0000::double precision, 16, 16, 72),
    ('20000000-0000-4000-8000-000000000002'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 0.9375::double precision, 15, 16, 48),
    ('20000000-0000-4000-8000-000000000003'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 0.8125::double precision, 13, 16, 24),
    ('20000000-0000-4000-8000-000000000101'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 0.9167::double precision, 11, 12, 60),
    ('20000000-0000-4000-8000-000000000102'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 0.8333::double precision, 10, 12, 36),
    ('20000000-0000-4000-8000-000000000103'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 0.6667::double precision, 8, 12, 12)
),
job_rows AS (
  INSERT INTO evaluation_jobs (
    id, solution_submission_id, challenge_name, target, eval_type, status, priority,
    payload_json, attempt_count, max_attempts, scheduled_at, claimed_at,
    finished_at, worker_id, created_at
  )
  SELECT
    ('30000000-0000-4000-8000-' || lpad(row_number() OVER (ORDER BY id)::text, 12, '0'))::uuid,
    id,
    challenge_name,
    target,
    'official',
    'completed',
    10,
    jsonb_build_object('demo', true),
    1,
    1,
    NOW() - (age_hours || ' hours')::interval,
    NOW() - (age_hours || ' hours')::interval + INTERVAL '1 minute',
    NOW() - (age_hours || ' hours')::interval + INTERVAL '8 minutes',
    'local-demo-seed',
    NOW() - (age_hours || ' hours')::interval
  FROM demo_submissions
  RETURNING id AS job_id, solution_submission_id, target, created_at, finished_at
)
INSERT INTO evaluations (
  id, solution_submission_id, job_id, target, eval_type, status,
  primary_score, rank_score, aggregate_metrics_json, run_metrics_json,
  public_results_json, official_summary_json, started_at, finished_at, created_at
)
SELECT
  ('40000000-0000-4000-8000-' || lpad(row_number() OVER (ORDER BY d.id)::text, 12, '0'))::uuid,
  d.id,
  j.job_id,
  d.target,
  'official',
  'completed',
  d.score,
  d.score,
  jsonb_build_array(
    jsonb_build_object('metric_name', 'score', 'value', d.score),
    jsonb_build_object('metric_name', 'passed_cases', 'value', d.passed)
  ),
  jsonb_build_array(
    jsonb_build_object('run_name', 'public_smoke', 'metrics', jsonb_build_array(jsonb_build_object('metric_name', 'score', 'value', LEAST(1.0, d.score + 0.03)))),
    jsonb_build_object('run_name', 'private_suite', 'metrics', jsonb_build_array(jsonb_build_object('metric_name', 'score', 'value', d.score)))
  ),
  jsonb_build_array(
    jsonb_build_object('case_name', 'public_smoke', 'status', 'passed', 'score', LEAST(1.0, d.score + 0.03), 'message', 'Demo public case passed.'),
    jsonb_build_object('case_name', 'private_suite', 'status', CASE WHEN d.passed = d.total THEN 'passed' ELSE 'failed' END, 'score', d.score, 'message', 'Seeded private benchmark summary.')
  ),
  jsonb_build_object('score', d.score, 'passed', d.passed, 'total', d.total),
  j.created_at + INTERVAL '1 minute',
  j.finished_at,
  j.created_at
FROM demo_submissions d
JOIN job_rows j ON j.solution_submission_id = d.id;

WITH ranked AS (
  SELECT
    s.challenge_name,
    s.target,
    s.agent_id,
    s.id AS submission_id,
    e.rank_score,
    e.primary_score,
    e.aggregate_metrics_json,
    e.public_results_json
  FROM solution_submissions s
  JOIN evaluations e ON e.solution_submission_id = s.id
  WHERE s.id IN (
    '20000000-0000-4000-8000-000000000001'::uuid,
    '20000000-0000-4000-8000-000000000002'::uuid,
    '20000000-0000-4000-8000-000000000003'::uuid,
    '20000000-0000-4000-8000-000000000101'::uuid,
    '20000000-0000-4000-8000-000000000102'::uuid,
    '20000000-0000-4000-8000-000000000103'::uuid
  )
)
INSERT INTO leaderboard_entries (
  challenge_name, target, agent_id, best_solution_submission_id, best_rank_score,
  public_results_json, aggregate_metrics_json, official_score, official_metrics_json, updated_at
)
SELECT
  challenge_name,
  target,
  agent_id,
  submission_id,
  rank_score,
  public_results_json,
  aggregate_metrics_json,
  primary_score,
  aggregate_metrics_json,
  NOW()
FROM ranked;

INSERT INTO service_heartbeats (service_name, last_seen_at, payload)
VALUES
  ('api-server', NOW(), '{"profile":"local-demo","status":"running"}'),
  ('worker', NOW() - INTERVAL '2 minutes', '{"profile":"local-demo","status":"not started; fake results seeded directly"}')
ON CONFLICT (service_name) DO UPDATE
SET last_seen_at = EXCLUDED.last_seen_at,
    payload = EXCLUDED.payload;
SQL
}

install_dependencies() {
  require_command docker
  require_command cargo
  require_command bun
  require_command curl
  require_command zip
  if ! cargo sqlx --version >/dev/null 2>&1; then
    printf '[agentics-demo] cargo-sqlx is required. Install with:\n' >&2
    printf '  cargo install sqlx-cli --no-default-features --features postgres,rustls\n' >&2
    exit 1
  fi
  log "installing frontend dependencies"
  (cd "$REPO_ROOT" && bun install)
}

run_migrations() {
  log "running database migrations"
  (cd "${REPO_ROOT}/backend" && DATABASE_URL="$AGENTICS_DATABASE_URL" cargo sqlx migrate run)
}

api_running() {
  curl -fsS "${AGENTICS_API_BASE_URL}/healthz" >/dev/null 2>&1
}

web_running() {
  curl -fsS "$AGENTICS_WEB_BASE_URL" >/dev/null 2>&1
}

up() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --lan)
        export AGENTICS_DEMO_API_HOST="0.0.0.0"
        export AGENTICS_DEMO_WEB_HOST="0.0.0.0"
        ;;
      -h|--help) usage; exit 0 ;;
      *) printf '[agentics-demo] unknown up argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
    shift
  done

  load_env
  mkdir -p "$RUNTIME_ROOT" "$PID_DIR" "$LOG_DIR"
  install_dependencies
  log "starting Postgres"
  compose up -d platform-db
  wait_for_db
  stop_process web
  stop_process api
  reset_demo_database
  run_migrations

  start_process api "$REPO_ROOT" cargo run -p api-server --bin api
  wait_for_http "API" "${AGENTICS_API_BASE_URL}/healthz"

  seed_demo_results

  start_process web "${REPO_ROOT}/frontends/web" env \
    AGENTICS_API_BASE_URL="$AGENTICS_API_BASE_URL" \
    AGENTICS_API_PORT="$AGENTICS_API_PORT" \
    AGENTICS_WEB_ALLOWED_DEV_ORIGINS="$AGENTICS_WEB_ALLOWED_DEV_ORIGINS" \
    bun run dev -- -H "$AGENTICS_WEB_HOST" -p "$AGENTICS_WEB_PORT"
  wait_for_http "web frontend" "$AGENTICS_WEB_BASE_URL"

  status
  local network_web_url
  network_web_url="$(demo_network_web_url)"
  cat <<EOF

Open:
  ${AGENTICS_WEB_BASE_URL}
$(if [ -n "$network_web_url" ]; then printf '  %s\n' "$network_web_url"; fi)
  ${AGENTICS_WEB_BASE_URL}/challenges/sample-sum/leaderboard?target=linux-arm64-cpu
  ${AGENTICS_WEB_BASE_URL}/challenges/grid-routing/solution-submissions

Logs:
  ${LOG_DIR}/api.log
  ${LOG_DIR}/web.log

Stop:
  scripts/dev/local-demo.sh down
EOF
}

down() {
  load_env
  local stop_db=0
  local purge_data=0
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --db) stop_db=1 ;;
      --purge-data) purge_data=1; stop_db=1 ;;
      -h|--help) usage; exit 0 ;;
      *) printf '[agentics-demo] unknown down argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
    shift
  done
  stop_process web
  stop_process api
  if [ "$stop_db" -eq 1 ]; then
    log "stopping Postgres"
    if [ "$purge_data" -eq 1 ]; then
      compose down -v
    else
      compose down
    fi
  fi
  if [ "$purge_data" -eq 1 ]; then
    purge_demo_files
  fi
}

status() {
  load_env
  printf 'API: '
  if api_running; then
    printf 'up at %s listening on %s:%s\n' "$AGENTICS_API_BASE_URL" "$AGENTICS_API_HOST" "$AGENTICS_API_PORT"
  else
    printf 'down\n'
  fi
  printf 'Web: '
  if web_running; then
    printf 'up at %s listening on %s:%s\n' "$AGENTICS_WEB_BASE_URL" "$AGENTICS_WEB_HOST" "$AGENTICS_WEB_PORT"
    local network_web_url
    network_web_url="$(demo_network_web_url)"
    if [ -n "$network_web_url" ]; then
      printf 'Web LAN: %s\n' "$network_web_url"
    fi
  else
    printf 'down\n'
  fi
  printf 'PIDs: '
  local pid_files
  pid_files="$(find "$PID_DIR" -maxdepth 1 -name '*.pid' -print 2>/dev/null | sort || true)"
  if [ -n "$pid_files" ]; then
    while IFS= read -r pid_file; do
      basename "$pid_file"
    done <<<"$pid_files" | tr '\n' ' '
  fi
  printf '\n'
}

logs() {
  mkdir -p "$LOG_DIR"
  touch "${LOG_DIR}/api.log" "${LOG_DIR}/web.log"
  tail -f "${LOG_DIR}/api.log" "${LOG_DIR}/web.log"
}

seed_only() {
  load_env
  wait_for_db
  seed_demo_results
}

main() {
  local command="${1:-up}"
  shift || true
  case "$command" in
    up) up "$@" ;;
    down) down "$@" ;;
    seed) seed_only "$@" ;;
    status) status "$@" ;;
    logs) logs "$@" ;;
    -h|--help) usage ;;
    *) printf '[agentics-demo] unknown command: %s\n' "$command" >&2; usage >&2; exit 2 ;;
  esac
}

main "$@"
