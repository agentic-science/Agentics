import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getHumanSession } from "@/lib/authApi";
import {
  CreatorApiError,
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
    createCreatorApiToken: vi.fn(),
    createCreatorApiTokenRequestSchema: passthroughSchema,
    listCreatorApiTokens: vi.fn(),
    revokeCreatorApiToken: vi.fn(),
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

  it("renders only creator API-token management", async () => {
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
    expect(view.queryByText("Creator identity")).toBeNull();
    expect(view.queryByText("octocat")).toBeNull();
    expect(
      view.queryByText(String(activeCreatorSession.github_user_id)),
    ).toBeNull();
    expect(view.queryByText("Human ID")).toBeNull();
    expect(view.queryByText(activeCreatorSession.human_id)).toBeNull();
    expect(await view.findByText("laptop")).toBeTruthy();
    expect(view.queryByText("Register PR for review")).toBeNull();
    expect(view.queryByText("Upload private asset")).toBeNull();
    expect(view.queryByText("Owner statistics")).toBeNull();
  });

  it("shows sign-in state without loading creator token data", async () => {
    const view = renderCreatorConsole();

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

    expect(
      await view.findByText(
        "Finish account setup with a pioneer code before using creator workflows.",
      ),
    ).toBeTruthy();
    expect(view.queryByRole("link", { name: "Finish Setup" })).toBeNull();
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

    await waitFor(() =>
      expect(
        view
          .getByRole("button", { name: "Create token" })
          .hasAttribute("disabled"),
      ).toBe(false),
    );
    fireEvent.input(view.getByLabelText("Label"), {
      target: { value: "laptop" },
    });
    fireEvent.input(view.getByLabelText("Expires at (UTC)"), {
      target: { value: "2026-06-05T12:30" },
    });
    expect(
      (view.getByLabelText("Local time") as HTMLInputElement).value,
    ).toContain("2026");
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    await waitFor(() =>
      expect(createCreatorApiTokenMock).toHaveBeenCalledWith(
        {
          label: "laptop",
          expires_at: "2026-06-05T12:30:00.000Z",
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
    await waitFor(() =>
      expect(view.queryByText("agentics_creator_created-secret")).toBeNull(),
    );
  });

  it("blocks duplicate active creator API token labels before posting", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);
    listCreatorApiTokensMock.mockResolvedValue({
      items: [{ ...creatorTokenRecord, label: " Laptop " }],
    });

    const view = renderCreatorConsole();

    await view.findByText("Laptop");
    fireEvent.input(view.getByLabelText("Label"), {
      target: { value: "laptop" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    expect(
      await view.findByText(
        "An active creator API token already uses this label.",
      ),
    ).toBeTruthy();
    expect(createCreatorApiTokenMock).not.toHaveBeenCalled();
  });

  it("allows creator API token labels used only by revoked tokens", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);
    listCreatorApiTokensMock.mockResolvedValue({
      items: [{ ...creatorTokenRecord, label: " Laptop ", status: "revoked" }],
    });

    const view = renderCreatorConsole();

    await view.findByText("Laptop");
    fireEvent.input(view.getByLabelText("Label"), {
      target: { value: "laptop" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    await waitFor(() =>
      expect(createCreatorApiTokenMock).toHaveBeenCalledWith(
        { label: "laptop" },
        "csrf-token",
      ),
    );
  });

  it("displays backend duplicate-label conflicts when the token list is stale", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);
    createCreatorApiTokenMock.mockRejectedValue(
      new CreatorApiError(
        409,
        "An active creator API token already uses this label.",
      ),
    );

    const view = renderCreatorConsole();

    await waitFor(() =>
      expect(
        view
          .getByRole("button", { name: "Create token" })
          .hasAttribute("disabled"),
      ).toBe(false),
    );
    fireEvent.input(view.getByLabelText("Label"), {
      target: { value: "laptop" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    expect(
      await view.findByText(
        "An active creator API token already uses this label.",
      ),
    ).toBeTruthy();
  });

  it("keeps the raw token visible when clipboard copy fails", async () => {
    getHumanSessionMock.mockResolvedValue(activeCreatorSession);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    });

    const view = renderCreatorConsole();

    await waitFor(() =>
      expect(
        view
          .getByRole("button", { name: "Create token" })
          .hasAttribute("disabled"),
      ).toBe(false),
    );
    fireEvent.input(view.getByLabelText("Label"), {
      target: { value: "laptop" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));
    expect(
      await view.findByText("agentics_creator_created-secret"),
    ).toBeTruthy();

    fireEvent.click(view.getByRole("button", { name: "Copy token" }));

    expect(
      await view.findByText("Token copy failed. Select and copy it manually."),
    ).toBeTruthy();
    expect(view.getByText("agentics_creator_created-secret")).toBeTruthy();
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
