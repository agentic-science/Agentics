ALTER TABLE evaluations
  ADD COLUMN IF NOT EXISTS rank_score DOUBLE PRECISION,
  ADD COLUMN IF NOT EXISTS aggregate_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  ADD COLUMN IF NOT EXISTS run_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE leaderboard_entries
  ADD COLUMN IF NOT EXISTS aggregate_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  ADD COLUMN IF NOT EXISTS official_metrics_json JSONB NOT NULL DEFAULT '[]'::jsonb;
