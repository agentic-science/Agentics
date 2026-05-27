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
    <h2 className="flex items-center gap-2 text-h3 font-semibold">
      <span className="text-data">{icon}</span>
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
      <span className="text-caption uppercase tracking-wide text-fg-muted">
        {label}
      </span>
      <input
        className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        required={required}
      />
    </label>
  );
}
