import type { ZodType } from "zod";
import {
  type ChallengeDraftResponse,
  type ChallengePrivateAssetResponse,
  type CreatorMeResponse,
  type CreatorSessionResponse,
  challengeDraftResponseSchema,
  challengePrivateAssetResponseSchema,
  creatorMeResponseSchema,
  creatorSessionResponseSchema,
  type GithubOauthLoginResponse,
  githubOauthLoginResponseSchema,
} from "@/lib/schemas";

const CREATOR_CSRF_STORAGE_KEY = "agentics.creator.csrf_token";
const DEFAULT_CSRF_COOKIE_NAME = "agentics_csrf";

export type ChallengeCreationManifest = ChallengeDraftResponse["manifest"];
export type ChallengePrivateAssetKind =
  ChallengeDraftResponse["private_assets"][number]["kind"];

export interface CreateChallengeDraftRequest {
  repo_url: string;
  pr_number: number;
  pr_url: string;
  commit_sha: string;
  challenge_path: string;
  pr_author_github_user_id: number;
  manifest: ChallengeCreationManifest;
}

export interface UploadChallengePrivateAssetRequest {
  asset_id: string;
  kind: ChallengePrivateAssetKind;
  required: boolean;
  asset_base64: string;
}

export class CreatorApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export function readCreatorCsrfToken(): string {
  if (typeof window === "undefined") {
    return "";
  }

  const stored = window.sessionStorage.getItem(CREATOR_CSRF_STORAGE_KEY);
  if (stored) {
    return stored;
  }

  return readCookie(DEFAULT_CSRF_COOKIE_NAME);
}

export function storeCreatorCsrfToken(csrfToken: string): void {
  if (typeof window !== "undefined") {
    window.sessionStorage.setItem(CREATOR_CSRF_STORAGE_KEY, csrfToken);
  }
}

export async function getCreatorMe(): Promise<CreatorMeResponse> {
  return creatorFetchJson("/api/creator/me", creatorMeResponseSchema);
}

export async function startGithubLogin(): Promise<GithubOauthLoginResponse> {
  return creatorFetchJson(
    "/api/auth/github/login",
    githubOauthLoginResponseSchema,
  );
}

export async function completeGithubLogin(
  code: string,
  state: string,
): Promise<CreatorSessionResponse> {
  const params = new URLSearchParams({ code, state });
  return creatorFetchJson(
    `/api/auth/github/callback?${params.toString()}`,
    creatorSessionResponseSchema,
  );
}

export async function createChallengeDraft(
  request: CreateChallengeDraftRequest,
  csrfToken: string,
): Promise<ChallengeDraftResponse> {
  return creatorFetchJson(
    "/api/creator/challenge-drafts",
    challengeDraftResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

export async function getChallengeDraft(
  id: string,
): Promise<ChallengeDraftResponse> {
  return creatorFetchJson(
    `/api/creator/challenge-drafts/${encodeURIComponent(id)}`,
    challengeDraftResponseSchema,
  );
}

export async function uploadPrivateAsset(
  draftId: string,
  request: UploadChallengePrivateAssetRequest,
  csrfToken: string,
): Promise<ChallengePrivateAssetResponse> {
  return creatorFetchJson(
    `/api/creator/challenge-drafts/${encodeURIComponent(draftId)}/private-assets`,
    challengePrivateAssetResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

async function creatorFetchJson<T>(
  path: string,
  schema: ZodType<T>,
  csrfToken?: string,
  init: RequestInit = {},
): Promise<T> {
  const headers: Record<string, string> = {
    "content-type": "application/json",
  };
  if (csrfToken) {
    headers["x-agentics-csrf-token"] = csrfToken;
  }

  const response = await fetch(path, {
    ...init,
    credentials: "include",
    headers: {
      ...headers,
      ...init.headers,
    },
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Non-JSON error responses still surface the status text.
    }
    throw new CreatorApiError(response.status, message);
  }

  return schema.parse(await response.json());
}

function readCookie(name: string): string {
  return (
    document.cookie
      .split(";")
      .map((cookie) => cookie.trim())
      .find((cookie) => cookie.startsWith(`${name}=`))
      ?.slice(name.length + 1) ?? ""
  );
}
