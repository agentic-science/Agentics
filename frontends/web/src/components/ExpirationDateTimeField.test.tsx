import { NextIntlClientProvider } from "next-intl";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { ExpirationDateTimeField } from "./ExpirationDateTimeField";

ensureDomEnvironment();
const { render, waitFor } = await import("@testing-library/react");

describe("ExpirationDateTimeField", () => {
  it("normalizes slash-formatted defaults and updates the local-time preview", async () => {
    const onChange = vi.fn();

    const view = render(
      <NextIntlClientProvider locale="en" messages={messages}>
        <ExpirationDateTimeField
          label="Expires at (UTC)"
          value="2026/06/06 12:30"
          onChange={onChange}
        />
      </NextIntlClientProvider>,
    );

    expect(
      (view.getByLabelText("Expires at (UTC)") as HTMLInputElement).value,
    ).toBe("2026-06-06T12:30");
    expect(
      (view.getByLabelText("Local time") as HTMLInputElement).value,
    ).toContain("2026");
    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith("2026-06-06T12:30"),
    );
  });

  it("syncs browser-autofilled datetime values back into component state", async () => {
    function AutofillHarness() {
      const [value, setValue] = useState("");
      return (
        <NextIntlClientProvider locale="en" messages={messages}>
          <ExpirationDateTimeField
            label="Expires at (UTC)"
            value={value}
            onChange={setValue}
          />
        </NextIntlClientProvider>
      );
    }

    const view = render(<AutofillHarness />);
    const utcInput = view.getByLabelText(
      "Expires at (UTC)",
    ) as HTMLInputElement;
    utcInput.value = "2026-06-06T12:30";

    await waitFor(() =>
      expect(
        (view.getByLabelText("Local time") as HTMLInputElement).value,
      ).toContain("2026"),
    );
  });
});
