CREATE TABLE IF NOT EXISTS challenge_review_records (
  id UUID PRIMARY KEY,
  challenge_name TEXT NOT NULL,
  request_kind TEXT NOT NULL CHECK (request_kind IN ('new_challenge', 'archive_challenge')),
  status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN ('pending_review', 'validated', 'approved', 'publishing', 'rejected', 'published', 'abandoned')),
  creator_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
  creator_github_user_id BIGINT NOT NULL,
  creator_github_login TEXT NOT NULL DEFAULT '',
  repo_url TEXT NOT NULL,
  repo_key TEXT NOT NULL,
  pr_number INTEGER NOT NULL CHECK (pr_number > 0),
  pr_url TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  challenge_path TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL,
  manifest_json JSONB NOT NULL,
  validation_message TEXT,
  validation_repository_path TEXT,
  validation_bundle_sha256 TEXT,
  approved_bundle_sha256 TEXT,
  publish_claim_id UUID,
  published_challenge_name TEXT REFERENCES challenges(challenge_name) ON DELETE SET NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (repo_key, pr_number, challenge_path)
);

CREATE TABLE IF NOT EXISTS challenge_private_assets (
  id UUID PRIMARY KEY,
  review_record_id UUID NOT NULL REFERENCES challenge_review_records(id) ON DELETE CASCADE,
  asset_name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('private_benchmark_data', 'private_evaluator_package', 'private_seeds', 'private_reference_outputs')),
  required BOOLEAN NOT NULL DEFAULT FALSE,
  size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
  sha256 TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  uploader_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('pending', 'active', 'failed', 'purging')),
  temporary_storage_key TEXT,
  activated_at TIMESTAMPTZ,
  failed_at TIMESTAMPTZ,
  failure_message TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS challenge_review_validation_records (
  id UUID PRIMARY KEY,
  review_record_id UUID NOT NULL REFERENCES challenge_review_records(id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK (status IN ('running', 'passed', 'failed')),
  message TEXT NOT NULL DEFAULT '',
  repository_path TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL,
  bundle_sha256 TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS challenge_review_audit_events (
  id UUID PRIMARY KEY,
  review_record_id UUID NOT NULL REFERENCES challenge_review_records(id) ON DELETE CASCADE,
  actor_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
  actor_admin_username TEXT,
  action TEXT NOT NULL,
  message TEXT NOT NULL DEFAULT '',
  metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE challenge_review_records
  ADD COLUMN IF NOT EXISTS active_validation_record_id UUID
    REFERENCES challenge_review_validation_records(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_challenge_review_records_status_updated_at
  ON challenge_review_records (status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_challenge_review_records_creator_agent_id
  ON challenge_review_records (creator_agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_challenge_private_assets_review_record_id
  ON challenge_private_assets (review_record_id);
CREATE INDEX IF NOT EXISTS idx_challenge_review_validation_records_review_record_id
  ON challenge_review_validation_records (review_record_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_challenge_review_records_one_active_validation
  ON challenge_review_records (id)
  WHERE active_validation_record_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_challenge_review_audit_events_review_record_id
  ON challenge_review_audit_events (review_record_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_challenge_private_assets_active_pending_name
  ON challenge_private_assets (review_record_id, asset_name)
  WHERE status IN ('pending', 'active');
CREATE INDEX IF NOT EXISTS idx_challenge_private_assets_pending_created_at
  ON challenge_private_assets (status, created_at)
  WHERE status = 'pending';
CREATE UNIQUE INDEX IF NOT EXISTS idx_challenge_review_records_publish_claim_id
  ON challenge_review_records (publish_claim_id)
  WHERE publish_claim_id IS NOT NULL;
