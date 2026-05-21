import {
  type CommunicationGraph,
  type DerivedTimelineLink,
  type DerivedTimelineModel,
  type DerivedTimelineNode,
  deriveCommunicationTimeline,
  formatCommunicationGraphJson,
  type TimelineLayout,
} from "./communicationGraph";

const jsonFilename = "agentics-communication-graph.json";
const webmFilename = "agentics-communication-graph.webm";
const gifFilename = "agentics-communication-graph.gif";

export const communicationGraphExportDefaults = {
  gifFps: 15,
  height: 720,
  webmFps: 30,
  width: 1280,
};

type ExportTheme = {
  amber: string;
  amberGlow: string;
  backgroundBase: string;
  backgroundSurface: string;
  grid: string;
  link: string;
  muted: string;
  primary: string;
  tealGlow: string;
};

type ExportOptions = {
  filename?: string;
  height?: number;
  layout?: TimelineLayout;
  theme?: ExportTheme;
  width?: number;
};

type TimedExportOptions = ExportOptions & {
  fps?: number;
};

export type CommunicationTimelineFrameLinkState = {
  link: DerivedTimelineLink;
  opacity: number;
  pathProgress: number;
};

export type CommunicationTimelineFrameNodeState = {
  haloOpacity: number;
  node: DerivedTimelineNode;
  nodeOpacity: number;
  state: "active" | "discovery" | "muted";
};

export type CommunicationTimelineFrameState = {
  links: CommunicationTimelineFrameLinkState[];
  model: DerivedTimelineModel;
  nodes: CommunicationTimelineFrameNodeState[];
  timeSeconds: number;
};

type RenderFrameOptions = ExportOptions & {
  graph: CommunicationGraph;
  timeSeconds: number;
};

/** Exports the current graph JSON with the stable editor filename. */
export function exportCommunicationGraphJson(graph: CommunicationGraph) {
  downloadBlob(
    new Blob([formatCommunicationGraphJson(graph)], {
      type: "application/json",
    }),
    jsonFilename,
  );
}

/** Records deterministic canvas frames to a WebM file in real time. */
export async function exportCommunicationGraphWebm(
  graph: CommunicationGraph,
  options: TimedExportOptions = {},
) {
  if (typeof document === "undefined") {
    throw new Error("WebM export is only available in a browser.");
  }
  if (typeof MediaRecorder === "undefined") {
    throw new Error("This browser does not support WebM recording.");
  }

  const fps = options.fps ?? communicationGraphExportDefaults.webmFps;
  const { canvas, context, model, renderFrame } = createExportCanvas(
    graph,
    options,
  );
  const stream = canvas.captureStream(fps);
  const mimeType = selectWebmMimeType();
  const recorder = new MediaRecorder(
    stream,
    mimeType ? { mimeType } : undefined,
  );
  const chunks: BlobPart[] = [];
  const stopped = new Promise<Blob>((resolve, reject) => {
    recorder.ondataavailable = (event) => {
      if (event.data.size > 0) {
        chunks.push(event.data);
      }
    };
    recorder.onerror = () => {
      reject(new Error("WebM recording failed."));
    };
    recorder.onstop = () => {
      resolve(
        new Blob(chunks, {
          type: recorder.mimeType || mimeType || "video/webm",
        }),
      );
    };
  });

  try {
    recorder.start();
    await renderRealtimeFrames(model.loop, fps, renderFrame);
    recorder.stop();
    const blob = await stopped;
    downloadBlob(blob, options.filename ?? webmFilename);
  } finally {
    for (const track of stream.getTracks()) {
      track.stop();
    }
    context.clearRect(0, 0, canvas.width, canvas.height);
  }
}

