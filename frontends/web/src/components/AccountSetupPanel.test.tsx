import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { completeHumanSetup, getHumanSession } from "@/lib/authApi";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { AccountSetupPanel } from "./AccountSetupPanel";

var returnTo: string | null = "/creator";

vi.mock("next/navigation", () => ({
  useSearchParams: () => ({
    get: (key: string) => (key === "return_to" ? returnTo : null),
  }),
}));

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  completeHumanSetup: vi.fn(),
  getHumanSession: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, render } = await import("@testing-library/react");

const completeHumanSetupMock = completeHumanSetup as Mock;
const getHumanSessionMock = getHumanSession as Mock;

describe("AccountSetupPanel", () => {
  beforeEach(() => {
    returnTo = "/creator";
    completeHumanSetupMock.mockResolvedValue({
      session: activeSession(),
    });
    getHumanSessionMock.mockResolvedValue(setupRequiredSession());
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("explains setup-required access without the old account badge or human wording", async () => {
    const view = renderAccountSetupPanel();

    expect(view.queryByText("Account setup")).toBeNull();
    expect(
      await view.findByText(
        "Signed in as octocat. Enter a pioneer code to unlock creator workflows.",
      ),
    ).toBeTruthy();
    expect(view.queryByText(/human pioneer code/u)).toBeNull();
    expect(
      view.getByText(/For new users, we currently require a pioneer code/u),
    ).toBeTruthy();
    const email = view.getByRole("link", { name: "email" });
    expect(email.getAttribute("href")).toBe("mailto:agentics@reify.ing");
  });
});

function renderAccountSetupPanel() {
  return render(
    <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
      <NextIntlClientProvider locale="en" messages={messages} timeZone="UTC">
        <AccountSetupPanel />
      </NextIntlClientProvider>
    </SWRConfig>,
  );
}

function setupRequiredSession() {
  return {
    human_id: "11111111-1111-4111-8111-111111111111",
    status: "setup_required",
    github_user_id: 123,
    github_login: "octocat",
    roles: [],
    csrf_token: "csrf-token",
    expires_at: "2026-06-16T00:00:00Z",
  };
}

function activeSession() {
  return {
    ...setupRequiredSession(),
    status: "active",
    roles: ["creator"],
  };
}
