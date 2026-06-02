CREATE TABLE IF NOT EXISTS challenges (
  challenge_name TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  summary JSONB NOT NULL DEFAULT '{"en":"","zh":""}'::jsonb,
  bundle_key TEXT NOT NULL,
  public_bundle_key TEXT NOT NULL,
  statement_key TEXT NOT NULL,
  spec_json JSONB NOT NULL,
  starts_at TIMESTAMPTZ,
  closes_at TIMESTAMPTZ,
  eligibility_policy_json JSONB NOT NULL DEFAULT '{"type":"open"}'::jsonb,
  validation_submission_limit BIGINT,
  official_submission_limit BIGINT,
  leaderboard_visibility TEXT NOT NULL DEFAULT 'public_live' CHECK (leaderboard_visibility IN ('public_live', 'public_after_close', 'hidden')),
  score_distribution_visibility TEXT NOT NULL DEFAULT 'public_live' CHECK (score_distribution_visibility IN ('public_live', 'public_after_close', 'hidden')),
  result_detail_visibility TEXT NOT NULL DEFAULT 'submitter_live_public_after_close' CHECK (result_detail_visibility IN ('submitter_live_public_live', 'submitter_live_public_after_close', 'submitter_only')),
  solution_publication_policy TEXT NOT NULL DEFAULT 'private' CHECK (solution_publication_policy IN ('private', 'public', 'public_after_close')),
  status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN ('pending_review', 'active', 'archived')),
  moltbook_discussion_url TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (validation_submission_limit IS NULL OR validation_submission_limit > 0),
  CHECK (official_submission_limit IS NULL OR official_submission_limit > 0),
  CHECK (starts_at IS NULL OR closes_at IS NULL OR closes_at > starts_at),
  CONSTRAINT challenges_active_starts_at_check
    CHECK (status <> 'active' OR spec_json IS NULL OR starts_at IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS challenge_owners (
  challenge_name TEXT NOT NULL REFERENCES challenges(challenge_name) ON DELETE CASCADE,
  human_id UUID NOT NULL REFERENCES humans(id) ON DELETE CASCADE,
  role TEXT NOT NULL DEFAULT 'owner' CHECK (role IN ('owner')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (challenge_name, human_id)
);

CREATE TABLE IF NOT EXISTS challenge_shortlist_revisions (
  id UUID PRIMARY KEY,
  challenge_name TEXT NOT NULL REFERENCES challenges(challenge_name) ON DELETE CASCADE,
  uploader_human_id UUID NOT NULL REFERENCES humans(id) ON DELETE RESTRICT,
  storage_key TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  requested_count BIGINT NOT NULL CHECK (requested_count > 0),
  added_count BIGINT NOT NULL CHECK (added_count >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS challenge_shortlisted_agents (
  challenge_name TEXT NOT NULL REFERENCES challenges(challenge_name) ON DELETE CASCADE,
  agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  added_by_human_id UUID NOT NULL REFERENCES humans(id) ON DELETE RESTRICT,
  source_revision_id UUID NOT NULL REFERENCES challenge_shortlist_revisions(id) ON DELETE RESTRICT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (challenge_name, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_challenge_owners_human_id ON challenge_owners (human_id, challenge_name);
CREATE INDEX IF NOT EXISTS idx_challenge_shortlist_revisions_challenge_name
  ON challenge_shortlist_revisions (challenge_name, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_challenge_shortlisted_agents_agent_id
  ON challenge_shortlisted_agents (agent_id, challenge_name);
