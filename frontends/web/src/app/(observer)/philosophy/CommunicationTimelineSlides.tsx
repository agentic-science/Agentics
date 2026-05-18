"use client";

import { motion, useReducedMotion } from "framer-motion";
import styles from "./page.module.css";

const t = 1.08;
const t_glow = 0.3;
const t_last = 0.7;
const t_fadeout = t / 2;
const t_start = 0.28;
const t_draw = t / 2;
const t_vertical_fade = t / 6;
const stepStartGap = 0.012;

type Point = {
  x: number;
  y: number;
};

type LinkKind = "draw" | "fade";
type LinkWeight = "soft" | "strong";

type TimelineLink = {
  id: string;
  from: Point;
  to: Point;
  kind: LinkKind;
  at: number;
  weight?: LinkWeight;
  opacity?: number;
  discovery?: boolean;
};

type TimelineNode = Point & {
  activeAt?: number;
  discoveryAt?: number;
};

type RowLabel = {
  label: string;
  detail?: string;
  y: number;
};

type TimelineGraphProps = {
  title: string;
  ariaLabel: string;
  viewBox: string;
  xs: number[];
  ys: number[];
  links: TimelineLink[];
  rowLabels?: RowLabel[];
  timeLabelY?: number;
  className?: string;
};

const keyFor = ({ x, y }: Point) => `${x}-${y}`;

const at = (step: number) => t_start + step * t_draw;

function linkArrival(link: TimelineLink) {
  return link.kind === "draw" ? link.at + t_draw : link.at + t_vertical_fade;
}

function deriveNodes(xs: number[], ys: number[], links: TimelineLink[]) {
  const active = new Map<string, number>();
  const discoveries = new Map<string, number>();

  for (const link of links) {
    const fromKey = keyFor(link.from);
    active.set(fromKey, Math.min(active.get(fromKey) ?? link.at, link.at));

    const toKey = keyFor(link.to);
    const arrival = linkArrival(link);
    if (link.discovery) {
      discoveries.set(
        toKey,
        Math.min(discoveries.get(toKey) ?? arrival, arrival),
      );
    } else {
      active.set(toKey, Math.min(active.get(toKey) ?? arrival, arrival));
    }
  }

  return ys.flatMap((y) =>
    xs.map((x) => {
      const key = keyFor({ x, y });
      return {
        x,
        y,
        activeAt: active.get(key),
        discoveryAt: discoveries.get(key),
      };
    }),
  );
}

function createTiming(links: TimelineLink[]) {
  const lastArrival = Math.max(...links.map(linkArrival));
  const fadeOutAt = lastArrival + t_glow + t_last;
  const loop = fadeOutAt + t_fadeout;
  const progress = (seconds: number) => seconds / loop;

  return { fadeOutAt, loop, progress };
}

function visibilityTimes(
  startSeconds: number,
  settleSeconds: number,
  timing: ReturnType<typeof createTiming>,
) {
  const start = timing.progress(startSeconds);
  const justBeforeStart = Math.max(0, start - stepStartGap);

  return [
    0,
    justBeforeStart,
    start,
    timing.progress(startSeconds + settleSeconds),
    timing.progress(timing.fadeOutAt),
    1,
  ];
}

function linkAnimation(
  link: TimelineLink,
  timing: ReturnType<typeof createTiming>,
) {
  const opacity = link.opacity ?? (link.weight === "strong" ? 0.96 : 0.72);

  if (link.kind === "fade") {
    return {
      initial: { opacity: 0 },
      animate: {
        opacity: [0, 0, opacity, opacity, 0],
      },
      times: [
        0,
        Math.max(0, timing.progress(link.at) - stepStartGap),
        timing.progress(link.at + t_vertical_fade),
        timing.progress(timing.fadeOutAt),
        1,
      ],
    };
  }

  return {
    initial: { opacity: 0, pathLength: 0 },
    animate: {
      opacity: [0, 0, opacity, opacity, opacity, 0],
      pathLength: [0, 0, 0, 1, 1, 1],
    },
    times: visibilityTimes(link.at, t_draw, timing),
  };
}

function activeNodeTimes(
  activeAtSeconds: number,
  timing: ReturnType<typeof createTiming>,
) {
  return [
    0,
    Math.max(0, timing.progress(activeAtSeconds) - stepStartGap),
    timing.progress(activeAtSeconds + t_vertical_fade),
    timing.progress(timing.fadeOutAt),
    1,
  ];
}

