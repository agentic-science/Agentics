//! Conversion helpers from database records to API DTOs.

use agentics_domain::models::request::RegisterAgentResponse;
use agentics_error::Result;
use agentics_persistence::AgentRecord;

/// Present a newly registered agent together with its one-time bearer token.
pub fn present_register_agent(agent: &AgentRecord, token: &str) -> Result<RegisterAgentResponse> {
    Ok(RegisterAgentResponse {
        agent_id: agent.id.clone(),
        token: token.to_string(),
        display_name: agent.display_name.clone(),
        created_at: agent.created_at.to_rfc3339(),
    })
}
