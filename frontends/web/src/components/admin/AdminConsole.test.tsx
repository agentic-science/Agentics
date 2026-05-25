import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  adminFetchJson,
  adminLogin,
  adminLogout,
  adminSession,
} from "@/lib/adminApi";
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
    adminLogin: vi.fn(),
    adminLogout: vi.fn(),
    adminSession: vi.fn(),
  };
});

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const adminFetchJsonMock = adminFetchJson as Mock;
const adminLoginMock = adminLogin as Mock;
const adminLogoutMock = adminLogout as Mock;
const adminSessionMock = adminSession as Mock;

describe("AdminConsole", () => {
  beforeEach(() => {
    adminLoginMock.mockResolvedValue({
      username: "root",
      csrf_token: "csrf-token",
      expires_at: "2026-05-15T00:00:00Z",
    });
    adminLogoutMock.mockResolvedValue(undefined);
    adminSessionMock.mockRejectedValue(
      Object.assign(new Error("Unauthorized"), { status: 401 }),
    );
    adminFetchJsonMock.mockImplementation(async (path: string) => {
      switch (path) {
        case "/admin/challenges":
          return adminChallengeListResponseSchema.parse({
            items: [
              {
                challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
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
        case "/admin/challenge-drafts":
          return { items: [] };
        case "/admin/solution-submissions":
          return {
            items: [
              {
                id: "11111111-1111-4111-8111-111111111111",
                challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
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
                challenge_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
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
                created_by_admin_username: "admin",
                created_at: "2026-05-15T00:00:00Z",
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

    fireEvent.click(await view.findByRole("button", { name: "Sign in" }));

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
    fireEvent.click(await view.findByRole("button", { name: "Sign in" }));

    await waitFor(() =>
      expect(adminLoginMock).toHaveBeenCalledWith({
        username: "admin",
        password: "secret",
      }),
    );
    await waitFor(() =>
      expect(adminFetchJsonMock).toHaveBeenCalledWith(
        "/admin/challenges",
        expect.anything(),
        "csrf-token",
      ),
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

    fireEvent.click(view.getByRole("button", { name: "Challenges" }));
    expect(view.getByText("matrix-multiplication")).toBeTruthy();
    expect(view.getByText("open")).toBeTruthy();
  });

  it("restores an existing cookie-backed admin session", async () => {
    adminSessionMock.mockResolvedValueOnce({
      username: "root",
      csrf_token: "restored-csrf-token",
      expires_at: "2026-05-15T01:00:00Z",
    });

    const view = renderAdminConsole();

    expect(await view.findByText("Signed in as root")).toBeTruthy();
    expect(adminLoginMock).not.toHaveBeenCalled();
    expect(adminFetchJsonMock).toHaveBeenCalledWith(
      "/admin/challenges",
      expect.anything(),
      "restored-csrf-token",
    );
    expect(view.queryByRole("button", { name: "Sign in" })).toBeNull();
  });

  it("renders the pioneer-code admin panel", async () => {
    const view = renderAdminConsole();

    fireEvent.input(view.getByLabelText("Password"), {
      target: { value: "secret" },
    });
    fireEvent.click(await view.findByRole("button", { name: "Sign in" }));
    await view.findByText("Signed in as root");

    fireEvent.click(view.getByRole("button", { name: "Pioneer codes" }));

    expect(await view.findByText("jack-deadbeef")).toBeTruthy();
    expect(view.getByText("test cohort")).toBeTruthy();
  });

  it("validates pioneer-code create input before posting", async () => {
    const view = renderAdminConsole();

    fireEvent.input(view.getByLabelText("Password"), {
      target: { value: "secret" },
    });
    fireEvent.click(await view.findByRole("button", { name: "Sign in" }));
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
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const view = renderAdminConsole();

    fireEvent.input(view.getByLabelText("Password"), {
      target: { value: "secret" },
    });
    fireEvent.click(await view.findByRole("button", { name: "Sign in" }));
    await view.findByText("Signed in as root");
    fireEvent.click(view.getByRole("button", { name: "Pioneer codes" }));

    fireEvent.click(await view.findByRole("button", { name: "Revoke" }));

    expect(confirm).toHaveBeenCalledWith(
      "Revoke jack-deadbeef and disable 1 created agents?",
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
