import { describe, expect, it } from "vitest";
import {
  normalizeUtcDateTimeLocalValue,
  utcDateTimeLocalToRfc3339,
} from "./dateTime";

describe("dateTime", () => {
  it("normalizes slash-formatted UTC defaults for datetime-local inputs", () => {
    expect(normalizeUtcDateTimeLocalValue("2026/06/06 12:30")).toBe(
      "2026-06-06T12:30",
    );
    expect(utcDateTimeLocalToRfc3339("2026/06/06 12:30")).toBe(
      "2026-06-06T12:30:00.000Z",
    );
  });

  it("rejects impossible UTC datetimes", () => {
    expect(normalizeUtcDateTimeLocalValue("2026/02/31 12:30")).toBeNull();
    expect(utcDateTimeLocalToRfc3339("2026/02/31 12:30")).toBeNull();
  });
});
