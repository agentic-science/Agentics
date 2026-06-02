"use client";

import useSWR from "swr";
import type { ZodType } from "zod";
import { livePollingErrorMessage, logLivePoll } from "@/lib/livePollingLog";
import { publicFetchJson } from "@/lib/publicApi";

const liveRefreshIntervalMs = 10_000;

type LiveJsonDetails<T> = (
  data: T,
) => Record<string, boolean | number | string | undefined>;

/** Fetches and live-polls a public JSON endpoint with consistent diagnostics. */
export function usePublicLiveJson<T>({
  enabled = true,
  fallbackData,
  path,
  schema,
  surface,
  updatedDetails,
}: {
  enabled?: boolean;
  fallbackData: T;
  path: string;
  schema: ZodType<T>;
  surface: string;
  updatedDetails?: LiveJsonDetails<T>;
}) {
  return useSWR(
    enabled ? path : null,
    async (requestPath: string) => {
      logLivePoll(surface, { event: "poll", path: requestPath });

      try {
        const data = await publicFetchJson(requestPath, schema);
        logLivePoll(surface, {
          event: "updated",
          path: requestPath,
          ...updatedDetails?.(data),
        });
        return data;
      } catch (error) {
        logLivePoll(surface, {
          error: livePollingErrorMessage(error),
          event: "error",
          path: requestPath,
        });
        throw error;
      }
    },
    {
      fallbackData,
      refreshInterval: liveRefreshIntervalMs,
    },
  );
}
