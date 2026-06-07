import type { CookiePageContent, LegalPageContent } from "@/lib/legalContent";

/** Renders long-form public legal notice content. */
export function LegalNoticePage({
  content,
}: {
  content: LegalPageContent | CookiePageContent;
}) {
  return (
    <article className="legal-page">
      <header className="legal-page-header">
        <p className="text-caption uppercase tracking-wide text-fg-muted">
          {content.effectiveDate}
        </p>
        <h1 className="text-h1 font-bold leading-h1">{content.title}</h1>
        <p className="max-w-3xl text-body text-fg-secondary">
          {content.subtitle}
        </p>
      </header>
      <div className="legal-page-body">
        {content.sections.map((section) => (
          <section className="legal-section" key={section.heading}>
            <h2 className="text-h2 font-semibold">{section.heading}</h2>
            {section.paragraphs?.map((paragraph) => (
              <p key={paragraph}>{paragraph}</p>
            ))}
            {section.items ? (
              <ul>
                {section.items.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            ) : null}
          </section>
        ))}
        {"tables" in content
          ? content.tables.map((table) => (
              <section className="legal-section" key={table.heading}>
                <h2 className="text-h2 font-semibold">{table.heading}</h2>
                <div className="overflow-x-auto">
                  <table className="data-table legal-cookie-table">
                    <thead>
                      <tr>
                        <th>{content.tableHeaders.name}</th>
                        <th>{content.tableHeaders.purpose}</th>
                        <th>{content.tableHeaders.duration}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {table.rows.map((row) => (
                        <tr key={row.name}>
                          <td className="font-mono">{row.name}</td>
                          <td>{row.purpose}</td>
                          <td>{row.duration}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>
            ))
          : null}
      </div>
    </article>
  );
}
