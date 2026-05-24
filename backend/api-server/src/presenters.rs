//! Conversion helpers from database records to API DTOs.

use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::pioneer_codes::{PioneerCodeStatus, PioneerCodeUseKind};
use agentics_domain::models::request::{
    PioneerCodeDetailResponse, PioneerCodeDto, PioneerCodeListResponse, PioneerCodeUseDto,
    RegisterAgentResponse,
};
use agentics_persistence::{AgentRecord, PioneerCodeRecord, PioneerCodeUseRecord};

/// Present a newly registered agent together with its one-time bearer token.
pub fn present_register_agent(agent: &AgentRecord, token: &str) -> Result<RegisterAgentResponse> {
    Ok(RegisterAgentResponse {
        agent_id: agent.id.clone(),
        token: token.to_string(),
        display_name: agent.display_name.clone(),
        created_at: agent.created_at.to_rfc3339(),
    })
}

/// Present a pioneer-code list for admin review.
pub fn present_pioneer_code_list(codes: &[PioneerCodeRecord]) -> Result<PioneerCodeListResponse> {
    Ok(PioneerCodeListResponse {
        items: codes
            .iter()
            .map(present_pioneer_code)
            .collect::<Result<Vec<_>>>()?,
    })
}

/// Present one pioneer-code detail response with its created agents.
pub fn present_pioneer_code_detail(
    code: &PioneerCodeRecord,
    uses: &[PioneerCodeUseRecord],
) -> Result<PioneerCodeDetailResponse> {
    Ok(PioneerCodeDetailResponse {
        code: present_pioneer_code(code)?,
        uses: uses
            .iter()
            .map(present_pioneer_code_use)
            .collect::<Result<Vec<_>>>()?,
    })
}

/// Present a pioneer-code row without exposing the hashed validation value.
fn present_pioneer_code(code: &PioneerCodeRecord) -> Result<PioneerCodeDto> {
    Ok(PioneerCodeDto {
        id: code.id.clone(),
        code_display: code.code_display.clone(),
        label: code.label.clone(),
        note: code.note.clone(),
        max_uses: code.max_uses,
        use_count: code.use_count,
        status: PioneerCodeStatus::from_storage_value(&code.status).ok_or_else(|| {
            ServiceError::Internal(format!(
                "stored invalid pioneer-code status `{}`",
                code.status
            ))
        })?,
        expires_at: code.expires_at.map(|expires_at| expires_at.to_rfc3339()),
        created_by_admin_username: code.created_by_admin_username.clone(),
        created_at: code.created_at.to_rfc3339(),
        revoked_at: code.revoked_at.map(|revoked_at| revoked_at.to_rfc3339()),
    })
}

/// Present an agent account created through a pioneer code.
fn present_pioneer_code_use(use_record: &PioneerCodeUseRecord) -> Result<PioneerCodeUseDto> {
    Ok(PioneerCodeUseDto {
        agent_id: use_record.agent_id.clone(),
        agent_display_name: use_record.agent_display_name.clone(),
        registration_kind: PioneerCodeUseKind::from_storage_value(&use_record.registration_kind)
            .ok_or_else(|| {
                ServiceError::Internal(format!(
                    "stored invalid pioneer-code registration kind `{}`",
                    use_record.registration_kind
                ))
            })?,
        used_at: use_record.used_at.to_rfc3339(),
    })
}