/** Encodes deterministic canvas frames to a GIF without waiting in real time. */
export async function exportCommunicationGraphGif(
  graph: CommunicationGraph,
  options: TimedExportOptions = {},
) {
  const { GIFEncoder, applyPalette, quantize } = await import("gifenc");
  const fps = options.fps ?? communicationGraphExportDefaults.gifFps;
  const { canvas, context, model, renderFrame } = createExportCanvas(
    graph,
    options,
  );
  const frameCount = frameCountFor(model.loop, fps);
  const delay = (model.loop * 1000) / frameCount;
  const gif = GIFEncoder();

  for (let frame = 0; frame < frameCount; frame += 1) {
    renderFrame(timeForFrame(frame, frameCount, model.loop));
    const image = context.getImageData(0, 0, canvas.width, canvas.height);
    const palette = quantize(image.data, 256);
    const index = applyPalette(image.data, palette);
    gif.writeFrame(index, canvas.width, canvas.height, {
      delay,
      palette,
      repeat: 0,
    });
  }

  gif.finish();
  const bytes = gif.bytes();
  const output = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(output).set(bytes);
  downloadBlob(
    new Blob([output], { type: "image/gif" }),
    options.filename ?? gifFilename,
  );
}

/** Renders one deterministic canvas frame for the given graph time. */
export function renderCommunicationTimelineFrame(
  context: CanvasRenderingContext2D,
  options: RenderFrameOptions,
) {
  const width = options.width ?? communicationGraphExportDefaults.width;
  const height = options.height ?? communicationGraphExportDefaults.height;
  const layout =
    options.layout ?? defaultExportLayout(width, height, options.graph);
  const model = deriveCommunicationTimeline(options.graph, layout);
  const state = deriveCommunicationTimelineFrameState(
    model,
    options.timeSeconds,
  );
  const theme = options.theme ?? resolveCommunicationTimelineExportTheme();

  context.clearRect(0, 0, width, height);
  drawBackground(context, width, height, theme);
  drawGrid(context, state.model, theme, width, height);
  drawLinks(context, state, theme, width, height);
  drawNodes(context, state, theme, width, height);

  return state;
}

/** Derives link progress, link opacity, and node activation at a timestamp. */
export function deriveCommunicationTimelineFrameState(
  model: DerivedTimelineModel,
  timeSeconds: number,
): CommunicationTimelineFrameState {
  const time = clamp(timeSeconds, 0, model.loop);
  const fadeMultiplier = 1 - progressBetween(time, model.fadeOutAt, model.loop);

  return {
    links: model.links.map((link) => ({
      link,
      opacity: linkOpacity(link, time, fadeMultiplier),
      pathProgress:
        link.kind === "draw"
          ? progressBetween(time, link.startAt, link.arrivalAt)
          : 1,
    })),
    model,
    nodes: model.nodes.map((node) =>
      node.discoveryAt !== undefined
        ? discoveryNodeState(node, model, time, fadeMultiplier)
        : activeNodeState(node, model, time, fadeMultiplier),
    ),
    timeSeconds: time,
  };
}

function createExportCanvas(graph: CommunicationGraph, options: ExportOptions) {
  if (typeof document === "undefined") {
    throw new Error("Animation export is only available in a browser.");
  }

  const width = options.width ?? communicationGraphExportDefaults.width;
  const height = options.height ?? communicationGraphExportDefaults.height;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (context === null) {
    throw new Error("Could not create a canvas rendering context.");
  }

  const layout = options.layout ?? defaultExportLayout(width, height, graph);
  const model = deriveCommunicationTimeline(graph, layout);
  const theme = options.theme ?? resolveCommunicationTimelineExportTheme();

  return {
    canvas,
    context,
    model,
    renderFrame: (timeSeconds: number) => {
      renderCommunicationTimelineFrame(context, {
        graph,
        height,
        layout,
        theme,
        timeSeconds,
        width,
      });
    },
  };
}

function defaultExportLayout(
  width: number,
  height: number,
  _graph: CommunicationGraph,
): TimelineLayout {
  return {
    bottom: height * 0.7,
    height,
    left: width * 0.18,
    right: width * 0.86,
    timeLabelY: height * 0.84,
    top: height * 0.24,
    width,
  };
}

