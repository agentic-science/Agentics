ALTER TABLE challenge_drafts
  DROP CONSTRAINT IF EXISTS challenge_drafts_status_check;

ALTER TABLE challenge_drafts
  ADD CONSTRAINT challenge_drafts_status_check
  CHECK (status IN ('draft', 'validated', 'approved', 'publishing', 'rejected', 'published', 'abandoned'));

ALTER TABLE challenge_private_assets
  ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active',
  ADD COLUMN IF NOT EXISTS temporary_storage_key TEXT,
  ADD COLUMN IF NOT EXISTS activated_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS failed_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS failure_message TEXT;

UPDATE challenge_private_assets
SET status = 'active',
    activated_at = COALESCE(activated_at, created_at)
WHERE status = 'active';

ALTER TABLE challenge_private_assets
  DROP CONSTRAINT IF EXISTS challenge_private_assets_status_check;

ALTER TABLE challenge_private_assets
  ADD CONSTRAINT challenge_private_assets_status_check
  CHECK (status IN ('pending', 'active', 'failed', 'purging'));

ALTER TABLE challenge_private_assets
  DROP CONSTRAINT IF EXISTS challenge_private_assets_draft_id_asset_name_key;

CREATE UNIQUE INDEX IF NOT EXISTS idx_challenge_private_assets_active_pending_name
  ON challenge_private_assets (draft_id, asset_name)
  WHERE status IN ('pending', 'active');

CREATE INDEX IF NOT EXISTS idx_challenge_private_assets_pending_created_at
  ON challenge_private_assets (status, created_at)
  WHERE status = 'pending';
