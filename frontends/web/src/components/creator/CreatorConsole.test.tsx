import type { RenderResult } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getHumanSession } from "@/lib/authApi";
import {
  createChallengeReviewRecord,
  createChallengeShortlistRevision,
  getChallengeReviewRecord,
  getChallengeShortlist,
  getCreatorChallengeParticipants,
  getCreatorChallengeStats,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import type {
  ChallengePrivateAssetResponse,
  CreatorChallengeReviewRecordResponse,
} from "@/lib/schemas";
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
    createChallengeReviewRecord: vi.fn(),
    createChallengeReviewRecordRequestSchema: passthroughSchema,
    createChallengeShortlistRevision: vi.fn(),
    createChallengeShortlistRevisionRequestSchema: passthroughSchema,
    getChallengeReviewRecord: vi.fn(),
    getChallengeShortlist: vi.fn(),
    getCreatorChallengeParticipants: vi.fn(),
    getCreatorChallengeStats: vi.fn(),
    uploadChallengePrivateAssetRequestSchema: passthroughSchema,
    uploadPrivateAsset: vi.fn(),
  };
});

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  getHumanSession: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor, within } = await import(
  "@testing-library/react"
);

const createChallengeReviewRecordMock = createChallengeReviewRecord as Mock;
const createChallengeShortlistRevisionMock =
  createChallengeShortlistRevision as Mock;
const getChallengeReviewRecordMock = getChallengeReviewRecord as Mock;
const getChallengeShortlistMock = getChallengeShortlist as Mock;
const getCreatorChallengeParticipantsMock =
  getCreatorChallengeParticipants as Mock;
const getCreatorChallengeStatsMock = getCreatorChallengeStats as Mock;
const getHumanSessionMock = getHumanSession as Mock;
const uploadPrivateAssetMock = uploadPrivateAsset as Mock;

