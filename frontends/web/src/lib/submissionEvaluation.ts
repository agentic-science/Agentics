import type { SolutionSubmissionResponse } from "@/lib/schemas";

type SubmissionEvaluationFields = Pick<
  SolutionSubmissionResponse,
  "official_evaluation" | "validation_evaluation" | "evaluation"
>;

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
