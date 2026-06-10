ALTER TABLE leaderboard_entries
  DROP COLUMN IF EXISTS best_rank_score;

ALTER TABLE evaluations
  DROP COLUMN IF EXISTS rank_score;
