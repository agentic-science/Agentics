"use client";

import { motion, useReducedMotion } from "framer-motion";
import styles from "./page.module.css";

// One conceptual timestep. The horizontal draw plus the first oblique draw equals t.
const t = 1.08;
const t_glow = 0.3;
const t_last = 0.7;
const t_fadeout = t / 2;
const t_start = 0.28;
const t_draw = t / 2;
const t_vertical_fade = t / 6;

const fadeOutAtSeconds = t_start + 3 * t_draw + t_glow + t_last;
const loopSeconds = fadeOutAtSeconds + t_fadeout;
const drawSeconds = t_draw;
const verticalFadeSeconds = t_vertical_fade;
const stepStartGap = 0.012;
const firstHopAtSeconds = t_start;
const secondHopAtSeconds = firstHopAtSeconds + drawSeconds;
const thirdHopAtSeconds = secondHopAtSeconds + drawSeconds;
const discoveryAtSeconds = thirdHopAtSeconds + drawSeconds;
const discoveryFadeSeconds = t_glow;

type TimelineNode = {
  x: number;
  y: number;
  activeAt?: number;
  discovery?: boolean;
};

type LinkKind = "draw" | "fade";
type LinkWeight = "soft" | "strong";

type TimelineLink = {
  d: string;
  kind: LinkKind;
  weight: LinkWeight;
  at: number;
  opacity: number;
};

const nodes: TimelineNode[] = [
  { x: 130, y: 135, activeAt: firstHopAtSeconds },
  { x: 250, y: 135, activeAt: secondHopAtSeconds },
  { x: 370, y: 135, activeAt: thirdHopAtSeconds },
  { x: 490, y: 135, discovery: true },
  { x: 130, y: 245, activeAt: firstHopAtSeconds },
  { x: 250, y: 245, activeAt: secondHopAtSeconds },
  { x: 370, y: 245, activeAt: thirdHopAtSeconds },
  { x: 490, y: 245, activeAt: discoveryAtSeconds },
  { x: 130, y: 355 },
  { x: 250, y: 355, activeAt: secondHopAtSeconds },
  { x: 370, y: 355, activeAt: thirdHopAtSeconds },
  { x: 490, y: 355 },
];

const links: TimelineLink[] = [
  {
    d: "M130 135 L250 135",
    kind: "draw",
    weight: "soft",
    at: firstHopAtSeconds,
    opacity: 0.72,
  },
  {
    d: "M130 245 L250 245",
    kind: "draw",
    weight: "soft",
    at: firstHopAtSeconds,
    opacity: 0.72,
  },
  {
    d: "M250 355 L250 245",
    kind: "fade",
    weight: "strong",
    at: secondHopAtSeconds,
    opacity: 0.92,
  },
  {
    d: "M250 245 L370 135",
    kind: "draw",
    weight: "strong",
    at: secondHopAtSeconds,
    opacity: 0.96,
  },
  {
    d: "M370 355 L490 245",
    kind: "draw",
    weight: "strong",
    at: thirdHopAtSeconds,
    opacity: 0.96,
  },
  {
    d: "M370 245 L490 135",
    kind: "draw",
    weight: "soft",
    at: thirdHopAtSeconds,
    opacity: 0.72,
  },
];

const toProgress = (seconds: number) => seconds / loopSeconds;

const visibilityTimes = (startSeconds: number, settleSeconds: number) => {
  const start = toProgress(startSeconds);
  const justBeforeStart = Math.max(0, start - stepStartGap);

  return [
    0,
    justBeforeStart,
    start,
    toProgress(startSeconds + settleSeconds),
    toProgress(fadeOutAtSeconds),
    1,
  ];
};

function timelineAnimation(link: TimelineLink) {
  if (link.kind === "fade") {
    return {
      initial: { opacity: 0 },
      animate: {
        opacity: [0, 0, link.opacity, link.opacity, 0],
      },
      times: [
        0,
        Math.max(0, toProgress(link.at) - stepStartGap),
        toProgress(link.at + verticalFadeSeconds),
        toProgress(fadeOutAtSeconds),
        1,
      ],
    };
  }

  return {
    initial: { opacity: 0, pathLength: 0 },
    animate: {
      opacity: [0, 0, link.opacity, link.opacity, link.opacity, 0],
      pathLength: [0, 0, 0, 1, 1, 1],
    },
    times: visibilityTimes(link.at, drawSeconds),
  };
}

