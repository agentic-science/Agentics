import { SWRConfig } from "swr";
import type { Mock } from "vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getHumanSession } from "@/lib/authApi";
import { ensureDomEnvironment } from "../test/dom";
import { useHumanSession } from "./humanSession";

vi.mock("@/lib/authApi", () => ({
  HUMAN_SESSION_CACHE_KEY: "human-session",
  getHumanSession: vi.fn(),
}));

ensureDomEnvironment();
const { cleanup, render, waitFor } = await import("@testing-library/react");

const getHumanSessionMock = getHumanSession as Mock;

describe("useHumanSession", () => {
  beforeEach(() => {
    getHumanSessionMock.mockRejectedValue(
      Object.assign(new Error("Unauthorized"), { status: 401 }),
    );
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("clears stale session data after auth revalidation fails", async () => {
    const view = render(
      <SWRConfig
        value={{
          dedupingInterval: 0,
          fallback: {
            "human-session": {
              human_id: "11111111-1111-4111-8111-111111111111",
              status: "active",
              github_user_id: 123,
              github_login: "octocat",
              roles: ["admin"],
              csrf_token: "csrf-token",
              expires_at: "2026-06-07T00:00:00Z",
            },
          },
          provider: () => new Map(),
        }}
      >
        <HumanSessionProbe />
      </SWRConfig>,
    );

    expect(view.getByText("octocat")).toBeTruthy();
    await waitFor(() => expect(view.getByText("signed-out")).toBeTruthy());
  });
});

function HumanSessionProbe() {
  const { data } = useHumanSession();
  return <div>{data?.github_login ?? "signed-out"}</div>;
}