function resolveCommunicationTimelineExportTheme(): ExportTheme {
  if (typeof document === "undefined") {
    return fallbackExportTheme("dark");
  }

  const root = document.documentElement;
  const styles = window.getComputedStyle(root);
  const mode = root.dataset.theme === "light" ? "light" : "dark";
  const fallback = fallbackExportTheme(mode);
  const cssValue = (name: string, fallbackValue: string) =>
    styles.getPropertyValue(name).trim() || fallbackValue;

  return {
    amber: cssValue("--accent-primary-500", fallback.amber),
    amberGlow: fallback.amberGlow,
    backgroundBase: cssValue("--bg-base", fallback.backgroundBase),
    backgroundSurface: cssValue(
      "--surface-secondary",
      fallback.backgroundSurface,
    ),
    grid: cssValue("--border-medium", fallback.grid),
    link: cssValue("--accent-secondary-text", fallback.link),
    muted: cssValue("--text-muted", fallback.muted),
    primary: cssValue("--text-primary", fallback.primary),
    tealGlow: fallback.tealGlow,
  };
}

function fallbackExportTheme(mode: "dark" | "light"): ExportTheme {
  if (mode === "light") {
    return {
      amber: "#f59e0b",
      amberGlow: "rgba(245, 158, 11, 0.09)",
      backgroundBase: "#f8fafc",
      backgroundSurface: "rgba(241, 245, 249, 0.8)",
      grid: "rgba(15, 23, 42, 0.12)",
      link: "#0f766e",
      muted: "#64748b",
      primary: "#0f172a",
      tealGlow: "rgba(45, 212, 191, 0.08)",
    };
  }

  return {
    amber: "#f59e0b",
    amberGlow: "rgba(245, 158, 11, 0.09)",
    backgroundBase: "#020617",
    backgroundSurface: "rgba(255, 255, 255, 0.02)",
    grid: "rgba(255, 255, 255, 0.1)",
    link: "#2dd4bf",
    muted: "#94a3b8",
    primary: "#f8fafc",
    tealGlow: "rgba(45, 212, 191, 0.08)",
  };
}

function drawBackground(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  theme: ExportTheme,
) {
  context.fillStyle = theme.backgroundBase;
  context.fillRect(0, 0, width, height);
  context.fillStyle = theme.backgroundSurface;
  context.fillRect(0, 0, width, height);

  const amberGlow = context.createRadialGradient(
    width * 0.76,
    height * 0.18,
    0,
    width * 0.76,
    height * 0.18,
    width * 0.28,
  );
  amberGlow.addColorStop(0, theme.amberGlow);
  amberGlow.addColorStop(1, "rgba(245, 158, 11, 0)");
  context.fillStyle = amberGlow;
  context.fillRect(0, 0, width, height);

  const tealGlow = context.createRadialGradient(
    width * 0.18,
    height * 0.84,
    0,
    width * 0.18,
    height * 0.84,
    width * 0.34,
  );
  tealGlow.addColorStop(0, theme.tealGlow);
  tealGlow.addColorStop(1, "rgba(45, 212, 191, 0)");
  context.fillStyle = tealGlow;
  context.fillRect(0, 0, width, height);
}

function drawGrid(
  context: CanvasRenderingContext2D,
  model: DerivedTimelineModel,
  theme: ExportTheme,
  width: number,
  height: number,
) {
  const unit = visualUnit(width, height);

  context.save();
  context.lineWidth = Math.max(1, unit);
  context.strokeStyle = theme.grid;
  context.globalAlpha = 0.55;
  for (const y of model.ys) {
    line(context, model.xs[0], y, model.xs.at(-1) ?? model.xs[0], y);
  }
  for (const x of model.xs) {
    line(context, x, model.ys[0], x, model.ys.at(-1) ?? model.ys[0]);
  }
  context.restore();

  context.save();
  context.fillStyle = theme.muted;
  context.font = `${Math.round(13 * unit)}px ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`;
  context.textBaseline = "middle";
  context.textAlign = "right";
  for (const [index, y] of model.ys.entries()) {
    context.fillText(String(index), model.xs[0] - 56 * unit, y);
  }
  context.textAlign = "center";
  for (const [index, x] of model.xs.entries()) {
    context.fillText(`t${index + 1}`, x, model.timeLabelY);
  }
  context.restore();
}

