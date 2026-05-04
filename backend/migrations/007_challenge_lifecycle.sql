ALTER TABLE challenge_versions
  DROP CONSTRAINT IF EXISTS challenge_versions_status_check;

ALTER TABLE challenge_versions
  ADD CONSTRAINT challenge_versions_status_check
  CHECK (status IN ('draft', 'published', 'superseded', 'archived'));
