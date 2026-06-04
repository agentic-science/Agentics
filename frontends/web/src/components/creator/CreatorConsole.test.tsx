import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getHumanSession } from "@/lib/authApi";
import {
  createCreatorApiToken,
  listCreatorApiTokens,
  revokeCreatorApiToken,
} from "@/lib/creatorApi";
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
    parse: (value: unknown) => value,
    safeParse: (value: unknown) => ({ success: true, data: value }),
  };

  return {
    CreatorApiError: MockCreatorApiError,
    createChallengeReviewRecord: vi.fn(),
    createChallengeReviewRecordRequestSchema: passthroughSchema,
    createChallengeShortlistRevision: vi.fn(),
    createChallengeShortlistRevisionRequestSchema: passthroughSchema,
    createCreatorApiToken: vi.fn(),
    createCreatorApiTokenRequestSchema: passthroughSchema,
    getChallengeReviewRecord: vi.fn(),
    getChallengeShortlist: vi.fn(),
    getCreatorChallengeParticipants: vi.fn(),
    getCreatorChallengeStats: vi.fn(),
    listCreatorApiTokens: vi.fn(),
    revokeCreatorApiToken: vi.fn(),
    uploadChallengePrivateAssetRequestSchema: passthroughSchema,
    uploadPrivateAsset: vi.fn(),
  };
});

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  getHumanSession: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const createCreatorApiTokenMock = createCreatorApiToken as Mock;
const getHumanSessionMock = getHumanSession as Mock;
const listCreatorApiTokensMock = listCreatorApiTokens as Mock;
const revokeCreatorApiTokenMock = revokeCreatorApiToken as Mock;

describe("CreatorConsole", () => {
  beforeEach(() => {
    getHumanSessionMock.mockRejectedValue(new Error("not signed in"));
    listCreatorApiTokensMock.mockResolvedValue({ items: [] });
    createCreatorApiTokenMock.mockResolvedValue({
      token: "agentics_creator_created-secret",
      token_record: creatorTokenRecord,
    });
    revokeCreatorApiTokenMock.mockResolvedValue({
      token_record: { ...creatorTokenRecord, status: "revoked" },
    });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("renders only identity and creator API-token management", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);
    listCreatorApiTokensMock.mockResolvedValue({ items: [creatorTokenRecord] });

    const view = renderCreatorConsole();

    expect(
      await view.findByRole("heading", {
        level: 1,
        name: "Creator API tokens",
      }),
    ).toBeTruthy();
    expect(view.getByText("Create creator API token")).toBeTruthy();
    expect(await view.findByText("laptop")).toBeTruthy();
    expect(view.queryByText("Register PR for review")).toBeNull();
    expect(view.queryByText("Upload private asset")).toBeNull();
    expect(view.queryByText("Owner statistics")).toBeNull();
  });

  it("shows sign-in state without loading creator token data", async () => {
    const view = renderCreatorConsole();

    const link = await view.findByRole("link", { name: "Sign in with GitHub" });
    expect(link.getAttribute("href")).toBe("/sign-in?return_to=/creator");
    expect(
      await view.findByText("Sign in with GitHub before continuing."),
    ).toBeTruthy();
    expect(listCreatorApiTokensMock).not.toHaveBeenCalled();
  });

  it("prompts setup-required humans to finish setup before token creation", async () => {
    getHumanSessionMock.mockResolvedValue({
      ...activeCreatorSession,
      status: "setup_required",
      roles: [],
    });

    const view = renderCreatorConsole();

    const link = await view.findByRole("link", { name: "Finish Setup" });
    expect(link.getAttribute("href")).toBe("/account/setup?return_to=/creator");
    expect(
      await view.findByText(
        "Finish account setup with a pioneer code before using creator workflows.",
      ),
    ).toBeTruthy();
    expect(
      view
        .getByRole("button", { name: "Create token" })
        .hasAttribute("disabled"),
    ).toBe(true);
    expect(listCreatorApiTokensMock).not.toHaveBeenCalled();
  });

  it("creates a creator API token and displays the raw token once", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);

    const view = renderCreatorConsole();

    await view.findByText(/octocat/);
    fireEvent.input(view.getByLabelText("Label"), {
      target: { value: "laptop" },
    });
    fireEvent.input(view.getByLabelText("Expires at"), {
      target: { value: "2026-06-05T12:30" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    await waitFor(() =>
      expect(createCreatorApiTokenMock).toHaveBeenCalledWith(
        {
          label: "laptop",
          expires_at: expect.any(String),
        },
        "csrf-token",
      ),
    );
    expect(
      await view.findByText("agentics_creator_created-secret"),
    ).toBeTruthy();

    fireEvent.click(view.getByRole("button", { name: "Copy token" }));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        "agentics_creator_created-secret",
      ),
    );
  });

  it("revokes creator API tokens and refreshes the list", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);
    listCreatorApiTokensMock.mockResolvedValue({ items: [creatorTokenRecord] });

    const view = renderCreatorConsole();

    await view.findByText("laptop");
    fireEvent.click(view.getByRole("button", { name: "Revoke" }));

    await waitFor(() =>
      expect(revokeCreatorApiTokenMock).toHaveBeenCalledWith(
        creatorTokenRecord.id,
        "csrf-token",
      ),
    );
    expect(await view.findByText("Creator API token revoked.")).toBeTruthy();
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

const activeCreatorSession = {
  human_id: "11111111-1111-4111-8111-111111111111",
  status: "active",
  github_user_id: 123,
  github_login: "octocat",
  roles: ["creator"],
  csrf_token: "csrf-token",
  expires_at: "2026-06-05T00:00:00Z",
};

const creatorTokenRecord = {
  id: "22222222-2222-4222-8222-222222222222",
  label: "laptop",
  status: "active",
  created_by_human_id: "11111111-1111-4111-8111-111111111111",
  created_at: "2026-06-05T00:00:00Z",
};
