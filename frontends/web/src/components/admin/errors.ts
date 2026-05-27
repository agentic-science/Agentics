import { AdminApiError } from "@/lib/adminApi";

export type AdminErrorFallback = {
  accessDenied?: string;
  unknown: string;
};

/** Normalizes unknown admin errors into a displayable message. */
export function adminErrorMessage(
  error: unknown,
  fallback: AdminErrorFallback,
): string {
  if (error instanceof AdminApiError) {
    if (error.status === 401 && fallback.accessDenied) {
      return fallback.accessDenied;
    }
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback.unknown;
}