function MutedDot({ x, y }: Point) {
  return <circle className={styles.nodeMuted} cx={x} cy={y} r="8" />;
}

function ActiveDot({
  x,
  y,
  activeAt,
  timing,
}: TimelineNode & { timing: ReturnType<typeof createTiming> }) {
  const reduceMotion = useReducedMotion();

  if (activeAt === undefined) {
    return <MutedDot x={x} y={y} />;
  }

  const times = activeNodeTimes(activeAt, timing);

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
        transition={
          reduceMotion
            ? undefined
            : {
                duration: timing.loop,
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
        transition={
          reduceMotion
            ? undefined
            : {
                duration: timing.loop,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times,
              }
        }
      />
    </g>
  );
}

function DiscoveryDot({
  x,
  y,
  discoveryAt,
  timing,
}: TimelineNode & { timing: ReturnType<typeof createTiming> }) {
  const reduceMotion = useReducedMotion();

  if (discoveryAt === undefined) {
    return <ActiveDot x={x} y={y} activeAt={undefined} timing={timing} />;
  }

  const times = activeNodeTimes(discoveryAt, timing);

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
                duration: timing.loop,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times,
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
        transition={
          reduceMotion
            ? undefined
            : {
                duration: timing.loop,
                ease: "easeInOut",
                repeat: Number.POSITIVE_INFINITY,
                times,
              }
        }
      />
    </g>
  );
}

function AnimatedLink({
  link,
  timing,
}: {
  link: TimelineLink;
  timing: ReturnType<typeof createTiming>;
}) {
  const reduceMotion = useReducedMotion();
  const { animate, initial, times } = linkAnimation(link, timing);
  const pathClass = `${styles.timelinePath} ${
    link.weight === "strong"
      ? styles.timelinePathStrong
      : styles.timelinePathSoft
  }`;
  const opacity = link.opacity ?? (link.weight === "strong" ? 0.96 : 0.72);

  return (
    <motion.path
      className={pathClass}
      d={`M${link.from.x} ${link.from.y} L${link.to.x} ${link.to.y}`}
      data-communication-link={link.id}
      initial={reduceMotion ? false : initial}
      animate={
        reduceMotion
          ? link.kind === "draw"
            ? { opacity, pathLength: 1 }
            : { opacity }
          : animate
      }
      transition={
        reduceMotion
          ? undefined
          : {
              duration: timing.loop,
              ease: "easeInOut",
              repeat: Number.POSITIVE_INFINITY,
              times,
            }
      }
    />
  );
}

function TimelineGraph({
  title,
  ariaLabel,
  viewBox,
  xs,
  ys,
  links,
  rowLabels,
  timeLabelY,
  className,
}: TimelineGraphProps) {
  const timing = createTiming(links);
  const nodes = deriveNodes(xs, ys, links);

  return (
    <svg className={className} viewBox={viewBox} aria-label={ariaLabel}>
      <title>{title}</title>
      <g>
        {ys.map((y) => (
          <path
            key={`row-${y}`}
            className={styles.gridLine}
            d={`M${xs[0]} ${y} H${xs.at(-1)}`}
          />
        ))}
        {xs.map((x) => (
          <path
            key={`col-${x}`}
            className={styles.gridLine}
            d={`M${x} ${ys[0]} V${ys.at(-1)}`}
          />
        ))}
      </g>
      <g>
        {links.map((link) => (
          <AnimatedLink key={link.id} link={link} timing={timing} />
        ))}
      </g>
      {rowLabels?.map((row) => (
        <g key={row.label}>
          <text
            className={`${styles.agentLabel} ${styles.agentLabelStrong}`}
            x="52"
            y={row.y + 4}
          >
            {row.label}
          </text>
          {row.detail ? (
            <text className={styles.agentLabel} x="72" y={row.y + 4}>
              {row.detail}
            </text>
          ) : null}
        </g>
      ))}
      {timeLabelY
        ? xs.map((x, index) => (
            <text key={x} className={styles.timeLabel} x={x - 8} y={timeLabelY}>
              t{index + 1}
            </text>
          ))
        : null}
      {nodes.map((node) =>
        node.discoveryAt !== undefined ? (
          <DiscoveryDot
            key={keyFor(node)}
            x={node.x}
            y={node.y}
            discoveryAt={node.discoveryAt}
            timing={timing}
          />
        ) : (
          <ActiveDot key={keyFor(node)} {...node} timing={timing} />
        ),
      )}
    </svg>
  );
}

