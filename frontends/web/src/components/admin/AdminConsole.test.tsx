import { NextIntlClientProvider } from "next-intl";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { adminFetchJson, adminLogin, adminLogout } from "@/lib/adminApi";
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
    adminLogin: vi.fn(),
    adminLogout: vi.fn(),
  };
});

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const adminFetchJsonMock = adminFetchJson as Mock;
const adminLoginMock = adminLogin as Mock;
const adminLogoutMock = adminLogout as Mock;

describe("AdminConsole", () => {
  beforeEach(() => {
    adminLoginMock.mockResolvedValue({
      username: "root",
      csrf_token: "csrf-token",
      expires_at: "2026-05-15T00:00:00Z",
    });
    adminLogoutMock.mockResolvedValue(undefined);
    adminFetchJsonMock.mockImplementation(async (path: string) => {
      switch (path) {
        case "/admin/challenges":
          return {
            items: [
              {
                id: "matrix-multiplication",
                title: "Matrix Multiplication",
                summary: "Benchmark matrix multiplication.",
                status: "active",
                targets: [],
                eligibility: { eligibility_type: "open" },
              },
            ],
          };
        case "/admin/challenge-drafts":
          return { items: [] };
        case "/admin/solution-submissions":
          return {
            items: [
              {
                id: "submission-queued",
                challenge_name: "matrix-multiplication",
                challenge_title: "Matrix Multiplication",
                target: "linux-arm64-cpu",
                agent_id: "agent-1",
                agent_name: "Agent One",
                status: "queued",
                visible_after_eval: false,
                created_at: "2026-05-15T00:00:00Z",
                updated_at: "2026-05-15T00:00:00Z",
              },
              {
                id: "submission-running",
                challenge_name: "matrix-multiplication",
                challenge_title: "Matrix Multiplication",
                target: "linux-arm64-cpu",
                agent_id: "agent-2",
                agent_name: "Agent Two",
                status: "running",
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

  it("validates credentials before calling the admin API", async () => {
    const view = renderAdminConsole();

    fireEvent.click(view.getByRole("button", { name: "Sign in" }));

    expect(
      await view.findByText(
        "Enter admin credentials before loading operator data.",
      ),
    ).toBeTruthy();
    expect(adminLoginMock).not.toHaveBeenCalled();
    expect(adminFetchJsonMock).not.toHaveBeenCalled();
  });

  it("loads operator data after a successful sign-in", async () => {
    const view = renderAdminConsole();

    fireEvent.input(view.getByLabelText("Password"), {
      target: { value: "secret" },
    });
    fireEvent.click(view.getByRole("button", { name: "Sign in" }));

    await waitFor(() =>
      expect(adminLoginMock).toHaveBeenCalledWith({
        username: "admin",
        password: "secret",
      }),
    );
    expect(adminFetchJsonMock).toHaveBeenCalledWith(
      "/admin/challenges",
      expect.anything(),
      "csrf-token",
    );
    expect(adminFetchJsonMock).toHaveBeenCalledWith(
      "/admin/capacity",
      expect.anything(),
      "csrf-token",
    );
    expect(await view.findByText("Signed in as root")).toBeTruthy();
    expect(
      view.getByText("Admin session started and operator data refreshed."),
    ).toBeTruthy();
    expect(view.getByText("1 / 1")).toBeTruthy();
    expect(view.getByText("1/20")).toBeTruthy();
  });
});

function renderAdminConsole() {
  return render(
    <NextIntlClientProvider locale="en" messages={{}}>
      <AdminConsole />
    </NextIntlClientProvider>,
  );
}
