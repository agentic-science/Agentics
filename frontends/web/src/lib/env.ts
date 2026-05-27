const DEFAULT_API_HOST = "127.0.0.1";
const DEFAULT_API_PORT = 3100;

export interface AgenticsWebEnv {
  serverApiBaseUrl: string;
  browserApiBaseUrl: string;
  backendOrigin: string;
  allowedDevOrigins: string[];
}

export function loadAgenticsWebEnv(
  source: Partial<NodeJS.ProcessEnv> = process.env,
): AgenticsWebEnv {
  const serverApiBaseUrl =
    optionalEnv(source.AGENTICS_API_BASE_URL) ??
    `http://${DEFAULT_API_HOST}:${parsePort(source.AGENTICS_API_PORT)}`;
  const normalizedServerApiBaseUrl = normalizeHttpUrl(
    "AGENTICS_API_BASE_URL",
    serverApiBaseUrl,
  );
  const browserApiBaseUrl = optionalEnv(
    source.NEXT_PUBLIC_AGENTICS_API_BASE_URL,
  );

  return {
    serverApiBaseUrl: normalizedServerApiBaseUrl,
    browserApiBaseUrl:
      browserApiBaseUrl === undefined
        ? ""
        : normalizeHttpUrl(
            "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
            browserApiBaseUrl,
          ),
    backendOrigin: normalizedServerApiBaseUrl,
    allowedDevOrigins: allowedDevOrigins(
      source.AGENTICS_WEB_ALLOWED_DEV_ORIGINS,
    ),
  };
}

function parsePort(value: string | undefined): number {
  const raw = optionalEnv(value);
  if (raw === undefined) {
    return DEFAULT_API_PORT;
  }
  if (!/^\d+$/.test(raw)) {
    throw new Error(
      `AGENTICS_API_PORT must be an integer; got ${JSON.stringify(raw)}`,
    );
  }
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(
      `AGENTICS_API_PORT must be between 1 and 65535; got ${raw}`,
    );
  }
  return port;
}

function normalizeHttpUrl(name: string, value: string): string {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch (error) {
    throw new Error(`${name} must be a valid URL: ${(error as Error).message}`);
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error(`${name} must use http or https`);
  }
  parsed.hash = "";
  parsed.search = "";
  return parsed.toString().replace(/\/$/, "");
}

function allowedDevOrigins(value: string | undefined): string[] {
  return [
    ...new Set(
      optionalEnv(value)
        ?.split(",")
        .map((origin) => origin.trim())
        .filter(Boolean)
        .map(validateAllowedDevOrigin) ?? [],
    ),
  ];
}

function validateAllowedDevOrigin(origin: string): string {
  if (/[\s/?#]/.test(origin)) {
    throw new Error(
      `AGENTICS_WEB_ALLOWED_DEV_ORIGINS entries must be host patterns without whitespace, paths, queries, or fragments; got ${JSON.stringify(origin)}`,
    );
  }
  return origin;
}

function optionalEnv(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed === "" ? undefined : trimmed;
}
