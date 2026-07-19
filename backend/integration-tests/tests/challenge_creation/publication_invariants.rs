use super::*;

/// Verifies a formerly eligible creator cannot publish a new challenge after losing eligibility.
#[sqlx::test(migrations = "../migrations")]
async fn new_challenge_publication_rejects_ineligible_creator(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let admin_auth = admin_service_token_header(&app);

    for (offset, eligibility_change) in [
        (0_i64, "setup_required"),
        (1, "disabled"),
        (2, "deleted"),
        (3, "role_revoked"),
    ] {
        let github_user_id = 9301 + offset;
        let creator = create_creator_session(
            &pool,
            github_user_id,
            &format!("publication-owner-{offset}"),
        )
        .await;
        let review_record_id = create_validate_approve_record(
            &client,
            &app,
            &creator,
            &admin_auth,
            public_repo.path(),
            github_user_id,
            i32::try_from(80 + offset).expect("test PR number should fit"),
            &commit_sha,
            manifest_json(),
        )
        .await;

        match eligibility_change {
            "setup_required" => {
                sqlx::query("UPDATE humans SET status = 'setup_required' WHERE id = $1::uuid")
                    .bind(&creator.human_id)
                    .execute(&pool)
                    .await
                    .expect("creator should become setup-required");
            }
            "disabled" => {
                sqlx::query(
                    "UPDATE humans SET status = 'disabled', disabled_at = NOW() WHERE id = $1::uuid",
                )
                .bind(&creator.human_id)
                .execute(&pool)
                .await
                .expect("creator should become disabled");
            }
            "deleted" => {
                sqlx::query(
                    "UPDATE humans SET status = 'disabled', disabled_at = NOW(), deleted_at = NOW() WHERE id = $1::uuid",
                )
                .bind(&creator.human_id)
                .execute(&pool)
                .await
                .expect("creator should become deleted");
            }
            "role_revoked" => {
                sqlx::query(
                    "UPDATE human_roles SET revoked_at = NOW() WHERE human_id = $1::uuid AND role = 'creator' AND revoked_at IS NULL",
                )
                .bind(&creator.human_id)
                .execute(&pool)
                .await
                .expect("creator role should be revoked");
            }
            unexpected => panic!("unexpected eligibility test case {unexpected}"),
        }

        let publish = client
            .post(api_url(
                &app,
                &format!("/admin/challenge-review-records/{review_record_id}/publish"),
            ))
            .header("Authorization", &admin_auth)
            .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
            .send()
            .await
            .expect("publish request should complete");
        assert_eq!(publish.status(), StatusCode::CONFLICT);

        let (status, claim_id): (String, Option<uuid::Uuid>) = sqlx::query_as(
            "SELECT status, publish_claim_id FROM challenge_review_records WHERE id = $1::uuid",
        )
        .bind(&review_record_id)
        .fetch_one(&pool)
        .await
        .expect("review record should load");
        assert_eq!(status, "approved");
        assert!(claim_id.is_none());
    }

    let challenge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenges WHERE challenge_name = 'sample-sum'")
            .fetch_one(&pool)
            .await
            .expect("challenge count should load");
    assert_eq!(challenge_count, 0);
}

/// Verifies archive publication also requires the owner's current creator eligibility.
#[sqlx::test(migrations = "../migrations")]
async fn archive_publication_rejects_ineligible_owner(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let admin_auth = admin_service_token_header(&app);
    let owner = create_creator_session(&pool, 1001, "archive-ineligible-owner").await;
    let publish_flow = ReviewRecordPublishFlow {
        client: &client,
        app: &app,
        creator: &owner,
        admin_auth: &admin_auth,
        public_repo: public_repo.path(),
    };
    create_validate_approve_publish_review_record(&publish_flow, &commit_sha, 90, manifest_json())
        .await;

    write_archive_manifest(public_repo.path());
    let archive_commit_sha = commit_all(public_repo.path(), "archive eligibility test");
    let archive_review_record_id = create_validate_approve_record(
        &client,
        &app,
        &owner,
        &admin_auth,
        public_repo.path(),
        1001,
        91,
        &archive_commit_sha,
        archive_manifest_json(),
    )
    .await;
    sqlx::query(
        "UPDATE human_roles SET revoked_at = NOW() WHERE human_id = $1::uuid AND role = 'creator' AND revoked_at IS NULL",
    )
    .bind(&owner.human_id)
    .execute(&pool)
    .await
    .expect("creator role should be revoked");

    let publish = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-review-records/{archive_review_record_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("archive publish request should complete");
    assert_eq!(publish.status(), StatusCode::CONFLICT);

    let challenge_status: String =
        sqlx::query_scalar("SELECT status FROM challenges WHERE challenge_name = 'sample-sum'")
            .fetch_one(&pool)
            .await
            .expect("published challenge should load");
    assert_eq!(challenge_status, "active");
}

/// Create, validate, and approve a review record without publishing it.
#[allow(
    clippy::too_many_arguments,
    reason = "test helper mirrors the review workflow inputs"
)]
async fn create_validate_approve_record(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &helpers::TestCreatorSession,
    admin_auth: &str,
    repository_path: &std::path::Path,
    github_user_id: i64,
    pr_number: i32,
    commit_sha: &str,
    manifest: serde_json::Value,
) -> String {
    let requires_private_asset = manifest["request"] != "archive_challenge";
    let review_record = create_review_record_with_author_and_commit(
        client,
        app,
        creator,
        pr_number,
        manifest,
        github_user_id,
        commit_sha,
    )
    .await;
    let review_record_id = review_record["id"]
        .as_str()
        .expect("review record id")
        .to_string();
    if requires_private_asset {
        creator_auth(
            client.post(api_url(
                app,
                &format!("/api/creator/challenge-review-records/{review_record_id}/private-assets"),
            )),
            creator,
        )
        .json(&json!({
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "required": false,
            "asset_base64": private_benchmark_asset_zip_base64()
        }))
        .send()
        .await
        .expect("private asset request should complete")
        .error_for_status()
        .expect("private asset should upload");
    }
    let validated: serde_json::Value = client
        .post(api_url(
            app,
            &format!("/admin/challenge-review-records/{review_record_id}/validate"),
        ))
        .header("Authorization", admin_auth)
        .json(&json!({ "repository_path": repository_path.to_string_lossy() }))
        .send()
        .await
        .expect("validation request should complete")
        .error_for_status()
        .expect("review record should validate")
        .json()
        .await
        .expect("validated review record should decode");
    client
        .post(api_url(
            app,
            &format!("/admin/challenge-review-records/{review_record_id}/approve"),
        ))
        .header("Authorization", admin_auth)
        .json(&json!({
            "message": "approved for eligibility test",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
        .send()
        .await
        .expect("approval request should complete")
        .error_for_status()
        .expect("review record should approve");
    review_record_id
}
