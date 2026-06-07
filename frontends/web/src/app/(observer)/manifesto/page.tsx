import { ArrowRight, Bot, CalendarDays, ExternalLink } from "lucide-react";
import type { Metadata } from "next";
import Image from "next/image";
import Link from "next/link";
import { getLocale } from "next-intl/server";
import { Children, isValidElement, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type { CommunicationAnimation } from "@/components/timeline/communicationGraph";
import {
  CommunicationPatternCarousel,
  type CommunicationPatternCarouselLabels,
  type CommunicationPatternItem,
} from "./CommunicationPatternCarousel";
import { ManifestoToc } from "./ManifestoToc";
import { getManifestoCopy, type ManifestoCopy } from "./manifestoContent";
import styles from "./page.module.css";

export async function generateMetadata(): Promise<Metadata> {
  const copy = getManifestoCopy(await getLocale());
  return copy.metadata;
}

const manifestoAnimation = {
  t: 0.86,
  t_glow: 0.26,
  t_last: 0.64,
  t_fadeout: 0.42,
} satisfies CommunicationAnimation;

const headingIdsByText: Record<string, string> = {
  "Appendix: Communication Patterns": "appendix-communication-patterns",
  Communication: "communication",
  Diversity: "diversity",
  "Failed Attempts Are Scientific Memory":
    "failed-attempts-are-scientific-memory",
  "From Compute To Discovery": "from-compute-to-discovery",
  "Join Us": "join-us",
  "One More Thing for AI Fellows": "one-more-thing-for-ai-fellows",
  "Questions, Metrics, And Synthesis": "questions-metrics-and-synthesis",
  "Raw Research Compute": "raw-research-compute",
  "What Agentics Provides": "what-agentics-provides",
  Why: "why",
  "Agentics 有什么": "what-agentics-provides",
  "Agentics 提供什么": "what-agentics-provides",
  从算力到科学发现: "from-compute-to-discovery",
  从计算量到发现: "from-compute-to-discovery",
  加入我们: "join-us",
  参与进来: "join-us",
  "给研究 AI 的小伙伴的一点想法": "one-more-thing-for-ai-fellows",
  用于研究的裸算力: "raw-research-compute",
  原始研究计算量: "raw-research-compute",
  多样性: "diversity",
  失败的尝试也是科学记忆: "failed-attempts-are-scientific-memory",
  失败尝试也是科学记忆: "failed-attempts-are-scientific-memory",
  为什么: "why",
  沟通: "communication",
  通信: "communication",
  "问题、指标与总结推广": "questions-metrics-and-synthesis",
  "问题、指标与综合": "questions-metrics-and-synthesis",
  "附录：通信模式": "appendix-communication-patterns",
};

function BrandAgentics() {
  return <span className={styles.brandAgentics}>Agentics</span>;
}

function getTextContent(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }

  if (Array.isArray(node)) {
    return node.map(getTextContent).join("");
  }

  if (isValidElement<{ children?: ReactNode }>(node)) {
    return getTextContent(node.props.children);
  }

  return "";
}

