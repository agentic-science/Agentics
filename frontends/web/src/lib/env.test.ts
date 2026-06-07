import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadAgenticsBrowserEnv, loadAgenticsWebEnv } from "@/lib/env";

describe("loadAgenticsWebEnv", () => {
  beforeEach(() => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("rejects missing deployment stage", () => {
    expect(() => loadAgenticsWebEnv({})).toThrow(/AGENTICS_DEPLOYMENT_STAGE/);
  });

  it("rejects malformed web ports instead of falling back", () => {
    expect(() =>
      loadAgenticsWebEnv({ ...baseEnv(), AGENTICS_WEB_PORT: "not-a-port" }),
    ).toThrow(/AGENTICS_WEB_PORT/);
  });

  it("normalizes configured API URLs", () => {
    expect(
      loadAgenticsWebEnv({
        ...baseEnv(),
        AGENTICS_API_BASE_URL: "https://api.example.test/",
        NEXT_PUBLIC_AGENTICS_API_BASE_URL: "https://public.example.test/",
      }),
    ).toMatchObject({
      deploymentStage: "dev",
      serverApiBaseUrl: "https://api.example.test",
      browserApiBaseUrl: "https://public.example.test",
    });
  });

  it("loads browser env without server-only variables", () => {
    expect(loadAgenticsBrowserEnv({})).toMatchObject({
      browserApiBaseUrl: "",
      warnings: [
        expect.objectContaining({
          name: "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
        }),
      ],
    });
  });

  it("normalizes browser API URLs independently", () => {
    expect(
      loadAgenticsBrowserEnv({
        NEXT_PUBLIC_AGENTICS_API_BASE_URL: "https://public.example.test/",
      }).browserApiBaseUrl,
    ).toBe("https://public.example.test");
  });

  it("rejects non-http API URLs", () => {
    expect(() =>
      loadAgenticsWebEnv({
        ...baseEnv(),
        AGENTICS_API_BASE_URL: "file:///tmp/api",
      }),
    ).toThrow(/AGENTICS_API_BASE_URL/);
  });

  it("deduplicates allowed development origins", () => {
    expect(
      loadAgenticsWebEnv({
        ...baseEnv(),
        AGENTICS_WEB_ALLOWED_DEV_ORIGINS:
          "maplespark.tailnet, localhost, maplespark.tailnet",
      }).allowedDevOrigins,
    ).toEqual(["maplespark.tailnet", "localhost"]);
  });

  it("rejects malformed allowed development origins", () => {
    expect(() =>
      loadAgenticsWebEnv({
        ...baseEnv(),
        AGENTICS_WEB_ALLOWED_DEV_ORIGINS: "maplespark.tailnet/admin",
      }),
    ).toThrow(/AGENTICS_WEB_ALLOWED_DEV_ORIGINS/);
  });

  it("loads an optional GA4 measurement id", () => {
    expect(
      loadAgenticsWebEnv({
        ...baseEnv(),
        NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID: " G-ABC123XYZ ",
      }).gaMeasurementId,
    ).toBe("G-ABC123XYZ");
  });

  it("rejects non-GA4 measurement ids", () => {
    expect(() =>
      loadAgenticsWebEnv({
        ...baseEnv(),
        NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID: "UA-123",
      }),
    ).toThrow(/NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID/);
  });

  it("reports optional env defaults", () => {
    const env = loadAgenticsWebEnv(baseEnv());

    expect(env.warnings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          name: "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
          message: expect.stringContaining("analytics disabled"),
        }),
        expect.objectContaining({
          name: "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
          message: expect.stringContaining("same-origin Next proxy"),
        }),
      ]),
    );
  });

  it("rejects removed env names", () => {
    expect(() =>
      loadAgenticsWebEnv({
        ...baseEnv(),
        AGENTICS_REHEARSAL_ENVIRONMENT: "true",
      }),
    ).toThrow(/AGENTICS_REHEARSAL_ENVIRONMENT/);
  });
});

function baseEnv(): Partial<NodeJS.ProcessEnv> {
  return {
    AGENTICS_DEPLOYMENT_STAGE: "dev",
    AGENTICS_API_BASE_URL: "http://api:3100",
    AGENTICS_WEB_PORT: "3001",
  };
}
