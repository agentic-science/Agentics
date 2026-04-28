import type { ZodType } from "zod";

const API_BASE_URL = process.env.API_BASE_URL || "http://127.0.0.1:3000";

/**
 * Error thrown when the backend responds with a non-2xx status.
 *
 * The backend keeps the old TS API shape of `{ error, message }`, so callers
 * can present `message` directly while still branching on the HTTP status.
 */
export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/**
 * Fetch JSON from the API server and validate it before rendering.
 *
 * Runtime validation keeps the Next frontend aligned with the Rust DTO
 * contract and catches accidental response drift early.
 */
export async function fetchJson<T>(
  path: string,
  schema: ZodType<T>,
): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${path}`, { cache: "no-store" });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = (await response.json()) as { message?: string };
      if (body.message) {
        message = body.message;
      }
    } catch {
      // Non-JSON error pages still surface the HTTP status text.
    }
    throw new ApiError(response.status, message);
  }

  return schema.parse(await response.json());
}
