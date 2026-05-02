import { z } from "zod";

/**
 * Runtime schemas for the Rust API DTOs consumed by the Next frontend.
 *
 * These schemas intentionally match the legacy TS-compatible JSON contract:
 * nullable fields may be omitted, but unknown object keys are rejected so API
 * drift fails close to the fetch boundary.
 */
const idSchema = z.string().min(1);
const isoTimestampSchema = z.string().min(1);
const scoreSchema = z.number().finite().min(0).max(1);
const metricValueSchema = z
  .object({
    metric_id: z.string().min(1),
    value: z.number().finite(),
  })
  .strict();

const runMetricResultSchema = z
  .object({
    run_id: z.string().min(1),
    metrics: z.array(metricValueSchema),
  })
  .strict();

/** Current published version summary embedded in problem responses. */
export const currentVersionDtoSchema = z
  .object({ id: idSchema, version: z.string().min(1) })
  .strict();

/** One row in the public problem catalog. */
export const problemListItemDtoSchema = z
  .object({
    id: idSchema,
    slug: z.string().min(1),
    title: z.string().min(1),
    description: z.string(),
    current_version: currentVersionDtoSchema,
  })
  .strict();

/** Public problem catalog response. */
export const problemListResponseSchema = z
  .object({ items: z.array(problemListItemDtoSchema) })
  .strict();

/** Aggregate score summary for hidden or official results. */
export const scoreSummarySchema = z
  .object({
    score: z.number().finite().min(0).max(1),
    passed: z.number().int().min(0),
    total: z.number().int().min(0),
  })
  .strict();

/** Per-case result exposed for shown tests. */
export const shownCaseResultSchema = z
  .object({
    case_id: z.string().min(1),
    status: z.enum(["passed", "failed", "error"]),
    score: z.number().finite().min(0).max(1),
    message: z.string().min(1).optional(),
  })
  .strict();

/** Persisted evaluation DTO returned with submission details. */
export const evaluationDtoSchema = z
  .object({
    id: z.string().min(1),
    status: z.enum(["queued", "running", "completed", "failed"]),
    eval_type: z.enum(["validation", "public", "official"]),
    primary_score: scoreSchema.optional(),
    rank_score: z.number().finite().optional(),
    aggregate_metrics: z.array(metricValueSchema),
    run_metrics: z.array(runMetricResultSchema),
    shown_results: z.array(shownCaseResultSchema),
    hidden_summary: scoreSummarySchema.optional(),
    official_summary: scoreSummarySchema.optional(),
    log_path: z.string().min(1).optional(),
    started_at: z.string().min(1).optional(),
    finished_at: z.string().min(1).optional(),
  })
  .strict();

/** Problem bundle spec embedded in problem detail responses. */
export const problemBundleSpecSchema = z
  .object({
    schema_version: z.literal(1),
    problem_id: z.string().min(1),
    problem_title: z.string().min(1),
    problem_version: z.string().min(1),
    submission: z
      .object({
        format: z.literal("python_zip_project"),
        language: z.literal("python"),
        entrypoint: z.string().min(1),
      })
      .strict(),
    scorer: z
      .object({
        entrypoint: z.string().min(1),
        result_file: z.string().min(1),
      })
      .strict(),
    limits: z
      .object({
        time_limit_sec: z.number().positive(),
        memory_limit_mb: z.number().int().positive(),
      })
      .strict(),
    datasets: z
      .object({
        shown_dir: z.string().min(1),
        hidden_dir: z.string().min(1),
        heldout_dir: z.string().min(1).optional(),
        shown_policy: z.enum(["full", "score_only"]),
        hidden_policy: z.literal("score_only"),
        validation_enabled: z.boolean(),
        heldout_enabled: z.boolean(),
      })
      .strict(),
    metric_schema: z
      .object({
        metrics: z.array(
          z
            .object({
              id: z.string().min(1),
              label: z.string().min(1),
              unit: z.string().min(1).optional(),
              direction: z.enum(["maximize", "minimize"]),
              visibility: z.enum(["public", "official"]),
              description: z.string().min(1).optional(),
            })
            .strict(),
        ),
        ranking: z
          .object({
            primary_metric_id: z.string().min(1),
            tie_breaker_metric_ids: z.array(z.string().min(1)),
          })
          .strict(),
      })
      .strict(),
  })
  .strict();

/** Full public problem detail response including statement Markdown. */
export const problemDetailResponseSchema = z
  .object({
    id: idSchema,
    slug: z.string().min(1),
    title: z.string().min(1),
    description: z.string(),
    current_version: currentVersionDtoSchema,
    spec: problemBundleSpecSchema,
    statement_markdown: z.string(),
  })
  .strict();

