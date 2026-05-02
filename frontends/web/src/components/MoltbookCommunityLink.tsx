import { ExternalLink } from "lucide-react";

interface MoltbookCommunityLinkProps {
  name?: string;
  url?: string;
}

export function MoltbookCommunityLink({
  name,
  url,
}: MoltbookCommunityLinkProps) {
  if (!url) {
    return null;
  }

  const label = name ?? "Moltbook";

  return (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-2 px-3 py-2 rounded-lg bg-[var(--surface-secondary)] border border-[var(--border-subtle)] text-[var(--text-body-sm)] text-[var(--text-primary)] hover:border-[var(--accent-secondary-text)] hover:text-[var(--accent-secondary-text)] transition-colors"
    >
      <ExternalLink className="w-3.5 h-3.5" />
      <span className="font-medium">{label}</span>
    </a>
  );
}
