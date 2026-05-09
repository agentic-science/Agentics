import type { ZodType } from "zod";
import { z } from "zod";

const ADMIN_API_BASE_URL =
  process.env.NEXT_PUBLIC_AGENTICS_API_BASE_URL?.replace(/\/$/, "") ?? "";

export interface AdminCredentials {
  username: string;
  password: string;
}

const adminSessionResponseSchema = z
  .object({
    username: z.string(),
    csrf_token: z.string(),
    expires_at: z.string(),
  })
  .strict();

export type AdminSessionResponse = z.infer<typeof adminSessionResponseSchema>;

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
  csrfToken: string,
  init: RequestInit = {},
): Promise<T> {
  const response = await fetch(adminEndpoint(path), {
    ...init,
    credentials: "include",
    headers: {
      "content-type": "application/json",
      "x-agentics-csrf-token": csrfToken,
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

export async function adminLogin(
  credentials: AdminCredentials,
): Promise<AdminSessionResponse> {
  const response = await fetch(adminEndpoint("/api/auth/admin/login"), {
    method: "POST",
    credentials: "include",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify(credentials),
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

  return adminSessionResponseSchema.parse(await response.json());
}

export async function adminLogout(csrfToken: string): Promise<void> {
  const response = await fetch(adminEndpoint("/api/auth/admin/logout"), {
    method: "POST",
    credentials: "include",
    headers: {
      "x-agentics-csrf-token": csrfToken,
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
}

function adminEndpoint(path: string): string {
  if (ADMIN_API_BASE_URL) {
    return `${ADMIN_API_BASE_URL}${path}`;
  }

  return path.replace(/^\/admin(\/|$)/, "/admin-api$1");
}
