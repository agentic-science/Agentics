import { NextIntlClientProvider } from "next-intl";
import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  adminFetchJson,
  createAdminServiceToken,
  grantHumanAdminRole,
  revokeAdminServiceToken,
  revokeHumanAdminRole,
} from "@/lib/adminApi";
import { getHumanSession } from "@/lib/authApi";
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
    createAdminServiceToken: vi.fn(),
    grantHumanAdminRole: vi.fn(),
    revokeAdminServiceToken: vi.fn(),
    revokeHumanAdminRole: vi.fn(),
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

const adminFetchJsonMock = adminFetchJson as Mock;
const createAdminServiceTokenMock = createAdminServiceToken as Mock;
const grantHumanAdminRoleMock = grantHumanAdminRole as Mock;
const revokeAdminServiceTokenMock = revokeAdminServiceToken as Mock;
const revokeHumanAdminRoleMock = revokeHumanAdminRole as Mock;
const getHumanSessionMock = getHumanSession as Mock;
const originalClipboard = globalThis.navigator.clipboard;
let adminServiceTokenItems: Array<Record<string, unknown>> = [];

describe("AdminConsole", () => {
  beforeEach(() => {
    adminServiceTokenItems = [];
    getHumanSessionMock.mockRejectedValue(
      Object.assign(new Error("Unauthorized"), { status: 401 }),
    );
    adminFetchJsonMock.mockImplementation(
      async (
        path: string,
        _schema: unknown,
        _csrfToken: string,
        init?: { method?: string },
      ) => {
        if (path === "/admin/pioneer-codes" && init?.method === "POST") {
          return {
            code: {
              id: "88888888-8888-4888-8888-888888888888",
              code_display: "jack-cafebabe",
              label: "jack",
              note: "test cohort",
              max_uses: 1,
              use_count: 0,
              status: "active",
              created_by_display: "@root",
              created_at: "2026-05-15T00:00:00Z",
            },
            uses: [],
          };
        }

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
            return { items: adminServiceTokenItems };
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
      },
    );
    createAdminServiceTokenMock.mockResolvedValue({
      token: "agt_admin_service_test_token",
      token_record: {
        id: "77777777-7777-4777-8777-777777777777",
        label: "ci",
        status: "active",
        created_by_human_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        created_at: "2026-05-15T00:00:00Z",
        expires_at: "2026-06-01T00:00:00Z",
        last_used_at: null,
        revoked_at: null,
      },
    });
    grantHumanAdminRoleMock.mockResolvedValue({});
    revokeAdminServiceTokenMock.mockResolvedValue({});
    revokeHumanAdminRoleMock.mockResolvedValue({});
  });

  afterEach(() => {
    cleanup();
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: originalClipboard,
      configurable: true,
    });
    vi.clearAllMocks();
  });

  it("does not load operator data before a restored admin session exists", async () => {
    const view = renderAdminConsole();

    expect(
      await view.findByText(
        "Use the account menu in the header to sign in as an admin.",
      ),
    ).toBeTruthy();

    expect(adminFetchJsonMock).not.toHaveBeenCalled();
  });

  it("restores an existing admin session and loads operator data", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());

    const view = renderAdminConsole();

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

    await view.findByText("1 / 1");

    fireEvent.click(view.getByRole("button", { name: "Pioneer Codes" }));

    expect(await view.findByText("jack-deadbeef")).toBeTruthy();
    expect(view.getByText("test cohort")).toBeTruthy();
  });

  it("creates pioneer codes from a label and UTC expiry only", async () => {
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Pioneer Codes" }));

    fireEvent.change(await view.findByLabelText("Label"), {
      target: { value: "jack" },
    });
    expect(view.queryByText("Generated code")).toBeNull();
    expect(view.queryByText("jack-auto-generated")).toBeNull();
    fireEvent.change(view.getByLabelText("Expires at (UTC)"), {
      target: { value: "2026-06-01T00:00" },
    });
    expect(
      (view.getByLabelText("Local time") as HTMLInputElement).value,
    ).toContain("2026");

    fireEvent.click(view.getByRole("button", { name: "Create Code" }));

    await waitFor(() => {
      const createCall = adminFetchJsonMock.mock.calls.find(
        ([path, _schema, _csrfToken, init]) =>
          path === "/admin/pioneer-codes" && init?.method === "POST",
      );
      expect(createCall?.[3]).toMatchObject({
        method: "POST",
      });
      expect(JSON.parse(String(createCall?.[3]?.body))).toEqual({
        max_uses: 1,
        label: "jack",
        expires_at: "2026-06-01T00:00:00.000Z",
      });
    });
    expect(
      await view.findByText("Created pioneer code jack-cafebabe."),
    ).toBeTruthy();

    fireEvent.click(
      view.getAllByRole("button", { name: "Copy jack-cafebabe" })[0],
    );
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("jack-cafebabe");
    });
    expect(
      view.getAllByRole("button", { name: "Copied jack-cafebabe" })[0],
    ).toBeTruthy();
  });

  it("creates admin service tokens from a label and UTC expiry", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Identity" }));

    fireEvent.change(await view.findByLabelText("Label"), {
      target: { value: "ci" },
    });
    fireEvent.change(view.getByLabelText("Expires at (UTC)"), {
      target: { value: "2026-06-01T00:00" },
    });
    expect(
      (view.getByLabelText("Local time") as HTMLInputElement).value,
    ).toContain("2026");

    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    await waitFor(() => {
      expect(createAdminServiceTokenMock).toHaveBeenCalledWith(
        {
          label: "ci",
          expires_at: "2026-06-01T00:00:00.000Z",
        },
        "csrf-token",
      );
    });
    expect(await view.findByText("agt_admin_service_test_token")).toBeTruthy();
  });

  it("blocks duplicate active admin service-token labels for the current admin", async () => {
    adminServiceTokenItems = [
      adminServiceTokenRecord({
        label: " CI ",
        created_by_human_id: humanAdminSession().human_id,
      }),
    ];
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Identity" }));
    await view.findByText("CI");

    fireEvent.change(await view.findByLabelText("Label"), {
      target: { value: "ci" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    expect(
      await view.findByText(
        "An active admin service token from this admin already uses this label.",
      ),
    ).toBeTruthy();
    expect(createAdminServiceTokenMock).not.toHaveBeenCalled();
  });

  it("allows admin service-token labels used by another admin", async () => {
    adminServiceTokenItems = [
      adminServiceTokenRecord({
        label: " CI ",
        created_by_human_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
      }),
    ];
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Identity" }));
    await view.findByText("CI");

    fireEvent.change(await view.findByLabelText("Label"), {
      target: { value: "ci" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create token" }));

    await waitFor(() =>
      expect(createAdminServiceTokenMock).toHaveBeenCalledWith(
        { label: "ci" },
        "csrf-token",
      ),
    );
  });

  it("disables admin revocation when only one active admin remains", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Identity" }));

    const revokeButton = await view.findByRole("button", {
      name: "Revoke admin",
    });
    expect((revokeButton as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(revokeButton);
    expect(revokeHumanAdminRoleMock).not.toHaveBeenCalled();
  });

  it("validates pioneer-code create input before posting", async () => {
    getHumanSessionMock.mockResolvedValueOnce(humanAdminSession());
    const view = renderAdminConsole();

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Pioneer Codes" }));

    fireEvent.input(await view.findByLabelText("Max uses"), {
      target: { value: "0" },
    });
    fireEvent.click(view.getByRole("button", { name: "Create Code" }));

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

    await view.findByText("1 / 1");
    fireEvent.click(view.getByRole("button", { name: "Pioneer Codes" }));

    fireEvent.click(await view.findByRole("button", { name: "Revoke" }));

    expect(confirm).toHaveBeenCalledWith(
      "Revoke jack-deadbeef and return 1 created accounts to setup-required state?",
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
    status: "active",
    github_user_id: 123,
    github_login: "root",
    roles: ["creator", "admin"],
    csrf_token: "csrf-token",
    expires_at: "2026-05-15T00:00:00Z",
  };
}

function adminServiceTokenRecord(
  overrides: Partial<Record<string, unknown>> = {},
) {
  return {
    id: "77777777-7777-4777-8777-777777777777",
    label: "ci",
    status: "active",
    created_by_human_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    created_at: "2026-05-15T00:00:00Z",
    last_used_at: null,
    expires_at: null,
    revoked_by_human_id: null,
    revoked_at: null,
    ...overrides,
  };
}
