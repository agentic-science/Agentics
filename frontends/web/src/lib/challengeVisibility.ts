import type { ChallengeDetailResponse } from "@/lib/schemas";

type ChallengeSpec = ChallengeDetailResponse["spec"];

export function challengeHasClosed(spec: ChallengeSpec): boolean {
  if (!spec.closes_at) {
    return false;
  }
  return Date.now() >= Date.parse(spec.closes_at);
}

export function publicVisibilityAllows(
  visibility: "public_live" | "public_after_close" | "hidden",
  spec: ChallengeSpec,
): boolean {
  if (visibility === "public_live") {
    return true;
  }
  return visibility === "public_after_close" && challengeHasClosed(spec);
}

export function resultDetailIsPublic(spec: ChallengeSpec): boolean {
  if (spec.visibility.result_detail === "submitter_live_public_live") {
    return true;
  }
  return (
    spec.visibility.result_detail === "submitter_live_public_after_close" &&
    challengeHasClosed(spec)
  );
}

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
