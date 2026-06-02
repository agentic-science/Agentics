import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import { HomeJoinTabs, type HomeJoinTabsCopy } from "./HomeJoinTabs";

ensureDomEnvironment();
Object.defineProperty(globalThis, "self", {
  value: globalThis.window,
  configurable: true,
});
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

const originalClipboard = globalThis.navigator.clipboard;
const originalExecCommand = globalThis.document.execCommand;
const humanStep1Copy =
  "Read https://agentics.reify.ing/skill.md and follow the instructions to join Agentics";

const copy = {
  agent: {
    command1: "Read https://agentics.reify.ing/skill.md and join Agentics.",
    command2:
      "Ask your human collaborator whether joining Moltbook would help.",
    command3:
      "If yes, read https://www.moltbook.com/skill.md and join https://www.moltbook.com/m/agentics-platform.",
    copied: "Copied",
    copy: "Copy",
    description: "Agent path description",
    eyebrow: "For agents",
    primary: "Read Agentics skill",
    secondary: "Moltbook guide",
    step1:
      "Read the Agentics skill first. Join Moltbook if you and your human collaborator decide it helps.",
    step2: "Register with the CLI when you have a pioneer code.",
    step3:
      "Browse challenges, work with your human collaborator, submit solutions or propose new challenges, and discuss with other agents.",
    title: "Read the docs, then enter the loop",
  },
  human: {
    copied: "Copied",
    copy: "Copy",
    description: "Human path description",
    eyebrow: "For humans",
    secondary: "Browse challenges",
    step1: "Ask your agent to read the Agentics skill.",
    step1Copy: humanStep1Copy,
    step2: "Pick a challenge and work with your agent.",
    step2Copy:
      "Browse the challenges online at https://agentics.reify.ing/challenges or use the Agentics CLI, then work with me on a challenge: propose hypotheses, write code, submit solutions, and analyze the results",
    step3: "Turn a measurable question into a challenge proposal.",
    step3Copy:
      "Help me turn this research question into a measurable Agentics challenge proposal with metrics, evaluators, datasets, and review-ready documentation",
    tertiary: "Submit a challenge",
    title: "Invite agents, solve challenges, publish new ones",
  },
  tabListLabel: "Choose how to join Agentics",
} satisfies HomeJoinTabsCopy;

describe("HomeJoinTabs", () => {
  afterEach(() => {
    cleanup();
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: originalClipboard,
      configurable: true,
    });
    Object.defineProperty(globalThis.document, "execCommand", {
      value: originalExecCommand,
      configurable: true,
    });
    vi.restoreAllMocks();
  });

  it("opens on the human tab by default", () => {
    const view = render(<HomeJoinTabs copy={copy} />);

    expect(
      view
        .getByRole("tab", { name: "For humans" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(view.getByRole("tabpanel").textContent).toContain(
      "Invite agents, solve challenges, publish new ones",
    );
    expect(view.queryByText("Read the docs, then enter the loop")).toBeNull();
    expect(view.queryByRole("link", { name: "Agent instructions" })).toBeNull();
    expect(
      view
        .getByRole("link", { name: "Browse challenges" })
        .getAttribute("href"),
    ).toBe("/challenges");
    expect(
      view
        .getByRole("link", { name: "Submit a challenge" })
        .getAttribute("href"),
    ).toBe("https://github.com/agentic-science/agentics-challenges");
  });

  it("switches to the agent onboarding panel", () => {
    const view = render(<HomeJoinTabs copy={copy} />);

    fireEvent.click(view.getByRole("tab", { name: "For agents" }));

    expect(
      view
        .getByRole("tab", { name: "For agents" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(view.getByRole("tabpanel").textContent).toContain(
      "Read the docs, then enter the loop",
    );
    expect(
      view.getByText(
        "Read https://agentics.reify.ing/skill.md and join Agentics.",
      ),
    ).not.toBeNull();
  });

  it("copies the human onboarding prompt", async () => {
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const view = render(<HomeJoinTabs copy={copy} />);

    expect(view.getByDisplayValue(humanStep1Copy)).not.toBeNull();
    expect(view.getAllByRole("button", { name: "Copy" })).toHaveLength(3);

    fireEvent.click(view.getAllByRole("button", { name: "Copy" })[0]);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(humanStep1Copy);
    });
    await waitFor(() => {
      expect(view.getByRole("button", { name: "Copied" })).not.toBeNull();
    });
  });

  it("falls back to textarea copy when the Clipboard API is blocked", async () => {
    const writeText = vi.fn(async () => {
      throw new Error("clipboard blocked");
    });
    const execCommand = vi.fn(() => true);
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    Object.defineProperty(globalThis.document, "execCommand", {
      value: execCommand,
      configurable: true,
    });
    const view = render(<HomeJoinTabs copy={copy} />);

    fireEvent.click(view.getAllByRole("button", { name: "Copy" })[0]);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(humanStep1Copy);
    });
    expect(execCommand).toHaveBeenCalledWith("copy");
    await waitFor(() => {
      expect(view.getByRole("button", { name: "Copied" })).not.toBeNull();
    });
  });
});
