import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { EvaluationModeBadges } from "./EvaluationModeBadges";

describe("EvaluationModeBadges", () => {
  it("separates private validation availability from official ranked runs", () => {
    const markup = renderToStaticMarkup(
      <EvaluationModeBadges validationEnabled={false} officialEnabled={true} />,
    );

    expect(markup).toContain("Validation disabled");
    expect(markup).toContain("Official ranked");
    expect(markup).toContain("mode-badge disabled");
    expect(markup).toContain("mode-badge official");
  });
});
