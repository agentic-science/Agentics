import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getHumanSession, startGithubLogin } from "@/lib/authApi";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { SignInPanel } from "./SignInPanel";

var returnTo: string | null = "/challenges";

vi.mock("next/navigation", () => ({
  useSearchParams: () => ({
    get: (key: string) => (key === "return_to" ? returnTo : null),
  }),
}));

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  getHumanSession: vi.fn(),
  startGithubLogin: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, render } = await import("@testing-library/react");

const getHumanSessionMock = getHumanSession as Mock;
const startGithubLoginMock = startGithubLogin as Mock;

describe("SignInPanel", () => {
  beforeEach(() => {
    returnTo = "/challenges";
    getHumanSessionMock.mockRejectedValue(
      Object.assign(new Error("Unauthorized"), { status: 401 }),
    );
    startGithubLoginMock.mockResolvedValue({
      authorization_url: "https://github.com/login/oauth/authorize",
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("renders the signed-out Agentics sign-in copy and return action", () => {
    const view = renderSignInPanel();

    expect(view.queryByText("Account")).toBeNull();
    expect(
      view.getByRole("heading", { name: "Agentics Sign-in" }),
    ).toBeTruthy();
    expect(view.getByText(/Sign-in Agentics with GitHub/u)).toBeTruthy();
    const email = view.getByRole("link", { name: "email" });
    expect(email.getAttribute("href")).toBe("mailto:agentics@reify.ing");
    expect(
      view.getByRole("button", { name: "Sign-in with GitHub" }),
    ).toBeTruthy();
    expect(
      view.getByRole("link", { name: "Cancel" }).getAttribute("href"),
    ).toBe("/challenges");
  });

  it("uses the home page as the safe cancel fallback", () => {
    returnTo = "https://example.com/phish";

    const view = renderSignInPanel();

    expect(
      view.getByRole("link", { name: "Cancel" }).getAttribute("href"),
    ).toBe("/");
  });
});

function renderSignInPanel() {
  return render(
    <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
      <NextIntlClientProvider locale="en" messages={messages} timeZone="UTC">
        <SignInPanel />
      </NextIntlClientProvider>
    </SWRConfig>,
  );
}
