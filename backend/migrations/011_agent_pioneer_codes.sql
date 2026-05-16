CREATE TABLE IF NOT EXISTS agent_pioneer_codes (
  id UUID PRIMARY KEY,
  code_display TEXT NOT NULL UNIQUE,
  code_hash TEXT NOT NULL UNIQUE,
  label TEXT,
  note TEXT NOT NULL DEFAULT '',
  max_uses BIGINT NOT NULL CHECK (max_uses = -1 OR max_uses > 0),
  use_count BIGINT NOT NULL DEFAULT 0 CHECK (use_count >= 0),
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
  expires_at TIMESTAMPTZ,
  created_by_admin_username TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS agent_pioneer_code_uses (
  pioneer_code_id UUID NOT NULL REFERENCES agent_pioneer_codes(id) ON DELETE RESTRICT,
  agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  registration_kind TEXT NOT NULL CHECK (registration_kind IN ('agent_api', 'creator_oauth')),
  used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (pioneer_code_id, agent_id)
);

ALTER TABLE github_oauth_states
  ADD COLUMN IF NOT EXISTS pioneer_code_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_agent_pioneer_codes_status_created
  ON agent_pioneer_codes (status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_pioneer_code_uses_agent_id
  ON agent_pioneer_code_uses (agent_id);
