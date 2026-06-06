/** Normalizes token labels for duplicate-label checks. */
export function normalizeTokenLabelForDuplicateCheck(label: string): string {
  return label.trim().toLowerCase();
}