describe("CreatorConsole", () => {
  beforeEach(() => {
    window.localStorage.clear();
    getHumanSessionMock.mockRejectedValue(new Error("not signed in"));
    getChallengeReviewRecordMock.mockRejectedValue(new Error("not configured"));
    getChallengeShortlistMock.mockResolvedValue({
      challenge_name: "frontier-cs-example-challenge",
      items: [],
    });
    getCreatorChallengeParticipantsMock.mockResolvedValue({
      challenge_name: "frontier-cs-example-challenge",
      target: "linux-arm64-cpu",
      items: [],
    });
    getCreatorChallengeStatsMock.mockResolvedValue({
      challenge_name: "frontier-cs-example-challenge",
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
      challenge_name: "frontier-cs-example-challenge",
      uploader_human_id: "11111111-1111-4111-8111-111111111111",
      requested_count: 1,
      added_count: 1,
      sha256:
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      storage_key:
        "challenge-shortlists/frontier-cs-example-challenge/33333333-3333-4333-8333-333333333333.json",
      created_at: "2026-05-15T00:00:00Z",
    });
    uploadPrivateAssetMock.mockRejectedValue(new Error("not configured"));
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("blocks review record creation until a GitHub creator session is loaded", async () => {
    const view = renderCreatorConsole();
    fillReviewRecordRequiredFields(view);

    fireEvent.click(
      view.getByRole("button", { name: "Register PR for review" }),
    );

    expect(
      await view.findByText(
        "Sign in with GitHub before creating a challenge review record.",
      ),
    ).toBeTruthy();
    expect(createChallengeReviewRecordMock).not.toHaveBeenCalled();
  });

  it("renders shared sign-in link when no creator session exists", async () => {
    const view = renderCreatorConsole();

    const link = await view.findByRole("link", { name: "Sign in with GitHub" });
    expect(link.getAttribute("href")).toBe("/sign-in?return_to=/creator");
  });

  it("prompts setup-required humans to finish setup", async () => {
    getHumanSessionMock.mockResolvedValue({
      human_id: "11111111-1111-4111-8111-111111111111",
      status: "setup_required",
      github_user_id: 123,
      github_login: "octocat",
      roles: [],
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });
    const view = renderCreatorConsole();

    const link = await view.findByRole("link", { name: "Finish Setup" });
    expect(link.getAttribute("href")).toBe("/account/setup?return_to=/creator");
    fillReviewRecordRequiredFields(view);
    fireEvent.click(
      view.getByRole("button", { name: "Register PR for review" }),
    );

    expect(
      await view.findByText(
        "Finish account setup with a pioneer code before using creator workflows.",
      ),
    ).toBeTruthy();
  });

  it("creates a review record with the loaded creator identity and CSRF token", async () => {
    getHumanSessionMock.mockResolvedValue({
      human_id: "11111111-1111-4111-8111-111111111111",
      status: "active",
      github_user_id: 123,
      github_login: "octocat",
      roles: ["creator"],
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });
    createChallengeReviewRecordMock.mockResolvedValue(
      challengeReviewRecordResponse,
    );

    const view = renderCreatorConsole();

    expect(await view.findByText(/octocat/)).toBeTruthy();
    fillReviewRecordRequiredFields(view);
    fireEvent.click(
      view.getByRole("button", { name: "Register PR for review" }),
    );

    await waitFor(() =>
      expect(createChallengeReviewRecordMock).toHaveBeenCalledWith(
        expect.objectContaining({
          repo_url: "https://github.com/agentics-reifying/agentics-challenges",
          pr_number: 42,
          pr_url:
            "https://github.com/agentics-reifying/agentics-challenges/pull/42",
          commit_sha: "0123456789abcdef0123456789abcdef01234567",
          challenge_path: "challenges/frontier-cs-example-challenge",
          pr_author_github_user_id: 123,
        }),
        "csrf-token",
      ),
    );
    expect(
      view.getByText(
        "Challenge review record created: 44444444-4444-4444-8444-444444444444",
      ),
    ).toBeTruthy();
    expect(
      window.localStorage.getItem("agentics.creator.last_review_record_id"),
    ).toBe("44444444-4444-4444-8444-444444444444");
  });

  it("rejects malformed PR numbers before creating a review record", async () => {
    getHumanSessionMock.mockResolvedValue({
      human_id: "11111111-1111-4111-8111-111111111111",
      status: "active",
      github_user_id: 123,
      github_login: "octocat",
      roles: ["creator"],
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });

    const view = renderCreatorConsole();

    expect(await view.findByText(/octocat/)).toBeTruthy();
    fillReviewRecordRequiredFields(view);
    fireEvent.input(view.getByLabelText("PR number"), {
      target: { value: "42abc" },
    });
    fireEvent.click(
      view.getByRole("button", { name: "Register PR for review" }),
    );

    expect(
      await view.findByText("PR number must be a positive integer."),
    ).toBeTruthy();
    expect(createChallengeReviewRecordMock).not.toHaveBeenCalled();
  });

  it("uploads a private asset and refreshes the review record detail", async () => {
    getHumanSessionMock.mockResolvedValue(creatorSessionResponse);
    uploadPrivateAssetMock.mockResolvedValue(privateAssetResponse);
    getChallengeReviewRecordMock.mockResolvedValue(
      challengeReviewRecordWithPrivateAsset,
    );

    const view = renderCreatorConsole();

    expect(await view.findByText(/octocat/)).toBeTruthy();
    const privateAssetForm = view
      .getByText("Upload private asset")
      .closest("form");
    if (!privateAssetForm) {
      throw new Error("private asset upload form was not rendered");
    }
    const privateAssetFields = within(privateAssetForm);
    fireEvent.input(privateAssetFields.getByLabelText("Review record ID"), {
      target: { value: challengeReviewRecordResponse.id },
    });
    const fileInput =
      privateAssetForm.querySelector<HTMLInputElement>('input[type="file"]');
    if (!fileInput) {
      throw new Error("private asset file input was not rendered");
    }
    fireEvent.change(fileInput, {
      target: {
        files: [new File(["asset-zip"], "official-seed-config.zip")],
      },
    });
    fireEvent.submit(privateAssetForm);

    await waitFor(() =>
      expect(uploadPrivateAssetMock).toHaveBeenCalledWith(
        challengeReviewRecordResponse.id,
        expect.objectContaining({
          asset_name: "official-seed-config",
          kind: "private_seeds",
          required: true,
          asset_base64: btoa("asset-zip"),
        }),
        creatorSessionResponse.csrf_token,
      ),
    );
    expect(
      await view.findByText("Uploaded private asset official-seed-config."),
    ).toBeTruthy();
    expect(view.getByText("eeeeeeeeeeeeeeee")).toBeTruthy();
  });

  it("uploads a shortlist revision and refreshes owner surfaces", async () => {
    getHumanSessionMock.mockResolvedValue(creatorSessionResponse);
    getChallengeShortlistMock.mockResolvedValue(shortlistWithAgent);

    const view = renderCreatorConsole();

    expect(await view.findByText(/octocat/)).toBeTruthy();
    fireEvent.input(view.getByLabelText("Published challenge name"), {
      target: { value: "frontier-cs-example-challenge" },
    });
    fireEvent.click(view.getByRole("button", { name: "Upload delta" }));

    await waitFor(() =>
      expect(createChallengeShortlistRevisionMock).toHaveBeenCalledWith(
        "frontier-cs-example-challenge",
        { agent_ids_to_add: ["11111111-1111-4111-8111-111111111111"] },
        creatorSessionResponse.csrf_token,
      ),
    );
    expect(
      await view.findByText(
        "Uploaded shortlist revision 33333333-3333-4333-8333-333333333333.",
      ),
    ).toBeTruthy();
    expect(await view.findByText("Ada Agent")).toBeTruthy();
  });
});

/** Builds the creator console test fixture with translations. */
function renderCreatorConsole() {
  return render(
    <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
      <NextIntlClientProvider locale="en" messages={messages}>
        <CreatorConsole />
      </NextIntlClientProvider>
    </SWRConfig>,
  );
}

/** Builds the fill review record required fields test fixture. */
function fillReviewRecordRequiredFields(view: RenderResult) {
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

const challengeReviewRecordResponse = {
  id: "44444444-4444-4444-8444-444444444444",
  challenge_name: "frontier-cs-example-challenge",
  request: "new_challenge",
  status: "pending_review",
  creator_human_id: "11111111-1111-4111-8111-111111111111",
  creator_github_user_id: 123,
  creator_github_login: "octocat",
  repo_url: "https://github.com/agentics-reifying/agentics-challenges",
  pr_number: 42,
  pr_url: "https://github.com/agentics-reifying/agentics-challenges/pull/42",
  commit_sha: "0123456789abcdef0123456789abcdef01234567",
  challenge_path: "challenges/frontier-cs-example-challenge",
  manifest_sha256:
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  manifest: {
    schema_version: 1,
    request: "new_challenge",
    challenge_name: "frontier-cs-example-challenge",
    title: "Frontier-CS Example Challenge",
    summary: {
      en: "Benchmark a small Frontier-CS style task.",
      zh: "评测一个小型 Frontier-CS 风格任务。",
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
} satisfies CreatorChallengeReviewRecordResponse;

const creatorSessionResponse = {
  human_id: "11111111-1111-4111-8111-111111111111",
  status: "active",
  github_user_id: 123,
  github_login: "octocat",
  roles: ["creator"],
  csrf_token: "csrf-token",
  expires_at: "2026-05-16T00:00:00Z",
};

const privateAssetResponse = {
  id: "55555555-5555-4555-8555-555555555555",
  review_record_id: challengeReviewRecordResponse.id,
  asset_name: "official-seed-config",
  kind: "private_seeds",
  required: true,
  storage_key:
    "challenge-review-records/44444444-4444-4444-8444-444444444444/private-assets/official-seed-config.zip",
  uploader_human_id: "11111111-1111-4111-8111-111111111111",
  size_bytes: 9,
  sha256: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
  created_at: "2026-05-15T00:00:00Z",
} satisfies ChallengePrivateAssetResponse;

const challengeReviewRecordWithPrivateAsset = {
  ...challengeReviewRecordResponse,
  private_assets: [privateAssetResponse],
} satisfies CreatorChallengeReviewRecordResponse;

const shortlistWithAgent = {
  challenge_name: "frontier-cs-example-challenge",
  items: [
    {
      agent_id: "11111111-1111-4111-8111-111111111111",
      agent_display_name: "Ada Agent",
      added_by_human_id: "22222222-2222-4222-8222-222222222222",
      created_at: "2026-05-15T00:00:00Z",
    },
  ],
};