export function SingleAgentTimelineSlide({ title }: { title: string }) {
  const xs = [110, 230, 350, 470, 590];
  const y = 130;
  const links: TimelineLink[] = [
    {
      id: "single-t1-t2",
      from: { x: xs[0], y },
      to: { x: xs[1], y },
      kind: "draw",
      at: at(0),
      weight: "strong",
    },
    {
      id: "single-t2-t3",
      from: { x: xs[1], y },
      to: { x: xs[2], y },
      kind: "draw",
      at: at(1),
      weight: "strong",
    },
    {
      id: "single-t3-t4",
      from: { x: xs[2], y },
      to: { x: xs[3], y },
      kind: "draw",
      at: at(2),
      weight: "strong",
    },
    {
      id: "single-t4-t5",
      from: { x: xs[3], y },
      to: { x: xs[4], y },
      kind: "draw",
      at: at(3),
      weight: "strong",
      discovery: true,
    },
  ];

  return (
    <TimelineGraph
      title={title}
      ariaLabel="A single agent timeline moving from left to right"
      viewBox="0 0 720 260"
      xs={xs}
      ys={[y]}
      links={links}
      timeLabelY={178}
    />
  );
}

export function ThreeAgentTimelineSlide({ title }: { title: string }) {
  const xs = [120, 220, 320, 420, 520, 620];
  const rows = [
    { label: "AI", y: 94 },
    { label: "Human", y: 204 },
    { label: "Human", y: 314 },
  ];
  const [a, b, c] = rows;
  const links: TimelineLink[] = [
    {
      id: "three-b1-b2",
      from: { x: xs[0], y: b.y },
      to: { x: xs[1], y: b.y },
      kind: "draw",
      at: at(0),
    },
    {
      id: "three-a2-b2",
      from: { x: xs[1], y: a.y },
      to: { x: xs[1], y: b.y },
      kind: "fade",
      at: at(1),
    },
    {
      id: "three-b2-b3",
      from: { x: xs[1], y: b.y },
      to: { x: xs[2], y: b.y },
      kind: "draw",
      at: at(1),
      weight: "strong",
    },
    {
      id: "three-b3-c3",
      from: { x: xs[2], y: b.y },
      to: { x: xs[2], y: c.y },
      kind: "fade",
      at: at(2),
    },
    {
      id: "three-b3-a4",
      from: { x: xs[2], y: b.y },
      to: { x: xs[3], y: a.y },
      kind: "draw",
      at: at(2),
      weight: "strong",
    },
    {
      id: "three-b3-b4",
      from: { x: xs[2], y: b.y },
      to: { x: xs[3], y: b.y },
      kind: "draw",
      at: at(2),
    },
    {
      id: "three-c3-c4",
      from: { x: xs[2], y: c.y },
      to: { x: xs[3], y: c.y },
      kind: "draw",
      at: at(2),
    },
    {
      id: "three-b4-b5",
      from: { x: xs[3], y: b.y },
      to: { x: xs[4], y: b.y },
      kind: "draw",
      at: at(3),
    },
    {
      id: "three-c4-c5",
      from: { x: xs[3], y: c.y },
      to: { x: xs[4], y: c.y },
      kind: "draw",
      at: at(3),
    },
    {
      id: "three-b5-a6",
      from: { x: xs[4], y: b.y },
      to: { x: xs[5], y: a.y },
      kind: "draw",
      at: at(4),
      weight: "strong",
      discovery: true,
    },
  ];

  return (
    <TimelineGraph
      title={title}
      ariaLabel="Three agents forming a communication grid"
      viewBox="0 0 720 410"
      xs={xs}
      ys={rows.map((row) => row.y)}
      links={links}
      rowLabels={rows}
      timeLabelY={366}
    />
  );
}

