import { z } from "zod";

const indexedPointSchema = z.tuple([z.number().int(), z.number().int()]);
const graphLinkSchema = z.tuple([indexedPointSchema, indexedPointSchema]);
const positiveFiniteNumberSchema = z
  .number()
  .positive()
  .refine(Number.isFinite, "Must be finite");

export const communicationAnimationSchema = z.object({
  t: positiveFiniteNumberSchema,
  t_glow: positiveFiniteNumberSchema,
  t_last: positiveFiniteNumberSchema,
  t_fadeout: positiveFiniteNumberSchema,
});

const communicationGraphBaseSchema = z.object({
  version: z.literal(1),
  agentCount: z.number().int().min(1),
  timeSteps: z.number().int().min(1),
  links: z.array(graphLinkSchema),
  discoveries: z.array(indexedPointSchema),
  animation: communicationAnimationSchema,
});

export const communicationGraphSchema =
  communicationGraphBaseSchema.superRefine((graph, ctx) => {
    const linkKeys = new Set<string>();
    const discoveryKeys = new Set<string>();

    graph.links.forEach((link, index) => {
      const [from, to] = link;
      const [fromAgent, fromTime] = from;
      const [toAgent, toTime] = to;

      validatePoint(from, graph, ctx, ["links", index, 0]);
      validatePoint(to, graph, ctx, ["links", index, 1]);

      if (toTime < fromTime) {
        ctx.addIssue({
          code: "custom",
          message: "Links cannot go backward in time.",
          path: ["links", index],
        });
      }

      if (fromAgent === toAgent && fromTime === toTime) {
        ctx.addIssue({
          code: "custom",
          message: "Links cannot connect a node to itself.",
          path: ["links", index],
        });
      }

      const linkKey = `${fromAgent}:${fromTime}->${toAgent}:${toTime}`;
      if (linkKeys.has(linkKey)) {
        ctx.addIssue({
          code: "custom",
          message: "Duplicate links are not allowed.",
          path: ["links", index],
        });
      }
      linkKeys.add(linkKey);
    });

    graph.discoveries.forEach((point, index) => {
      validatePoint(point, graph, ctx, ["discoveries", index]);

      const discoveryKey = keyForIndexedPoint(point);
      if (discoveryKeys.has(discoveryKey)) {
        ctx.addIssue({
          code: "custom",
          message: "Duplicate discoveries are not allowed.",
          path: ["discoveries", index],
        });
      }
      discoveryKeys.add(discoveryKey);
    });
  });

export type IndexedPoint = z.infer<typeof indexedPointSchema>;
export type GraphLink = z.infer<typeof graphLinkSchema>;
export type CommunicationAnimation = z.infer<
  typeof communicationAnimationSchema
>;
export type CommunicationGraph = z.infer<typeof communicationGraphSchema>;

export type CommunicationGraphValidation =
  | { data: CommunicationGraph; success: true }
  | { errors: string[]; success: false };

export type TimelineLayout = {
  bottom?: number;
  height?: number;
  left?: number;
  right?: number;
  timeLabelY?: number;
  top?: number;
  width?: number;
};

export type TimelinePoint = {
  x: number;
  y: number;
};

export type DerivedTimelineLink = {
  arrivalAt: number;
  duration: number;
  from: TimelinePoint;
  fromIndex: IndexedPoint;
  groupKey: string;
  id: string;
  kind: "draw" | "fade";
  startAt: number;
  to: TimelinePoint;
  toIndex: IndexedPoint;
};

export type DerivedTimelineNode = TimelinePoint & {
  activeAt?: number;
  discoveryAt?: number;
  index: IndexedPoint;
};

export type DerivedTimelineModel = {
  animation: CommunicationAnimation;
  fadeOutAt: number;
  height: number;
  links: DerivedTimelineLink[];
  loop: number;
  nodes: DerivedTimelineNode[];
  timeLabelY: number;
  width: number;
  xs: number[];
  ys: number[];
};

export const defaultCommunicationAnimation: CommunicationAnimation = {
  t: 1,
  t_glow: 0.3,
  t_last: 0.7,
  t_fadeout: 0.5,
};

export const defaultCommunicationGraph: CommunicationGraph = {
  version: 1,
  agentCount: 3,
  timeSteps: 5,
  links: [
    [
      [1, 1],
      [1, 2],
    ],
    [
      [0, 2],
      [1, 2],
    ],
    [
      [1, 2],
      [1, 3],
    ],
    [
      [1, 3],
      [2, 3],
    ],
    [
      [1, 3],
      [0, 4],
    ],
    [
      [2, 3],
      [2, 4],
    ],
    [
      [2, 4],
      [1, 5],
    ],
  ],
  discoveries: [[1, 5]],
  animation: defaultCommunicationAnimation,
};

/** Validates graph-like JSON and returns user-facing error messages. */
export function validateCommunicationGraph(
  value: unknown,
): CommunicationGraphValidation {
  const parsed = communicationGraphSchema.safeParse(value);
  if (parsed.success) {
    return { success: true, data: parsed.data };
  }

  return {
    success: false,
    errors: parsed.error.issues.map((issue) => {
      const path = issue.path.length > 0 ? `${issue.path.join(".")}: ` : "";
      return `${path}${issue.message}`;
    }),
  };
}

