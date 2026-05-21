ALTER TABLE evaluation_jobs
  ADD COLUMN IF NOT EXISTS required_accelerator TEXT NOT NULL DEFAULT 'none';

DO $$
BEGIN
  ALTER TABLE evaluation_jobs
    ADD CONSTRAINT evaluation_jobs_required_accelerator_check
    CHECK (required_accelerator IN ('none', 'gpu'));
EXCEPTION
  WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_evaluation_jobs_claim_accelerator
  ON evaluation_jobs (status, required_accelerator, scheduled_at, priority DESC);
