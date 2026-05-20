import { CommunicationTimelineGraph } from "@/components/timeline/CommunicationTimelineGraph";
import {
  type CommunicationGraph,
  defaultCommunicationAnimation,
} from "@/components/timeline/communicationGraph";
import styles from "./page.module.css";

export function SingleAgentTimelineSlide({ title }: { title: string }) {
  const graph: CommunicationGraph = {
    version: 1,
    agentCount: 1,
    timeSteps: 5,
    links: [
      [
        [0, 1],
        [0, 2],
      ],
      [
        [0, 2],
        [0, 3],
      ],
      [
        [0, 3],
        [0, 4],
      ],
      [
        [0, 4],
        [0, 5],
      ],
    ],
    discoveries: [[0, 5]],
    animation: defaultCommunicationAnimation,
  };

  return (
    <CommunicationTimelineGraph
      title={title}
      ariaLabel="A single agent timeline moving from left to right"
      graph={graph}
      layout={{
        width: 720,
        height: 260,
        left: 110,
        right: 590,
        top: 130,
        bottom: 130,
        timeLabelY: 178,
      }}
    />
  );
}

export function ThreeAgentTimelineSlide({ title }: { title: string }) {
  const graph: CommunicationGraph = {
    version: 1,
    agentCount: 3,
    timeSteps: 6,
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
        [1, 3],
        [1, 4],
      ],
      [
        [2, 3],
        [2, 4],
      ],
      [
        [1, 4],
        [1, 5],
      ],
      [
        [2, 4],
        [2, 5],
      ],
      [
        [1, 5],
        [0, 6],
      ],
    ],
    discoveries: [[0, 6]],
    animation: defaultCommunicationAnimation,
  };

  return (
    <CommunicationTimelineGraph
      title={title}
      ariaLabel="Three agents forming a communication grid"
      graph={graph}
      rowLabels={[{ label: "AI" }, { label: "Human" }, { label: "Human" }]}
      layout={{
        width: 720,
        height: 410,
        left: 120,
        right: 620,
        top: 94,
        bottom: 314,
        timeLabelY: 366,
      }}
    />
  );
}

export function SixAgentTimelineSlide({ title }: { title: string }) {
  const graph: CommunicationGraph = {
    version: 1,
    agentCount: 6,
    timeSteps: 7,
    links: [
      [
        [0, 1],
        [0, 2],
      ],
      [
        [2, 1],
        [2, 2],
      ],
      [
        [5, 1],
        [5, 2],
      ],
      [
        [2, 2],
        [5, 2],
      ],
      [
        [0, 2],
        [1, 3],
      ],
      [
        [2, 2],
        [3, 3],
      ],
      [
        [5, 2],
        [4, 3],
      ],
      [
        [1, 3],
        [4, 3],
      ],
      [
        [1, 3],
        [0, 4],
      ],
      [
        [3, 3],
        [3, 4],
      ],
      [
        [4, 3],
        [5, 4],
      ],
      [
        [0, 4],
        [3, 4],
      ],
      [
        [0, 4],
        [1, 5],
      ],
      [
        [3, 4],
        [4, 5],
      ],
      [
        [5, 4],
        [4, 5],
      ],
      [
        [1, 5],
        [0, 6],
      ],
      [
        [4, 5],
        [3, 6],
      ],
      [
        [0, 6],
        [0, 7],
      ],
      [
        [3, 6],
        [2, 7],
      ],
    ],
    discoveries: [
      [0, 7],
      [2, 7],
    ],
    animation: defaultCommunicationAnimation,
  };

  return (
    <CommunicationTimelineGraph
      title={title}
      ariaLabel="Six agents creating a complex interaction pattern"
      graph={graph}
      rowLabels={[
        { label: "AI" },
        { label: "Human" },
        { label: "AI" },
        { label: "Human" },
        { label: "AI" },
        { label: "Human" },
      ]}
      layout={{
        width: 720,
        height: 420,
        left: 110,
        right: 620,
        top: 80,
        bottom: 340,
        timeLabelY: 388,
      }}
      className={styles.largeGraph}
    />
  );
}
