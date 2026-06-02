//! Challenge shell and published challenge queries.

mod admin;
mod catalog;
mod creator_insights;
mod helpers;
mod owners;
mod publishing;
mod records;
mod shortlists;

pub use admin::{
    clear_challenge_moltbook_discussion, list_admin_challenges, set_challenge_moltbook_discussion,
};
pub use catalog::{
    get_public_challenge, get_published_challenge, get_published_challenge_by_name,
    list_published_challenges,
};
pub use creator_insights::{get_creator_challenge_stats, list_creator_challenge_participants};
pub(super) use helpers::localized_text_from_row;
pub use owners::{add_challenge_owner, add_challenge_owner_tx, human_owns_challenge};
pub use publishing::{
    archive_challenge, publish_challenge, publish_challenge_tx, refresh_seeded_challenge,
};
pub use records::{
    AdminChallengeListItemRecord, ChallengeCatalogFilters, ChallengeMoltbookDiscussionRecord,
    ChallengeRecord, ChallengeShortlistRecord, ChallengeShortlistRevisionRecord,
    ChallengeShortlistedAgentRecord, CreateChallengeShortlistRevisionInput,
    CreatorChallengeParticipantRecord, CreatorChallengeParticipantsRecord,
    CreatorChallengeStatsRecord, PublishChallengeInput, PublishChallengeRecord,
    PublishedChallengeList, PublishedChallengeListItemRecord,
};
pub use shortlists::{
    agent_is_shortlisted, challenge_has_shortlist, create_challenge_shortlist_revision,
    list_challenge_shortlist,
};
