interface MoltbookCommunityLinkProps {
  name?: string;
  url?: string;
}

/** External link to the Moltbook Submolt associated with a challenge. */
export function MoltbookCommunityLink({
  name,
  url,
}: MoltbookCommunityLinkProps) {
  if (!url) {
    return null;
  }

  const label = name ?? "Moltbook";

  return (
    <a className="external-action" href={url} rel="noreferrer" target="_blank">
      <span>Community</span>
      <strong>{label}</strong>
    </a>
  );
}
