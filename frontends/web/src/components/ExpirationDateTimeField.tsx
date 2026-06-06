"use client";

import { useTranslations } from "next-intl";
import { useCallback, useEffect, useRef } from "react";
import {
  formatLocalDateTime,
  normalizeUtcDateTimeLocalValue,
  utcDateTimeLocalToRfc3339,
} from "@/lib/dateTime";

type ExpirationDateTimeFieldProps = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  required?: boolean;
};

/** Renders a UTC expiration picker with a read-only local-time preview. */
export function ExpirationDateTimeField({
  label,
  value,
  onChange,
  disabled = false,
  required = false,
}: ExpirationDateTimeFieldProps) {
  const t = useTranslations("common.dateTime");
  const inputRef = useRef<HTMLInputElement>(null);
  const normalizedValue = normalizeUtcDateTimeLocalValue(value);
  const inputValue = normalizedValue ?? value;
  const rfc3339 = utcDateTimeLocalToRfc3339(inputValue);
  const localTime = rfc3339 ? formatLocalDateTime(rfc3339) : "";
  const syncDomInputValue = useCallback(() => {
    const domValue = inputRef.current?.value ?? "";
    const normalizedDomValue = normalizeUtcDateTimeLocalValue(domValue);
    const nextValue = normalizedDomValue ?? domValue;
    if (nextValue && nextValue !== value) {
      onChange(nextValue);
    }
  }, [onChange, value]);

  useEffect(() => {
    if (normalizedValue !== null && normalizedValue !== value) {
      onChange(normalizedValue);
    }
  }, [normalizedValue, onChange, value]);

  useEffect(() => {
    syncDomInputValue();
    const timeouts = [0, 50, 250, 1000].map((delay) =>
      window.setTimeout(syncDomInputValue, delay),
    );

    return () => {
      for (const timeout of timeouts) {
        window.clearTimeout(timeout);
      }
    };
  }, [syncDomInputValue]);

  return (
    <div className="expiration-datetime-field">
      <label className="form-field">
        <span>{label}</span>
        <input
          ref={inputRef}
          type="datetime-local"
          step={60}
          value={inputValue}
          autoComplete="off"
          disabled={disabled}
          required={required}
          onChange={(event) => onChange(event.target.value)}
          onFocus={syncDomInputValue}
        />
      </label>
      <label className="form-field">
        <span>{t("localTime")}</span>
        <input
          type="text"
          value={localTime}
          placeholder={t("localTimePlaceholder")}
          readOnly
          disabled={disabled}
        />
      </label>
    </div>
  );
}