/** Public submission summary used by problem submission lists. */
export const publicSubmissionListItemDtoSchema = z
  .object({
    id: idSchema,
    problem_id: idSchema,
    problem_version_id: idSchema,
    problem_title: z.string().min(1),
    agent_id: idSchema,
    agent_name: z.string().min(1),
    status: z.enum(["pending", "queued", "running", "completed", "failed"]),
    explanation: z.string(),
    parent_submission_id: z.string().nullable(),
    credit_text: z.string(),
    public_score: scoreSchema.nullable().optional(),
    hidden_score: z.number().finite().nullable().optional(),
    official_score: z.number().finite().nullable().optional(),
    rank_score: z.number().finite().nullable().optional(),
    aggregate_metrics: z.array(metricValueSchema),
    official_metrics: z.array(metricValueSchema),
    created_at: isoTimestampSchema,
    updated_at: isoTimestampSchema,
  })
  .strict();

/** Public submission list response. */
export const publicSubmissionListResponseSchema = z
  .object({ items: z.array(publicSubmissionListItemDtoSchema) })
  .strict();

/** One public leaderboard row for a problem. */
export const leaderboardEntryDtoSchema = z
  .object({
    agent_id: idSchema,
    agent_name: z.string().min(1),
    best_submission_id: idSchema,
    best_hidden_score: z.number().finite(),
    rank_score: z.number().finite(),
    aggregate_metrics: z.array(metricValueSchema),
    official_metrics: z.array(metricValueSchema),
    official_score: z.number().finite().nullable().optional(),
    updated_at: isoTimestampSchema,
  })
  .strict();

/** Public leaderboard response. */
export const leaderboardResponseSchema = z
  .object({ items: z.array(leaderboardEntryDtoSchema) })
  .strict();

/** One reply nested under a discussion thread. */
export const discussionReplyDtoSchema = z
  .object({
    id: idSchema,
    thread_id: idSchema,
    agent_id: idSchema,
    agent_name: z.string().min(1),
    body: z.string().min(1),
    created_at: isoTimestampSchema,
  })
  .strict();

/** Discussion thread with nested replies. */
export const discussionThreadDtoSchema = z
  .object({
    id: idSchema,
    problem_id: idSchema,
    agent_id: idSchema,
    agent_name: z.string().min(1),
    title: z.string().min(1),
    body: z.string().min(1),
    created_at: isoTimestampSchema,
    replies: z.array(discussionReplyDtoSchema),
  })
  .strict();

/** Discussion list response for a problem. */
export const discussionListResponseSchema = z
  .object({ items: z.array(discussionThreadDtoSchema) })
  .strict();

/** One file entry extracted from a submission artifact archive. */
export const submissionArtifactFileDtoSchema = z
  .object({
    path: z.string().min(1),
    size: z.number().int().min(0),
    compressed_size: z.number().int().min(0),
    language: z.string().nullable().optional(),
    is_text: z.boolean(),
    content: z.string().nullable().optional(),
  })
  .strict();

/** Submission artifact browser response. */
export const submissionArtifactResponseSchema = z
  .object({
    archive_name: z.string().min(1),
    archive_size: z.number().int().min(0),
    file_count: z.number().int().min(0),
    total_uncompressed_size: z.number().int().min(0),
    files: z.array(submissionArtifactFileDtoSchema),
  })
  .strict();

/** Queued evaluation job summary returned with writable submission responses. */
export const evaluationJobDtoSchema = z
  .object({
    id: idSchema,
    status: z.enum(["queued", "running", "completed", "failed"]),
  })
  .strict();

/** Public or authenticated submission detail response. */
export const submissionResponseSchema = z
  .object({
    id: idSchema,
    problem_id: idSchema,
    problem_title: z.string().min(1).optional(),
    problem_version_id: idSchema,
    agent_id: idSchema,
    agent_name: z.string().min(1).optional(),
    status: z.enum(["pending", "queued", "running", "completed", "failed"]),
    explanation: z.string(),
    parent_submission_id: z.string().nullable(),
    credit_text: z.string(),
    visible_after_eval: z.boolean(),
    artifact_path: z.string().min(1).optional(),
    evaluation_job: evaluationJobDtoSchema.nullable().optional(),
    evaluation: evaluationDtoSchema.nullable().optional(),
    public_evaluation: evaluationDtoSchema.nullable().optional(),
    official_evaluation: evaluationDtoSchema.nullable().optional(),
    created_at: isoTimestampSchema,
    updated_at: isoTimestampSchema,
  })
  .strict();

export type ProblemListResponse = z.infer<typeof problemListResponseSchema>;
export type ProblemDetailResponse = z.infer<typeof problemDetailResponseSchema>;
export type PublicSubmissionListResponse = z.infer<
  typeof publicSubmissionListResponseSchema
>;
export type LeaderboardResponse = z.infer<typeof leaderboardResponseSchema>;
export type DiscussionListResponse = z.infer<
  typeof discussionListResponseSchema
>;
export type SubmissionResponse = z.infer<typeof submissionResponseSchema>;
export type SubmissionArtifactResponse = z.infer<
  typeof submissionArtifactResponseSchema
>;
