import type { RenderResult } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createChallengeDraft,
  createChallengeShortlistRevision,
  getChallengeDraft,
  getChallengeShortlist,
  getCreatorChallengeParticipants,
  getCreatorChallengeStats,
  getCreatorSession,
  startGithubLogin,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import type { CreatorChallengeDraftResponse } from "@/lib/schemas";
import messages from "../../../messages/en.json";
import { ensureDomEnvironment } from "../../test/dom";

import { CreatorConsole } from "./CreatorConsole";

vi.mock("@/lib/creatorApi", () => {
  class MockCreatorApiError extends Error {
    readonly status: number;

    constructor(status: number, message: string) {
      super(message);
      this.status = status;
    }
  }

  const passthroughSchema = {
    safeParse: (value: unknown) => ({ success: true, data: value }),
  };

  return {
    CreatorApiError: MockCreatorApiError,
    createChallengeDraft: vi.fn(),
    createChallengeDraftRequestSchema: passthroughSchema,
    createChallengeShortlistRevision: vi.fn(),
    createChallengeShortlistRevisionRequestSchema: passthroughSchema,
    getChallengeDraft: vi.fn(),
    getChallengeShortlist: vi.fn(),
    getCreatorChallengeParticipants: vi.fn(),
    getCreatorChallengeStats: vi.fn(),
    getCreatorSession: vi.fn(),
    startGithubLogin: vi.fn(),
    uploadChallengePrivateAssetRequestSchema: passthroughSchema,
    uploadPrivateAsset: vi.fn(),
  };
});

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const createChallengeDraftMock = createChallengeDraft as Mock;
const createChallengeShortlistRevisionMock =
  createChallengeShortlistRevision as Mock;
const getChallengeDraftMock = getChallengeDraft as Mock;
const getChallengeShortlistMock = getChallengeShortlist as Mock;
const getCreatorChallengeParticipantsMock =
  getCreatorChallengeParticipants as Mock;
const getCreatorChallengeStatsMock = getCreatorChallengeStats as Mock;
const getCreatorSessionMock = getCreatorSession as Mock;
const startGithubLoginMock = startGithubLogin as Mock;
const uploadPrivateAssetMock = uploadPrivateAsset as Mock;