/** Creates a deep copy suitable for state updates. */
export function cloneCommunicationGraph(
  graph: CommunicationGraph,
): CommunicationGraph {
  return {
    ...graph,
    links: graph.links.map(([from, to]) => [[...from], [...to]]),
    discoveries: graph.discoveries.map((point) => [...point]),
    animation: { ...graph.animation },
  };
}

/** Produces a deterministic, pretty JSON representation for the editor. */
export function formatCommunicationGraphJson(graph: CommunicationGraph) {
  return `${JSON.stringify(graph, null, 2)}\n`;
}

/** Derives coordinates, link timing, node timing, and loop timing from a graph. */
export function deriveCommunicationTimeline(
  graph: CommunicationGraph,
  layout: TimelineLayout = {},
): DerivedTimelineModel {
  const width = layout.width ?? 720;
  const height =
    layout.height ??
    Math.max(260, 120 + Math.max(0, graph.agentCount - 1) * 64);
  const left = layout.left ?? 92;
  const right = layout.right ?? width - 72;
  const top = layout.top ?? 72;
  const bottom = layout.bottom ?? height - 72;
  const timeLabelY = layout.timeLabelY ?? height - 28;
  const xs = positions(graph.timeSteps, left, right);
  const ys = positions(graph.agentCount, top, bottom);
  const verticalFadeDuration = graph.animation.t / 6;

  const links = graph.links.map(([fromIndex, toIndex], index) => {
    const [fromAgent, fromTime] = fromIndex;
    const [toAgent, toTime] = toIndex;
    const kind = fromTime === toTime ? "fade" : "draw";
    const startAt = (fromTime - 1) * graph.animation.t;
    const duration =
      kind === "draw"
        ? (toTime - fromTime) * graph.animation.t
        : verticalFadeDuration;
    const arrivalAt = startAt + duration;

    return {
      id: `link-${index}-${fromAgent}-${fromTime}-${toAgent}-${toTime}`,
      from: pointFor(xs, ys, fromIndex),
      to: pointFor(xs, ys, toIndex),
      fromIndex,
      toIndex,
      kind,
      groupKey: `${fromTime}->${toTime}`,
      startAt,
      duration,
      arrivalAt,
    } satisfies DerivedTimelineLink;
  });

  const activeNodes = new Map<string, number>();
  for (const link of links) {
    setEarliest(activeNodes, keyForIndexedPoint(link.fromIndex), link.startAt);
    setEarliest(
      activeNodes,
      keyForIndexedPoint(link.toIndex),
      link.kind === "fade" ? link.startAt : link.arrivalAt,
    );
  }

  const discoveryKeys = new Set(graph.discoveries.map(keyForIndexedPoint));
  const nodes: DerivedTimelineNode[] = [];
  for (let agent = 0; agent < graph.agentCount; agent += 1) {
    for (let step = 1; step <= graph.timeSteps; step += 1) {
      const index: IndexedPoint = [agent, step];
      const key = keyForIndexedPoint(index);
      const activeAt = activeNodes.get(key);
      nodes.push({
        ...pointFor(xs, ys, index),
        index,
        activeAt: discoveryKeys.has(key) ? undefined : activeAt,
        discoveryAt: discoveryKeys.has(key) ? activeAt : undefined,
      });
    }
  }

  const lastArrival = links.reduce(
    (latest, link) => Math.max(latest, link.arrivalAt),
    0,
  );
  const fadeOutAt =
    lastArrival + graph.animation.t_glow + graph.animation.t_last;
  const loop = fadeOutAt + graph.animation.t_fadeout;

  return {
    animation: graph.animation,
    fadeOutAt,
    height,
    links,
    loop,
    nodes,
    timeLabelY,
    width,
    xs,
    ys,
  };
}

function validatePoint(
  [agent, time]: IndexedPoint,
  graph: Pick<CommunicationGraph, "agentCount" | "timeSteps">,
  ctx: z.RefinementCtx,
  path: (number | string)[],
) {
  if (agent < 0 || agent >= graph.agentCount) {
    ctx.addIssue({
      code: "custom",
      message: `Agent index must be between 0 and ${graph.agentCount - 1}.`,
      path: [...path, 0],
    });
  }
  if (time < 1 || time > graph.timeSteps) {
    ctx.addIssue({
      code: "custom",
      message: `Time step must be between 1 and ${graph.timeSteps}.`,
      path: [...path, 1],
    });
  }
}

function keyForIndexedPoint([agent, time]: IndexedPoint) {
  return `${agent}:${time}`;
}

function pointFor(xs: number[], ys: number[], [agent, time]: IndexedPoint) {
  return {
    x: xs[time - 1],
    y: ys[agent],
  };
}

function positions(count: number, start: number, end: number) {
  if (count === 1) {
    return [(start + end) / 2];
  }

  const step = (end - start) / (count - 1);
  return Array.from({ length: count }, (_, index) => start + step * index);
}

function setEarliest(map: Map<string, number>, key: string, value: number) {
  map.set(key, Math.min(map.get(key) ?? value, value));
}
