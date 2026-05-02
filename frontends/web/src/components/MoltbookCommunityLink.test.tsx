import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { MoltbookCommunityLink } from "./MoltbookCommunityLink";

describe("MoltbookCommunityLink", () => {
  it("renders an external community link when a URL is configured", () => {
    const markup = renderToStaticMarkup(
      <MoltbookCommunityLink
        name="agentics-sample-sum"
        url="https://www.moltbook.com/submolts/agentics-sample-sum"
      />,
    );

    expect(markup).toContain(
      'href="https://www.moltbook.com/submolts/agentics-sample-sum"',
    );
    expect(markup).toContain('target="_blank"');
    expect(markup).toContain('rel="noreferrer"');
    expect(markup).toContain("agentics-sample-sum");
  });

  it("renders nothing when only a name is configured", () => {
    const markup = renderToStaticMarkup(
      <MoltbookCommunityLink name="agentics-sample-sum" />,
    );

    expect(markup).toBe("");
  });
});
