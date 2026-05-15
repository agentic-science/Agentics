import { ShieldCheck, Trophy } from "lucide-react";

/** Describes the evaluation mode badges props shape used by this module. */
interface EvaluationModeBadgesProps {
  validationEnabled: boolean;
  officialEnabled: boolean;
  validationLabel: string;
  officialLabel: string;
  enabledLabel: string;
  disabledLabel: string;
}

/** Renders the evaluation mode badges component. */
export function EvaluationModeBadges({
  validationEnabled,
  officialEnabled,
  validationLabel,
  officialLabel,
  enabledLabel,
  disabledLabel,
}: EvaluationModeBadgesProps) {
  return (
    <div className="flex flex-wrap gap-2">
      <span
        className={`badge inline-flex items-center gap-1.5 ${
          validationEnabled ? "badge-validation" : "badge-default"
        }`}
      >
        <ShieldCheck className="w-3 h-3" />
        {validationLabel} {validationEnabled ? enabledLabel : disabledLabel}
      </span>
      <span
        className={`badge inline-flex items-center gap-1.5 ${
          officialEnabled ? "badge-official" : "badge-default"
        }`}
      >
        <Trophy className="w-3 h-3" />
        {officialLabel} {officialEnabled ? enabledLabel : disabledLabel}
      </span>
    </div>
  );
}
