DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM challenges WHERE name = 'sample-sum') THEN
    RAISE EXCEPTION 'sample-sum challenge was not seeded; start the API before seeding demo results';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM challenges WHERE name = 'grid-routing') THEN
    RAISE EXCEPTION 'grid-routing challenge was not seeded; start the API before seeding demo results';
  END IF;
END $$;

DELETE FROM challenges
WHERE name LIKE 'demo-ui-%';

UPDATE challenges
SET created_at = NOW(), updated_at = NOW()
WHERE name = 'sample-sum';

UPDATE challenges
SET created_at = NOW() - INTERVAL '1 second', updated_at = NOW()
WHERE name = 'grid-routing';

WITH source AS (
  SELECT *
  FROM challenges
  WHERE name = 'sample-sum'
),
fake(name, title, summary_en, summary_zh, keywords, ordinal) AS (
  VALUES
    ('demo-ui-alpha', 'Orbital Protein Folding', 'Predict compact protein conformations under synthetic orbital constraints.', '在合成轨道约束下预测紧凑蛋白构象。', jsonb_build_array('biology', 'protein folding', 'simulation'), 1),
    ('demo-ui-beta', 'Catalyst Search', 'Find reaction pathways that maximize yield while minimizing unsafe intermediates.', '寻找最大化产率并减少不安全中间体的反应路径。', jsonb_build_array('chemistry', 'catalysis', 'optimization'), 2),
    ('demo-ui-gamma', 'Cellular Maze', 'Route signaling molecules through a noisy cellular grid without crossing blocked regions.', '让信号分子穿过嘈杂细胞网格，并避开阻塞区域。', jsonb_build_array('biology', 'planning', 'grid search'), 3),
    ('demo-ui-delta', 'Climate Patch', 'Select localized interventions that reduce simulated heat stress under budget limits.', '在预算限制下选择局部干预以降低模拟热应激。', jsonb_build_array('climate', 'optimization', 'policy'), 4),
    ('demo-ui-epsilon', 'Lab Scheduler', 'Optimize robotic wet-lab batches while preserving reagent and timing constraints.', '在保持试剂和时间约束的同时优化机器人湿实验批次。', jsonb_build_array('lab automation', 'scheduling', 'robotics'), 5),
    ('demo-ui-zeta', 'Spectra Denoising', 'Recover clean spectral peaks from corrupted instrument traces.', '从受干扰的仪器轨迹中恢复干净的光谱峰。', jsonb_build_array('signal processing', 'spectra', 'denoising'), 6),
    ('demo-ui-eta', 'Genome Primer', 'Design primer sets that cover target regions while avoiding off-target matches.', '设计覆盖目标区域并避免脱靶匹配的引物集合。', jsonb_build_array('genomics', 'primer design', 'biology'), 7),
    ('demo-ui-theta', 'Graph Molecules', 'Generate candidate molecules that satisfy graph constraints and scoring rules.', '生成满足图约束和评分规则的候选分子。', jsonb_build_array('chemistry', 'graph search', 'molecules'), 8),
    ('demo-ui-iota', 'Signal Forecast', 'Forecast sparse experimental signals with uncertainty-aware ranking.', '使用不确定性感知排序预测稀疏实验信号。', jsonb_build_array('forecasting', 'uncertainty', 'signals'), 9),
    ('demo-ui-kappa', 'Microscopy Segment', 'Segment cell boundaries from noisy microscopy tiles with hidden labels.', '在隐藏标签下从噪声显微图块中分割细胞边界。', jsonb_build_array('microscopy', 'segmentation', 'biology'), 10)
)
INSERT INTO challenges (
  name, title, summary, bundle_path, statement_path, spec_json,
  starts_at, closes_at, eligibility_policy_json, validation_submission_limit,
  official_submission_limit, leaderboard_visibility, score_distribution_visibility,
  result_detail_visibility, solution_publication_policy, status, created_at, updated_at
)
SELECT
  fake.name,
  fake.title,
  jsonb_build_object('en', fake.summary_en, 'zh', fake.summary_zh),
  source.bundle_path,
  source.statement_path,
  jsonb_set(
    jsonb_set(
      jsonb_set(source.spec_json, '{challenge_name}', to_jsonb(fake.name)),
      '{challenge_title}', to_jsonb(fake.title)
    ),
    '{summary}', jsonb_build_object('en', fake.summary_en, 'zh', fake.summary_zh)
  ) || jsonb_build_object('keywords', fake.keywords),
  source.starts_at,
  source.closes_at,
  source.eligibility_policy_json,
  source.validation_submission_limit,
  source.official_submission_limit,
  source.leaderboard_visibility,
  source.score_distribution_visibility,
  source.result_detail_visibility,
  source.solution_publication_policy,
  'active',
  NOW() - ((fake.ordinal + 2) || ' seconds')::interval,
  NOW()
FROM fake
CROSS JOIN source;

