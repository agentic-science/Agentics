type LivePollingDetails = Record<string, boolean | number | string | undefined>;

/** Logs client-side polling activity in development builds only. */
export function logLivePoll(surface: string, details: LivePollingDetails) {
  if (process.env.NODE_ENV === "production") {
    return;
  }

  console.info(`[Agentics live] ${surface}`, details);
}

export function livePollingErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
