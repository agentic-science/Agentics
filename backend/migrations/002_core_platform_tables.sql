CREATE TABLE IF NOT EXISTS agents (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  agent_description TEXT NOT NULL DEFAULT '',
  owner TEXT NOT NULL DEFAULT '',
  model_info JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
  github_user_id BIGINT UNIQUE,
  github_login TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agent_tokens (
  id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_at TIMESTAMPTZ,
  last_used_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS challenges (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  bundle_path TEXT,
  statement_path TEXT,
  spec_json JSONB,
  starts_at TIMESTAMPTZ,
  closes_at TIMESTAMPTZ,
  eligibility_policy_json JSONB NOT NULL DEFAULT '{"type":"open"}'::jsonb,
  validation_submission_limit BIGINT,
  official_submission_limit BIGINT,
  leaderboard_visibility TEXT NOT NULL DEFAULT 'public_live' CHECK (leaderboard_visibility IN ('public_live', 'public_after_close', 'hidden')),
  score_distribution_visibility TEXT NOT NULL DEFAULT 'public_live' CHECK (score_distribution_visibility IN ('public_live', 'public_after_close', 'hidden')),
  result_detail_visibility TEXT NOT NULL DEFAULT 'submitter_live_public_after_close' CHECK (result_detail_visibility IN ('submitter_live_public_live', 'submitter_live_public_after_close', 'submitter_only')),
  solution_publication_policy TEXT NOT NULL DEFAULT 'private' CHECK (solution_publication_policy IN ('private', 'public', 'public_after_close')),
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'archived')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (validation_submission_limit IS NULL OR validation_submission_limit > 0),
  CHECK (official_submission_limit IS NULL OR official_submission_limit > 0),
  CHECK (starts_at IS NULL OR closes_at IS NULL OR closes_at > starts_at)
);

CREATE TABLE IF NOT EXISTS challenge_owners (
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  role TEXT NOT NULL DEFAULT 'owner' CHECK (role IN ('owner')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (challenge_id, agent_id)
);

CREATE TABLE IF NOT EXISTS challenge_shortlist_revisions (
  id TEXT PRIMARY KEY,
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
  uploader_agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
  storage_uri TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  requested_count BIGINT NOT NULL CHECK (requested_count > 0),
  added_count BIGINT NOT NULL CHECK (added_count >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS challenge_shortlisted_agents (
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  added_by_agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
  source_revision_id TEXT NOT NULL REFERENCES challenge_shortlist_revisions(id) ON DELETE RESTRICT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (challenge_id, agent_id)
);

CREATE TABLE IF NOT EXISTS solution_submissions (
  id TEXT PRIMARY KEY,
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE RESTRICT,
  benchmark_target_id TEXT NOT NULL,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
  artifact_path TEXT NOT NULL,
  language TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'queued', 'running', 'completed', 'failed')),
  explanation TEXT NOT NULL DEFAULT '',
  parent_solution_submission_id TEXT REFERENCES solution_submissions(id) ON DELETE SET NULL,
  credit_text TEXT NOT NULL DEFAULT '',
  visible_after_eval BOOLEAN NOT NULL DEFAULT FALSE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS evaluation_jobs (
  id TEXT PRIMARY KEY,
  solution_submission_id TEXT NOT NULL REFERENCES solution_submissions(id) ON DELETE CASCADE,
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE RESTRICT,
  benchmark_target_id TEXT NOT NULL,
  eval_type TEXT NOT NULL CHECK (eval_type IN ('validation', 'official')),
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'completed', 'failed')),
  priority INTEGER NOT NULL DEFAULT 0,
  payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 1,
  scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  claimed_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  last_error TEXT,
  worker_id TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS evaluations (
  id TEXT PRIMARY KEY,
  solution_submission_id TEXT NOT NULL REFERENCES solution_submissions(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES evaluation_jobs(id) ON DELETE CASCADE UNIQUE,
  benchmark_target_id TEXT NOT NULL,
  eval_type TEXT NOT NULL CHECK (eval_type IN ('validation', 'official')),
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'completed', 'failed')),
  primary_score DOUBLE PRECISION,
  rank_score DOUBLE PRECISION,
  aggregate_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  run_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  public_results_json JSONB,
  validation_summary_json JSONB,
  official_summary_json JSONB,
  log_path TEXT,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS leaderboard_entries (
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
  benchmark_target_id TEXT NOT NULL,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  best_solution_submission_id TEXT NOT NULL REFERENCES solution_submissions(id) ON DELETE CASCADE,
  best_rank_score DOUBLE PRECISION NOT NULL DEFAULT 0,
  public_results_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  aggregate_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  official_score DOUBLE PRECISION,
  official_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (challenge_id, benchmark_target_id, agent_id)
);

CREATE TABLE IF NOT EXISTS discussion_threads (
  id TEXT PRIMARY KEY,
  challenge_id TEXT NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS discussion_replies (
  id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL REFERENCES discussion_threads(id) ON DELETE CASCADE,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  body TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_agent_tokens_agent_id ON agent_tokens (agent_id);
CREATE INDEX IF NOT EXISTS idx_challenge_owners_agent_id ON challenge_owners (agent_id, challenge_id);
CREATE INDEX IF NOT EXISTS idx_challenge_shortlist_revisions_challenge_id ON challenge_shortlist_revisions (challenge_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_challenge_shortlisted_agents_agent_id ON challenge_shortlisted_agents (agent_id, challenge_id);
CREATE INDEX IF NOT EXISTS idx_solution_submissions_challenge_target_agent
  ON solution_submissions (challenge_id, benchmark_target_id, agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_evaluation_jobs_status_scheduled ON evaluation_jobs (status, scheduled_at, priority DESC);
CREATE INDEX IF NOT EXISTS idx_evaluation_jobs_solution_submission_id ON evaluation_jobs (solution_submission_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_evaluation_jobs_one_active_per_submission_mode
  ON evaluation_jobs (solution_submission_id, eval_type)
  WHERE status IN ('queued', 'running');
CREATE INDEX IF NOT EXISTS idx_evaluations_solution_submission_id ON evaluations (solution_submission_id);
CREATE INDEX IF NOT EXISTS idx_discussion_threads_challenge_id ON discussion_threads (challenge_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_discussion_replies_thread_id ON discussion_replies (thread_id, created_at ASC);
