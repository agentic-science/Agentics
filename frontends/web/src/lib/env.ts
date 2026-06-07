const DEPLOYMENT_STAGES = ["dev", "test", "rehearsal", "production"] as const;

export type DeploymentStage = (typeof DEPLOYMENT_STAGES)[number];

interface EnvWarning {
  name: string;
  message: string;
}

export interface AgenticsWebEnv {
  deploymentStage: DeploymentStage;
  serverApiBaseUrl: string;
  browserApiBaseUrl: string;
  backendOrigin: string;
  allowedDevOrigins: string[];
  gaMeasurementId?: string;
  warnings: EnvWarning[];
}

export function loadAgenticsWebEnv(
  source: Partial<NodeJS.ProcessEnv> = process.env,
): AgenticsWebEnv {
  const deploymentStage = parseDeploymentStage(
    requiredEnv("AGENTICS_DEPLOYMENT_STAGE", source.AGENTICS_DEPLOYMENT_STAGE),
  );
  rejectRemovedEnv(source);
  const warnings = ignoredEnvWarnings(source);
  const webPort = parsePort(
    "AGENTICS_WEB_PORT",
    requiredEnv("AGENTICS_WEB_PORT", source.AGENTICS_WEB_PORT),
  );
  void webPort;
  const serverApiBaseUrl = requiredEnv(
    "AGENTICS_API_BASE_URL",
    source.AGENTICS_API_BASE_URL,
  );
  const normalizedServerApiBaseUrl = normalizeHttpUrl(
    "AGENTICS_API_BASE_URL",
    serverApiBaseUrl,
  );
  rejectHostedPlaceholder(
    deploymentStage,
    "AGENTICS_API_BASE_URL",
    serverApiBaseUrl,
  );
  const browserApiBaseUrl = optionalEnv(
    source.NEXT_PUBLIC_AGENTICS_API_BASE_URL,
  );
  if (browserApiBaseUrl === undefined) {
    warnings.push({
      name: "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
      message: "unset; default: same-origin Next proxy",
    });
  }
  if (optionalEnv(source.AGENTICS_WEB_ALLOWED_DEV_ORIGINS) === undefined) {
    warnings.push({
      name: "AGENTICS_WEB_ALLOWED_DEV_ORIGINS",
      message: "unset; default: 127.0.0.1 and localhost",
    });
  }
  if (
    optionalEnv(source.NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID) === undefined
  ) {
    warnings.push({
      name: "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
      message: "unset; default: analytics disabled",
    });
  }
  emitWarnings(warnings);

  return {
    deploymentStage,
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
    gaMeasurementId: optionalGaMeasurementId(
      source.NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID,
    ),
    warnings,
  };
}

function parsePort(name: string, raw: string): number {
  if (!/^\d+$/.test(raw)) {
    throw new Error(`${name} must be an integer; got ${JSON.stringify(raw)}`);
  }
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`${name} must be between 1 and 65535; got ${raw}`);
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

function optionalGaMeasurementId(
  value: string | undefined,
): string | undefined {
  const measurementId = optionalEnv(value);
  if (measurementId === undefined) {
    return undefined;
  }
  if (!/^G-[A-Z0-9]+$/.test(measurementId)) {
    throw new Error(
      `NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID must be a GA4 measurement id like G-XXXXXXXXXX; got ${JSON.stringify(measurementId)}`,
    );
  }
  return measurementId;
}

function parseDeploymentStage(value: string): DeploymentStage {
  if (DEPLOYMENT_STAGES.includes(value as DeploymentStage)) {
    return value as DeploymentStage;
  }
  throw new Error(
    `AGENTICS_DEPLOYMENT_STAGE must be one of ${DEPLOYMENT_STAGES.join(", ")}; got ${JSON.stringify(value)}`,
  );
}

function requiredEnv(name: string, value: string | undefined): string {
  const raw = optionalEnv(value);
  if (raw === undefined) {
    throw new Error(`${name} must be set`);
  }
  return raw;
}

function optionalEnv(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed === "" ? undefined : trimmed;
}

function rejectRemovedEnv(source: Partial<NodeJS.ProcessEnv>) {
  if (optionalEnv(source.AGENTICS_REHEARSAL_ENVIRONMENT) !== undefined) {
    throw new Error(
      "AGENTICS_REHEARSAL_ENVIRONMENT has been removed; use AGENTICS_DEPLOYMENT_STAGE=rehearsal",
    );
  }
  if (
    optionalEnv(
      source.AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_AGENT,
    ) !== undefined
  ) {
    throw new Error(
      "AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_AGENT has been removed; use AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_HUMAN",
    );
  }
}

function ignoredEnvWarnings(source: Partial<NodeJS.ProcessEnv>): EnvWarning[] {
  const warnings: EnvWarning[] = [];
  if (optionalEnv(source.AGENTICS_WEB_HOST) !== undefined) {
    warnings.push({
      name: "AGENTICS_WEB_HOST",
      message: "ignored; web bind host is owned by the Compose command",
    });
  }
  if (optionalEnv(source.RUST_LOG) !== undefined) {
    warnings.push({
      name: "RUST_LOG",
      message: "ignored; use AGENTICS_LOG_LEVEL for Agentics service logging",
    });
  }
  return warnings;
}

function rejectHostedPlaceholder(
  stage: DeploymentStage,
  name: string,
  value: string,
) {
  if (
    (stage === "rehearsal" || stage === "production") &&
    value.includes("replace-with-")
  ) {
    throw new Error(`${name} still uses a replace-with-* placeholder`);
  }
}

const emittedWarnings = new Set<string>();

function emitWarnings(warnings: EnvWarning[]) {
  for (const warning of warnings) {
    const key = `${warning.name}:${warning.message}`;
    if (emittedWarnings.has(key)) {
      continue;
    }
    emittedWarnings.add(key);
    console.warn(`[agentics-web] WARN env ${warning.name}: ${warning.message}`);
  }
}