DELETE FROM leaderboard_entries
WHERE agent_id IN (
  '10000000-0000-4000-8000-000000000001'::uuid,
  '10000000-0000-4000-8000-000000000002'::uuid,
  '10000000-0000-4000-8000-000000000003'::uuid,
  '10000000-0000-4000-8000-000000000004'::uuid
);

DELETE FROM solution_submissions
WHERE id IN (
  '20000000-0000-4000-8000-000000000001'::uuid,
  '20000000-0000-4000-8000-000000000002'::uuid,
  '20000000-0000-4000-8000-000000000003'::uuid,
  '20000000-0000-4000-8000-000000000101'::uuid,
  '20000000-0000-4000-8000-000000000102'::uuid,
  '20000000-0000-4000-8000-000000000103'::uuid
);

DELETE FROM agent_tokens
WHERE agent_id IN (
  '10000000-0000-4000-8000-000000000001'::uuid,
  '10000000-0000-4000-8000-000000000002'::uuid,
  '10000000-0000-4000-8000-000000000003'::uuid,
  '10000000-0000-4000-8000-000000000004'::uuid
);

DELETE FROM agents
WHERE id IN (
  '10000000-0000-4000-8000-000000000001'::uuid,
  '10000000-0000-4000-8000-000000000002'::uuid,
  '10000000-0000-4000-8000-000000000003'::uuid,
  '10000000-0000-4000-8000-000000000004'::uuid
);

INSERT INTO agents (id, display_name, agent_description, owner, model_info, status, created_at)
VALUES
  ('10000000-0000-4000-8000-000000000001', 'Maple Baseline', 'Deterministic reference implementation for local demo data.', 'Agentics Demo', '{"model":"baseline","profile":"demo"}', 'active', NOW() - INTERVAL '5 days'),
  ('10000000-0000-4000-8000-000000000002', 'Vector Alchemist', 'Optimized vectorized solution with strong private benchmark results.', 'Agentics Demo', '{"model":"demo-optimizer","profile":"demo"}', 'active', NOW() - INTERVAL '4 days'),
  ('10000000-0000-4000-8000-000000000003', 'Careful Optimizer', 'Conservative solution with lower variance across cases.', 'Agentics Demo', '{"model":"careful-demo","profile":"demo"}', 'active', NOW() - INTERVAL '3 days'),
  ('10000000-0000-4000-8000-000000000004', 'Experimental Draft', 'Fresh demo participant used for pending UI states.', 'Agentics Demo', '{"model":"experimental","profile":"demo"}', 'active', NOW() - INTERVAL '2 days');

WITH demo_submissions(id, challenge_name, target, agent_id, note, explanation, score, passed, total, age_hours) AS (
  VALUES
    ('20000000-0000-4000-8000-000000000001'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 'Reference arithmetic implementation.', 'Straightforward parser with exact integer arithmetic.', 1.0000::double precision, 16, 16, 72),
    ('20000000-0000-4000-8000-000000000002'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 'Fast path for compact JSON inputs.', 'Vectorized decode path and minimal allocation.', 0.9375::double precision, 15, 16, 48),
    ('20000000-0000-4000-8000-000000000003'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 'Handles edge cases but misses overflow probe.', 'Careful implementation that intentionally leaves one case unresolved.', 0.8125::double precision, 13, 16, 24),
    ('20000000-0000-4000-8000-000000000101'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 'Shortest-path routing with deterministic tie breaking.', 'A* style route search tuned for narrow corridors.', 0.9167::double precision, 11, 12, 60),
    ('20000000-0000-4000-8000-000000000102'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 'Conservative BFS route planner.', 'Prioritizes valid paths over path length.', 0.8333::double precision, 10, 12, 36),
    ('20000000-0000-4000-8000-000000000103'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 'Baseline Manhattan fallback.', 'Simple fallback route planner with obstacle checks.', 0.6667::double precision, 8, 12, 12)
)
INSERT INTO solution_submissions (
  id, challenge_name, target, agent_id, artifact_key, note, status, explanation,
  credit_text, visible_after_eval, created_at, updated_at
)
SELECT
  id,
  challenge_name,
  target,
  agent_id,
  'solution-submissions/' || id::text || '.zip',
  note,
  'completed',
  explanation,
  'Seeded by agentics-local-demo',
  TRUE,
  NOW() - (age_hours || ' hours')::interval,
  NOW() - (age_hours || ' hours')::interval + INTERVAL '8 minutes'
FROM demo_submissions;

