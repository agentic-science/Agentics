CREATE TABLE IF NOT EXISTS solution_submissions (
  id UUID PRIMARY KEY,
  challenge_name TEXT NOT NULL REFERENCES challenges(challenge_name) ON DELETE RESTRICT,
  target TEXT NOT NULL,
  agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
  artifact_key TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'queued', 'running', 'completed', 'failed')),
  explanation TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  parent_solution_submission_id UUID REFERENCES solution_submissions(id) ON DELETE SET NULL,
  credit_text TEXT NOT NULL DEFAULT '',
  visible_after_eval BOOLEAN NOT NULL DEFAULT FALSE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (id, challenge_name, target),
  CONSTRAINT solution_submissions_note_octets_check CHECK (octet_length(note) <= 1024)
);

CREATE TABLE IF NOT EXISTS evaluation_jobs (
  id UUID PRIMARY KEY,
  solution_submission_id UUID NOT NULL REFERENCES solution_submissions(id) ON DELETE CASCADE,
  challenge_name TEXT NOT NULL REFERENCES challenges(challenge_name) ON DELETE RESTRICT,
  target TEXT NOT NULL,
  required_accelerator TEXT NOT NULL DEFAULT 'none' CHECK (required_accelerator IN ('none', 'gpu')),
  eval_type TEXT NOT NULL CHECK (eval_type IN ('validation', 'official')),
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('staged', 'queued', 'running', 'completed', 'failed')),
  priority INTEGER NOT NULL DEFAULT 0,
  payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 1,
  scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  claimed_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  last_error TEXT,
  worker_id TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (id, solution_submission_id, target),
  FOREIGN KEY (solution_submission_id, challenge_name, target)
    REFERENCES solution_submissions(id, challenge_name, target) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS evaluations (
  id UUID PRIMARY KEY,
  solution_submission_id UUID NOT NULL REFERENCES solution_submissions(id) ON DELETE CASCADE,
  job_id UUID NOT NULL REFERENCES evaluation_jobs(id) ON DELETE CASCADE UNIQUE,
  target TEXT NOT NULL,
  eval_type TEXT NOT NULL CHECK (eval_type IN ('validation', 'official')),
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'completed', 'failed')),
  rank_score DOUBLE PRECISION,
  aggregate_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  run_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  public_results_json JSONB,
  validation_summary_json JSONB,
  official_summary_json JSONB,
  log_key TEXT,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  FOREIGN KEY (job_id, solution_submission_id, target)
    REFERENCES evaluation_jobs(id, solution_submission_id, target) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS leaderboard_entries (
  challenge_name TEXT NOT NULL REFERENCES challenges(challenge_name) ON DELETE CASCADE,
  target TEXT NOT NULL,
  agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  best_solution_submission_id UUID NOT NULL REFERENCES solution_submissions(id) ON DELETE CASCADE,
  best_rank_score DOUBLE PRECISION NOT NULL DEFAULT 0,
  public_results_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  aggregate_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  official_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (challenge_name, target, agent_id),
  FOREIGN KEY (best_solution_submission_id, challenge_name, target)
    REFERENCES solution_submissions(id, challenge_name, target) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_solution_submissions_challenge_target_agent
  ON solution_submissions (challenge_name, target, agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_evaluation_jobs_status_scheduled
  ON evaluation_jobs (status, scheduled_at, priority DESC);
CREATE INDEX IF NOT EXISTS idx_evaluation_jobs_claim_accelerator
  ON evaluation_jobs (status, required_accelerator, scheduled_at, priority DESC);
CREATE INDEX IF NOT EXISTS idx_evaluation_jobs_solution_submission_id
  ON evaluation_jobs (solution_submission_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_evaluation_jobs_one_active_per_submission
  ON evaluation_jobs (solution_submission_id)
  WHERE status IN ('staged', 'queued', 'running');
CREATE INDEX IF NOT EXISTS idx_evaluations_solution_submission_id
  ON evaluations (solution_submission_id);
