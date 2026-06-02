use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    CreatePioneerCodeInput, PioneerCodeRecord, PioneerCodeUseRecord, RevokePioneerCodeOutcome,
};
use agentics_domain::models::ids::PioneerCodeId;
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct PioneerCodesRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl PioneerCodesRepository<'_> {
    pub async fn create(&self, input: &CreatePioneerCodeInput) -> Result<PioneerCodeRecord> {
        db::pioneer_codes::create_pioneer_code(self.pool, input).await
    }

    pub async fn list(&self) -> Result<Vec<PioneerCodeRecord>> {
        db::pioneer_codes::list_pioneer_codes(self.pool).await
    }

    pub async fn detail(
        &self,
        id: &PioneerCodeId,
    ) -> Result<(PioneerCodeRecord, Vec<PioneerCodeUseRecord>)> {
        db::pioneer_codes::get_pioneer_code_detail(self.pool, id).await
    }

    pub async fn revoke(&self, id: &PioneerCodeId) -> Result<RevokePioneerCodeOutcome> {
        db::pioneer_codes::revoke_pioneer_code(self.pool, id).await
    }
}