WITH demo_submissions(id, challenge_name, target, agent_id, score, passed, total, age_hours) AS (
  VALUES
    ('20000000-0000-4000-8000-000000000001'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 1.0000::double precision, 16, 16, 72),
    ('20000000-0000-4000-8000-000000000002'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 0.9375::double precision, 15, 16, 48),
    ('20000000-0000-4000-8000-000000000003'::uuid, 'sample-sum', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 0.8125::double precision, 13, 16, 24),
    ('20000000-0000-4000-8000-000000000101'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000002'::uuid, 0.9167::double precision, 11, 12, 60),
    ('20000000-0000-4000-8000-000000000102'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000003'::uuid, 0.8333::double precision, 10, 12, 36),
    ('20000000-0000-4000-8000-000000000103'::uuid, 'grid-routing', 'linux-arm64-cpu', '10000000-0000-4000-8000-000000000001'::uuid, 0.6667::double precision, 8, 12, 12)
),
job_rows AS (
  INSERT INTO evaluation_jobs (
    id, solution_submission_id, challenge_name, target, eval_type, status, priority,
    payload_json, attempt_count, max_attempts, scheduled_at, claimed_at,
    finished_at, worker_id, created_at
  )
  SELECT
    ('30000000-0000-4000-8000-' || lpad(row_number() OVER (ORDER BY id)::text, 12, '0'))::uuid,
    id,
    challenge_name,
    target,
    'official',
    'completed',
    10,
    jsonb_build_object('demo', true),
    1,
    1,
    NOW() - (age_hours || ' hours')::interval,
    NOW() - (age_hours || ' hours')::interval + INTERVAL '1 minute',
    NOW() - (age_hours || ' hours')::interval + INTERVAL '8 minutes',
    'local-demo-seed',
    NOW() - (age_hours || ' hours')::interval
  FROM demo_submissions
  RETURNING id AS job_id, solution_submission_id, target, created_at, finished_at
)
INSERT INTO evaluations (
  id, solution_submission_id, job_id, target, eval_type, status,
  rank_score, aggregate_metrics_json, run_metrics_json,
  public_results_json, official_summary_json, started_at, finished_at, created_at
)
SELECT
  ('40000000-0000-4000-8000-' || lpad(row_number() OVER (ORDER BY d.id)::text, 12, '0'))::uuid,
  d.id,
  j.job_id,
  d.target,
  'official',
  'completed',
  d.score,
  jsonb_build_array(
    jsonb_build_object('metric_name', 'score', 'value', d.score),
    jsonb_build_object('metric_name', 'passed_cases', 'value', d.passed)
  ),
  jsonb_build_array(
    jsonb_build_object('run_name', 'public_smoke', 'metrics', jsonb_build_array(jsonb_build_object('metric_name', 'score', 'value', LEAST(1.0, d.score + 0.03)))),
    jsonb_build_object('run_name', 'private_suite', 'metrics', jsonb_build_array(jsonb_build_object('metric_name', 'score', 'value', d.score)))
  ),
  jsonb_build_array(
    jsonb_build_object('case_name', 'public_smoke', 'status', 'passed', 'score', LEAST(1.0, d.score + 0.03), 'message', 'Demo public case passed.'),
    jsonb_build_object('case_name', 'private_suite', 'status', CASE WHEN d.passed = d.total THEN 'passed' ELSE 'failed' END, 'score', d.score, 'message', 'Seeded private benchmark summary.')
  ),
  jsonb_build_object('score', d.score, 'passed', d.passed, 'total', d.total),
  j.created_at + INTERVAL '1 minute',
  j.finished_at,
  j.created_at
FROM demo_submissions d
JOIN job_rows j ON j.solution_submission_id = d.id;

WITH ranked AS (
  SELECT
    s.challenge_name,
    s.target,
    s.agent_id,
    s.id AS submission_id,
    e.rank_score,
    e.aggregate_metrics_json,
    e.public_results_json
  FROM solution_submissions s
  JOIN evaluations e ON e.solution_submission_id = s.id
  WHERE s.id IN (
    '20000000-0000-4000-8000-000000000001'::uuid,
    '20000000-0000-4000-8000-000000000002'::uuid,
    '20000000-0000-4000-8000-000000000003'::uuid,
    '20000000-0000-4000-8000-000000000101'::uuid,
    '20000000-0000-4000-8000-000000000102'::uuid,
    '20000000-0000-4000-8000-000000000103'::uuid
  )
)
INSERT INTO leaderboard_entries (
  challenge_name, target, agent_id, best_solution_submission_id, best_rank_score,
  public_results_json, aggregate_metrics_json, official_metrics_json, updated_at
)
SELECT
  challenge_name,
  target,
  agent_id,
  submission_id,
  rank_score,
  public_results_json,
  aggregate_metrics_json,
  aggregate_metrics_json,
  NOW()
FROM ranked;

INSERT INTO service_heartbeats (service_name, last_seen_at, payload)
VALUES
  ('api-server', NOW(), '{"profile":"local-demo","status":"running"}'),
  ('worker', NOW() - INTERVAL '2 minutes', '{"profile":"local-demo","status":"not started; fake results seeded directly"}')
ON CONFLICT (service_name) DO UPDATE
SET last_seen_at = EXCLUDED.last_seen_at,
    payload = EXCLUDED.payload;
