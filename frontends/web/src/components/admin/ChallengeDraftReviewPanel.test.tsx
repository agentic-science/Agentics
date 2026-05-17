import type { Mock } from "vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { adminFetchJson } from "@/lib/adminApi";
import type { ChallengeDraftListItem } from "@/lib/schemas";
import { ensureDomEnvironment } from "../../test/dom";
import { ChallengeDraftReviewPanel } from "./ChallengeDraftReviewPanel";

vi.mock("@/lib/adminApi", () => {
  class MockAdminApiError extends Error {
    readonly status: number;

    constructor(status: number, message: string) {
      super(message);
      this.status = status;
    }
  }

  return {
    AdminApiError: MockAdminApiError,
    adminFetchJson: vi.fn(),
  };
});

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const adminFetchJsonMock = adminFetchJson as Mock;

describe("ChallengeDraftReviewPanel", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("uses action-specific reject and abandon review messages", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    adminFetchJsonMock.mockResolvedValue({ id: draft.id });
    const onRefresh = vi.fn(async () => {});
    const view = render(
      <ChallengeDraftReviewPanel
        csrfToken="csrf-token"
        drafts={[draft]}
        locale="en"
        onRefresh={onRefresh}
        onError={vi.fn()}
        onMessage={vi.fn()}
      />,
    );

    fireEvent.click(view.getByRole("button", { name: "Reject" }));
    await waitFor(() =>
      expect(adminFetchJsonMock).toHaveBeenCalledWith(
        "/admin/challenge-drafts/44444444-4444-4444-8444-444444444444/reject",
        expect.anything(),
        "csrf-token",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ message: "rejected" }),
        }),
      ),
    );

    fireEvent.click(view.getByRole("button", { name: "Abandon" }));
    await waitFor(() =>
      expect(adminFetchJsonMock).toHaveBeenCalledWith(
        "/admin/challenge-drafts/44444444-4444-4444-8444-444444444444/abandon",
        expect.anything(),
        "csrf-token",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ message: "abandoned" }),
        }),
      ),
    );
    confirm.mockRestore();
  });
});

const draft = {
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
  validation_bundle_sha256: undefined,
  approved_bundle_sha256: undefined,
  manifest: {
    schema_version: 1,
    request: "new_challenge",
    challenge_name: "matrix-multiplication",
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
} satisfies ChallengeDraftListItem;
