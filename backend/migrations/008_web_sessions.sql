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
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_web_sessions_expires_at ON web_sessions (expires_at);
CREATE INDEX IF NOT EXISTS idx_web_sessions_agent_id ON web_sessions (agent_id);
CREATE INDEX IF NOT EXISTS idx_github_oauth_states_expires_at ON github_oauth_states (expires_at);
