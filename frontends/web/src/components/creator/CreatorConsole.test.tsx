import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { CreatorConsole } from "./CreatorConsole";

describe("CreatorConsole", () => {
  it("renders the GitHub-backed challenge draft workflow", () => {
    const markup = renderToStaticMarkup(<CreatorConsole />);

    expect(markup).toContain("Creator Observatory");
    expect(markup).toContain("Challenge draft console");
    expect(markup).toContain("Sign in with GitHub");
    expect(markup).toContain("Create draft");
    expect(markup).toContain("Upload private asset");
  });
});
