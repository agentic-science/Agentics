import type { RenderResult } from "@testing-library/react";
import type { SVGProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../../test/dom";

type MotionSvgProps<Element> = SVGProps<Element> & {
  animate?: unknown;
  initial?: unknown;
  transition?: unknown;
};

vi.mock("framer-motion", () => ({
  motion: {
    circle: ({
      animate: _animate,
      initial: _initial,
      transition: _transition,
      ...props
    }: MotionSvgProps<SVGCircleElement>) => <circle {...props} />,
    path: ({
      animate: _animate,
      initial: _initial,
      transition: _transition,
      ...props
    }: MotionSvgProps<SVGPathElement>) => <path {...props} />,
  },
  useReducedMotion: () => true,
}));

const mediaExportMocks = {
  gif: vi.fn(() => Promise.resolve()),
  webm: vi.fn(() => Promise.resolve()),
};

vi.doMock("./communicationGraphExport", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("./communicationGraphExport")>();

  return {
    ...actual,
    exportCommunicationGraphGif: mediaExportMocks.gif,
    exportCommunicationGraphWebm: mediaExportMocks.webm,
  };
});

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);
const { EasterEditor } = await import("./EasterEditor");

const createObjectUrlMock = vi.fn(
  (_blob: Blob) => "blob:agentics-communication-graph",
);
const revokeObjectUrlMock = vi.fn((_url: string) => undefined);
let anchorClickSpy: ReturnType<typeof vi.spyOn>;

async function exportCurrentGraph(view: RenderResult) {
  fireEvent.click(view.getByRole("button", { name: "Export" }));
  fireEvent.click(view.getByRole("menuitem", { name: "JSON" }));
  await waitFor(() => expect(createObjectUrlMock).toHaveBeenCalled());
  const exportedBlob =
    createObjectUrlMock.mock.calls[
      createObjectUrlMock.mock.calls.length - 1
    ]?.[0];

  expect(exportedBlob).toBeInstanceOf(Blob);
  if (!(exportedBlob instanceof Blob)) {
    throw new Error("Expected export to create a JSON Blob.");
  }

  return JSON.parse(await exportedBlob.text()) as {
    agentCount: number;
    animation: {
      t: number;
      t_fadeout: number;
      t_glow: number;
      t_last: number;
    };
    discoveries: unknown[];
    links: unknown[];
    timeSteps: number;
  };
}

