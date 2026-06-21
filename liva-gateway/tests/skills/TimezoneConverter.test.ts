import { describe, it, expect, vi, beforeEach } from "vitest";
import { metadata, execute } from "../../src/skills/core/TimezoneConverter";

vi.mock("../../src/utils/logger", () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

describe("TimezoneConverter", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should have correct metadata", () => {
    expect(metadata.name).toBe("convert_timezone");
    expect(metadata.category).toBe("core");
    expect(metadata.parameters.required).toContain("targetTimezone");
  });

  it("should convert time between valid timezones", async () => {
    const res = await execute({
      targetTimezone: "Asia/Ho_Chi_Minh",
      sourceTimezone: "UTC",
      dateTimeStr: "2026-06-21 12:00:00",
    });

    expect(res).toContain("Asia/Ho_Chi_Minh");
    expect(res).toContain("UTC");
    expect(res).toContain("2026-06-21 19:00:00");
  });

  it("should handle date-only string", async () => {
    const res = await execute({
      targetTimezone: "Asia/Ho_Chi_Minh",
      sourceTimezone: "UTC",
      dateTimeStr: "2026-06-21",
    });

    expect(res).toContain("2026-06-21 07:00:00");
  });

  it("should handle date-hour-minute string", async () => {
    const res = await execute({
      targetTimezone: "Asia/Ho_Chi_Minh",
      sourceTimezone: "UTC",
      dateTimeStr: "2026-06-21 12:30",
    });

    expect(res).toContain("2026-06-21 19:30:00");
  });

  it("should handle iso string with offset", async () => {
    const res = await execute({
      targetTimezone: "UTC",
      dateTimeStr: "2026-06-21T12:00:00+07:00",
    });

    // 12:00+07:00 is 05:00 UTC
    expect(res).toContain("2026-06-21 05:00:00");
  });

  it("should fallback to local system timezone if sourceTimezone is not specified", async () => {
    const localTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const res = await execute({
      targetTimezone: "UTC",
      dateTimeStr: "2026-06-21 12:00:00",
    });

    expect(res).toContain(localTz);
    expect(res).toContain("UTC");
  });

  it("should fallback to current time if dateTimeStr is not specified", async () => {
    const res = await execute({
      targetTimezone: "UTC",
    });

    expect(res).toContain("UTC");
    expect(res).toContain("Timezone Conversion");
  });

  it("should return error for invalid target timezone", async () => {
    const res = await execute({
      targetTimezone: "Invalid/Timezone",
      dateTimeStr: "2026-06-21 12:00:00",
    });

    expect(res).toContain("Error");
    expect(res).toContain("Invalid target timezone");
  });

  it("should return error for invalid source timezone", async () => {
    const res = await execute({
      targetTimezone: "UTC",
      sourceTimezone: "Invalid/Timezone",
      dateTimeStr: "2026-06-21 12:00:00",
    });

    expect(res).toContain("Error");
    expect(res).toContain("Invalid source timezone");
  });

  it("should return error for invalid date-time format", async () => {
    const res = await execute({
      targetTimezone: "UTC",
      dateTimeStr: "not-a-date",
    });

    expect(res).toContain("Error");
    expect(res).toContain("Invalid date-time format");
  });

  it("should fail on invalid arguments (non-object or missing targetTimezone)", async () => {
    // @ts-expect-error - Testing runtime invalid args
    const res1 = await execute(null);
    expect(res1).toContain("Error");

    // @ts-expect-error - Testing runtime invalid args
    const res2 = await execute("UTC");
    expect(res2).toContain("Error");

    // @ts-expect-error - Testing runtime invalid args
    const res3 = await execute({});
    expect(res3).toContain("Error");
    expect(res3).toContain("targetTimezone is required");
  });

  it("should support millisecond formats", async () => {
    const res = await execute({
      targetTimezone: "Asia/Ho_Chi_Minh",
      sourceTimezone: "UTC",
      dateTimeStr: "2026-06-21 12:00:00.123",
    });

    expect(res).toContain("Asia/Ho_Chi_Minh");
    expect(res).toContain("UTC");
    expect(res).toContain("2026-06-21 19:00:00");
  });

  it("should support timezone abbreviations and 2-digit/4-digit offsets", async () => {
    const res1 = await execute({
      targetTimezone: "UTC",
      dateTimeStr: "2026-06-21T12:00:00 EST",
    });
    expect(res1).toContain("2026-06-21 17:00:00");

    const res2 = await execute({
      targetTimezone: "UTC",
      dateTimeStr: "2026-06-21T12:00:00+07",
    });
    expect(res2).toContain("2026-06-21 05:00:00");
  });
});
