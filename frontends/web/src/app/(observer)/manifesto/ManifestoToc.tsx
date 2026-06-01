"use client";

import { useEffect, useState } from "react";
import type { ManifestoTocItem } from "./manifestoContent";
import styles from "./page.module.css";

function currentSectionId(items: ManifestoTocItem[]) {
  const readingLine = window.scrollY + Math.min(window.innerHeight * 0.35, 260);
  let activeId = items[0]?.id ?? "";

  for (const item of items) {
    const section = document.getElementById(item.id);
    if (!section) {
      continue;
    }

    if (section.offsetTop <= readingLine) {
      activeId = item.id;
    }
  }

  return activeId;
}

function tocLinkClassName(item: ManifestoTocItem, activeId: string) {
  return [
    item.depth === 3 ? styles.tocSubLink : undefined,
    item.id === activeId ? styles.tocActive : undefined,
  ]
    .filter(Boolean)
    .join(" ");
}

/** Renders a scroll-aware table of contents for the Agentics manifesto. */
export function ManifestoToc({
  ariaLabel,
  items,
  title,
}: {
  ariaLabel: string;
  items: ManifestoTocItem[];
  title: string;
}) {
  const [activeId, setActiveId] = useState(items[0]?.id ?? "");

  useEffect(() => {
    let frame = 0;

    const updateActiveSection = () => {
      if (frame !== 0) {
        return;
      }

      frame = window.requestAnimationFrame(() => {
        frame = 0;
        const nextActiveId = currentSectionId(items);
        setActiveId((currentActiveId) =>
          currentActiveId === nextActiveId ? currentActiveId : nextActiveId,
        );
      });
    };

    updateActiveSection();
    window.addEventListener("scroll", updateActiveSection, { passive: true });
    window.addEventListener("resize", updateActiveSection);
    window.addEventListener("hashchange", updateActiveSection);

    return () => {
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
      window.removeEventListener("scroll", updateActiveSection);
      window.removeEventListener("resize", updateActiveSection);
      window.removeEventListener("hashchange", updateActiveSection);
    };
  }, [items]);

  return (
    <aside className={styles.toc} aria-label={ariaLabel}>
      <p>{title}</p>
      {items.map((item) => (
        <a
          aria-current={item.id === activeId ? "location" : undefined}
          className={tocLinkClassName(item, activeId)}
          href={`#${item.id}`}
          key={item.id}
        >
          {item.label}
        </a>
      ))}
    </aside>
  );
}
