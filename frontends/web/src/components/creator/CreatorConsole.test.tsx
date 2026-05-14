import type { RenderResult } from "@testing-library/react";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createChallengeDraft,
  createChallengeShortlistRevision,
  getChallengeDraft,
  getChallengeShortlist,
  getCreatorChallengeParticipants,
  getCreatorChallengeStats,
  getCreatorMe,
  readCreatorCsrfToken,
  startGithubLogin,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import type { ChallengeDraftResponse } from "@/lib/schemas";
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

  return {
    CreatorApiError: MockCreatorApiError,
    createChallengeDraft: vi.fn(),
    createChallengeShortlistRevision: vi.fn(),
    getChallengeDraft: vi.fn(),
    getChallengeShortlist: vi.fn(),
    getCreatorChallengeParticipants: vi.fn(),
    getCreatorChallengeStats: vi.fn(),
    getCreatorMe: vi.fn(),
    readCreatorCsrfToken: vi.fn(),
    startGithubLogin: vi.fn(),
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
const getCreatorMeMock = getCreatorMe as Mock;
const readCreatorCsrfTokenMock = readCreatorCsrfToken as Mock;
const startGithubLoginMock = startGithubLogin as Mock;
const uploadPrivateAssetMock = uploadPrivateAsset as Mock;

describe("CreatorConsole", () => {
  beforeEach(() => {
    window.localStorage.clear();
    window.sessionStorage.clear();
    readCreatorCsrfTokenMock.mockReturnValue("");
    getCreatorMeMock.mockRejectedValue(new Error("not signed in"));
    startGithubLoginMock.mockResolvedValue({
      authorization_url: "https://github.com/login/oauth/authorize",
    });
    getChallengeDraftMock.mockRejectedValue(new Error("not configured"));
    getChallengeShortlistMock.mockResolvedValue({
      challenge_id: "matrix-multiplication",
      items: [],
    });
    getCreatorChallengeParticipantsMock.mockResolvedValue({
      challenge_id: "matrix-multiplication",
      benchmark_target_id: "linux-arm64-cpu",
      items: [],
    });
    getCreatorChallengeStatsMock.mockResolvedValue({
      challenge_id: "matrix-multiplication",
      benchmark_target_id: "linux-arm64-cpu",
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
      id: "revision-1",
      challenge_id: "matrix-multiplication",
      uploader_agent_id: "agent-creator",
      requested_count: 1,
      added_count: 1,
      sha256: "shortlist-sha",
      storage_uri: "local://shortlist",
      created_at: "2026-05-15T00:00:00Z",
    });
    uploadPrivateAssetMock.mockRejectedValue(new Error("not configured"));
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("blocks draft creation until a GitHub creator session is loaded", async () => {
    const view = render(<CreatorConsole />);
    fillDraftRequiredFields(view);

    fireEvent.click(view.getByRole("button", { name: "Create draft" }));

    expect(
      await view.findByText(
        "Sign in with GitHub before creating a challenge draft.",
      ),
    ).toBeTruthy();
    expect(createChallengeDraftMock).not.toHaveBeenCalled();
  });

  it("creates a draft with the loaded creator identity and CSRF token", async () => {
    readCreatorCsrfTokenMock.mockReturnValue("csrf-token");
    getCreatorMeMock.mockResolvedValue({
      agent_id: "agent-creator",
      github_user_id: 123,
      github_login: "octocat",
    });
    createChallengeDraftMock.mockResolvedValue(challengeDraftResponse);

    const view = render(<CreatorConsole />);

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
          commit_sha: "0123456789abcdef",
          challenge_path: "challenges/matrix-multiplication",
          pr_author_github_user_id: 123,
        }),
        "csrf-token",
      ),
    );
    expect(
      view.getByText("Challenge draft created: draft-matrix-1"),
    ).toBeTruthy();
    expect(window.localStorage.getItem("agentics.creator.last_draft_id")).toBe(
      "draft-matrix-1",
    );
  });
});

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
    target: { value: "0123456789abcdef" },
  });
}

const challengeDraftResponse = {
  id: "draft-matrix-1",
  challenge_id: "matrix-multiplication",
  request: "new_challenge",
  status: "draft",
  creator_agent_id: "agent-creator",
  creator_github_user_id: 123,
  creator_github_login: "octocat",
  repo_url: "https://github.com/agentics-reifying/agentics-challenges",
  pr_number: 42,
  pr_url: "https://github.com/agentics-reifying/agentics-challenges/pull/42",
  commit_sha: "0123456789abcdef",
  challenge_path: "challenges/matrix-multiplication",
  manifest_sha256: "manifest-sha",
  manifest: {
    schema_version: 1,
    request: "new_challenge",
    challenge_id: "matrix-multiplication",
    title: "Matrix Multiplication",
    summary: "Benchmark matrix multiplication solutions.",
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
} satisfies ChallengeDraftResponse;
