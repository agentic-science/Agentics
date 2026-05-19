ALTER TABLE challenge_drafts
  ADD COLUMN IF NOT EXISTS publish_claim_id UUID;

CREATE UNIQUE INDEX IF NOT EXISTS idx_challenge_drafts_publish_claim_id
  ON challenge_drafts (publish_claim_id)
  WHERE publish_claim_id IS NOT NULL;
