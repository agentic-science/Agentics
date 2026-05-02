import type { ZodType } from "zod";

const ADMIN_API_BASE_URL =
  process.env.NEXT_PUBLIC_AGENTICS_API_BASE_URL?.replace(/\/$/, "") ?? "";

export interface AdminCredentials {
  username: string;
  password: string;
}

export class AdminApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export async function adminFetchJson<T>(
  path: string,
  schema: ZodType<T>,
  credentials: AdminCredentials,
  init: RequestInit = {},
): Promise<T> {
  const response = await fetch(adminEndpoint(path), {
    ...init,
    headers: {
      "content-type": "application/json",
      Authorization: `Basic ${encodeBasicAuth(credentials)}`,
      ...init.headers,
    },
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Non-JSON error responses still surface the status text.
    }
    throw new AdminApiError(response.status, message);
  }

  return schema.parse(await response.json());
}

function adminEndpoint(path: string): string {
  if (ADMIN_API_BASE_URL) {
    return `${ADMIN_API_BASE_URL}${path}`;
  }

  return path.replace(/^\/admin(\/|$)/, "/admin-api$1");
}

function encodeBasicAuth(credentials: AdminCredentials): string {
  const raw = `${credentials.username}:${credentials.password}`;
  if (typeof btoa === "function") {
    return btoa(raw);
  }
  throw new Error("Basic auth encoding is unavailable in this browser");
}
