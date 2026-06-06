const UTC_DATE_TIME_PATTERN =
  /^(\d{4})([-/])(\d{2})\2(\d{2})(?:T|\s+)(\d{2}):(\d{2})(?::(\d{2}))?$/;

type ParsedUtcDateTime = {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
};

/** Convert a UTC datetime-local field into the RFC3339 value expected by the API. */
export function utcDateTimeLocalToRfc3339(value: string): string | null {
  const parsed = parseUtcDateTimeLocal(value);
  if (parsed === "") {
    return "";
  }
  if (parsed === null) {
    return null;
  }
  return utcPartsToDate(parsed).toISOString();
}

/** Normalize accepted UTC datetime input into the value expected by datetime-local. */
export function normalizeUtcDateTimeLocalValue(value: string): string | null {
  const parsed = parseUtcDateTimeLocal(value);
  if (parsed === "") {
    return "";
  }
  if (parsed === null) {
    return null;
  }
  const base = `${pad4(parsed.year)}-${pad2(parsed.month)}-${pad2(parsed.day)}T${pad2(parsed.hour)}:${pad2(parsed.minute)}`;
  return parsed.second === 0 ? base : `${base}:${pad2(parsed.second)}`;
}

function parseUtcDateTimeLocal(value: string): ParsedUtcDateTime | "" | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }
  const match = UTC_DATE_TIME_PATTERN.exec(trimmed);
  if (!match) {
    return null;
  }

  const [, yearRaw, , monthRaw, dayRaw, hourRaw, minuteRaw, secondRaw = "00"] =
    match;
  const year = Number(yearRaw);
  const month = Number(monthRaw);
  const day = Number(dayRaw);
  const hour = Number(hourRaw);
  const minute = Number(minuteRaw);
  const second = Number(secondRaw);
  const parsed = { year, month, day, hour, minute, second };
  const date = utcPartsToDate(parsed);
  if (!dateMatchesUtcParts(date, parsed)) {
    return null;
  }
  return parsed;
}

function utcPartsToDate(parts: ParsedUtcDateTime): Date {
  const { year, month, day, hour, minute, second } = parts;
  const timestamp = Date.UTC(year, month - 1, day, hour, minute, second);
  return new Date(timestamp);
}

function dateMatchesUtcParts(date: Date, parts: ParsedUtcDateTime): boolean {
  const { year, month, day, hour, minute, second } = parts;
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() === month - 1 &&
    date.getUTCDate() === day &&
    date.getUTCHours() === hour &&
    date.getUTCMinutes() === minute &&
    date.getUTCSeconds() === second
  );
}

function pad2(value: number): string {
  return value.toString().padStart(2, "0");
}

function pad4(value: number): string {
  return value.toString().padStart(4, "0");
}

/** Format a selected UTC datetime in the browser's detected local time zone. */
export function formatLocalDateTime(value: string): string {
  const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const formatted = new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    timeZoneName: "short",
  }).format(new Date(value));
  return timeZone ? `${formatted} (${timeZone})` : formatted;
}
