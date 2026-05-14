import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { EvaluationModeBadges } from "./EvaluationModeBadges";

describe("EvaluationModeBadges", () => {
  it.each([
    {
      validationEnabled: false,
      officialEnabled: true,
      validationText: "Validation disabled",
      officialText: "Official enabled",
      validationClass: "badge-default",
      officialClass: "badge-official",
    },
    {
      validationEnabled: true,
      officialEnabled: false,
      validationText: "Validation enabled",
      officialText: "Official disabled",
      validationClass: "badge-validation",
      officialClass: "badge-default",
    },
  ])(
    "renders validation=$validationEnabled and official=$officialEnabled independently",
    ({
      validationEnabled,
      officialEnabled,
      validationText,
      officialText,
      validationClass,
      officialClass,
    }) => {
      const markup = renderToStaticMarkup(
        <EvaluationModeBadges
          validationEnabled={validationEnabled}
          officialEnabled={officialEnabled}
          validationLabel="Validation"
          officialLabel="Official"
          enabledLabel="enabled"
          disabledLabel="disabled"
        />,
      );

      expect(markup).toContain(validationText);
      expect(markup).toContain(officialText);
      expect(markup).toContain(validationClass);
      expect(markup).toContain(officialClass);
    },
  );
});
