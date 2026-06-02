import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { adminFetchJson } from "@/lib/adminApi";
import { getHumanSession, logoutHuman, startGithubLogin } from "@/lib/authApi";
import { adminChallengeListResponseSchema } from "@/lib/schemas";
import messages from "../../../messages/en.json";
import { ensureDomEnvironment } from "../../test/dom";

import { AdminConsole } from "./AdminConsole";

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

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  getHumanSession: vi.fn(),
  logoutHuman: vi.fn(),
  startGithubLogin: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const adminFetchJsonMock = adminFetchJson as Mock;
const getHumanSessionMock = getHumanSession as Mock;
const logoutHumanMock = logoutHuman as Mock;
const startGithubLoginMock = startGithubLogin as Mock;

describe("AdminConsole", () => {
  beforeEach(() => {
    startGithubLoginMock.mockResolvedValue({
      authorization_url:
        "https://github.com/login/oauth/authorize?client_id=test",
    });
    logoutHumanMock.mockResolvedValue(undefined);
    getHumanSessionMock.mockRejectedValue(
      Object.assign(new Error("Unauthorized"), { status: 401 }),
    );
    adminFetchJsonMock.mockImplementation(async (path: string) => {
      switch (path) {
        case "/admin/challenges":
          return adminChallengeListResponseSchema.parse({
            items: [
              {
                challenge_name: "matrix-multiplication",
                title: "Matrix Multiplication",
                summary: {
                  en: "Benchmark matrix multiplication.",
                  zh: "评测矩阵乘法。",
                },
                status: "active",
                targets: [],
                eligibility: { type: "open" },
                private_benchmark_enabled: true,
                created_at: "2026-05-15T00:00:00Z",
                updated_at: "2026-05-15T00:00:00Z",
              },
            ],
          });
        case "/admin/challenge-review-records":
          return { items: [] };
        case "/admin/solution-submissions":
          return {
            items: [
              {
                id: "11111111-1111-4111-8111-111111111111",
                challenge_name: "matrix-multiplication",
                challenge_title: "Matrix Multiplication",
                target: "linux-arm64-cpu",
                agent_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                agent_display_name: "Agent One",
                status: "queued",
                note: "operator-visible note",
                visible_after_eval: false,
                created_at: "2026-05-15T00:00:00Z",
                updated_at: "2026-05-15T00:00:00Z",
              },
              {
                id: "22222222-2222-4222-8222-222222222222",
                challenge_name: "matrix-multiplication",
                challenge_title: "Matrix Multiplication",
                target: "linux-arm64-cpu",
                agent_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                agent_display_name: "Agent Two",
                status: "running",
                note: "",
                visible_after_eval: false,
                created_at: "2026-05-15T00:00:00Z",
                updated_at: "2026-05-15T00:00:00Z",
              },
            ],
          };
        case "/admin/service-heartbeats":
          return {
            items: [
              {
                service_name: "worker",
                last_seen_at: "2026-05-15T00:00:00Z",
                payload: { worker_id: "agentics-worker-test" },
              },
            ],
          };
        case "/admin/pioneer-codes":
          return {
            items: [
              {
                id: "99999999-9999-4999-8999-999999999999",
                code_display: "jack-deadbeef",
                label: "jack",
                note: "test cohort",
                max_uses: 5,
                use_count: 1,
                status: "active",
                created_by_display: "@root",
                created_at: "2026-05-15T00:00:00Z",
              },
            ],
          };
        case "/admin/humans":
          return {
            items: [
              {
                human_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                status: "active",
                github_user_id: 123,
                github_login: "root",
                roles: ["creator", "admin"],
                created_at: "2026-05-15T00:00:00Z",
              },
            ],
          };
        case "/admin/admin-service-tokens":
          return { items: [] };
        case "/admin/capacity":
          return {
            quota_window_seconds: 86_400,
            quotas: {
              validation_runs_per_agent_challenge_day: 20,
              official_runs_per_agent_challenge_day: 5,
              max_active_official_jobs: 20,
              max_active_agents: 1000,
            },
            usage: {
              active_agents: 2,
              active_validation_jobs: 0,
              active_official_jobs: 1,
            },
          };
        default:
          throw new Error(`Unexpected admin endpoint ${path}`);
      }
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("does not load operator data before a restored admin session exists", async () => {
    const view = renderAdminConsole();

    expect(
      await view.findByRole("button", { name: "Sign in with GitHub" }),
    ).toBeTruthy();

    expect(adminFetchJsonMock).not.toHaveBeenCalled();
  });

  it("starts GitHub OAuth when signing in", async () => {
    const view = renderAdminConsole();

    fireEvent.click(
      await view.findByRole("button", { name: "Sign in with GitHub" }),
    );

    await waitFor(() =>
      expect(startGithubLoginMock).toHaveBeenCalledWith("", "/admin"),
    );
    expect(await view.findByText("Redirecting to GitHub.")).toBeTruthy();
  });

  it("restores an existing admin session and loads operator data", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());

    const view = renderAdminConsole();

    expect(await view.findByText("Signed in as root")).toBeTruthy();
    await waitFor(() =>
      expect(adminFetchJsonMock).toHaveBeenCalledWith(
        "/admin/challenges",
        expect.anything(),
        "csrf-token",
      ),
    );
    await waitFor(() =>
      expect(adminFetchJsonMock).toHaveBeenCalledWith(
        "/admin/capacity",
        expect.anything(),
        "csrf-token",
      ),
    );
    expect(view.getByText("1 / 1")).toBeTruthy();
    expect(view.getByText("1/20")).toBeTruthy();

    fireEvent.click(view.getByRole("button", { name: "Challenges" }));
    expect(view.getByText("matrix-multiplication")).toBeTruthy();
    expect(view.getByText("open")).toBeTruthy();
  });

  it("renders forbidden state for signed-in non-admin humans", async () => {
    getHumanSessionMock.mockResolvedValueOnce({
      ...humanAdminSession(),
      roles: ["creator"],
    });

    const view = renderAdminConsole();

    expect(
      await view.findByText(
        "Access denied. Your GitHub account does not have admin access.",
      ),
    ).toBeTruthy();
    expect(adminFetchJsonMock).not.toHaveBeenCalled();
  });

  it("renders the pioneer-code admin panel", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("Signed in as root");

    fireEvent.click(view.getByRole("button", { name: "Pioneer codes" }));

    expect(await view.findByText("jack-deadbeef")).toBeTruthy();
    expect(view.getByText("test cohort")).toBeTruthy();
  });

  it("validates pioneer-code create input before posting", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("Signed in as root");
    fireEvent.click(view.getByRole("button", { name: "Pioneer codes" }));

    fireEvent.input(await view.findByLabelText("Max uses"), {
      target: { value: "0" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create code" }));

    expect(
      await view.findByText("max_uses must be -1 or a positive integer."),
    ).toBeTruthy();
    expect(adminFetchJsonMock).not.toHaveBeenCalledWith(
      "/admin/pioneer-codes",
      expect.anything(),
      "csrf-token",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("requires confirmation before revoking a pioneer code", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const view = renderAdminConsole();

    await view.findByText("Signed in as root");
    fireEvent.click(view.getByRole("button", { name: "Pioneer codes" }));

    fireEvent.click(await view.findByRole("button", { name: "Revoke" }));

    expect(confirm).toHaveBeenCalledWith(
      "Revoke jack-deadbeef and disable 1 created accounts?",
    );
    expect(adminFetchJsonMock).not.toHaveBeenCalledWith(
      "/admin/pioneer-codes/99999999-9999-4999-8999-999999999999/revoke",
      expect.anything(),
      "csrf-token",
      expect.anything(),
    );
    confirm.mockRestore();
  });
});

/** Builds the render admin console test fixture. */
function renderAdminConsole() {
  return render(
    <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
      <NextIntlClientProvider locale="en" messages={messages}>
        <AdminConsole />
      </NextIntlClientProvider>
    </SWRConfig>,
  );
}

function humanAdminSession() {
  return {
    human_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    github_user_id: 123,
    github_login: "root",
    roles: ["creator", "admin"],
    csrf_token: "csrf-token",
    expires_at: "2026-05-15T00:00:00Z",
  };
}
