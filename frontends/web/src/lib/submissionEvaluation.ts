import type { SolutionSubmissionResponse } from "@/lib/schemas";

/** Describes the submission evaluation fields shape used by this module. */
type SubmissionEvaluationFields = Pick<
  SolutionSubmissionResponse,
  "official_evaluation" | "validation_evaluation" | "evaluation"
>;

/** Selects submission display evaluation from response data. */
export function selectSubmissionDisplayEvaluation(
  submission: SubmissionEvaluationFields,
): NonNullable<SolutionSubmissionResponse["evaluation"]> | null {
  return (
    submission.official_evaluation ??
    submission.validation_evaluation ??
    submission.evaluation ??
    null
  );
}
