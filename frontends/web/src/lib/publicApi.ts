import type { ZodType } from "zod";
import { browserApiBaseUrl, fetchJson } from "@/lib/http";

/** Fetches public API JSON from browser-side components. */
export async function publicFetchJson<T>(
  path: string,
  schema: ZodType<T>,
): Promise<T> {
  return fetchJson(path, schema, {
    baseUrl: browserApiBaseUrl(),
  });
}
