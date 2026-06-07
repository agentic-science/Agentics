import { afterEach, describe, expect, it } from "vitest";
import { cookieContent, privacyContent } from "@/lib/legalContent";
import { ensureDomEnvironment } from "../test/dom";
import { LegalNoticePage } from "./LegalNoticePage";

ensureDomEnvironment();
const { cleanup, render } = await import("@testing-library/react");

describe("LegalNoticePage", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders the English privacy notice", () => {
    const view = render(<LegalNoticePage content={privacyContent.en} />);

    expect(view.getByRole("heading", { name: "Privacy Notice" })).toBeTruthy();
    expect(view.getByText(/Agentic Science/u)).toBeTruthy();
    expect(view.getAllByText(/agentics@reify.ing/u).length).toBeGreaterThan(0);
  });

  it("renders the Chinese cookie notice", () => {
    const view = render(<LegalNoticePage content={cookieContent.zh} />);

    expect(view.getByRole("heading", { name: "Cookie 声明" })).toBeTruthy();
    expect(view.getByText(/严格必要 cookie/u)).toBeTruthy();
    expect(view.getByText("agentics_cookie_consent")).toBeTruthy();
  });
});
