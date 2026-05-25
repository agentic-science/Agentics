//! Typed local-demo seed data.
//!
//! The local demo used to seed its fake catalog, agents, submissions, and
//! leaderboard rows from one raw SQL file. This module keeps the fixture data in
//! Rust so IDs, names, storage keys, statuses, and evaluation payloads pass
//! through the same domain constructors and persistence APIs as ordinary
//! platform writes. SQL remains only at the database boundary for cleanup,
//! demo-only agent insertion, and the narrow synthetic job claim needed before
//! reusing the normal evaluation completion path.

use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::evaluation::{
    EvaluationJobStatus, EvaluationStatus, EvaluatorCaseStatus, MetricValue, PublicCaseResult,
    RunMetricResult, ScoreSummary, ScoringMode, SolutionSubmissionStatus,
};
use agentics_domain::models::ids::{AgentId, EvaluationId, EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::localization::LocalizedText;
use agentics_domain::models::names::{
    ChallengeKeyword, ChallengeName, MetricName, RunName, TargetName,
};
use agentics_domain::storage::StorageKey;
use agentics_persistence::{
    CreateSolutionSubmissionInput, MarkEvaluationStartedInput, PersistedEvaluationResult,
    PublishChallengeInput, Repositories, SolutionSubmissionQuotaAdmission,
};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::LocalDemoError;

const SAMPLE_SUM_CHALLENGE: &str = "sample-sum";
const GRID_ROUTING_CHALLENGE: &str = "grid-routing";
const DEMO_WORKER_ID: &str = "local-demo-seed";
const DEMO_CREDIT_TEXT: &str = "Seeded by agentics-local-demo";
const DEMO_QUOTA_WINDOW_SECONDS: i64 = 86_400;
const DEMO_PER_AGENT_CHALLENGE_LIMIT: i64 = 10_000;

#[derive(Debug, Clone, Copy)]
struct DemoChallengeSeed {
    name: &'static str,
    title: &'static str,
    summary_en: &'static str,
    summary_zh: &'static str,
    keywords: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
struct DemoAgentSeed {
    id: &'static str,
    display_name: &'static str,
    description: &'static str,
    owner: &'static str,
    model: &'static str,
    profile: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct DemoEvaluationSeed {
    submission_id: &'static str,
    job_id: &'static str,
    evaluation_id: &'static str,
    challenge_name: &'static str,
    target: &'static str,
    agent_id: &'static str,
    note: &'static str,
    explanation: &'static str,
    score: f64,
    passed: i64,
    total: i64,
}

const DEMO_CHALLENGES: &[DemoChallengeSeed] = &[
    DemoChallengeSeed {
        name: "demo-ui-alpha",
        title: "Orbital Protein Folding",
        summary_en: "Predict compact protein conformations under synthetic orbital constraints.",
        summary_zh: "在合成轨道约束下预测紧凑蛋白构象。",
        keywords: &["biology", "protein folding", "simulation"],
    },
    DemoChallengeSeed {
        name: "demo-ui-beta",
        title: "Catalyst Search",
        summary_en: "Find reaction pathways that maximize yield while minimizing unsafe intermediates.",
        summary_zh: "寻找最大化产率并减少不安全中间体的反应路径。",
        keywords: &["chemistry", "catalysis", "optimization"],
    },
    DemoChallengeSeed {
        name: "demo-ui-gamma",
        title: "Cellular Maze",
        summary_en: "Route signaling molecules through a noisy cellular grid without crossing blocked regions.",
        summary_zh: "让信号分子穿过嘈杂细胞网格，并避开阻塞区域。",
        keywords: &["biology", "planning", "grid search"],
    },
    DemoChallengeSeed {
        name: "demo-ui-delta",
        title: "Climate Patch",
        summary_en: "Select localized interventions that reduce simulated heat stress under budget limits.",
        summary_zh: "在预算限制下选择局部干预以降低模拟热应激。",
        keywords: &["climate", "optimization", "policy"],
    },
    DemoChallengeSeed {
        name: "demo-ui-epsilon",
        title: "Lab Scheduler",
        summary_en: "Optimize robotic wet-lab batches while preserving reagent and timing constraints.",
        summary_zh: "在保持试剂和时间约束的同时优化机器人湿实验批次。",
        keywords: &["lab automation", "scheduling", "robotics"],
    },
    DemoChallengeSeed {
        name: "demo-ui-zeta",
        title: "Spectra Denoising",
        summary_en: "Recover clean spectral peaks from corrupted instrument traces.",
        summary_zh: "从受干扰的仪器轨迹中恢复干净的光谱峰。",
        keywords: &["signal processing", "spectra", "denoising"],
    },
    DemoChallengeSeed {
        name: "demo-ui-eta",
        title: "Genome Primer",
        summary_en: "Design primer sets that cover target regions while avoiding off-target matches.",
        summary_zh: "设计覆盖目标区域并避免脱靶匹配的引物集合。",
        keywords: &["genomics", "primer design", "biology"],
    },
    DemoChallengeSeed {
        name: "demo-ui-theta",
        title: "Graph Molecules",
        summary_en: "Generate candidate molecules that satisfy graph constraints and scoring rules.",
        summary_zh: "生成满足图约束和评分规则的候选分子。",
        keywords: &["chemistry", "graph search", "molecules"],
    },
    DemoChallengeSeed {
        name: "demo-ui-iota",
        title: "Signal Forecast",
        summary_en: "Forecast sparse experimental signals with uncertainty-aware ranking.",
        summary_zh: "使用不确定性感知排序预测稀疏实验信号。",
        keywords: &["forecasting", "uncertainty", "signals"],
    },
    DemoChallengeSeed {
        name: "demo-ui-kappa",
        title: "Microscopy Segment",
        summary_en: "Segment cell boundaries from noisy microscopy tiles with hidden labels.",
        summary_zh: "在隐藏标签下从噪声显微图块中分割细胞边界。",
        keywords: &["microscopy", "segmentation", "biology"],
    },
];

const DEMO_AGENTS: &[DemoAgentSeed] = &[
    DemoAgentSeed {
        id: "10000000-0000-4000-8000-000000000001",
        display_name: "Maple Baseline",
        description: "Deterministic reference implementation for local demo data.",
        owner: "Agentics Demo",
        model: "baseline",
        profile: "demo",
    },
    DemoAgentSeed {
        id: "10000000-0000-4000-8000-000000000002",
        display_name: "Vector Alchemist",
        description: "Optimized vectorized solution with strong private benchmark results.",
        owner: "Agentics Demo",
        model: "demo-optimizer",
        profile: "demo",
    },
    DemoAgentSeed {
        id: "10000000-0000-4000-8000-000000000003",
        display_name: "Careful Optimizer",
        description: "Conservative solution with lower variance across cases.",
        owner: "Agentics Demo",
        model: "careful-demo",
        profile: "demo",
    },
    DemoAgentSeed {
        id: "10000000-0000-4000-8000-000000000004",
        display_name: "Experimental Draft",
        description: "Fresh demo participant used for pending UI states.",
        owner: "Agentics Demo",
        model: "experimental",
        profile: "demo",
    },
];

const DEMO_RESULTS: &[DemoEvaluationSeed] = &[
    DemoEvaluationSeed {
        submission_id: "20000000-0000-4000-8000-000000000001",
        job_id: "30000000-1000-4000-8000-000000000001",
        evaluation_id: "40000000-0000-4000-8000-000000000001",
        challenge_name: SAMPLE_SUM_CHALLENGE,
        target: "linux-arm64-cpu",
        agent_id: "10000000-0000-4000-8000-000000000001",
        note: "Reference arithmetic implementation.",
        explanation: "Straightforward parser with exact integer arithmetic.",
        score: 1.0000,
        passed: 16,
        total: 16,
    },
    DemoEvaluationSeed {
        submission_id: "20000000-0000-4000-8000-000000000002",
        job_id: "30000000-1000-4000-8000-000000000002",
        evaluation_id: "40000000-0000-4000-8000-000000000002",
        challenge_name: SAMPLE_SUM_CHALLENGE,
        target: "linux-arm64-cpu",
        agent_id: "10000000-0000-4000-8000-000000000002",
        note: "Fast path for compact JSON inputs.",
        explanation: "Vectorized decode path and minimal allocation.",
        score: 0.9375,
        passed: 15,
        total: 16,
    },
    DemoEvaluationSeed {
        submission_id: "20000000-0000-4000-8000-000000000003",
        job_id: "30000000-1000-4000-8000-000000000003",
        evaluation_id: "40000000-0000-4000-8000-000000000003",
        challenge_name: SAMPLE_SUM_CHALLENGE,
        target: "linux-arm64-cpu",
        agent_id: "10000000-0000-4000-8000-000000000003",
        note: "Handles edge cases but misses overflow probe.",
        explanation: "Careful implementation that intentionally leaves one case unresolved.",
        score: 0.8125,
        passed: 13,
        total: 16,
    },
    DemoEvaluationSeed {
        submission_id: "20000000-0000-4000-8000-000000000101",
        job_id: "30000000-1000-4000-8000-000000000101",
        evaluation_id: "40000000-0000-4000-8000-000000000101",
        challenge_name: GRID_ROUTING_CHALLENGE,
        target: "linux-arm64-cpu",
        agent_id: "10000000-0000-4000-8000-000000000002",
        note: "Shortest-path routing with deterministic tie breaking.",
        explanation: "A* style route search tuned for narrow corridors.",
        score: 0.9167,
        passed: 11,
        total: 12,
    },
    DemoEvaluationSeed {
        submission_id: "20000000-0000-4000-8000-000000000102",
        job_id: "30000000-1000-4000-8000-000000000102",
        evaluation_id: "40000000-0000-4000-8000-000000000102",
        challenge_name: GRID_ROUTING_CHALLENGE,
        target: "linux-arm64-cpu",
        agent_id: "10000000-0000-4000-8000-000000000003",
        note: "Conservative BFS route planner.",
        explanation: "Prioritizes valid paths over path length.",
        score: 0.8333,
        passed: 10,
        total: 12,
    },
    DemoEvaluationSeed {
        submission_id: "20000000-0000-4000-8000-000000000103",
        job_id: "30000000-1000-4000-8000-000000000103",
        evaluation_id: "40000000-0000-4000-8000-000000000103",
        challenge_name: GRID_ROUTING_CHALLENGE,
        target: "linux-arm64-cpu",
        agent_id: "10000000-0000-4000-8000-000000000001",
        note: "Baseline Manhattan fallback.",
        explanation: "Simple fallback route planner with obstacle checks.",
        score: 0.6667,
        passed: 8,
        total: 12,
    },
];

pub(super) async fn seed_database(pool: &PgPool) -> Result<(), LocalDemoError> {
    let repos = Repositories::new(pool);
    let source_challenge = load_challenge(&repos, SAMPLE_SUM_CHALLENGE).await?;
    load_challenge(&repos, GRID_ROUTING_CHALLENGE).await?;

    cleanup_demo_rows(pool).await?;
    touch_base_challenges(pool).await?;
    publish_demo_challenges(&repos, &source_challenge).await?;
    insert_demo_agents(pool).await?;
    seed_demo_results(pool, &repos).await?;
    upsert_demo_heartbeats(pool).await?;
    Ok(())
}

pub(super) fn demo_artifact_keys() -> Result<Vec<StorageKey>, LocalDemoError> {
    DEMO_RESULTS
        .iter()
        .map(|result| result.artifact_key())
        .collect()
}

async fn load_challenge(
    repos: &Repositories,
    name: &str,
) -> Result<agentics_persistence::ChallengeRecord, LocalDemoError> {
    let challenge_name = challenge_name(name)?;
    repos
        .challenges()
        .get_published_by_name(&challenge_name)
        .await?
        .ok_or_else(|| {
            LocalDemoError::InvalidConfig(format!(
                "{name} challenge was not seeded; start the API before seeding demo results"
            ))
        })
}

async fn cleanup_demo_rows(pool: &PgPool) -> Result<(), LocalDemoError> {
    for result in DEMO_RESULTS {
        let submission_id = solution_submission_id(result.submission_id)?;
        sqlx::query("DELETE FROM solution_submissions WHERE id = $1::uuid")
            .bind(submission_id.as_str())
            .execute(pool)
            .await?;
    }
    for agent in DEMO_AGENTS {
        let agent_id = agent_id(agent.id)?;
        sqlx::query("DELETE FROM agent_tokens WHERE agent_id = $1::uuid")
            .bind(agent_id.as_str())
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM agents WHERE id = $1::uuid")
            .bind(agent_id.as_str())
            .execute(pool)
            .await?;
    }
    for challenge in DEMO_CHALLENGES {
        let name = challenge_name(challenge.name)?;
        sqlx::query("DELETE FROM challenges WHERE challenge_name = $1")
            .bind(name.as_str())
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn touch_base_challenges(pool: &PgPool) -> Result<(), LocalDemoError> {
    sqlx::query(
        "UPDATE challenges SET created_at = NOW(), updated_at = NOW() WHERE challenge_name = $1",
    )
    .bind(SAMPLE_SUM_CHALLENGE)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE challenges SET created_at = NOW() - INTERVAL '1 second', updated_at = NOW() WHERE challenge_name = $1",
    )
    .bind(GRID_ROUTING_CHALLENGE)
    .execute(pool)
    .await?;
    Ok(())
}

async fn publish_demo_challenges(
    repos: &Repositories,
    source_challenge: &agentics_persistence::ChallengeRecord,
) -> Result<(), LocalDemoError> {
    let source_spec: ChallengeBundleSpec =
        serde_json::from_value(source_challenge.spec_json.clone()).map_err(|error| {
            LocalDemoError::InvalidConfig(format!(
                "stored sample challenge spec is invalid: {error}"
            ))
        })?;

    for challenge in DEMO_CHALLENGES {
        let challenge_name = challenge_name(challenge.name)?;
        let summary = LocalizedText::new(challenge.summary_en, challenge.summary_zh);
        let mut spec = source_spec.clone();
        spec.challenge_name = challenge_name.clone();
        spec.challenge_title = challenge.title.to_string();
        spec.summary = summary.clone();
        spec.keywords = challenge_keywords(challenge.keywords)?;

        let input = PublishChallengeInput {
            challenge_name: &challenge_name,
            bundle_key: &source_challenge.bundle_key,
            public_bundle_key: &source_challenge.public_bundle_key,
            statement_key: &source_challenge.statement_key,
            spec: &spec,
            title: challenge.title,
            summary: &summary,
        };
        repos.challenges().publish(&input).await?;
    }
    Ok(())
}

async fn insert_demo_agents(pool: &PgPool) -> Result<(), LocalDemoError> {
    for agent in DEMO_AGENTS {
        let agent_id = agent_id(agent.id)?;
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, display_name, agent_description, owner, model_info, status, created_at
            )
            VALUES ($1::uuid, $2, $3, $4, $5, 'active', NOW() - INTERVAL '2 days')
            "#,
        )
        .bind(agent_id.as_str())
        .bind(agent.display_name)
        .bind(agent.description)
        .bind(agent.owner)
        .bind(agent_model_info(agent))
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn seed_demo_results(pool: &PgPool, repos: &Repositories) -> Result<(), LocalDemoError> {
    for result in DEMO_RESULTS {
        let challenge = load_challenge(repos, result.challenge_name).await?;
        let submission_id = solution_submission_id(result.submission_id)?;
        let job_id = evaluation_job_id(result.job_id)?;
        let target = target_name(result.target)?;

        repos
            .solution_submissions()
            .create_with_job(&CreateSolutionSubmissionInput {
                solution_submission_id: submission_id.clone(),
                job_id: job_id.clone(),
                agent_id: agent_id(result.agent_id)?,
                challenge_name: challenge.challenge_name,
                target: target.clone(),
                artifact_key: result.artifact_key()?,
                note: result.note.to_string(),
                eval_type: ScoringMode::Official,
                explanation: result.explanation.to_string(),
                parent_solution_submission_id: None,
                credit_text: DEMO_CREDIT_TEXT.to_string(),
                quota_admission: SolutionSubmissionQuotaAdmission {
                    window_seconds: DEMO_QUOTA_WINDOW_SECONDS,
                    per_agent_challenge_limit: DEMO_PER_AGENT_CHALLENGE_LIMIT,
                    challenge_lifetime_limit: None,
                    max_active_official_jobs: None,
                },
            })
            .await?;

        let attempt_count = claim_demo_job(pool, &job_id, &submission_id).await?;
        mark_demo_evaluation_finished(
            repos,
            result,
            &submission_id,
            &job_id,
            &target,
            attempt_count,
        )
        .await?;
    }
    Ok(())
}

async fn claim_demo_job(
    pool: &PgPool,
    job_id: &EvaluationJobId,
    submission_id: &SolutionSubmissionId,
) -> Result<i32, LocalDemoError> {
    let mut tx = pool.begin().await?;
    let attempt_count = sqlx::query_scalar::<_, i32>(
        r#"
        UPDATE evaluation_jobs
        SET status = $3,
            claimed_at = NOW(),
            worker_id = $4,
            attempt_count = attempt_count + 1
        WHERE id = $1::uuid
          AND solution_submission_id = $2::uuid
          AND status = $5
        RETURNING attempt_count
        "#,
    )
    .bind(job_id.as_str())
    .bind(submission_id.as_str())
    .bind(EvaluationJobStatus::Running.as_str())
    .bind(DEMO_WORKER_ID)
    .bind(EvaluationJobStatus::Staged.as_str())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        LocalDemoError::InvalidConfig(format!(
            "demo evaluation job {job_id} was not staged for the synthetic seed claim"
        ))
    })?;

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = $2, updated_at = NOW()
        WHERE id = $1::uuid
          AND visible_after_eval = FALSE
        "#,
    )
    .bind(submission_id.as_str())
    .bind(SolutionSubmissionStatus::Running.as_str())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(attempt_count)
}

async fn mark_demo_evaluation_finished(
    repos: &Repositories,
    result: &DemoEvaluationSeed,
    submission_id: &SolutionSubmissionId,
    job_id: &EvaluationJobId,
    target: &TargetName,
    attempt_count: i32,
) -> Result<(), LocalDemoError> {
    let evaluation_id = evaluation_id(result.evaluation_id)?;
    let started = repos
        .evaluation_jobs()
        .mark_started(&MarkEvaluationStartedInput {
            evaluation_id,
            solution_submission_id: submission_id.clone(),
            job_id: job_id.clone(),
            worker_id: DEMO_WORKER_ID.to_string(),
            claim_attempt_count: attempt_count,
            target: target.clone(),
            eval_type: ScoringMode::Official,
        })
        .await?;
    if !started {
        return Err(LocalDemoError::InvalidConfig(format!(
            "demo evaluation job {job_id} could not be marked running"
        )));
    }

    let finished = repos
        .evaluation_jobs()
        .mark_finished(&PersistedEvaluationResult {
            solution_submission_id: submission_id.clone(),
            job_id: job_id.clone(),
            worker_id: DEMO_WORKER_ID.to_string(),
            claim_attempt_count: attempt_count,
            target: target.clone(),
            eval_type: ScoringMode::Official,
            status: EvaluationStatus::Completed,
            rank_score: Some(result.score),
            aggregate_metrics: result.aggregate_metrics()?,
            run_metrics: result.run_metrics()?,
            public_results: result.public_results(),
            validation_summary: None,
            official_summary: Some(result.official_summary()),
            log_key: None,
            last_error: None,
        })
        .await?;
    if !finished {
        return Err(LocalDemoError::InvalidConfig(format!(
            "demo evaluation job {job_id} could not be marked completed"
        )));
    }
    Ok(())
}

async fn upsert_demo_heartbeats(pool: &PgPool) -> Result<(), LocalDemoError> {
    for (service_name, payload) in [
        (
            "api-server",
            json!({"profile": "local-demo", "status": "running"}),
        ),
        (
            "worker",
            json!({"profile": "local-demo", "status": "not started; fake results seeded directly"}),
        ),
    ] {
        sqlx::query(
            r#"
            INSERT INTO service_heartbeats (service_name, last_seen_at, payload)
            VALUES ($1, NOW(), $2)
            ON CONFLICT (service_name) DO UPDATE
            SET last_seen_at = EXCLUDED.last_seen_at,
                payload = EXCLUDED.payload
            "#,
        )
        .bind(service_name)
        .bind(payload)
        .execute(pool)
        .await?;
    }
    Ok(())
}

impl DemoEvaluationSeed {
    fn artifact_key(self) -> Result<StorageKey, LocalDemoError> {
        StorageKey::try_new(format!("solution-submissions/{}.zip", self.submission_id)).map_err(
            |error| LocalDemoError::InvalidConfig(format!("invalid demo artifact key: {error}")),
        )
    }

    fn aggregate_metrics(self) -> Result<Vec<MetricValue>, LocalDemoError> {
        Ok(vec![
            MetricValue {
                metric_name: metric_name("score")?,
                value: self.score,
            },
            MetricValue {
                metric_name: metric_name("passed_cases")?,
                value: self.passed as f64,
            },
        ])
    }

    fn run_metrics(self) -> Result<Vec<RunMetricResult>, LocalDemoError> {
        Ok(vec![
            RunMetricResult {
                run_name: run_name("public_smoke")?,
                metrics: vec![MetricValue {
                    metric_name: metric_name("score")?,
                    value: self.public_score(),
                }],
            },
            RunMetricResult {
                run_name: run_name("private_suite")?,
                metrics: vec![MetricValue {
                    metric_name: metric_name("score")?,
                    value: self.score,
                }],
            },
        ])
    }

    fn public_results(self) -> Vec<PublicCaseResult> {
        vec![
            PublicCaseResult {
                case_name: "public_smoke".to_string(),
                status: EvaluatorCaseStatus::Passed,
                score: self.public_score(),
                message: Some("Demo public case passed.".to_string()),
            },
            PublicCaseResult {
                case_name: "private_suite".to_string(),
                status: if self.passed == self.total {
                    EvaluatorCaseStatus::Passed
                } else {
                    EvaluatorCaseStatus::Failed
                },
                score: self.score,
                message: Some("Seeded private benchmark summary.".to_string()),
            },
        ]
    }

    fn official_summary(self) -> ScoreSummary {
        ScoreSummary {
            score: self.score,
            passed: self.passed,
            total: self.total,
        }
    }

    #[allow(
        clippy::arithmetic_side_effects,
        reason = "demo score adjustment is bounded by f64::min and uses fixture constants"
    )]
    fn public_score(self) -> f64 {
        (self.score + 0.03).min(1.0)
    }
}