describe("EasterEditor", () => {
  beforeEach(() => {
    createObjectUrlMock.mockClear();
    revokeObjectUrlMock.mockClear();
    mediaExportMocks.gif.mockReset();
    mediaExportMocks.webm.mockReset();
    mediaExportMocks.gif.mockResolvedValue(undefined);
    mediaExportMocks.webm.mockResolvedValue(undefined);
    anchorClickSpy = vi
      .spyOn(window.HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);
    Object.defineProperty(URL, "createObjectURL", {
      value: createObjectUrlMock,
      configurable: true,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      value: revokeObjectUrlMock,
      configurable: true,
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows import validation errors in a dialog without replacing the current graph", async () => {
    const view = render(<EasterEditor />);
    const fileInput =
      view.container.querySelector<HTMLInputElement>('input[type="file"]');

    expect(fileInput).not.toBeNull();
    fireEvent.change(fileInput as HTMLInputElement, {
      target: {
        files: [
          new File(['{"version":1,"agentCount":0}'], "invalid.json", {
            type: "application/json",
          }),
        ],
      },
    });

    expect(await view.findByRole("alertdialog")).toBeTruthy();
    expect(view.getByText("Import failed")).toBeTruthy();
    expect(view.getByText(/Too small: expected number to be >=1/)).toBeTruthy();

    fireEvent.click(view.getByRole("button", { name: "OK" }));

    expect(view.queryByRole("alertdialog")).toBeNull();

    const graph = await exportCurrentGraph(view);
    expect(graph.agentCount).toBe(3);
    expect(graph.timeSteps).toBe(5);
    expect(graph.links).toHaveLength(7);
  });

  it("keeps the editor visual-first without manual JSON or form sections", () => {
    const view = render(<EasterEditor />);

    expect(view.queryByText("Visual editor")).toBeNull();
    expect(view.queryByText(/agents ·/)).toBeNull();
    expect(view.getByRole("button", { name: "Play" })).toBeTruthy();
    expect(view.queryByText("Graph is valid.")).toBeNull();
    expect(view.getByRole("button", { name: "Clear" })).toBeTruthy();
    expect(view.queryByLabelText("Communication graph JSON")).toBeNull();
    expect(view.queryByText("Add link")).toBeNull();
    expect(view.queryByText("Add discovery")).toBeNull();
    expect(view.queryByText("Reset sample")).toBeNull();
    expect(view.queryByRole("button", { name: "Edit" })).toBeNull();
    expect(view.queryByRole("button", { name: "Present" })).toBeNull();
  });

  it("opens the export menu with JSON, WebM, and GIF actions", () => {
    const view = render(<EasterEditor />);

    fireEvent.click(view.getByRole("button", { name: "Export" }));

    expect(view.getByRole("menuitem", { name: "JSON" })).toBeTruthy();
    expect(view.getByRole("menuitem", { name: "WebM" })).toBeTruthy();
    expect(view.getByRole("menuitem", { name: "GIF" })).toBeTruthy();
  });

  it("shows friendly field names and updated animation defaults", () => {
    const view = render(<EasterEditor />);

    expect(view.getByLabelText("Number of Agents")).toBeTruthy();
    expect(view.getByLabelText("Step Duration")).toHaveProperty("value", "1");
    expect(view.getByLabelText("Glow Duration")).toHaveProperty("value", "0.3");
    expect(view.getByLabelText("Glow Hold")).toHaveProperty("value", "0.7");
    expect(view.getByLabelText("Fade-out Duration")).toHaveProperty(
      "value",
      "0.5",
    );
  });

  it("adds links by clicking dots in the visual editor", async () => {
    const view = render(<EasterEditor />);

    const source = view.getByLabelText(/Node agent 0, t1/);
    fireEvent.click(source);
    await waitFor(() =>
      expect(source.getAttribute("aria-pressed")).toBe("true"),
    );

    fireEvent.click(view.getByLabelText(/Node agent 0, t2/));
    await waitFor(() =>
      expect(source.getAttribute("aria-pressed")).toBe("false"),
    );

    const graph = await exportCurrentGraph(view);
    expect(graph.links).toHaveLength(8);
  });

  it("deselects a selected source when empty editor space is clicked", async () => {
    const view = render(<EasterEditor />);
    const source = view.getByLabelText(/Node agent 0, t1/);

    fireEvent.click(source);
    await waitFor(() =>
      expect(source.getAttribute("aria-pressed")).toBe("true"),
    );

    const previewFrame = view.getByLabelText(
      "Interactive communication graph editor",
    ).parentElement;
    expect(previewFrame).not.toBeNull();
    fireEvent.pointerDown(previewFrame as HTMLElement);

    expect(source.getAttribute("aria-pressed")).toBe("false");
  });

  it("toggles discovery dots with double click and right click", async () => {
    const view = render(<EasterEditor />);
    const node = view.getByLabelText(/Node agent 0, t1/);

    fireEvent.doubleClick(node);

    let graph = await exportCurrentGraph(view);
    expect(graph.discoveries).toHaveLength(2);

    fireEvent.contextMenu(node);

    graph = await exportCurrentGraph(view);
    expect(graph.discoveries).toHaveLength(1);
  });

  it("clears links and discoveries without resetting graph dimensions", async () => {
    const view = render(<EasterEditor />);

    fireEvent.click(view.getByRole("button", { name: "Clear" }));

    const graph = await exportCurrentGraph(view);
    expect(graph.agentCount).toBe(3);
    expect(graph.timeSteps).toBe(5);
    expect(graph.links).toHaveLength(0);
    expect(graph.discoveries).toHaveLength(0);
  });

  it("toggles presentation with the single play button", () => {
    const view = render(<EasterEditor />);

    fireEvent.click(view.getByRole("button", { name: "Play" }));

    expect(
      view.getByLabelText("Communication graph presentation"),
    ).toBeTruthy();
    expect(view.getByRole("button", { name: "Stop" })).toBeTruthy();

    fireEvent.click(view.getByRole("button", { name: "Stop" }));

    expect(
      view.getByLabelText("Interactive communication graph editor"),
    ).toBeTruthy();
  });

  it("exports the current valid graph JSON", async () => {
    const view = render(<EasterEditor />);

    fireEvent.click(view.getByRole("button", { name: "Clear" }));
    const graph = await exportCurrentGraph(view);

    expect(graph.links).toHaveLength(0);
    expect(graph.discoveries).toHaveLength(0);
    expect(anchorClickSpy).toHaveBeenCalled();
    expect(revokeObjectUrlMock).toHaveBeenCalledWith(
      "blob:agentics-communication-graph",
    );
  });

  it("exports WebM and GIF through the media helpers", async () => {
    const view = render(<EasterEditor />);

    fireEvent.click(view.getByRole("button", { name: "Export" }));
    fireEvent.click(view.getByRole("menuitem", { name: "WebM" }));

    await waitFor(() => expect(mediaExportMocks.webm).toHaveBeenCalledTimes(1));
    expect(mediaExportMocks.webm).toHaveBeenCalledWith(
      expect.objectContaining({
        agentCount: 3,
        timeSteps: 5,
      }),
    );

    fireEvent.click(view.getByRole("button", { name: "Export" }));
    fireEvent.click(view.getByRole("menuitem", { name: "GIF" }));

    await waitFor(() => expect(mediaExportMocks.gif).toHaveBeenCalledTimes(1));
    expect(mediaExportMocks.gif).toHaveBeenCalledWith(
      expect.objectContaining({
        agentCount: 3,
        timeSteps: 5,
      }),
    );
  });

  it("disables export actions while a media export is running", async () => {
    let resolveWebm: (() => void) | undefined;
    mediaExportMocks.webm.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        resolveWebm = resolve;
      }),
    );
    const view = render(<EasterEditor />);

    fireEvent.click(view.getByRole("button", { name: "Export" }));
    fireEvent.click(view.getByRole("menuitem", { name: "WebM" }));

    const exportButton = view.getByRole("button", { name: "Exporting..." });
    expect(exportButton).toHaveProperty("disabled", true);

    resolveWebm?.();
    await waitFor(() =>
      expect(view.getByRole("button", { name: "Export" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
  });
});
