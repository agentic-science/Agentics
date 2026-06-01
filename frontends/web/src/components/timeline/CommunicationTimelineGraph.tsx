"use client";

import { motion, type Transition, useReducedMotion } from "framer-motion";
import styles from "./CommunicationTimelineGraph.module.css";
import {
  type CommunicationGraph,
  type DerivedTimelineLink,
  type DerivedTimelineModel,
  type DerivedTimelineNode,
  deriveCommunicationTimeline,
  type TimelineLayout,
} from "./communicationGraph";

const stepStartGap = 0.012;

type RowLabel = {
  detail?: string;
  label: string;
};

type CommunicationTimelineGraphProps = {
  ariaLabel?: string;
  className?: string;
  graph: CommunicationGraph;
  layout?: TimelineLayout;
  play?: boolean;
  rowLabels?: RowLabel[];
  showTimeLabels?: boolean;
  title: string;
};

/** Renders an animated communication graph from the minimal graph JSON model. */
export function CommunicationTimelineGraph({
  ariaLabel,
  className,
  graph,
  layout,
  play = true,
  rowLabels,
  showTimeLabels = true,
  title,
}: CommunicationTimelineGraphProps) {
  const model = deriveCommunicationTimeline(graph, layout);

  return (
    <svg
      className={`${styles.graph}${className ? ` ${className}` : ""}`}
      viewBox={`0 0 ${model.width} ${model.height}`}
      aria-label={ariaLabel ?? title}
    >
      <title>{title}</title>
      <g>
        {model.ys.map((y) => (
          <path
            key={`row-${y}`}
            className={styles.gridLine}
            d={`M${model.xs[0]} ${y} H${model.xs.at(-1)}`}
          />
        ))}
        {model.xs.map((x) => (
          <path
            key={`col-${x}`}
            className={styles.gridLine}
            d={`M${x} ${model.ys[0]} V${model.ys.at(-1)}`}
          />
        ))}
      </g>
      <g>
        {model.links.map((link) => (
          <AnimatedLink key={link.id} link={link} model={model} play={play} />
        ))}
      </g>
      {rowLabels?.map((row, index) => {
        const y = model.ys[index];
        return (
          <g key={`${row.label}-${row.detail ?? "label"}-${y}`}>
            <text
              className={`${styles.agentLabel} ${styles.agentLabelStrong}`}
              x="52"
              y={y + 4}
            >
              {row.label}
            </text>
            {row.detail ? (
              <text className={styles.agentLabel} x="72" y={y + 4}>
                {row.detail}
              </text>
            ) : null}
          </g>
        );
      })}
      {showTimeLabels
        ? model.xs.map((x, index) => (
            <text
              key={x}
              className={styles.timeLabel}
              x={x - 8}
              y={model.timeLabelY}
            >
              t{index + 1}
            </text>
          ))
        : null}
      {model.nodes.map((node) =>
        node.discoveryAt !== undefined ? (
          <DiscoveryDot
            key={`${node.index[0]}-${node.index[1]}`}
            model={model}
            node={node}
            play={play}
          />
        ) : (
          <ActiveDot
            key={`${node.index[0]}-${node.index[1]}`}
            model={model}
            node={node}
            play={play}
          />
        ),
      )}
    </svg>
  );
}

function AnimatedLink({
  link,
  model,
  play,
}: {
  link: DerivedTimelineLink;
  model: DerivedTimelineModel;
  play: boolean;
}) {
  const reduceMotion = useReducedMotion();
  const opacity = link.kind === "draw" ? 0.9 : 0.7;
  const pathClass = `${styles.timelinePath} ${
    link.kind === "draw" ? styles.timelinePathDraw : styles.timelinePathFade
  }`;

  if (!play) {
    return (
      <path
        className={pathClass}
        d={`M${link.from.x} ${link.from.y} L${link.to.x} ${link.to.y}`}
        data-communication-link={link.id}
        opacity={opacity * 0.42}
      />
    );
  }

  if (link.kind === "fade") {
    const times = [
      0,
      Math.max(0, progress(link.startAt, model) - stepStartGap),
      progress(link.arrivalAt, model),
      progress(model.fadeOutAt, model),
      1,
    ];

    return (
      <motion.path
        className={pathClass}
        d={`M${link.from.x} ${link.from.y} L${link.to.x} ${link.to.y}`}
        data-communication-link={link.id}
        initial={reduceMotion ? false : { opacity: 0 }}
        animate={
          reduceMotion ? { opacity } : { opacity: [0, 0, opacity, opacity, 0] }
        }
        transition={transition(model, times, reduceMotion)}
      />
    );
  }

  const times = [
    0,
    Math.max(0, progress(link.startAt, model) - stepStartGap),
    progress(link.startAt, model),
    progress(link.arrivalAt, model),
    progress(model.fadeOutAt, model),
    1,
  ];

  return (
    <motion.path
      className={pathClass}
      d={`M${link.from.x} ${link.from.y} L${link.to.x} ${link.to.y}`}
      data-communication-link={link.id}
      initial={reduceMotion ? false : { opacity: 0, pathLength: 0 }}
      animate={
        reduceMotion
          ? { opacity, pathLength: 1 }
          : {
              opacity: [0, 0, opacity, opacity, opacity, 0],
              pathLength: [0, 0, 0, 1, 1, 1],
            }
      }
      transition={transition(model, times, reduceMotion)}
    />
  );
}