function drawLinks(
  context: CanvasRenderingContext2D,
  state: CommunicationTimelineFrameState,
  theme: ExportTheme,
  width: number,
  height: number,
) {
  const unit = visualUnit(width, height);

  for (const linkState of state.links) {
    if (linkState.opacity <= 0) {
      continue;
    }

    const { link, pathProgress } = linkState;
    const toX = lerp(link.from.x, link.to.x, pathProgress);
    const toY = lerp(link.from.y, link.to.y, pathProgress);
    context.save();
    context.globalAlpha = linkState.opacity;
    context.strokeStyle = theme.link;
    context.lineCap = "round";
    context.lineJoin = "round";
    context.lineWidth = (link.kind === "draw" ? 7 : 5) * unit;
    line(context, link.from.x, link.from.y, toX, toY);
    context.restore();
  }
}

function drawNodes(
  context: CanvasRenderingContext2D,
  state: CommunicationTimelineFrameState,
  theme: ExportTheme,
  width: number,
  height: number,
) {
  const unit = visualUnit(width, height);

  for (const nodeState of state.nodes) {
    drawCircle(context, nodeState.node.x, nodeState.node.y, 8 * unit, {
      alpha: 0.55,
      fill: theme.muted,
    });

    if (nodeState.nodeOpacity <= 0) {
      continue;
    }

    const isDiscovery = nodeState.state === "discovery";
    const color = isDiscovery ? theme.amber : theme.primary;
    const haloRadius = (isDiscovery ? 34 : 22) * unit;
    drawHalo(
      context,
      nodeState.node.x,
      nodeState.node.y,
      haloRadius,
      color,
      nodeState.haloOpacity,
    );
    drawCircle(context, nodeState.node.x, nodeState.node.y, 12 * unit, {
      alpha: nodeState.nodeOpacity,
      fill: color,
    });
  }
}

function drawHalo(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  radius: number,
  color: string,
  opacity: number,
) {
  const gradient = context.createRadialGradient(x, y, 0, x, y, radius);
  gradient.addColorStop(0, colorWithAlpha(color, opacity));
  gradient.addColorStop(1, colorWithAlpha(color, 0));
  context.save();
  context.fillStyle = gradient;
  context.beginPath();
  context.arc(x, y, radius, 0, Math.PI * 2);
  context.fill();
  context.restore();
}

function drawCircle(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  radius: number,
  options: { alpha: number; fill: string },
) {
  context.save();
  context.globalAlpha = options.alpha;
  context.fillStyle = options.fill;
  context.beginPath();
  context.arc(x, y, radius, 0, Math.PI * 2);
  context.fill();
  context.restore();
}

function line(
  context: CanvasRenderingContext2D,
  fromX: number,
  fromY: number,
  toX: number,
  toY: number,
) {
  context.beginPath();
  context.moveTo(fromX, fromY);
  context.lineTo(toX, toY);
  context.stroke();
}

function activeNodeState(
  node: DerivedTimelineNode,
  model: DerivedTimelineModel,
  time: number,
  fadeMultiplier: number,
): CommunicationTimelineFrameNodeState {
  if (node.activeAt === undefined) {
    return {
      haloOpacity: 0,
      node,
      nodeOpacity: 0,
      state: "muted",
    };
  }

  const opacity =
    progressBetween(
      time,
      node.activeAt,
      node.activeAt + model.animation.t / 6,
    ) * fadeMultiplier;

  return {
    haloOpacity: 0.08 * opacity,
    node,
    nodeOpacity: opacity,
    state: opacity > 0 ? "active" : "muted",
  };
}

