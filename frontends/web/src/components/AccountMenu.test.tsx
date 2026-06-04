import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getHumanSession, logoutHuman } from "@/lib/authApi";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { AccountMenu } from "./AccountMenu";

vi.mock("next/navigation", () => ({
  usePathname: () => "/challenges",
}));

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  getHumanSession: vi.fn(),
  logoutHuman: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, fireEvent, render } = await import("@testing-library/react");

const getHumanSessionMock = getHumanSession as Mock;
const logoutHumanMock = logoutHuman as Mock;

describe("AccountMenu", () => {
  beforeEach(() => {
    getHumanSessionMock.mockRejectedValue(
      Object.assign(new Error("Unauthorized"), { status: 401 }),
    );
    logoutHumanMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("links signed-out users to the unified sign-in page", () => {
    const view = renderAccountMenu();

    const link = view.getByRole("link", { name: "Sign in" });
    expect(link.getAttribute("href")).toBe("/sign-in?return_to=%2Fchallenges");
  });

  it("shows Finish Setup for setup-required humans", async () => {
    getHumanSessionMock.mockResolvedValueOnce({
      human_id: "11111111-1111-4111-8111-111111111111",
      status: "setup_required",
      github_user_id: 123,
      github_login: "octocat",
      roles: [],
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });

    const view = renderAccountMenu();
    fireEvent.click(await view.findByRole("button", { name: "Account" }));

    expect(
      await view.findByRole("menuitem", { name: "Finish Setup" }),
    ).toBeTruthy();
    expect(
      view.queryByRole("menuitem", { name: "Creator Console" }),
    ).toBeNull();
  });

  it("shows both consoles for active admins", async () => {
    getHumanSessionMock.mockResolvedValueOnce({
      human_id: "11111111-1111-4111-8111-111111111111",
      status: "active",
      github_user_id: 123,
      github_login: "root",
      roles: ["admin"],
      csrf_token: "csrf-token",
      expires_at: "2026-05-16T00:00:00Z",
    });

    const view = renderAccountMenu();
    fireEvent.click(await view.findByRole("button", { name: "Account" }));

    expect(
      await view.findByRole("menuitem", { name: "Creator Console" }),
    ).toBeTruthy();
    expect(view.getByRole("menuitem", { name: "Admin Panel" })).toBeTruthy();
  });
});

function renderAccountMenu() {
  return render(
    <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
      <NextIntlClientProvider locale="en" messages={messages} timeZone="UTC">
        <AccountMenu />
      </NextIntlClientProvider>
    </SWRConfig>,
  );
}
