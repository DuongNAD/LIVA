import { z } from "zod";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";

const GoogleCalendarSchema = z.object({
  action: z.enum(["listEvents", "createEvent"]),
  calendarId: z.string().optional().default("primary"),
  summary: z.string().optional(),
  description: z.string().optional(),
  startTime: z.string().optional().describe("ISO 8601 DateTime"),
  endTime: z.string().optional().describe("ISO 8601 DateTime"),
  maxResults: z.number().optional().default(10),
});

export const metadata = {
  name: "google_calendar",
  search_keywords: ["calendar", "google calendar", "schedule", "events", "meetings", "lịch", "sự kiện", "gcal"],
  description: "[ASK_FIRST] Access and manage Google Calendar events. Supports listing events and creating events. Creation actions require HITL approval.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["listEvents", "createEvent"],
        description: "Action to perform on Google Calendar."
      },
      calendarId: {
        type: "string",
        description: "The calendar ID to use (e.g., 'primary' or a specific email address). Defaults to 'primary'."
      },
      summary: {
        type: "string",
        description: "The title/summary of the event (required for createEvent)."
      },
      description: {
        type: "string",
        description: "The description of the event (optional)."
      },
      startTime: {
        type: "string",
        description: "The start time of the event in ISO 8601 format (required for createEvent, e.g., '2026-06-10T10:00:00Z')."
      },
      endTime: {
        type: "string",
        description: "The end time of the event in ISO 8601 format (required for createEvent, e.g., '2026-06-10T11:00:00Z')."
      },
      maxResults: {
        type: "integer",
        description: "Maximum number of events to list. Defaults to 10."
      }
    },
    required: ["action"]
  }
};

export const execute = async (argsObj: unknown): Promise<string> => {
  try {
    const parsed = GoogleCalendarSchema.parse(argsObj);
    const { action, calendarId, summary, description, startTime, endTime, maxResults } = parsed;

    if (action === "createEvent") {
      if (!summary || !startTime || !endTime) {
        return `[CALENDAR ERROR] 'summary', 'startTime', and 'endTime' are required to create an event.`;
      }
    }

    const isMock = process.env.LIVA_MOCK_CALENDAR === 'true' ||
                   (process.env.LIVA_MOCK_CALENDAR !== 'false' && (!process.env.GOOGLE_APPLICATION_CREDENTIALS || process.env.NODE_ENV === 'test'));

    if (action === "createEvent") {
      logger.info(`[GoogleCalendar] Action 'createEvent' requires HITL approval...`);
      try {
        await HITLGuard.requestApproval({
          toolName: "google_calendar",
          args: { action, calendarId, summary, description, startTime, endTime },
          reason: `LIVA wants to create a Google Calendar event "${summary}" from ${startTime} to ${endTime}`
        });
        logger.info(`[GoogleCalendar] ✅ HITL Approved for action: ${action}`);
      } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.warn(`[GoogleCalendar] ❌ HITL Rejected: ${errMsg}`);
        return `[CALENDAR ACTION BLOCKED] Action '${action}' was rejected by user: ${errMsg}`;
      }
    }

    if (isMock) {
      let resultSummary = "";
      if (action === "listEvents") {
        resultSummary = `- Event 1: Mock Meeting with Team (Start: 2026-06-10T10:00:00Z, End: 2026-06-10T11:00:00Z)\n- Event 2: Lunch Break (Start: 2026-06-10T12:00:00Z, End: 2026-06-10T13:00:00Z)`;
      } else if (action === "createEvent") {
        resultSummary = `Successfully created event "${summary}"\nID: mock-event-id-12345\nCalendar: ${calendarId}\nTime: ${startTime} - ${endTime}`;
      }
      return `[CALENDAR SUCCESS] Action: ${action}\n\n[OUTPUT]\n${resultSummary}`;
    }

    if (!process.env.GOOGLE_APPLICATION_CREDENTIALS) {
      return `[CALENDAR ERROR] GOOGLE_APPLICATION_CREDENTIALS environment variable is not defined. Please configure it in your environment.`;
    }

    // Dynamic import googleapis
    const { google } = await import("googleapis");
    const auth = new google.auth.GoogleAuth({
      scopes: ["https://www.googleapis.com/auth/calendar"],
    });
    const calendar = google.calendar({ version: "v3", auth });

    if (action === "listEvents") {
      const response = await calendar.events.list({
        calendarId,
        maxResults,
        singleEvents: true,
        orderBy: "startTime",
      });
      const events = response.data.items || [];
      interface CalendarEvent {
        summary?: string | null;
        start?: { dateTime?: string | null; date?: string | null } | null;
        end?: { dateTime?: string | null; date?: string | null } | null;
      }
      const resultSummary = (events as CalendarEvent[])
        .map((item) => `- Event: ${item.summary || "No Title"} (Start: ${item.start?.dateTime || item.start?.date}, End: ${item.end?.dateTime || item.end?.date})`)
        .join("\n") || "No events found.";
      return `[CALENDAR SUCCESS] Action: listEvents\n\n[OUTPUT]\n${resultSummary}`;
    } else {
      const response = await calendar.events.insert({
        calendarId,
        requestBody: {
          summary,
          description,
          start: { dateTime: startTime },
          end: { dateTime: endTime },
        },
      });
      const event = response.data;
      const resultSummary = `Successfully created event "${event.summary}"\nID: ${event.id}\nCalendar: ${calendarId}\nTime: ${event.start?.dateTime || event.start?.date} - ${event.end?.dateTime || event.end?.date}`;
      return `[CALENDAR SUCCESS] Action: createEvent\n\n[OUTPUT]\n${resultSummary}`;
    }

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[GoogleCalendar] Error: ${errMsg}`);
    if (error instanceof z.ZodError) {
      return `[CALENDAR ERROR] Parameter validation failed: ${error.issues.map(e => e.message).join(", ")}`;
    }
    return `[CALENDAR ERROR] Failed to execute action: ${errMsg}`;
  }
};