function discoveryNodeState(
  node: DerivedTimelineNode,
  model: DerivedTimelineModel,
  time: number,
  fadeMultiplier: number,
): CommunicationTimelineFrameNodeState {
  if (node.discoveryAt === undefined) {
    return {
      haloOpacity: 0,
      node,
      nodeOpacity: 0,
      state: "muted",
    };
  }

  const opacity =
    progressBetween(
      time,
      node.discoveryAt,
      node.discoveryAt + model.animation.t / 6,
    ) * fadeMultiplier;

  return {
    haloOpacity: 0.12 * opacity,
    node,
    nodeOpacity: opacity,
    state: opacity > 0 ? "discovery" : "muted",
  };
}

function linkOpacity(
  link: DerivedTimelineLink,
  time: number,
  fadeMultiplier: number,
) {
  const baseOpacity = link.kind === "draw" ? 0.9 : 0.7;
  if (link.kind === "draw") {
    return time >= link.startAt ? baseOpacity * fadeMultiplier : 0;
  }

  return (
    baseOpacity *
    progressBetween(time, link.startAt, link.arrivalAt) *
    fadeMultiplier
  );
}

async function renderRealtimeFrames(
  durationSeconds: number,
  fps: number,
  renderFrame: (timeSeconds: number) => void,
) {
  const frameCount = frameCountFor(durationSeconds, fps);
  const delay = (durationSeconds * 1000) / frameCount;

  for (let frame = 0; frame < frameCount; frame += 1) {
    renderFrame(timeForFrame(frame, frameCount, durationSeconds));
    await wait(delay);
  }
}

function frameCountFor(durationSeconds: number, fps: number) {
  return Math.max(1, Math.ceil(durationSeconds * fps));
}

function timeForFrame(
  frame: number,
  frameCount: number,
  durationSeconds: number,
) {
  if (frameCount === 1) {
    return 0;
  }

  return (frame / (frameCount - 1)) * durationSeconds;
}

function selectWebmMimeType() {
  if (MediaRecorder.isTypeSupported?.("video/webm;codecs=vp9") === true) {
    return "video/webm;codecs=vp9";
  }

  return MediaRecorder.isTypeSupported?.("video/webm") === true
    ? "video/webm"
    : "";
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

function visualUnit(width: number, height: number) {
  return clamp(Math.min(width, height) / 420, 0.8, 1.8);
}

function progressBetween(value: number, start: number, end: number) {
  if (end <= start) {
    return value >= end ? 1 : 0;
  }

  return easeInOut(clamp((value - start) / (end - start), 0, 1));
}

function easeInOut(value: number) {
  return value * value * (3 - 2 * value);
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function lerp(from: number, to: number, progress: number) {
  return from + (to - from) * progress;
}

function colorWithAlpha(color: string, alpha: number) {
  const normalized = color.trim();
  if (normalized.startsWith("#")) {
    const hex = normalized.slice(1);
    const values =
      hex.length === 3
        ? hex.split("").map((part) => Number.parseInt(`${part}${part}`, 16))
        : [
            Number.parseInt(hex.slice(0, 2), 16),
            Number.parseInt(hex.slice(2, 4), 16),
            Number.parseInt(hex.slice(4, 6), 16),
          ];

    if (values.every(Number.isFinite)) {
      return `rgba(${values[0]}, ${values[1]}, ${values[2]}, ${alpha})`;
    }
  }

  const rgb = normalized.match(/^rgba?\(([^)]+)\)$/);
  if (rgb) {
    const values = rgb[1].match(/[\d.]+/g)?.slice(0, 3);
    if (values?.length === 3) {
      return `rgba(${values.join(", ")}, ${alpha})`;
    }
  }

  return normalized;
}

function wait(milliseconds: number) {
  return new Promise((resolve) => {
    window.setTimeout(resolve, milliseconds);
  });
}
