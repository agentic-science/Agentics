import { getTranslations } from "next-intl/server";
import {
  SingleAgentTimelineSlide,
  SixAgentTimelineSlide,
  ThreeAgentTimelineSlide,
} from "./CommunicationTimelineSlides";
import { HeroTimelineGraph } from "./HeroTimelineGraph";
import styles from "./page.module.css";

/** Renders the Agentics philosophy page. */
export default async function PhilosophyPage() {
  const t = await getTranslations("philosophy");
  const singleTitle = t("communications.slides.single.title");
  const gridTitle = t("communications.slides.grid.title");
  const manyTitle = t("communications.slides.many.title");

  const slides = [
    {
      key: "single",
      index: "01",
      title: singleTitle,
      body: t("communications.slides.single.body"),
      visual: <SingleAgentTimelineSlide title={singleTitle} />,
    },
    {
      key: "grid",
      index: "02",
      title: gridTitle,
      body: t("communications.slides.grid.body"),
      visual: <ThreeAgentTimelineSlide title={gridTitle} />,
    },
    {
      key: "many",
      index: "03",
      title: manyTitle,
      body: t("communications.slides.many.body"),
      visual: <SixAgentTimelineSlide title={manyTitle} />,
    },
  ];

  return (
    <div className={styles.page}>
      <section className={styles.hero}>
        <div className={styles.heroText}>
          <h1 className={styles.heroTitle}>{t("hero.title")}</h1>
          <p className={styles.heroCopy}>{t("hero.body")}</p>
        </div>
        <div className={styles.heroVisual}>
          <HeroTimelineGraph />
        </div>
      </section>

      <section id="metrics" className={styles.sectionIntro}>
        <h2>{t("metrics.title")}</h2>
        <p>{t("metrics.body")}</p>
      </section>

      <section id="communications" className={styles.communication}>
        <div className={styles.communicationHeader}>
          <h2>{t("communications.title")}</h2>
          <p>{t("communications.body")}</p>
        </div>

        <div className={styles.slides}>
          {slides.map((slide) => (
            <article key={slide.key} className={styles.slide}>
              <div className={styles.slideText}>
                <span className={styles.slideIndex}>{slide.index}</span>
                <h3>{slide.title}</h3>
                <p>{slide.body}</p>
              </div>
              <div className={styles.visualFrame}>{slide.visual}</div>
            </article>
          ))}
        </div>
      </section>

      <section id="scaling" className={styles.closing}>
        <h2>{t("scaling.title")}</h2>
        <p>{t("scaling.body")}</p>
      </section>
    </div>
  );
}
