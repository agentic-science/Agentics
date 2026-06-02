import { afterEach, describe, expect, it } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import { NavConsoleDropdown } from "./NavConsoleDropdown";

ensureDomEnvironment();
Object.defineProperty(globalThis, "self", {
  value: globalThis.window,
  configurable: true,
});
const { cleanup, fireEvent, render } = await import("@testing-library/react");

const copy = {
  adminPanel: "Admin Panel",
  consoles: "Consoles",
  creatorConsole: "Creator Console",
};

describe("NavConsoleDropdown", () => {
  afterEach(() => {
    cleanup();
  });

  it("opens the console links and closes when the page is clicked", () => {
    const view = render(<NavConsoleDropdown copy={copy} />);
    const trigger = view.getByRole("button", { name: "Consoles" });

    fireEvent.click(trigger);

    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    const creatorConsole = view.getByRole("menuitem", {
      name: "Creator Console",
    });
    const adminPanel = view.getByRole("menuitem", { name: "Admin Panel" });

    expect(creatorConsole.getAttribute("href")).toBe("/creator");
    expect(creatorConsole.getAttribute("target")).toBe("_blank");
    expect(creatorConsole.getAttribute("rel")).toBe("noopener noreferrer");
    expect(adminPanel.getAttribute("href")).toBe("/admin");
    expect(adminPanel.getAttribute("target")).toBe("_blank");
    expect(adminPanel.getAttribute("rel")).toBe("noopener noreferrer");

    fireEvent.pointerDown(document.body);

    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(
      view.queryByRole("menuitem", { name: "Creator Console" }),
    ).toBeNull();
  });

  it("closes with Escape and restores focus to the trigger", () => {
    const view = render(<NavConsoleDropdown copy={copy} />);
    const trigger = view.getByRole("button", { name: "Consoles" });

    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: "Escape" });

    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(trigger);
  });
});
