//! Agent submission and public projection probes for production rehearsal.

use std::time::{Duration, Instant};

use reqwest::{Client, Url};

use super::ProductionRehearsalError;
use super::http::{get_json, join_url};
use super::report::{CheckEvidence, RehearsalChallengeEvidence};

pub(super) async fn create_agent_submission(
    client: &Client,
    api_base_url: &Url,
    token: &str,
    path: &str,
    challenge: &RehearsalChallengeEvidence,
    artifact_base64: &str,
    explanation: &str,
) -> Result<String, ProductionRehearsalError> {
    let value = client
        .post(join_url(api_base_url, path)?)
        .bearer_auth(token)
        .json(&serde_json::json!({
            "challenge_name": challenge.name.as_str(),
            "target": challenge.target.as_str(),
            "artifact_base64": artifact_base64,
            "explanation": explanation,
            "credit_text": "Agentics production rehearsal"
        }))
        .send()
        .await
        .map_err(ProductionRehearsalError::HttpClient)?
        .error_for_status()
        .map_err(ProductionRehearsalError::HttpClient)?
        .json::<serde_json::Value>()
        .await
        .map_err(ProductionRehearsalError::HttpClient)?;
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ProductionRehearsalError::InvalidResponse("missing submission id".to_string())
        })
}

pub(super) async fn expect_submission_rejected(
    client: &Client,
    api_base_url: &Url,
    token: &str,
    challenge: &RehearsalChallengeEvidence,
    artifact_base64: &str,
    name: &str,
) -> CheckEvidence {
    let url = match join_url(api_base_url, "api/agent/validation-runs") {
        Ok(url) => url,
        Err(error) => return CheckEvidence::failed(name, error.to_string()),
    };
    match client
        .post(url)
        .bearer_auth(token)
        .json(&serde_json::json!({
            "challenge_name": challenge.name.as_str(),
            "target": challenge.target.as_str(),
            "artifact_base64": artifact_base64,
            "explanation": format!("adversarial rehearsal: {name}")
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_client_error() => {
            CheckEvidence::passed(name, format!("rejected with {}", response.status()))
        }
        Ok(response) => CheckEvidence::failed(
            name,
            format!("expected client error rejection, got {}", response.status()),
        ),
        Err(error) => CheckEvidence::failed(name, error.to_string()),
    }
}

pub(super) async fn wait_for_submission(
    client: &Client,
    api_base_url: &Url,
    token: &str,
    path: &str,
    name: &str,
    timeout: Duration,
) -> CheckEvidence {
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        return CheckEvidence::failed(name, "timeout is too large");
    };
    loop {
        let url = match join_url(api_base_url, path) {
            Ok(url) => url,
            Err(error) => return CheckEvidence::failed(name, error.to_string()),
        };
        match client.get(url).bearer_auth(token).send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(value) => {
                        let status = value
                            .get("status")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("<missing>");
                        match status {
                            "completed" => {
                                if let Some(primary_metric) =
                                    value.pointer("/official_primary_metric")
                                {
                                    return CheckEvidence::passed(
                                        name,
                                        format!("completed with primary metric {primary_metric:?}"),
                                    );
                                }
                                return CheckEvidence::passed(name, "completed");
                            }
                            "failed" => return CheckEvidence::failed(name, "status failed"),
                            _ => {}
                        }
                    }
                    Err(error) => return CheckEvidence::failed(name, error.to_string()),
                }
            }
            Ok(response) => {
                return CheckEvidence::failed(name, format!("poll returned {}", response.status()));
            }
            Err(error) => return CheckEvidence::failed(name, error.to_string()),
        }
        if Instant::now() >= deadline {
            return CheckEvidence::failed(name, format!("timed out after {}s", timeout.as_secs()));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub(super) async fn public_projection_check(
    client: &Client,
    api_base_url: &Url,
    challenge: &RehearsalChallengeEvidence,
    submission_id: &str,
) -> CheckEvidence {
    let detail = get_json(
        client,
        api_base_url,
        &format!("api/public/solution-submissions/{submission_id}"),
    )
    .await;
    let report = get_json(
        client,
        api_base_url,
        &format!("api/public/solution-submissions/{submission_id}/result-report"),
    )
    .await;
    let ranking = get_json(
        client,
        api_base_url,
        &format!(
            "api/public/solution-submissions/{submission_id}/ranking-context?challenge_name={}&target={}",
            challenge.name, challenge.target
        ),
    )
    .await;
    let list = get_json(
        client,
        api_base_url,
        &format!(
            "api/public/challenges/{}/solution-submissions?target={}&limit=10",
            challenge.name, challenge.target
        ),
    )
    .await;
    let leaderboard = get_json(
        client,
        api_base_url,
        &format!(
            "api/public/challenges/{}/leaderboard?target={}",
            challenge.name, challenge.target
        ),
    )
    .await;
    match (detail, report, ranking, list, leaderboard) {
        (Ok(detail), Ok(result_report), Ok(ranking), Ok(list), Ok(leaderboard)) => {
            let leaked_validation = detail.get("validation_evaluation").is_some();
            let ranked = leaderboard
                .get("items")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|items| !items.is_empty());
            let listed = list
                .get("items")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|items| {
                    items.iter().any(|item| {
                        item.get("id").and_then(serde_json::Value::as_str) == Some(submission_id)
                    })
                });
            let has_report = result_report.get("solution_submission").is_some();
            let has_ranking = ranking.get("rank").is_some() || ranking.get("entry").is_some();
            if leaked_validation {
                CheckEvidence::failed(
                    format!("{} public redaction", challenge.mode),
                    "public detail exposed validation_evaluation",
                )
            } else if !ranked {
                CheckEvidence::failed(
                    format!("{} leaderboard", challenge.mode),
                    "leaderboard has no ranked entries",
                )
            } else if !listed {
                CheckEvidence::failed(
                    format!("{} public list", challenge.mode),
                    "public submission list did not include the official submission",
                )
            } else if !has_report || !has_ranking {
                CheckEvidence::failed(
                    format!("{} public detail surfaces", challenge.mode),
                    "public report or ranking context had an unexpected shape",
                )
            } else {
                CheckEvidence::passed(
                    format!("{} public projection", challenge.mode),
                    "public detail/report/ranking/list/leaderboard surfaces are reachable and redacted",
                )
            }
        }
        (Err(error), _, _, _, _)
        | (_, Err(error), _, _, _)
        | (_, _, Err(error), _, _)
        | (_, _, _, Err(error), _)
        | (_, _, _, _, Err(error)) => CheckEvidence::failed(
            format!("{} public projection", challenge.mode),
            error.to_string(),
        ),
    }
}