describe("CreatorConsole", () => {
  beforeEach(() => {
    window.localStorage.clear();
    getCreatorSessionMock.mockRejectedValue(new Error("not signed in"));
    startGithubLoginMock.mockResolvedValue({
      authorization_url: "https://github.com/login/oauth/authorize",
    });
    getChallengeDraftMock.mockRejectedValue(new Error("not configured"));
    getChallengeShortlistMock.mockResolvedValue({
      challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      challenge_name: "matrix-multiplication",
      items: [],
    });
    getCreatorChallengeParticipantsMock.mockResolvedValue({
      challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      challenge_name: "matrix-multiplication",
      target: "linux-arm64-cpu",
      items: [],
    });
    getCreatorChallengeStatsMock.mockResolvedValue({
      challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      challenge_name: "matrix-multiplication",
      target: "linux-arm64-cpu",
      agent_count: 0,
      solution_submission_count: 0,
      completed_solution_submission_count: 0,
      failed_solution_submission_count: 0,
      queued_or_running_solution_submission_count: 0,
      visible_solution_submission_count: 0,
      validation_run_count: 0,
      official_run_count: 0,
    });
    createChallengeShortlistRevisionMock.mockResolvedValue({
      id: "33333333-3333-4333-8333-333333333333",
      challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      challenge_name: "matrix-multiplication",
      uploader_agent_id: "11111111-1111-4111-8111-111111111111",
      requested_count: 1,
      added_count: 1,
      sha256:
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      storage_key:
        "challenge-shortlists/matrix-multiplication/33333333-3333-4333-8333-333333333333.json",
      created_at: "2026-05-15T00:00:00Z",
    });
    uploadPrivateAssetMock.mockRejectedValue(new Error("not configured"));
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("blocks draft creation until a GitHub creator session is loaded", async () => {
    const view = renderCreatorConsole();
    fillDraftRequiredFields(view);

    fireEvent.click(view.getByRole("button", { name: "Create draft" }));

    expect(
      await view.findByText(
        "Sign in with GitHub before creating a challenge draft.",
      ),
    ).toBeTruthy();
    expect(createChallengeDraftMock).not.toHaveBeenCalled();
  });

  it("starts GitHub OAuth without a pioneer code for returning creators", async () => {
    const view = renderCreatorConsole();

    fireEvent.click(view.getByRole("button", { name: "Sign in with GitHub" }));

    await waitFor(() => expect(startGithubLoginMock).toHaveBeenCalledWith(""));
  });

  it("starts GitHub OAuth with a pioneer code", async () => {
    const view = renderCreatorConsole();

    fireEvent.input(view.getByLabelText("Pioneer code for new creators"), {
      target: { value: " jack-deadbeef " },
    });
    fireEvent.click(view.getByRole("button", { name: "Sign in with GitHub" }));

    await waitFor(() =>
      expect(startGithubLoginMock).toHaveBeenCalledWith("jack-deadbeef"),
    );
  });

  it("creates a draft with the loaded creator identity and CSRF token", async () => {
    getCreatorSessionMock.mockResolvedValue({
      agent_id: "11111111-1111-4111-8111-111111111111",
      github_user_id: 123,
      github_login: "octocat",
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });
    createChallengeDraftMock.mockResolvedValue(challengeDraftResponse);

    const view = renderCreatorConsole();

    expect(await view.findByText(/octocat/)).toBeTruthy();
    fillDraftRequiredFields(view);
    fireEvent.click(view.getByRole("button", { name: "Create draft" }));

    await waitFor(() =>
      expect(createChallengeDraftMock).toHaveBeenCalledWith(
        expect.objectContaining({
          repo_url: "https://github.com/agentics-reifying/agentics-challenges",
          pr_number: 42,
          pr_url:
            "https://github.com/agentics-reifying/agentics-challenges/pull/42",
          commit_sha: "0123456789abcdef0123456789abcdef01234567",
          challenge_path: "challenges/matrix-multiplication",
          pr_author_github_user_id: 123,
        }),
        "csrf-token",
      ),
    );
    expect(
      view.getByText(
        "Challenge draft created: 44444444-4444-4444-8444-444444444444",
      ),
    ).toBeTruthy();
    expect(window.localStorage.getItem("agentics.creator.last_draft_id")).toBe(
      "44444444-4444-4444-8444-444444444444",
    );
  });

  it("rejects malformed PR numbers before creating a draft", async () => {
    getCreatorSessionMock.mockResolvedValue({
      agent_id: "11111111-1111-4111-8111-111111111111",
      github_user_id: 123,
      github_login: "octocat",
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });

    const view = renderCreatorConsole();

    expect(await view.findByText(/octocat/)).toBeTruthy();
    fillDraftRequiredFields(view);
    fireEvent.input(view.getByLabelText("PR number"), {
      target: { value: "42abc" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create draft" }));

    expect(
      await view.findByText("PR number must be a positive integer."),
    ).toBeTruthy();
    expect(createChallengeDraftMock).not.toHaveBeenCalled();
  });
});

/** Builds the creator console test fixture with translations. */
function renderCreatorConsole() {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <CreatorConsole />
    </NextIntlClientProvider>,
  );
}

/** Builds the fill draft required fields test fixture. */
function fillDraftRequiredFields(view: RenderResult) {
  fireEvent.input(view.getByLabelText("PR number"), {
    target: { value: "42" },
  });
  fireEvent.input(view.getByLabelText("PR URL"), {
    target: {
      value: "https://github.com/agentics-reifying/agentics-challenges/pull/42",
    },
  });
  fireEvent.input(view.getByLabelText("Commit SHA"), {
    target: { value: "0123456789abcdef0123456789abcdef01234567" },
  });
}

const challengeDraftResponse = {
  id: "44444444-4444-4444-8444-444444444444",
  challenge_name: "matrix-multiplication",
  request: "new_challenge",
  status: "draft",
  creator_agent_id: "11111111-1111-4111-8111-111111111111",
  creator_github_user_id: 123,
  creator_github_login: "octocat",
  repo_url: "https://github.com/agentics-reifying/agentics-challenges",
  pr_number: 42,
  pr_url: "https://github.com/agentics-reifying/agentics-challenges/pull/42",
  commit_sha: "0123456789abcdef0123456789abcdef01234567",
  challenge_path: "challenges/matrix-multiplication",
  manifest_sha256:
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  manifest: {
    schema_version: 1,
    request: "new_challenge",
    challenge_name: "matrix-multiplication",
    title: "Matrix Multiplication",
    summary: {
      en: "Benchmark matrix multiplication solutions.",
      zh: "评测矩阵乘法解决方案。",
    },
    keywords: ["linear algebra"],
    readme_path: "README.md",
    bundle_path: "v1",
    private_assets: [],
    ci: {
      validate_manifest: true,
      validate_public_bundle: true,
      smoke_test_public_validation: true,
    },
  },
  private_assets: [],
  validation_records: [],
  created_at: "2026-05-15T00:00:00Z",
  updated_at: "2026-05-15T00:00:00Z",
} satisfies CreatorChallengeDraftResponse;
