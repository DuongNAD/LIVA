import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { mockListEvents, mockInsertEvent, mockRequestApproval } = vi.hoisted(() => ({
  mockListEvents: vi.fn(),
  mockInsertEvent: vi.fn(),
  mockRequestApproval: vi.fn().mockResolvedValue(true)
}));

import { execute, metadata } from "../../../src/skills/personal/GoogleCalendar";

// Mock googleapis
vi.mock("googleapis", () => {
  class GoogleAuth {}
  const list = mockListEvents;
  const insert = mockInsertEvent;
  const calendar = vi.fn().mockImplementation(() => ({
    events: {
      list,
      insert
    }
  }));
  return {
    google: {
      auth: {
        GoogleAuth
      },
      calendar
    }
  };
});

vi.mock("@security/HITLGuard", () => ({
  HITLGuard: {
    get requestApproval() { return mockRequestApproval; }
  }
}));

vi.mock("@utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn()
  }
}));

describe("Skill - GoogleCalendar", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    process.env = { ...originalEnv };
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it("should export metadata", () => {
    expect(metadata.name).toBe("google_calendar");
    expect(metadata.kit).toBe("PERSONAL_KIT");
  });

  describe("Validation Errors", () => {
    it("should return zod parameter validation error", async () => {
      const result = await execute({ action: "invalid_action" });
      expect(result).toContain("[CALENDAR ERROR] Parameter validation failed");
    });

    it("should return error if missing required parameters for createEvent", async () => {
      const result = await execute({ action: "createEvent", summary: "Missing times" });
      expect(result).toContain("[CALENDAR ERROR] 'summary', 'startTime', and 'endTime' are required");
    });

    it("should return error if credentials missing and mock mode disabled", async () => {
      process.env.NODE_ENV = "production";
      process.env.LIVA_MOCK_CALENDAR = "false";
      delete process.env.GOOGLE_APPLICATION_CREDENTIALS;

      const result = await execute({ action: "listEvents" });
      expect(result).toContain("[CALENDAR ERROR] GOOGLE_APPLICATION_CREDENTIALS environment variable is not defined");
    });
  });

  describe("Mock Mode", () => {
    beforeEach(() => {
      process.env.NODE_ENV = "test";
    });

    it("should list mock events", async () => {
      const result = await execute({ action: "listEvents" });
      expect(result).toContain("[CALENDAR SUCCESS]");
      expect(result).toContain("Mock Meeting with Team");
      expect(mockListEvents).not.toHaveBeenCalled();
    });

    it("should create mock event with HITL approval", async () => {
      mockRequestApproval.mockResolvedValueOnce(true);

      const result = await execute({
        action: "createEvent",
        summary: "Mock Event Summary",
        startTime: "2026-06-10T10:00:00Z",
        endTime: "2026-06-10T11:00:00Z"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[CALENDAR SUCCESS]");
      expect(result).toContain("Mock Event Summary");
      expect(result).toContain("mock-event-id-12345");
      expect(mockInsertEvent).not.toHaveBeenCalled();
    });

    it("should reject creating mock event if HITL rejected", async () => {
      mockRequestApproval.mockRejectedValueOnce(new Error("REJECTED_BY_USER"));

      const result = await execute({
        action: "createEvent",
        summary: "Mock Event Summary",
        startTime: "2026-06-10T10:00:00Z",
        endTime: "2026-06-10T11:00:00Z"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[CALENDAR ACTION BLOCKED]");
      expect(result).toContain("rejected by user: REJECTED_BY_USER");
      expect(mockInsertEvent).not.toHaveBeenCalled();
    });
  });

  describe("Real API Mode", () => {
    beforeEach(() => {
      process.env.NODE_ENV = "production";
      process.env.GOOGLE_APPLICATION_CREDENTIALS = "path/to/credentials.json";
      process.env.LIVA_MOCK_CALENDAR = "false";
    });

    it("should list events from googleapis", async () => {
      mockListEvents.mockResolvedValueOnce({
        data: {
          items: [
            {
              summary: "Real Meeting",
              start: { dateTime: "2026-06-10T15:00:00Z" },
              end: { dateTime: "2026-06-10T16:00:00Z" }
            }
          ]
        }
      });

      const result = await execute({ action: "listEvents" });
      expect(result).toContain("[CALENDAR SUCCESS]");
      expect(result).toContain("Real Meeting");
      expect(result).toContain("2026-06-10T15:00:00Z");
      expect(mockListEvents).toHaveBeenCalledTimes(1);
    });

    it("should create event via googleapis with HITL approval", async () => {
      mockRequestApproval.mockResolvedValueOnce(true);
      mockInsertEvent.mockResolvedValueOnce({
        data: {
          id: "real-id-9999",
          summary: "Real Shared Event",
          start: { dateTime: "2026-06-10T15:00:00Z" },
          end: { dateTime: "2026-06-10T16:00:00Z" }
        }
      });

      const result = await execute({
        action: "createEvent",
        summary: "Real Shared Event",
        startTime: "2026-06-10T15:00:00Z",
        endTime: "2026-06-10T16:00:00Z"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(mockInsertEvent).toHaveBeenCalledTimes(1);
      expect(result).toContain("[CALENDAR SUCCESS]");
      expect(result).toContain("real-id-9999");
      expect(result).toContain("Real Shared Event");
    });
  });
});
