"use client";

import { useReducedMotion } from "framer-motion";
import { useEffect, useState } from "react";
import { CommunicationTimelineGraph } from "@/components/timeline/CommunicationTimelineGraph";
import type { CommunicationGraph } from "@/components/timeline/communicationGraph";
import styles from "./page.module.css";

type RowLabel = {
  detail?: string;
  label: string;
};

export type CommunicationPatternItem = {
  caption: string;
  graph: CommunicationGraph;
  rowLabels: RowLabel[];
  title: string;
};

const slideDurationMs = 8000;

export function CommunicationPatternCarousel({
  items,
}: {
  items: readonly CommunicationPatternItem[];
}) {
  const reduceMotion = useReducedMotion();
  const [activeIndex, setActiveIndex] = useState(0);
  const [autoRotate, setAutoRotate] = useState(true);
  const [cycle, setCycle] = useState(0);

  useEffect(() => {
    if (!autoRotate || reduceMotion || items.length <= 1) {
      return;
    }

    const interval = window.setInterval(() => {
      setActiveIndex((current) => (current + 1) % items.length);
      setCycle((current) => current + 1);
    }, slideDurationMs);

    return () => window.clearInterval(interval);
  }, [autoRotate, items.length, reduceMotion]);

  function selectSlide(index: number) {
    setAutoRotate(false);
    setActiveIndex(index);
    setCycle((current) => current + 1);
  }

  return (
    <section
      className={styles.showcaseCarousel}
      aria-roledescription="carousel"
      aria-label="Communication pattern examples"
    >
      <div className={styles.showcaseTrack}>
        {items.map((item, index) => {
          const position = slidePosition(index, activeIndex, items.length);
          const active = position === "active";

          return (
            <figure
              aria-hidden={!active}
              className={styles.showcaseItem}
              data-showcase-position={position}
              key={item.title}
            >
              {active ? (
                <>
                  <div className={styles.visualCanvas}>
                    <CommunicationTimelineGraph
                      key={`${item.title}-${cycle}`}
                      ariaLabel={`${item.title} communication pattern`}
                      className={styles.showcaseGraph}
                      graph={item.graph}
                      layout={{
                        width: 620,
                        height: Math.max(230, 96 + item.graph.agentCount * 54),
                        left: 122,
                        right: 552,
                        top: 58,
                        bottom:
                          58 + Math.max(0, item.graph.agentCount - 1) * 54,
                        timeLabelY:
                          92 + Math.max(0, item.graph.agentCount - 1) * 54,
                      }}
                      play={!reduceMotion}
                      rowLabels={[...item.rowLabels]}
                      title={item.title}
                    />
                  </div>
                  <figcaption>
                    <strong>{item.title}</strong>
                    <span>{item.caption}</span>
                  </figcaption>
                </>
              ) : null}
            </figure>
          );
        })}
      </div>
      <fieldset className={styles.showcaseDots}>
        <legend className={styles.showcaseDotsLabel}>
          Select communication pattern
        </legend>
        {items.map((item, index) => (
          <button
            aria-current={index === activeIndex ? "true" : undefined}
            aria-label={`Show ${item.title}`}
            className={`${styles.showcaseDot} ${
              index === activeIndex ? styles.showcaseDotActive : ""
            }`}
            key={`${item.title}-dot`}
            onClick={() => selectSlide(index)}
            type="button"
          />
        ))}
      </fieldset>
    </section>
  );
}

function slidePosition(index: number, activeIndex: number, count: number) {
  const delta = (index - activeIndex + count) % count;

  if (delta === 0) {
    return "active";
  }

  if (delta === 1) {
    return "next";
  }

  if (delta === count - 1) {
    return "previous";
  }

  return "hidden";
}
