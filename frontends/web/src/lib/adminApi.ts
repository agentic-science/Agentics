import type { ZodType } from "zod";
import {
  type AdminSessionResponse,
  adminSessionResponseSchema,
} from "@/lib/schemas";

const ADMIN_API_BASE_URL =
  process.env.NEXT_PUBLIC_AGENTICS_API_BASE_URL?.replace(/\/$/, "") ?? "";

/** Describes the admin credentials shape used by this module. */
export interface AdminCredentials {
  username: string;
  password: string;
}

/** Error thrown when an authenticated admin API request fails. */
export class AdminApiError extends Error {
  readonly status: number;

  /** Stores the HTTP status alongside the backend error message. */
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** Handles admin fetch json behavior for this module. */
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
      /** Handles body behavior for this component. */
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Non-JSON error responses still surface the status text.
    }
    throw new AdminApiError(response.status, message);
  }

  return schema.parse(await response.json());
}

/** Handles admin login behavior for this module. */
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
      /** Handles body behavior for this component. */
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Non-JSON error responses still surface the status text.
    }
    throw new AdminApiError(response.status, message);
  }

  return adminSessionResponseSchema.parse(await response.json());
}

/** Restores an admin browser session from the existing cookies. */
export async function adminSession(): Promise<AdminSessionResponse> {
  const response = await fetch(adminEndpoint("/api/auth/admin/session"), {
    method: "GET",
    credentials: "include",
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

/** Handles admin logout behavior for this module. */
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
      /** Handles body behavior for this component. */
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Non-JSON error responses still surface the status text.
    }
    throw new AdminApiError(response.status, message);
  }
}

/** Handles admin endpoint behavior for this module. */
function adminEndpoint(path: string): string {
  if (ADMIN_API_BASE_URL) {
    return `${ADMIN_API_BASE_URL}${path}`;
  }

  return path.replace(/^\/admin(\/|$)/, "/admin-api$1");
}