const activeNodeTimes = (activeAtSeconds: number) => [
  0,
  Math.max(0, toProgress(activeAtSeconds) - stepStartGap),
  toProgress(activeAtSeconds + t_vertical_fade),
  toProgress(fadeOutAtSeconds),
  1,
];

function MutedDot({ x, y }: Pick<TimelineNode, "x" | "y">) {
  return <circle className={styles.nodeMuted} cx={x} cy={y} r="8" />;
}

function ActiveDot({ x, y, activeAt }: TimelineNode) {
  const reduceMotion = useReducedMotion();

  if (activeAt === undefined) {
    return <MutedDot x={x} y={y} />;
  }

  const times = activeNodeTimes(activeAt);

  return (
    <g>
      <MutedDot x={x} y={y} />
      <motion.circle
        className={styles.activeNodeHalo}
        cx={x}
        cy={y}
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
        data-active-node={`${x}-${y}-halo`}
        transition={
          reduceMotion
            ? undefined
            : {
                duration: loopSeconds,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times,
              }
        }
      />
      <motion.circle
        className={styles.node}
        cx={x}
        cy={y}
        r="12"
        initial={reduceMotion ? false : { opacity: 0 }}
        animate={reduceMotion ? { opacity: 1 } : { opacity: [0, 0, 1, 1, 0] }}
        data-active-node={`${x}-${y}`}
        transition={
          reduceMotion
            ? undefined
            : {
                duration: loopSeconds,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times,
              }
        }
      />
    </g>
  );
}

function DiscoveryDot({ x, y }: Pick<TimelineNode, "x" | "y">) {
  const reduceMotion = useReducedMotion();
  const discoveryTimes = [
    0,
    Math.max(0, toProgress(discoveryAtSeconds) - stepStartGap),
    toProgress(discoveryAtSeconds + discoveryFadeSeconds),
    toProgress(fadeOutAtSeconds),
    1,
  ];

  return (
    <g>
      <MutedDot x={x} y={y} />
      <motion.circle
        className={styles.discoveryHalo}
        cx={x}
        cy={y}
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
        transition={
          reduceMotion
            ? undefined
            : {
                duration: loopSeconds,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times: discoveryTimes,
              }
        }
      />
      <motion.circle
        className={styles.nodeAmber}
        cx={x}
        cy={y}
        r="12"
        initial={reduceMotion ? false : { opacity: 0 }}
        animate={reduceMotion ? { opacity: 1 } : { opacity: [0, 0, 1, 1, 0] }}
        data-active-node={`${x}-${y}-discovery`}
        transition={
          reduceMotion
            ? undefined
            : {
                duration: loopSeconds,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times: discoveryTimes,
              }
        }
      />
    </g>
  );
}

/** Renders the animated hero timeline graph for the philosophy page. */
export function HeroTimelineGraph() {
  const reduceMotion = useReducedMotion();

  return (
    <svg viewBox="70 70 500 350" aria-hidden="true">
      <g>
        <path
          className={styles.gridLine}
          d="M130 135 H490 M130 245 H490 M130 355 H490"
        />
        <path
          className={styles.gridLine}
          d="M130 135 V355 M250 135 V355 M370 135 V355 M490 135 V355"
        />
      </g>
      <g>
        {links.map((link) => {
          const { animate, initial, times } = timelineAnimation(link);
          const pathClass = `${styles.timelinePath} ${
            link.weight === "strong"
              ? styles.timelinePathStrong
              : styles.timelinePathSoft
          }`;

          return (
            <motion.path
              key={link.d}
              className={pathClass}
              d={link.d}
              initial={reduceMotion ? false : initial}
              animate={
                reduceMotion
                  ? link.kind === "draw"
                    ? { opacity: link.opacity, pathLength: 1 }
                    : { opacity: link.opacity }
                  : animate
              }
              transition={
                reduceMotion
                  ? undefined
                  : {
                      duration: loopSeconds,
                      ease: "easeInOut",
                      repeat: Number.POSITIVE_INFINITY,
                      times,
                    }
              }
            />
          );
        })}
      </g>
      {nodes.map((node) =>
        node.discovery ? (
          <DiscoveryDot key={`${node.x}-${node.y}`} x={node.x} y={node.y} />
        ) : (
          <ActiveDot key={`${node.x}-${node.y}`} {...node} />
        ),
      )}
    </svg>
  );
}
