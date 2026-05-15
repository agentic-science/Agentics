import type { ChallengeDetailResponse } from "@/lib/schemas";

/** Describes the challenge spec shape used by this module. */
type ChallengeSpec = ChallengeDetailResponse["spec"];

/** Handles challenge has closed behavior for this module. */
export function challengeHasClosed(spec: ChallengeSpec): boolean {
  if (!spec.closes_at) {
    return false;
  }
  return Date.now() >= Date.parse(spec.closes_at);
}

/** Handles public visibility allows behavior for this module. */
export function publicVisibilityAllows(
  visibility: "public_live" | "public_after_close" | "hidden",
  spec: ChallengeSpec,
): boolean {
  if (visibility === "public_live") {
    return true;
  }
  return visibility === "public_after_close" && challengeHasClosed(spec);
}

/** Handles result detail is public behavior for this module. */
export function resultDetailIsPublic(spec: ChallengeSpec): boolean {
  if (spec.visibility.result_detail === "submitter_live_public_live") {
    return true;
  }
  return (
    spec.visibility.result_detail === "submitter_live_public_after_close" &&
    challengeHasClosed(spec)
  );
}

/** Handles artifact is public behavior for this module. */
export function artifactIsPublic(spec: ChallengeSpec): boolean {
  if (!resultDetailIsPublic(spec)) {
    return false;
  }
  if (spec.solution_publication === "public") {
    return true;
  }
  return (
    spec.solution_publication === "public_after_close" &&
    challengeHasClosed(spec)
  );
}