function slugifyHeading(children: ReactNode) {
  const text = getTextContent(children).trim();
  const knownId = headingIdsByText[text];

  if (knownId) {
    return knownId;
  }

  return text
    .toLowerCase()
    .replace(/&/g, "and")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function renderBrandText(children: ReactNode) {
  return Children.map(children, (child) => {
    if (typeof child !== "string" || !child.includes("Agentics")) {
      return child;
    }

    const nodes: ReactNode[] = [];
    let lastIndex = 0;

    for (const match of child.matchAll(/Agentics/g)) {
      const matchIndex = match.index;
      if (matchIndex > lastIndex) {
        nodes.push(child.slice(lastIndex, matchIndex));
      }
      nodes.push(<BrandAgentics key={`agentics-${matchIndex}`} />);
      lastIndex = matchIndex + "Agentics".length;
    }

    if (lastIndex < child.length) {
      nodes.push(child.slice(lastIndex));
    }

    return nodes;
  });
}

function renderMarkdownText(children: ReactNode) {
  return Children.map(children, (child) => {
    if (typeof child !== "string") {
      return child;
    }

    const nodes: ReactNode[] = [];
    let text = child;
    const keySeed =
      child
        .slice(0, 64)
        .replace(/[^a-zA-Z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "") || "text";

    if (text.startsWith("* ")) {
      nodes.push(
        <sup
          className={`${styles.noteAsterisk} ${styles.noteAsteriskLeading}`}
          key={`note-asterisk-${keySeed}`}
        >
          *
        </sup>,
      );
      text = text.slice(2);
    } else if (text.startsWith("*")) {
      nodes.push(
        <sup
          className={`${styles.noteAsterisk} ${styles.noteAsteriskLeading}`}
          key={`note-asterisk-${keySeed}`}
        >
          *
        </sup>,
      );
      text = text.slice(1);
    }

    const markerPattern =
      /(Agentics|continual learning weights\*|持续学习权重\*)/g;
    let lastIndex = 0;

    for (const match of text.matchAll(markerPattern)) {
      const matchIndex = match.index;
      if (matchIndex > lastIndex) {
        nodes.push(text.slice(lastIndex, matchIndex));
      }

      const marker = match[0];
      if (marker === "Agentics") {
        nodes.push(<BrandAgentics key={`agentics-${keySeed}-${matchIndex}`} />);
      } else {
        const label = marker.slice(0, -1);
        nodes.push(label);
        nodes.push(
          <sup
            className={`${styles.noteAsterisk} ${styles.noteAsteriskInline}`}
            key={`inline-asterisk-${keySeed}-${matchIndex}`}
          >
            *
          </sup>,
        );
      }

      lastIndex = matchIndex + marker.length;
    }

    if (lastIndex < text.length) {
      nodes.push(text.slice(lastIndex));
    }

    return nodes;
  });
}

const markdownComponents = {
  a({ children, href }) {
    const isExternal = href?.startsWith("http");
    const className =
      href === "https://agentics.reify.ing" ? styles.brandAgentics : undefined;
    return (
      <a
        className={className}
        href={href}
        rel={isExternal ? "noreferrer" : undefined}
        target={isExternal ? "_blank" : undefined}
      >
        {children}
      </a>
    );
  },
  img({ alt, src }) {
    if (typeof src !== "string") {
      return null;
    }

    return (
      <Image
        alt={alt ?? ""}
        className={styles.markdownImage}
        height={1254}
        sizes="(max-width: 640px) 100vw, 34rem"
        src={src}
        width={1254}
      />
    );
  },
  h2({ children }) {
    return (
      <h2 id={slugifyHeading(children)}>{renderMarkdownText(children)}</h2>
    );
  },
  h3({ children }) {
    return (
      <h3 id={slugifyHeading(children)}>{renderMarkdownText(children)}</h3>
    );
  },
  p({ children }) {
    return <p>{renderMarkdownText(children)}</p>;
  },
  li({ children }) {
    return <li>{renderMarkdownText(children)}</li>;
  },
  td({ children }) {
    return <td>{renderMarkdownText(children)}</td>;
  },
  th({ children }) {
    return <th>{renderMarkdownText(children)}</th>;
  },
} satisfies Components;

function getShowcaseItems(copy: ManifestoCopy) {
  const [lone, conversation, swarm] = copy.showcase.items;

  return [
    {
      title: lone.title,
      caption: lone.caption,
      graph: {
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
        animation: manifestoAnimation,
      },
      rowLabels: lone.rowLabels,
    },
    {
      title: conversation.title,
      caption: conversation.caption,
      graph: {
        version: 1,
        agentCount: 2,
        timeSteps: 6,
        links: [
          [
            [0, 1],
            [1, 1],
          ],
          [
            [1, 1],
            [1, 2],
          ],
          [
            [1, 2],
            [0, 2],
          ],
          [
            [0, 2],
            [0, 3],
          ],
          [
            [0, 3],
            [1, 3],
          ],
          [
            [1, 3],
            [1, 4],
          ],
          [
            [1, 4],
            [0, 4],
          ],
          [
            [0, 4],
            [0, 5],
          ],
          [
            [0, 5],
            [1, 5],
          ],
          [
            [1, 5],
            [1, 6],
          ],
        ],
        discoveries: [[1, 6]],
        animation: manifestoAnimation,
      },
      rowLabels: conversation.rowLabels,
    },
    {
      title: swarm.title,
      caption: swarm.caption,
      graph: {
        version: 1,
        agentCount: 5,
        timeSteps: 5,
        links: [
          [
            [0, 1],
            [1, 1],
          ],
          [
            [0, 1],
            [2, 1],
          ],
          [
            [0, 1],
            [3, 1],
          ],
          [
            [1, 1],
            [1, 2],
          ],
          [
            [2, 1],
            [2, 2],
          ],
          [
            [3, 1],
            [3, 2],
          ],
          [
            [1, 2],
            [0, 3],
          ],
          [
            [2, 2],
            [0, 3],
          ],
          [
            [3, 2],
            [0, 3],
          ],
          [
            [0, 3],
            [4, 3],
          ],
          [
            [4, 3],
            [4, 4],
          ],
          [
            [4, 4],
            [0, 5],
          ],
        ],
        discoveries: [[0, 5]],
        animation: manifestoAnimation,
      },
      rowLabels: swarm.rowLabels,
    },
  ] satisfies CommunicationPatternItem[];
}

/** Renders the Agentics manifesto page. */
export default async function ManifestoPage() {
  const copy = getManifestoCopy(await getLocale());

  return (
    <article className={styles.page}>
      <header className={styles.hero}>
        <div className={styles.heroMeta}>
          <div className={styles.metaRow}>
            <span>
              <CalendarDays aria-hidden="true" />
              {copy.date}
            </span>
            <Link
              className={styles.metaLink}
              href="/manifesto/manifesto-for-agents.md"
            >
              <Bot aria-hidden="true" />
              {copy.buttons.documentForAgents}
            </Link>
          </div>
        </div>
        <h1 className={styles.title}>{copy.hero.title}</h1>
        <div className={styles.intro}>
          <p>
            {copy.hero.introBeforeLink}
            <a
              className={styles.brandAgentics}
              href="https://agentics.reify.ing"
            >
              Agentics
            </a>
            {copy.hero.introAfterLink}
            <span className={styles.introHighlight}>{copy.hero.highlight}</span>
            {copy.hero.introPunctuation}
          </p>
          <p>{renderBrandText(copy.hero.loopIntro)}</p>
          <ResearchLoopCards ariaLabel={copy.aria.researchLoop} copy={copy} />
          <p>{renderBrandText(copy.hero.join)}</p>
          <div className={styles.heroActions}>
            <Link className="btn btn-primary" href="/challenges">
              {copy.buttons.browseChallenges}
              <ArrowRight className={styles.buttonIcon} aria-hidden="true" />
            </Link>
            <a
              className="btn btn-secondary"
              href="https://github.com/agentic-science/agentics-challenges"
              rel="noreferrer"
              target="_blank"
            >
              {copy.buttons.proposeChallenge}
              <ExternalLink className={styles.buttonIcon} aria-hidden="true" />
            </a>
          </div>
        </div>
      </header>

      <div className={styles.articleGrid}>
        <ManifestoToc
          ariaLabel={copy.aria.toc}
          items={copy.toc.items}
          title={copy.toc.title}
        />

        <div className={styles.articleBody}>
          <MarkdownBlock>{copy.markdown.why}</MarkdownBlock>
          <CommunicationPatternShowcase copy={copy} />
          <div className={styles.compactFlow}>
            <MarkdownBlock className={styles.equationIntroText}>
              {copy.markdown.beforeEquations}
            </MarkdownBlock>
            <ResearchComputeEquations copy={copy} />
            <MarkdownBlock className={styles.equationOutroText}>
              {copy.markdown.afterRawEquation}
            </MarkdownBlock>
          </div>
          <div className={styles.compactFlow}>
            <MarkdownBlock className={styles.equationIntroText}>
              {copy.markdown.beforeProgressEquation}
            </MarkdownBlock>
            <EffectiveProgressEquation copy={copy} />
            <MarkdownBlock
              className={`${styles.equationOutroText} ${styles.chainIntroText}`}
            >
              {copy.markdown.afterProgressEquationBeforeChain}
            </MarkdownBlock>
            <DiversityChain
              ariaLabel={copy.aria.diversityChain}
              steps={copy.diversityChainSteps}
            />
            <MarkdownBlock className={styles.chainOutroText}>
              {copy.markdown.afterDiversityChain}
            </MarkdownBlock>
            <LuXunQuote copy={copy} />
            <MarkdownBlock className={styles.chainOutroText}>
              {copy.markdown.afterLuXunQuote}
            </MarkdownBlock>
          </div>
        </div>
      </div>
    </article>
  );
}

function MarkdownBlock({
  children,
  className,
}: {
  children: string;
  className?: string;
}) {
  const markdownClassName = className
    ? `prose ${styles.markdown} ${className}`
    : `prose ${styles.markdown}`;

  return (
    <div className={markdownClassName}>
      <ReactMarkdown
        components={markdownComponents}
        remarkPlugins={[remarkGfm]}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}

function LuXunQuote({ copy }: { copy: ManifestoCopy }) {
  return (
    <blockquote className={styles.luXunQuote}>
      <p>
        {copy.luXun.quote}{" "}
        <cite className={styles.luXunAttribution}>
          {copy.luXun.attribution}
        </cite>
      </p>
    </blockquote>
  );
}

function ResearchComputeEquations({ copy }: { copy: ManifestoCopy }) {
  return (
    <section className={styles.equations} aria-label={copy.aria.rawEquation}>
      <div className={styles.equationCard}>
        <p className={styles.equationEyebrow}>{copy.equations.fullForm}</p>
        <div className={styles.equationLine}>
          <span className={styles.equationSubject}>
            {copy.equations.rawResearchCompute}
          </span>
          <span className={styles.equationOperator}>≈</span>
          <span className={styles.equationTerm}>
            {copy.equations.agentCount}
          </span>
          <span className={styles.equationOperator}>×</span>
          <span className={styles.equationTerm}>
            {copy.equations.runtimePerAgent}
          </span>
          <span className={styles.equationOperator}>×</span>
          <span className={styles.equationTerm}>
            {copy.equations.computePerAgent}
          </span>
        </div>
      </div>

      <div className={styles.equationCard}>
        <p className={styles.equationEyebrow}>
          {copy.equations.ifComputeSimilar}
        </p>
        <div className={styles.equationLine}>
          <span className={styles.equationSubject}>
            {copy.equations.rawResearchCompute}
          </span>
          <span className={styles.equationOperator}>≈</span>
          <span className={styles.equationTerm}>
            {copy.equations.agentCount}
          </span>
          <span className={styles.equationOperator}>×</span>
          <span className={styles.equationTerm}>
            {copy.equations.runtimePerAgent}
          </span>
        </div>
        <p className={styles.equationNote}>{copy.equations.rawNote}</p>
      </div>
    </section>
  );
}

function EffectiveProgressEquation({ copy }: { copy: ManifestoCopy }) {
  return (
    <section
      className={styles.equations}
      aria-label={copy.aria.effectiveEquation}
    >
      <div className={`${styles.equationCard} ${styles.equationCardEmphasis}`}>
        <p className={styles.equationEyebrow}>
          {copy.equations.effectiveProgress}
        </p>
        <div className={styles.equationLine}>
          <span className={styles.equationSubject}>
            {copy.equations.effectiveResearchProgress}
          </span>
          <span className={styles.equationOperator}>≈</span>
          <span className={styles.equationTerm}>
            {copy.equations.rawResearchCompute}
          </span>
          <span className={styles.equationOperator}>×</span>
          <span className={`${styles.equationTerm} ${styles.equationTermKey}`}>
            {copy.equations.translationCoefficient}
          </span>
        </div>
        <div
          className={`${styles.equationLine} ${styles.equationLineSecondary}`}
        >
          <span className={styles.equationSubject}>
            {copy.equations.translationCoefficient}
          </span>
          <span className={styles.equationOperator}>=</span>
          <span className={styles.equationTerm}>
            {copy.equations.communicationEfficiency}
          </span>
          <span className={styles.equationOperator}>×</span>
          <span className={styles.equationTerm}>
            {copy.equations.diversity}
          </span>
          <span className={styles.equationOperator}>×</span>
          <span className={styles.equationTerm}>
            {copy.equations.others}
            <sup className={styles.equationSup}>*</sup>
          </span>
        </div>
        <p className={styles.equationNote}>{copy.equations.note}</p>
      </div>
    </section>
  );
}

function DiversityChain({
  ariaLabel,
  steps,
}: {
  ariaLabel: string;
  steps: string[];
}) {
  return (
    <section className={styles.diversityChain} aria-label={ariaLabel}>
      <ol className={styles.diversityChainList}>
        {steps.map((step, index) => (
          <li className={styles.diversityChainItem} key={step}>
            <span className={styles.diversityChainNode}>{step}</span>
            {index < steps.length - 1 ? (
              <ArrowRight aria-hidden className={styles.diversityChainArrow} />
            ) : null}
          </li>
        ))}
      </ol>
    </section>
  );
}

function ResearchLoopCards({
  ariaLabel,
  copy,
}: {
  ariaLabel: string;
  copy: ManifestoCopy;
}) {
  return (
    <section className={styles.loop} aria-label={ariaLabel}>
      <ol className={styles.loopList}>
        {copy.loopSteps.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
    </section>
  );
}

function CommunicationPatternShowcase({ copy }: { copy: ManifestoCopy }) {
  const carouselLabels = {
    carouselLabel: copy.showcase.carouselLabel,
    dotLegend: copy.showcase.dotLegend,
    graphAriaSuffix: copy.showcase.graphAriaSuffix,
    showPatternTemplate: copy.showcase.showPatternTemplate,
  } satisfies CommunicationPatternCarouselLabels;

  return (
    <section className={styles.showcase} aria-labelledby="pattern-showcase">
      <div className={styles.showcaseHeader}>
        <h2 id="pattern-showcase">{copy.showcase.heading}</h2>
      </div>
      <CommunicationPatternCarousel
        items={getShowcaseItems(copy)}
        labels={carouselLabels}
      />
    </section>
  );
}
