ALTER TABLE solution_submissions
  ADD COLUMN IF NOT EXISTS note TEXT NOT NULL DEFAULT '';

ALTER TABLE solution_submissions
  DROP COLUMN IF EXISTS language;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'solution_submissions_note_octets_check'
      AND conrelid = 'solution_submissions'::regclass
  ) THEN
    ALTER TABLE solution_submissions
      ADD CONSTRAINT solution_submissions_note_octets_check
      CHECK (octet_length(note) <= 1024);
  END IF;
END $$;
