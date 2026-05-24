use secrecy::SecretString;
use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    AgentRecord, AuthenticatedAgent, PioneerCodeRegistrationKind, RegisterAgentInput,
};
use agentics_domain::error::Result;

#[derive(Debug, Clone, Copy)]
pub struct AgentsRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl AgentsRepository<'_> {
    pub async fn register_agent(
        &self,
        input: &RegisterAgentInput,
        max_active_agents: i64,
    ) -> Result<AgentRecord> {
        db::agents::register_agent(self.pool, input, max_active_agents).await
    }

    pub async fn register_agent_with_pioneer_code(
        &self,
        input: &RegisterAgentInput,
        code_hash: &str,
        max_active_agents: i64,
        kind: PioneerCodeRegistrationKind,
    ) -> Result<AgentRecord> {
        db::agents::register_agent_with_pioneer_code(
            self.pool,
            input,
            code_hash,
            kind,
            max_active_agents,
        )
        .await
    }

    pub async fn count_active(&self) -> Result<i64> {
        db::agents::count_active_agents(self.pool).await
    }

    pub async fn authenticate_token(
        &self,
        token: &SecretString,
    ) -> Result<Option<AuthenticatedAgent>> {
        db::agents::authenticate_agent_token(self.pool, token).await
    }

    pub async fn disable(&self, agent_id: &str) -> Result<()> {
        db::agents::disable_agent(self.pool, agent_id).await
    }
}
