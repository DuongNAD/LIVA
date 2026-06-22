import { SkillMetadata } from "../SkillMetadata";
import { logger } from "../../utils/logger";
import { z } from "zod";

export const metadata: SkillMetadata = {
  name: "convert_timezone",
  category: "core",
  short_desc: "Convert a date-time between timezones.",
  description: "Convert a given date-time string from a source timezone (or local system timezone) to a target timezone using native Intl.DateTimeFormat.",
  parameters: {
    type: "object",
    properties: {
      targetTimezone: {
        type: "string",
        description: "The target timezone to convert to (e.g., 'America/New_York', 'Asia/Ho_Chi_Minh', 'UTC')."
      },
      sourceTimezone: {
        type: "string",
        description: "The source timezone of the input date-time. Defaults to the system local timezone."
      },
      dateTimeStr: {
        type: "string",
        description: "The date-time string to convert (e.g., '2026-06-21 14:30:00'). Defaults to current time."
      }
    },
    required: ["targetTimezone"]
  }
};

function isValidTimezone(tz: string): boolean {
  try {
    Intl.DateTimeFormat(undefined, { timeZone: tz });
    return true;
  } catch {
    return false;
  }
}

function getTimezoneOffset(timeZone: string, date: Date): number {
  const formatter = new Intl.DateTimeFormat('en-US', {
    timeZone,
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
    hour12: false,
  });

  const parts = formatter.formatToParts(date);
  const getPart = (type: string) => parseInt(parts.find(p => p.type === type)?.value || '0', 10);

  const year = getPart('year');
  const month = getPart('month') - 1;
  const day = getPart('day');
  let hour = getPart('hour');
  if (hour === 24) hour = 0;
  const minute = getPart('minute');
  const second = getPart('second');

  const tzDateUTC = Date.UTC(year, month, day, hour, minute, second);
  return tzDateUTC - date.getTime();
}

function parseNaiveDateTime(str: string): Date {
  const cleanStr = str.trim();
  
  // YYYY-MM-DD HH:mm:ss.SSS or YYYY-MM-DD HH:mm:ss
  const regexFull = /^(\d{4})-(\d{2})-(\d{2})[T\s](\d{2}):(\d{2}):(\d{2})(?:\.(\d{3}))?$/;
  const matchFull = cleanStr.match(regexFull);
  if (matchFull) {
    const [, y, m, d, h, min, s, ms] = matchFull;
    const milliseconds = ms ? parseInt(ms) : 0;
    return new Date(Date.UTC(parseInt(y), parseInt(m) - 1, parseInt(d), parseInt(h), parseInt(min), parseInt(s), milliseconds));
  }

  // YYYY-MM-DD HH:mm
  const regexMin = /^(\d{4})-(\d{2})-(\d{2})[T\s](\d{2}):(\d{2})$/;
  const matchMin = cleanStr.match(regexMin);
  if (matchMin) {
    const [, y, m, d, h, min] = matchMin;
    return new Date(Date.UTC(parseInt(y), parseInt(m) - 1, parseInt(d), parseInt(h), parseInt(min), 0));
  }

  // YYYY-MM-DD
  const regexDate = /^(\d{4})-(\d{2})-(\d{2})$/;
  const matchDate = cleanStr.match(regexDate);
  if (matchDate) {
    const [, y, m, d] = matchDate;
    return new Date(Date.UTC(parseInt(y), parseInt(m) - 1, parseInt(d), 0, 0, 0));
  }

  throw new Error(`Invalid date-time format: ${str}. Supported formats: YYYY-MM-DD, YYYY-MM-DD HH:mm, YYYY-MM-DD HH:mm:ss, YYYY-MM-DD HH:mm:ss.SSS`);
}

function formatInTimezone(date: Date, timeZone: string): string {
  const formatter = new Intl.DateTimeFormat('en-US', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
  const parts = formatter.formatToParts(date);
  const getPart = (type: string) => parts.find(p => p.type === type)?.value || '';
  
  let hour = getPart('hour');
  if (hour === '24') hour = '00';
  
  return `${getPart('year')}-${getPart('month')}-${getPart('day')} ${hour}:${getPart('minute')}:${getPart('second')}`;
}

const argsSchema = z.object({
  targetTimezone: z.string({
    message: "targetTimezone is required.",
  }).trim().min(1, "targetTimezone is required."),
  sourceTimezone: z.string().trim().optional(),
  dateTimeStr: z.string().trim().optional()
});

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = argsSchema.safeParse(rawArgs);
  if (!parsed.success) {
    return `Error: ${parsed.error.issues.map(e => e.message).join(", ")}`;
  }
  const args = parsed.data;

  const targetTz = args.targetTimezone;
  const sourceTz = (args.sourceTimezone || Intl.DateTimeFormat().resolvedOptions().timeZone).trim();

  if (!isValidTimezone(targetTz)) {
    return `Error: Invalid target timezone: '${targetTz}'`;
  }
  if (!isValidTimezone(sourceTz)) {
    return `Error: Invalid source timezone: '${sourceTz}'`;
  }

  try {
    let utcDate: Date;
    if (args.dateTimeStr && args.dateTimeStr.trim()) {
      let dtStr = args.dateTimeStr.trim();
      const hasOffset = (/[Zz]|[+-]\d{2}(?::?\d{2})?$/.test(dtStr) && !/^\d{4}-\d{2}-\d{2}$/.test(dtStr)) || /\b[A-Z]{3,4}$/.test(dtStr);
      if (hasOffset) {
        if (/[+-]\d{2}$/.test(dtStr)) {
          dtStr += ":00";
        } else {
          const tzMatch = dtStr.match(/\b([A-Z]{3,4})$/);
          if (tzMatch) {
            const tz = tzMatch[1];
            const tzOffsets: Record<string, string> = {
              UTC: "+00:00",
              GMT: "+00:00",
              EST: "-05:00",
              EDT: "-04:00",
              CST: "-06:00",
              CDT: "-05:00",
              MST: "-07:00",
              MDT: "-06:00",
              PST: "-08:00",
              PDT: "-07:00",
            };
            if (tzOffsets[tz]) {
              dtStr = dtStr.replace(/\s*[A-Z]{3,4}$/, tzOffsets[tz]);
            }
          }
        }
        utcDate = new Date(dtStr);
        if (isNaN(utcDate.getTime())) {
          return `Error: Invalid date-time format with offset: '${args.dateTimeStr.trim()}'`;
        }
      } else {
        const naiveDate = parseNaiveDateTime(dtStr);
        const offset1 = getTimezoneOffset(sourceTz, naiveDate);
        const utcTime1 = new Date(naiveDate.getTime() - offset1);
        const offset2 = getTimezoneOffset(sourceTz, utcTime1);
        utcDate = new Date(naiveDate.getTime() - offset2);
      }
    } else {
      utcDate = new Date();
    }

    const sourceFormatted = formatInTimezone(utcDate, sourceTz);
    const targetFormatted = formatInTimezone(utcDate, targetTz);

    logger.info(`[Skill: convert_timezone] Converted ${sourceFormatted} (${sourceTz}) -> ${targetFormatted} (${targetTz})`);

    return `[Timezone Conversion]
Source Time: ${sourceFormatted} (${sourceTz})
Target Time: ${targetFormatted} (${targetTz})`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: convert_timezone] Error: ${errMsg}`);
    return `Error: ${errMsg}`;
  }
};
