import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { deleteHumanAccount, getHumanSession } from "@/lib/authApi";
import { clearCreatorApiTokenCaches } from "@/lib/creatorData";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { AccountSettingsPanel } from "./AccountSettingsPanel";

const replaceMock = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    replace: replaceMock,
  }),
}));

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  deleteHumanAccount: vi.fn(),
  getHumanSession: vi.fn(),
}));

vi.mock("@/lib/creatorData", () => ({
  clearCreatorApiTokenCaches: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const getHumanSessionMock = getHumanSession as Mock;
const deleteHumanAccountMock = deleteHumanAccount as Mock;
const clearCreatorApiTokenCachesMock = clearCreatorApiTokenCaches as Mock;

describe("AccountSettingsPanel", () => {
  beforeEach(() => {
    window.localStorage.clear();
    getHumanSessionMock.mockResolvedValue({
      human_id: "11111111-1111-4111-8111-111111111111",
      status: "active",
      github_user_id: 123,
      github_login: "octocat",
      roles: ["creator"],
      csrf_token: "csrf-token",
      expires_at: "2026-06-07T00:00:00Z",
    });
    deleteHumanAccountMock.mockResolvedValue(undefined);
    clearCreatorApiTokenCachesMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("deletes the account with CSRF and clears signed-in state", async () => {
    const view = renderAccountSettingsPanel();

    expect(await view.findByText("@octocat")).toBeTruthy();
    expect(view.queryByText("GitHub user id")).toBeNull();
    expect(view.queryByText("123")).toBeNull();
    fireEvent.change(view.getByLabelText("Type uppercase DELETE to confirm"), {
      target: { value: "DELETE" },
    });
    fireEvent.click(view.getByRole("button", { name: "Delete my account" }));

    await waitFor(() => {
      expect(deleteHumanAccountMock).toHaveBeenCalledWith("csrf-token");
    });
    expect(clearCreatorApiTokenCachesMock).toHaveBeenCalled();
    expect(replaceMock).toHaveBeenCalledWith("/");
  });

  it("stores appearance preferences under an account-hashed key", async () => {
    const view = renderAccountSettingsPanel();

    expect(await view.findByText("Appearance Preferences")).toBeTruthy();
    expect(
      (view.getByRole("radio", { name: "Auto" }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(
      (view.getByRole("radio", { name: "System" }) as HTMLInputElement).checked,
    ).toBe(true);

    fireEvent.click(view.getByRole("radio", { name: "Bright" }));

    await waitFor(() => {
      expect(window.localStorage.getItem("agentics-theme")).toBe("light");
    });
    const accountKeys = Array.from(
      { length: window.localStorage.length },
      (_, index) => window.localStorage.key(index) ?? "",
    ).filter((key) => key.startsWith("agentics-account-appearance:v1:"));
    expect(accountKeys).toHaveLength(1);
    expect(accountKeys[0]).not.toContain(
      "11111111-1111-4111-8111-111111111111",
    );
    expect(accountKeys[0]).not.toContain("123");
    expect(
      JSON.parse(window.localStorage.getItem(accountKeys[0]) ?? "{}"),
    ).toEqual({
      language: "auto",
      mode: "light",
    });
  });
});

function renderAccountSettingsPanel() {
  return render(
    <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
      <NextIntlClientProvider locale="en" messages={messages} timeZone="UTC">
        <AccountSettingsPanel />
      </NextIntlClientProvider>
    </SWRConfig>,
  );
}
