ALTER TABLE solution_submissions
  ADD COLUMN IF NOT EXISTS artifact_zip_bytes BIGINT,
  ADD COLUMN IF NOT EXISTS artifact_uncompressed_bytes BIGINT,
  ADD COLUMN IF NOT EXISTS artifact_file_count BIGINT,
  ADD COLUMN IF NOT EXISTS artifact_sha256 TEXT;

ALTER TABLE solution_submissions
  DROP CONSTRAINT IF EXISTS solution_submissions_artifact_metadata_complete_check;

ALTER TABLE solution_submissions
  ADD CONSTRAINT solution_submissions_artifact_metadata_complete_check CHECK (
    (
      artifact_zip_bytes IS NULL
      AND artifact_uncompressed_bytes IS NULL
      AND artifact_file_count IS NULL
      AND artifact_sha256 IS NULL
    )
    OR (
      artifact_zip_bytes IS NOT NULL
      AND artifact_uncompressed_bytes IS NOT NULL
      AND artifact_file_count IS NOT NULL
      AND artifact_sha256 IS NOT NULL
      AND artifact_zip_bytes >= 0
      AND artifact_uncompressed_bytes >= 0
      AND artifact_file_count >= 0
      AND artifact_sha256 ~ '^[0-9a-f]{64}$'
    )
  );
