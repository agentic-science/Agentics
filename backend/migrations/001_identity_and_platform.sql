CREATE TABLE IF NOT EXISTS service_heartbeats (
  service_name TEXT PRIMARY KEY,
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS agents (
  id UUID PRIMARY KEY,
  display_name TEXT NOT NULL,
  agent_description TEXT NOT NULL DEFAULT '',
  model_info JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
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

CREATE TABLE IF NOT EXISTS humans (
  id UUID PRIMARY KEY,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  disabled_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS human_external_identities (
  human_id UUID NOT NULL REFERENCES humans(id) ON DELETE CASCADE,
  provider TEXT NOT NULL CHECK (provider IN ('github')),
  provider_user_id BIGINT NOT NULL,
  provider_login TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (human_id, provider),
  UNIQUE (provider, provider_user_id)
);

CREATE TABLE IF NOT EXISTS human_roles (
  id UUID PRIMARY KEY,
  human_id UUID NOT NULL REFERENCES humans(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK (role IN ('creator', 'admin')),
  granted_by_human_id UUID REFERENCES humans(id) ON DELETE SET NULL,
  granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_by_human_id UUID REFERENCES humans(id) ON DELETE SET NULL,
  revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS human_sessions (
  id UUID PRIMARY KEY,
  session_token_hash TEXT NOT NULL UNIQUE,
  csrf_token_hash TEXT NOT NULL,
  human_id UUID NOT NULL REFERENCES humans(id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS admin_service_tokens (
  id UUID PRIMARY KEY,
  token_hash TEXT NOT NULL UNIQUE,
  label TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
  created_by_human_id UUID NOT NULL REFERENCES humans(id) ON DELETE RESTRICT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ,
  revoked_by_human_id UUID REFERENCES humans(id) ON DELETE SET NULL,
  revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS github_sign_in_states (
  state_hash TEXT PRIMARY KEY,
  browser_nonce_hash TEXT NOT NULL,
  pioneer_code_hash TEXT,
  return_to TEXT,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS pioneer_codes (
  id UUID PRIMARY KEY,
  code_display TEXT NOT NULL UNIQUE,
  code_hash TEXT NOT NULL UNIQUE,
  label TEXT,
  note TEXT NOT NULL DEFAULT '',
  max_uses BIGINT NOT NULL CHECK (max_uses = -1 OR max_uses > 0),
  use_count BIGINT NOT NULL DEFAULT 0 CHECK (use_count >= 0),
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
  expires_at TIMESTAMPTZ,
  created_by_human_id UUID REFERENCES humans(id) ON DELETE SET NULL,
  created_by_admin_service_token_id UUID REFERENCES admin_service_tokens(id) ON DELETE SET NULL,
  created_by_display TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_at TIMESTAMPTZ,
  CHECK (
    (created_by_human_id IS NOT NULL AND created_by_admin_service_token_id IS NULL)
    OR (created_by_human_id IS NULL AND created_by_admin_service_token_id IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS pioneer_code_uses (
  id UUID PRIMARY KEY,
  pioneer_code_id UUID NOT NULL REFERENCES pioneer_codes(id) ON DELETE RESTRICT,
  subject_kind TEXT NOT NULL CHECK (subject_kind IN ('human', 'agent')),
  human_id UUID REFERENCES humans(id) ON DELETE CASCADE,
  agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
  registration_kind TEXT NOT NULL CHECK (registration_kind IN ('human_github_sign_in', 'agent_api')),
  used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (
    (subject_kind = 'human' AND human_id IS NOT NULL AND agent_id IS NULL)
    OR (subject_kind = 'agent' AND agent_id IS NOT NULL AND human_id IS NULL)
  )
);

CREATE TABLE IF NOT EXISTS quota_admission_locks (
  scope TEXT PRIMARY KEY,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_agent_tokens_agent_id ON agent_tokens (agent_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_human_roles_one_active_role
  ON human_roles (human_id, role)
  WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_human_external_identities_provider_user
  ON human_external_identities (provider, provider_user_id);
CREATE INDEX IF NOT EXISTS idx_human_sessions_expires_at ON human_sessions (expires_at);
CREATE INDEX IF NOT EXISTS idx_human_sessions_human_id ON human_sessions (human_id);
CREATE INDEX IF NOT EXISTS idx_admin_service_tokens_status_created
  ON admin_service_tokens (status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_github_sign_in_states_expires_at ON github_sign_in_states (expires_at);
CREATE INDEX IF NOT EXISTS idx_pioneer_codes_status_created
  ON pioneer_codes (status, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_pioneer_code_uses_human_once
  ON pioneer_code_uses (pioneer_code_id, human_id)
  WHERE human_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_pioneer_code_uses_agent_once
  ON pioneer_code_uses (pioneer_code_id, agent_id)
  WHERE agent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pioneer_code_uses_human_id
  ON pioneer_code_uses (human_id)
  WHERE human_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pioneer_code_uses_agent_id
  ON pioneer_code_uses (agent_id)
  WHERE agent_id IS NOT NULL;
