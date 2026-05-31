DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'evaluations'
      AND column_name = 'log_key'
  ) AND NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'evaluations'
      AND column_name = 'runner_log_storage_key'
  ) THEN
    ALTER TABLE evaluations RENAME COLUMN log_key TO runner_log_storage_key;
  END IF;
END $$;