fn agent_model_info(agent: &DemoAgentSeed) -> Value {
    json!({
        "model": agent.model,
        "profile": agent.profile,
    })
}

fn challenge_keywords(values: &[&str]) -> Result<Vec<ChallengeKeyword>, LocalDemoError> {
    values
        .iter()
        .map(|value| challenge_keyword(value))
        .collect()
}

fn agent_id(value: &str) -> Result<AgentId, LocalDemoError> {
    AgentId::try_new(value).map_err(|error| seed_parse_error("agent_id", value, error))
}

fn solution_submission_id(value: &str) -> Result<SolutionSubmissionId, LocalDemoError> {
    SolutionSubmissionId::try_new(value)
        .map_err(|error| seed_parse_error("solution_submission_id", value, error))
}

fn evaluation_job_id(value: &str) -> Result<EvaluationJobId, LocalDemoError> {
    EvaluationJobId::try_new(value)
        .map_err(|error| seed_parse_error("evaluation_job_id", value, error))
}

fn evaluation_id(value: &str) -> Result<EvaluationId, LocalDemoError> {
    EvaluationId::try_new(value).map_err(|error| seed_parse_error("evaluation_id", value, error))
}

fn challenge_name(value: &str) -> Result<ChallengeName, LocalDemoError> {
    ChallengeName::try_new(value.to_string())
        .map_err(|error| seed_parse_error("challenge_name", value, error))
}

fn target_name(value: &str) -> Result<TargetName, LocalDemoError> {
    TargetName::try_new(value.to_string()).map_err(|error| seed_parse_error("target", value, error))
}

fn metric_name(value: &str) -> Result<MetricName, LocalDemoError> {
    MetricName::try_new(value.to_string())
        .map_err(|error| seed_parse_error("metric_name", value, error))
}

fn run_name(value: &str) -> Result<RunName, LocalDemoError> {
    RunName::try_new(value.to_string()).map_err(|error| seed_parse_error("run_name", value, error))
}

fn challenge_keyword(value: &str) -> Result<ChallengeKeyword, LocalDemoError> {
    ChallengeKeyword::try_new(value.to_string())
        .map_err(|error| seed_parse_error("challenge keyword", value, error))
}

fn seed_parse_error(label: &str, value: &str, error: impl std::fmt::Display) -> LocalDemoError {
    LocalDemoError::InvalidConfig(format!("invalid demo {label} `{value}`: {error}"))
}
