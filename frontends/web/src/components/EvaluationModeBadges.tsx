interface EvaluationModeBadgesProps {
  validationEnabled: boolean;
  officialEnabled: boolean;
}

/** Compact status badges for challenge evaluation surfaces. */
export function EvaluationModeBadges({
  validationEnabled,
  officialEnabled,
}: EvaluationModeBadgesProps) {
  return (
    <div className="mode-strip">
      <span
        className={`mode-badge ${validationEnabled ? "validation" : "disabled"}`}
      >
        Validation {validationEnabled ? "enabled" : "disabled"}
      </span>
      <span
        className={`mode-badge ${officialEnabled ? "official" : "disabled"}`}
      >
        Official {officialEnabled ? "ranked" : "unavailable"}
      </span>
    </div>
  );
}
