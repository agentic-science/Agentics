import { describe, expect, it } from "vitest";

import {
  type CommunicationGraph,
  defaultCommunicationAnimation,
  deriveCommunicationTimeline,
  validateCommunicationGraph,
} from "./communicationGraph";
import { deriveCommunicationTimelineFrameState } from "./communicationGraphFrame";

function graphWith(
  overrides: Partial<CommunicationGraph> = {},
): CommunicationGraph {
  return {
    version: 1,
    agentCount: 2,
    timeSteps: 3,
    links: [],
    discoveries: [],
    animation: defaultCommunicationAnimation,
    ...overrides,
  };
}

describe("validateCommunicationGraph", () => {
  it("accepts a valid minimal graph", () => {
    const result = validateCommunicationGraph(
      graphWith({
        agentCount: 1,
        timeSteps: 1,
      }),
    );

    expect(result.success).toBe(true);
  });

  it("rejects out-of-range agent indices and timesteps", () => {
    const result = validateCommunicationGraph(
      graphWith({
        agentCount: 1,
        timeSteps: 1,
        links: [
          [
            [1, 1],
            [0, 2],
          ],
        ],
        discoveries: [[0, 2]],
      }),
    );

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.errors.join("\n")).toContain(
        "Agent index must be between 0 and 0.",
      );
      expect(result.errors.join("\n")).toContain(
        "Time step must be between 1 and 1.",
      );
    }
  });

  it("rejects links that go backward in time", () => {
    const result = validateCommunicationGraph(
      graphWith({
        links: [
          [
            [0, 3],
            [1, 2],
          ],
        ],
      }),
    );

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.errors.join("\n")).toContain(
        "Links cannot go backward in time.",
      );
    }
  });

  it("rejects duplicate links and duplicate discoveries", () => {
    const result = validateCommunicationGraph(
      graphWith({
        links: [
          [
            [0, 1],
            [1, 2],
          ],
          [
            [0, 1],
            [1, 2],
          ],
        ],
        discoveries: [
          [1, 3],
          [1, 3],
        ],
      }),
    );

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.errors.join("\n")).toContain(
        "Duplicate links are not allowed.",
      );
      expect(result.errors.join("\n")).toContain(
        "Duplicate discoveries are not allowed.",
      );
    }
  });
});

describe("deriveCommunicationTimeline", () => {
  it("groups concurrent links by identical time intervals", () => {
    const graph = graphWith({
      links: [
        [
          [0, 1],
          [0, 2],
        ],
        [
          [1, 1],
          [1, 2],
        ],
      ],
    });

    const model = deriveCommunicationTimeline(graph);
    const [first, second] = model.links;

    expect(first.groupKey).toBe("1->2");
    expect(second.groupKey).toBe("1->2");
    expect(first.startAt).toBe(0);
    expect(second.startAt).toBe(0);
    expect(first.duration).toBe(graph.animation.t);
    expect(second.duration).toBe(graph.animation.t);
    expect(first.arrivalAt).toBe(graph.animation.t);
    expect(second.arrivalAt).toBe(graph.animation.t);
  });

  it("classifies same-timestep vertical links as fade links", () => {
    const graph = graphWith({
      links: [
        [
          [0, 2],
          [1, 2],
        ],
      ],
    });

    const model = deriveCommunicationTimeline(graph);

    expect(model.links[0].kind).toBe("fade");
    expect(model.links[0].startAt).toBe(graph.animation.t);
    expect(model.links[0].duration).toBeCloseTo(graph.animation.t / 6);
  });
});

describe("deriveCommunicationTimelineFrameState", () => {
  it("grows horizontal and oblique links by their derived timestamp", () => {
    const graph = graphWith({
      links: [
        [
          [0, 1],
          [0, 2],
        ],
      ],
    });
    const model = deriveCommunicationTimeline(graph);

    const midway = deriveCommunicationTimelineFrameState(
      model,
      graph.animation.t / 2,
    );
    const arrived = deriveCommunicationTimelineFrameState(
      model,
      graph.animation.t,
    );

    expect(midway.links[0].pathProgress).toBeGreaterThan(0);
    expect(midway.links[0].pathProgress).toBeLessThan(1);
    expect(midway.links[0].opacity).toBeGreaterThan(0);
    expect(arrived.links[0].pathProgress).toBe(1);
  });

  it("fades same-timestep vertical links as complete segments", () => {
    const graph = graphWith({
      links: [
        [
          [0, 2],
          [1, 2],
        ],
      ],
    });
    const model = deriveCommunicationTimeline(graph);
    const link = model.links[0];

    const before = deriveCommunicationTimelineFrameState(
      model,
      link.startAt - 0.01,
    );
    const during = deriveCommunicationTimelineFrameState(
      model,
      link.startAt + link.duration / 2,
    );

    expect(before.links[0].opacity).toBe(0);
    expect(during.links[0].pathProgress).toBe(1);
    expect(during.links[0].opacity).toBeGreaterThan(0);
  });

  it("keeps discovery nodes muted before activation and amber after activation", () => {
    const graph = graphWith({
      links: [
        [
          [0, 1],
          [1, 2],
        ],
      ],
      discoveries: [[1, 2]],
    });
    const model = deriveCommunicationTimeline(graph);
    const targetNode = (timeSeconds: number) =>
      deriveCommunicationTimelineFrameState(model, timeSeconds).nodes.find(
        (node) => node.node.index[0] === 1 && node.node.index[1] === 2,
      );

    expect(targetNode(graph.animation.t / 2)?.state).toBe("muted");

    const active = targetNode(graph.animation.t + graph.animation.t / 6);

    expect(active?.state).toBe("discovery");
    expect(active?.nodeOpacity).toBe(1);
    expect(active?.haloOpacity).toBeGreaterThan(0);
  });

  it("fades active links and nodes together after the derived fade-out point", () => {
    const graph = graphWith({
      links: [
        [
          [0, 1],
          [0, 2],
        ],
      ],
    });
    const model = deriveCommunicationTimeline(graph);

    const finalFrame = deriveCommunicationTimelineFrameState(model, model.loop);

    expect(finalFrame.links[0].opacity).toBe(0);
    expect(
      finalFrame.nodes
        .filter((node) => node.node.activeAt !== undefined)
        .every((node) => node.nodeOpacity === 0),
    ).toBe(true);
  });
});
