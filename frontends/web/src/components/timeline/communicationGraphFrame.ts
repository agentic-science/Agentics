import type {
  DerivedTimelineLink,
  DerivedTimelineModel,
  DerivedTimelineNode,
} from "./communicationGraph";

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
