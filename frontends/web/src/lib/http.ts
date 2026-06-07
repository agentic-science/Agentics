import type { ZodType } from "zod";
import { loadAgenticsBrowserEnv, loadAgenticsWebEnv } from "@/lib/env";
import { errorResponseSchema } from "@/lib/schemas";

let cachedServerApiBaseUrl: string | undefined;
let cachedBrowserApiBaseUrl: string | undefined;

/** Error thrown when an Agentics API request fails. */
export class ApiClientError extends Error {
  readonly status: number;

  /** Stores the HTTP status alongside the backend error message. */
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** Shared options for typed Agentics JSON requests. */
export interface AgenticsFetchOptions {
  init?: RequestInit;
  csrfToken?: string;
  credentials?: RequestCredentials;
  baseUrl?: string;
  rewriteEndpoint?: (path: string) => string;
}

/** Fetches JSON and validates the response against the provided Zod schema. */
export async function fetchJson<T>(
  path: string,
  schema: ZodType<T>,
  options: AgenticsFetchOptions = {},
): Promise<T> {
  const response = await fetch(agenticsEndpoint(path, options), {
    cache: "no-store",
    ...options.init,
    credentials: options.credentials ?? options.init?.credentials,
    headers: requestHeaders(options),
  });

  if (!response.ok) {
    throw await apiErrorFromResponse(response);
  }

  return schema.parse(await response.json());
}

/** Sends an API request that is expected not to return a JSON response body. */
export async function fetchNoContent(
  path: string,
  options: AgenticsFetchOptions = {},
): Promise<void> {
  const response = await fetch(agenticsEndpoint(path, options), {
    cache: "no-store",
    ...options.init,
    credentials: options.credentials ?? options.init?.credentials,
    headers: requestHeaders(options),
  });

  if (!response.ok) {
    throw await apiErrorFromResponse(response);
  }
}

/** Server-side API base used by public observer rendering. */
export function serverApiBaseUrl(): string {
  cachedServerApiBaseUrl ??= loadAgenticsWebEnv().serverApiBaseUrl;
  return cachedServerApiBaseUrl;
}

/** Browser-side API base used by credentialed admin and creator calls. */
export function browserApiBaseUrl(): string {
  cachedBrowserApiBaseUrl ??= loadAgenticsBrowserEnv().browserApiBaseUrl;
  return cachedBrowserApiBaseUrl;
}

/** Rewrites direct admin backend paths to the Next admin proxy when needed. */
export function rewriteAdminEndpoint(path: string): string {
  return path.replace(/^\/admin(\/|$)/, "/admin-api$1");
}

function agenticsEndpoint(path: string, options: AgenticsFetchOptions): string {
  const rewritten = options.rewriteEndpoint?.(path) ?? path;
  if (options.baseUrl) {
    return `${options.baseUrl}${rewritten}`;
  }
  return rewritten;
}

function requestHeaders(options: AgenticsFetchOptions): HeadersInit {
  const headers: Record<string, string> = {};
  if (options.init?.body !== undefined) {
    headers["content-type"] = "application/json";
  }
  if (options.csrfToken) {
    headers["x-agentics-csrf-token"] = options.csrfToken;
  }
  return {
    ...headers,
    ...options.init?.headers,
  };
}

async function apiErrorFromResponse(
  response: Response,
): Promise<ApiClientError> {
  let message = response.statusText;
  try {
    const parsed = errorResponseSchema.safeParse(await response.json());
    if (parsed.success) {
      message = parsed.data.error.message;
    }
  } catch {
    // Non-JSON responses still surface the HTTP status text.
  }
  return new ApiClientError(response.status, message);
}
