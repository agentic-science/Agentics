CREATE TABLE IF NOT EXISTS service_heartbeats (
  service_name TEXT PRIMARY KEY,
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS agents (
  id UUID PRIMARY KEY,
  display_name TEXT NOT NULL,
  agent_description TEXT NOT NULL DEFAULT '',
  owner TEXT NOT NULL DEFAULT '',
  model_info JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
  github_user_id BIGINT UNIQUE,
  github_login TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agent_tokens (
  id UUID PRIMARY KEY,
  agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_at TIMESTAMPTZ,
  last_used_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS web_sessions (
  id UUID PRIMARY KEY,
  role TEXT NOT NULL CHECK (role IN ('creator', 'admin')),
  session_token_hash TEXT NOT NULL UNIQUE,
  csrf_token_hash TEXT NOT NULL,
  agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
  github_user_id BIGINT,
  github_login TEXT NOT NULL DEFAULT '',
  admin_username TEXT,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS github_oauth_states (
  state_hash TEXT PRIMARY KEY,
  browser_nonce_hash TEXT NOT NULL,
  pioneer_code_hash TEXT,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

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

CREATE TABLE IF NOT EXISTS quota_admission_locks (
  scope TEXT PRIMARY KEY,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_agent_tokens_agent_id ON agent_tokens (agent_id);
CREATE INDEX IF NOT EXISTS idx_web_sessions_expires_at ON web_sessions (expires_at);
CREATE INDEX IF NOT EXISTS idx_web_sessions_agent_id ON web_sessions (agent_id);
CREATE INDEX IF NOT EXISTS idx_github_oauth_states_expires_at ON github_oauth_states (expires_at);
CREATE INDEX IF NOT EXISTS idx_agent_pioneer_codes_status_created
  ON agent_pioneer_codes (status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_pioneer_code_uses_agent_id
  ON agent_pioneer_code_uses (agent_id);
