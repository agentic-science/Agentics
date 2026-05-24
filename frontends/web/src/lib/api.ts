import type { ZodType } from "zod";
import {
  ApiClientError,
  fetchJson as fetchAgenticsJson,
  serverApiBaseUrl,
} from "@/lib/http";

export { ApiClientError as ApiError };

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
  return fetchAgenticsJson(path, schema, { baseUrl: serverApiBaseUrl() });
}
