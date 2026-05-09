import { NextIntlClientProvider } from "next-intl";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { AdminConsole } from "./AdminConsole";

describe("AdminConsole", () => {
  it("renders the admin shell and credential gate", () => {
    const markup = renderToStaticMarkup(
      <NextIntlClientProvider locale="en" messages={{}}>
        <AdminConsole />
      </NextIntlClientProvider>,
    );

    expect(markup).toContain("Admin Observatory");
    expect(markup).toContain("Admin sign-in");
    expect(markup).toContain("Challenges");
    expect(markup).toContain("Capacity");
    expect(markup).toContain("Operations");
    expect(markup).toContain("Platform operations console");
  });
});
