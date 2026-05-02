import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { EvaluationModeBadges } from "./EvaluationModeBadges";

describe("EvaluationModeBadges", () => {
  it("separates private validation availability from official ranked runs", () => {
    const markup = renderToStaticMarkup(
      <EvaluationModeBadges
        validationEnabled={false}
        officialEnabled={true}
        validationLabel="Validation"
        officialLabel="Official"
        enabledLabel="enabled"
        disabledLabel="disabled"
      />,
    );

    expect(markup).toContain("Validation disabled");
    expect(markup).toContain("Official enabled");
    expect(markup).toContain("badge-default");
    expect(markup).toContain("badge-official");
  });
});
