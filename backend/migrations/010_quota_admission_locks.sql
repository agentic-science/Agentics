CREATE TABLE IF NOT EXISTS quota_admission_locks (
  scope TEXT PRIMARY KEY,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
