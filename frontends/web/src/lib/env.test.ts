import { describe, expect, it } from "vitest";
import { loadAgenticsWebEnv } from "@/lib/env";

describe("loadAgenticsWebEnv", () => {
  it("derives a local API URL from the default port", () => {
    expect(loadAgenticsWebEnv({}).serverApiBaseUrl).toBe(
      "http://127.0.0.1:3100",
    );
  });

  it("rejects malformed API ports instead of falling back", () => {
    expect(() =>
      loadAgenticsWebEnv({ AGENTICS_API_PORT: "not-a-port" }),
    ).toThrow(/AGENTICS_API_PORT/);
  });

  it("normalizes configured API URLs", () => {
    expect(
      loadAgenticsWebEnv({
        AGENTICS_API_BASE_URL: "https://api.example.test/",
        NEXT_PUBLIC_AGENTICS_API_BASE_URL: "https://public.example.test/",
      }),
    ).toMatchObject({
      serverApiBaseUrl: "https://api.example.test",
      browserApiBaseUrl: "https://public.example.test",
    });
  });

  it("rejects non-http API URLs", () => {
    expect(() =>
      loadAgenticsWebEnv({ AGENTICS_API_BASE_URL: "file:///tmp/api" }),
    ).toThrow(/AGENTICS_API_BASE_URL/);
  });

  it("deduplicates allowed development origins", () => {
    expect(
      loadAgenticsWebEnv({
        AGENTICS_WEB_ALLOWED_DEV_ORIGINS:
          "maplespark.tailnet, localhost, maplespark.tailnet",
      }).allowedDevOrigins,
    ).toEqual(["maplespark.tailnet", "localhost"]);
  });

  it("rejects malformed allowed development origins", () => {
    expect(() =>
      loadAgenticsWebEnv({
        AGENTICS_WEB_ALLOWED_DEV_ORIGINS: "maplespark.tailnet/admin",
      }),
    ).toThrow(/AGENTICS_WEB_ALLOWED_DEV_ORIGINS/);
  });
});