export function SixAgentTimelineSlide({ title }: { title: string }) {
  const xs = [110, 195, 280, 365, 450, 535, 620];
  const ys = [80, 132, 184, 236, 288, 340];
  const links: TimelineLink[] = [
    {
      id: "six-ai1-ai2",
      from: { x: xs[0], y: ys[0] },
      to: { x: xs[1], y: ys[0] },
      kind: "draw",
      at: at(0),
    },
    {
      id: "six-h3-h4",
      from: { x: xs[0], y: ys[2] },
      to: { x: xs[1], y: ys[2] },
      kind: "draw",
      at: at(0),
    },
    {
      id: "six-h6-h6",
      from: { x: xs[0], y: ys[5] },
      to: { x: xs[1], y: ys[5] },
      kind: "draw",
      at: at(0),
    },
    {
      id: "six-h3-h6-sync",
      from: { x: xs[1], y: ys[2] },
      to: { x: xs[1], y: ys[5] },
      kind: "fade",
      at: at(1),
    },
    {
      id: "six-ai2-ai3",
      from: { x: xs[1], y: ys[0] },
      to: { x: xs[2], y: ys[1] },
      kind: "draw",
      at: at(1),
      weight: "strong",
    },
    {
      id: "six-h4-h5",
      from: { x: xs[1], y: ys[2] },
      to: { x: xs[2], y: ys[3] },
      kind: "draw",
      at: at(1),
      weight: "strong",
    },
    {
      id: "six-h6-ai5",
      from: { x: xs[1], y: ys[5] },
      to: { x: xs[2], y: ys[4] },
      kind: "draw",
      at: at(1),
    },
    {
      id: "six-column-three-sync",
      from: { x: xs[2], y: ys[1] },
      to: { x: xs[2], y: ys[4] },
      kind: "fade",
      at: at(2),
    },
    {
      id: "six-ai3-ai4",
      from: { x: xs[2], y: ys[1] },
      to: { x: xs[3], y: ys[0] },
      kind: "draw",
      at: at(2),
    },
    {
      id: "six-h5-h5",
      from: { x: xs[2], y: ys[3] },
      to: { x: xs[3], y: ys[3] },
      kind: "draw",
      at: at(2),
      weight: "strong",
    },
    {
      id: "six-ai5-h6",
      from: { x: xs[2], y: ys[4] },
      to: { x: xs[3], y: ys[5] },
      kind: "draw",
      at: at(2),
    },
    {
      id: "six-column-four-sync",
      from: { x: xs[3], y: ys[0] },
      to: { x: xs[3], y: ys[3] },
      kind: "fade",
      at: at(3),
    },
    {
      id: "six-ai4-ai3",
      from: { x: xs[3], y: ys[0] },
      to: { x: xs[4], y: ys[1] },
      kind: "draw",
      at: at(3),
    },
    {
      id: "six-h5-ai5",
      from: { x: xs[3], y: ys[3] },
      to: { x: xs[4], y: ys[4] },
      kind: "draw",
      at: at(3),
    },
    {
      id: "six-h6-ai5",
      from: { x: xs[3], y: ys[5] },
      to: { x: xs[4], y: ys[4] },
      kind: "draw",
      at: at(3),
    },
    {
      id: "six-ai3-ai2",
      from: { x: xs[4], y: ys[1] },
      to: { x: xs[5], y: ys[0] },
      kind: "draw",
      at: at(4),
      weight: "strong",
    },
    {
      id: "six-ai5-h5",
      from: { x: xs[4], y: ys[4] },
      to: { x: xs[5], y: ys[3] },
      kind: "draw",
      at: at(4),
      weight: "strong",
    },
    {
      id: "six-ai2-frontier",
      from: { x: xs[5], y: ys[0] },
      to: { x: xs[6], y: ys[0] },
      kind: "draw",
      at: at(5),
      discovery: true,
    },
    {
      id: "six-h5-frontier",
      from: { x: xs[5], y: ys[3] },
      to: { x: xs[6], y: ys[2] },
      kind: "draw",
      at: at(5),
      discovery: true,
    },
  ];
  const rowLabels = ys.map((y, index) => ({
    label: index % 2 === 0 ? "AI" : "Human",
    y,
  }));

  return (
    <TimelineGraph
      title={title}
      ariaLabel="Six agents creating a complex interaction pattern"
      viewBox="0 0 720 420"
      xs={xs}
      ys={ys}
      links={links}
      rowLabels={rowLabels}
      timeLabelY={388}
      className={styles.largeGraph}
    />
  );
}
