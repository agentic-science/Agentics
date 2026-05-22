import type { ZodType } from "zod";
import { errorResponseSchema } from "@/lib/schemas";

const AGENTICS_API_BASE_URL =
  process.env.AGENTICS_API_BASE_URL ||
  `http://127.0.0.1:${process.env.AGENTICS_API_PORT ?? "3100"}`;

/**
 * Error thrown when the backend responds with a non-2xx status.
 *
 * The backend returns `{ error: { code, message, details? } }`, so callers can
 * present the nested public message while still branching on the HTTP status.
 */
export class ApiError extends Error {
  readonly status: number;

  /** Stores the HTTP status alongside the backend error message. */
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
  const response = await fetch(`${AGENTICS_API_BASE_URL}${path}`, {
    cache: "no-store",
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      /** Handles body behavior for this component. */
      const parsed = errorResponseSchema.safeParse(await response.json());
      if (parsed.success) {
        message = parsed.data.error.message;
      }
    } catch {
      // Non-JSON error pages still surface the HTTP status text.
    }
    throw new ApiError(response.status, message);
  }

  return schema.parse(await response.json());
}
