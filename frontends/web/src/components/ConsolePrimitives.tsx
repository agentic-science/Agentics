import type { ReactNode } from "react";

/** Renders a shared console section title. */
export function ConsoleSectionTitle({
  icon,
  title,
}: {
  icon: ReactNode;
  title: string;
}) {
  return (
    <h2 className="flex items-center gap-2 text-[var(--text-h3)] font-semibold">
      <span className="text-[var(--accent-secondary-text)]">{icon}</span>
      {title}
    </h2>
  );
}

/** Renders a shared console text input. */
export function ConsoleTextInput({
  label,
  value,
  onChange,
  required,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  required?: boolean;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
        {label}
      </span>
      <input
        className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        required={required}
      />
    </label>
  );
}
