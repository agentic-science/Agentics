ALTER TABLE humans
  ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

ALTER TABLE humans
  DROP CONSTRAINT IF EXISTS humans_status_check;

ALTER TABLE humans
  ADD CONSTRAINT humans_status_check
  CHECK (status IN ('active', 'setup_required', 'disabled', 'deleted'));
