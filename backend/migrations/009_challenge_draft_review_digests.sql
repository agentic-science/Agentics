ALTER TABLE challenge_drafts
  ADD COLUMN IF NOT EXISTS validation_bundle_sha256 TEXT,
  ADD COLUMN IF NOT EXISTS approved_bundle_sha256 TEXT;

ALTER TABLE challenge_draft_validation_records
  ADD COLUMN IF NOT EXISTS bundle_sha256 TEXT;