function ActiveDot({
  model,
  node,
  play,
}: {
  model: DerivedTimelineModel;
  node: DerivedTimelineNode;
  play: boolean;
}) {
  const reduceMotion = useReducedMotion();

  if (node.activeAt === undefined) {
    return <MutedDot x={node.x} y={node.y} />;
  }

  if (!play) {
    return (
      <g>
        <MutedDot x={node.x} y={node.y} />
        <circle
          className={styles.node}
          cx={node.x}
          cy={node.y}
          opacity="0.28"
          r="8"
        />
      </g>
    );
  }

  const times = nodeTimes(node.activeAt, model);

  return (
    <g>
      <MutedDot x={node.x} y={node.y} />
      <motion.circle
        className={styles.activeNodeHalo}
        cx={node.x}
        cy={node.y}
        r="22"
        initial={reduceMotion ? false : { opacity: 0, scale: 0.9 }}
        animate={
          reduceMotion
            ? { opacity: 0.08, scale: 1 }
            : {
                opacity: [0, 0, 0.08, 0.08, 0],
                scale: [0.9, 0.9, 1, 1, 0.9],
              }
        }
        transition={transition(model, times, reduceMotion)}
      />
      <motion.circle
        className={styles.node}
        cx={node.x}
        cy={node.y}
        r="12"
        initial={reduceMotion ? false : { opacity: 0 }}
        animate={reduceMotion ? { opacity: 1 } : { opacity: [0, 0, 1, 1, 0] }}
        transition={transition(model, times, reduceMotion)}
      />
    </g>
  );
}

function DiscoveryDot({
  model,
  node,
  play,
}: {
  model: DerivedTimelineModel;
  node: DerivedTimelineNode;
  play: boolean;
}) {
  const reduceMotion = useReducedMotion();

  if (node.discoveryAt === undefined) {
    return <MutedDot x={node.x} y={node.y} />;
  }

  if (!play) {
    return (
      <g>
        <MutedDot x={node.x} y={node.y} />
        <circle
          className={styles.nodeAmber}
          cx={node.x}
          cy={node.y}
          opacity="0.42"
          r="8"
        />
      </g>
    );
  }

  const times = nodeTimes(node.discoveryAt, model);

  return (
    <g>
      <MutedDot x={node.x} y={node.y} />
      <motion.circle
        className={styles.discoveryHalo}
        cx={node.x}
        cy={node.y}
        r="34"
        initial={reduceMotion ? false : { opacity: 0, scale: 0.82 }}
        animate={
          reduceMotion
            ? { opacity: 0.12, scale: 1 }
            : {
                opacity: [0, 0, 0.12, 0.12, 0],
                scale: [0.82, 0.82, 1, 1, 0.82],
              }
        }
        transition={transition(model, times, reduceMotion)}
      />
      <motion.circle
        className={styles.nodeAmber}
        cx={node.x}
        cy={node.y}
        r="12"
        initial={reduceMotion ? false : { opacity: 0 }}
        animate={reduceMotion ? { opacity: 1 } : { opacity: [0, 0, 1, 1, 0] }}
        transition={transition(model, times, reduceMotion)}
      />
    </g>
  );
}

function MutedDot({ x, y }: { x: number; y: number }) {
  return <circle className={styles.nodeMuted} cx={x} cy={y} r="8" />;
}

function nodeTimes(activeAt: number, model: DerivedTimelineModel) {
  const settleAt = activeAt + model.animation.t / 6;

  return [
    0,
    Math.max(0, progress(activeAt, model) - stepStartGap),
    progress(settleAt, model),
    progress(model.fadeOutAt, model),
    1,
  ];
}

function progress(seconds: number, model: DerivedTimelineModel) {
  return seconds / model.loop;
}

function transition(
  model: DerivedTimelineModel,
  times: number[],
  reduceMotion: boolean | null,
): Transition | undefined {
  return reduceMotion
    ? undefined
    : {
        duration: model.loop,
        ease: "easeInOut" as const,
        repeat: Number.POSITIVE_INFINITY,
        times,
      };
}
