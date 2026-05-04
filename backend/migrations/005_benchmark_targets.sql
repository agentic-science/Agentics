ALTER TABLE solution_submissions
  ADD COLUMN IF NOT EXISTS benchmark_target_id TEXT NOT NULL DEFAULT 'cpu-linux-arm64';

ALTER TABLE solution_submissions
  ALTER COLUMN benchmark_target_id DROP DEFAULT;

ALTER TABLE evaluation_jobs
  ADD COLUMN IF NOT EXISTS benchmark_target_id TEXT NOT NULL DEFAULT 'cpu-linux-arm64';

ALTER TABLE evaluation_jobs
  ALTER COLUMN benchmark_target_id DROP DEFAULT;

ALTER TABLE evaluations
  ADD COLUMN IF NOT EXISTS benchmark_target_id TEXT NOT NULL DEFAULT 'cpu-linux-arm64';

ALTER TABLE evaluations
  ALTER COLUMN benchmark_target_id DROP DEFAULT;

ALTER TABLE leaderboard_entries
  ADD COLUMN IF NOT EXISTS benchmark_target_id TEXT NOT NULL DEFAULT 'cpu-linux-arm64';

ALTER TABLE leaderboard_entries
  DROP CONSTRAINT IF EXISTS leaderboard_entries_pkey;

ALTER TABLE leaderboard_entries
  ADD PRIMARY KEY (challenge_id, benchmark_target_id, agent_id);

ALTER TABLE leaderboard_entries
  ALTER COLUMN benchmark_target_id DROP DEFAULT;

DROP INDEX IF EXISTS idx_solution_submissions_challenge_agent;
CREATE INDEX IF NOT EXISTS idx_solution_submissions_challenge_agent
  ON solution_submissions (challenge_id, benchmark_target_id, agent_id, created_at DESC);

DROP INDEX IF EXISTS idx_solution_submissions_challenge_version;
CREATE INDEX IF NOT EXISTS idx_solution_submissions_challenge_version
  ON solution_submissions (challenge_version_id, benchmark_target_id, created_at DESC);
