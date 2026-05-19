//! Cross-field GitHub provenance validation for challenge drafts.

use crate::error::{AppError, Result};
use crate::models::github::GithubPullRequestNumber;
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};

/// Validated GitHub pull-request provenance tuple.
#[derive(Debug, Clone)]
pub struct GithubPullRequestRef {
    repo_url: GithubRepoRemote,
    pr_url: GithubPullRequestUrl,
    pr_number: GithubPullRequestNumber,
}

impl GithubPullRequestRef {
    /// Validate that repository URL, PR URL, and PR number describe the same PR.
    pub fn try_new(
        repo_url: GithubRepoRemote,
        pr_url: GithubPullRequestUrl,
        pr_number: GithubPullRequestNumber,
    ) -> Result<Self> {
        let pr_repo_key = pr_url
            .repository_key()
            .map_err(|e| AppError::Validation(e.to_string()))?;
        if repo_url.repository_key() != &pr_repo_key {
            return Err(AppError::Validation(format!(
                "pr_url repository `{pr_repo_key}` must match repo_url repository `{}`",
                repo_url.repository_key()
            )));
        }
        let pr_url_number = pr_url
            .number()
            .map_err(|e| AppError::Validation(e.to_string()))?;
        if pr_number.as_str() != pr_url_number {
            return Err(AppError::Validation(format!(
                "pr_url pull request number `{pr_url_number}` must match pr_number `{pr_number}`"
            )));
        }

        Ok(Self {
            repo_url,
            pr_url,
            pr_number,
        })
    }

    /// Borrow the repository remote.
    pub fn repo_url(&self) -> &GithubRepoRemote {
        &self.repo_url
    }

    /// Borrow the pull request URL.
    pub fn pr_url(&self) -> &GithubPullRequestUrl {
        &self.pr_url
    }

    /// Borrow the pull request number.
    pub fn pr_number(&self) -> &GithubPullRequestNumber {
        &self.pr_number
    }
}

#[cfg(test)]
mod tests {
    use crate::models::github::GithubPullRequestNumber;
    use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};

    use super::GithubPullRequestRef;

    #[test]
    fn validates_matching_pull_request_reference() {
        let reference = GithubPullRequestRef::try_new(
            GithubRepoRemote::try_new("https://github.com/Agentics-Reifying/Agentics-Challenges")
                .expect("repo"),
            GithubPullRequestUrl::try_new(
                "https://github.com/agentics-reifying/agentics-challenges/pull/42",
            )
            .expect("pr"),
            GithubPullRequestNumber::try_new("42".to_string()).expect("number"),
        )
        .expect("reference should validate");

        assert_eq!(
            reference.repo_url().repository_key().as_str(),
            "agentics-reifying/agentics-challenges"
        );
        assert_eq!(reference.pr_number().as_str(), "42");
    }

    #[test]
    fn rejects_cross_field_mismatch() {
        assert!(
            GithubPullRequestRef::try_new(
                GithubRepoRemote::try_new("https://github.com/agentics-reifying/agentics")
                    .expect("repo"),
                GithubPullRequestUrl::try_new(
                    "https://github.com/agentics-reifying/agentics-challenges/pull/42",
                )
                .expect("pr"),
                GithubPullRequestNumber::try_new("42".to_string()).expect("number"),
            )
            .is_err()
        );

        assert!(
            GithubPullRequestRef::try_new(
                GithubRepoRemote::try_new(
                    "git@github.com:agentics-reifying/agentics-challenges.git",
                )
                .expect("repo"),
                GithubPullRequestUrl::try_new(
                    "https://github.com/agentics-reifying/agentics-challenges/pull/43",
                )
                .expect("pr"),
                GithubPullRequestNumber::try_new("42".to_string()).expect("number"),
            )
            .is_err()
        );
    }
}
