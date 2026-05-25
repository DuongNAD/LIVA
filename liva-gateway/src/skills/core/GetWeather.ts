import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";
import { z } from "zod";

export const metadata = {
  name: "get_weather_forecast",
  search_keywords: ["get_weather_forecast","get weather forecast","thời tiết","weather","trời","mưa","nắng","nhiệt độ","dự báo","forecast","temperature","rain","nóng","lạnh","bão","ngày mai","tomorrow"],
  short_desc: "Retrieve current weather or weather forecast up to 7 days for a location.",
  description: "[AUTO_RUN] Retrieve weather for a location. This tool's name is EXACTLY 'get_weather_forecast'. Supports current weather AND multi-day forecast up to 7 days. CRITICAL: Use the 'days' parameter (integer) to control forecast range — do NOT invent a 'date' parameter. Examples: today → days=1, tomorrow → days=2, next 3 days → days=3.",
  parameters: {
    type: "object",
    properties: {
      location: {
        type: "string",
        description:
          "City name only (e.g., 'Hanoi', 'Da Nang'). Do NOT include country. If user does not specify, infer from <CONTEXT> location. Execute immediately without confirmation.",
      },
      days: {
        type: "number",
        description:
          "Number of forecast days (1–7). REQUIRED when user asks about future dates. 1=today only, 2=today+tomorrow, 3=today+2 days, etc. If user asks about 'tomorrow' or 'ngày mai', you MUST set days=2. There is NO 'date' parameter — only 'days'.",
      },
    },
    required: [],
  },
};

// WeatherAPI.com Schemas
const WeatherApiConditionSchema = z.object({
  text: z.string(),
  icon: z.string().optional(),
  code: z.number().optional(),
});

const WeatherApiCurrentSchema = z.object({
  temp_c: z.number(),
  humidity: z.number(),
  condition: WeatherApiConditionSchema,
});

const WeatherApiDaySchema = z.object({
  maxtemp_c: z.number(),
  mintemp_c: z.number(),
  daily_chance_of_rain: z.number().optional().default(0),
  condition: WeatherApiConditionSchema,
});

const WeatherApiForecastDaySchema = z.object({
  date: z.string(),
  day: WeatherApiDaySchema,
});

const WeatherApiForecastSchema = z.object({
  location: z.object({
    name: z.string(),
    country: z.string(),
  }),
  current: WeatherApiCurrentSchema,
  forecast: z.object({
    forecastday: z.array(WeatherApiForecastDaySchema),
  }),
});

// Cache Store
interface CacheEntry {
  report: string;
  timestamp: number;
}

const cache = new Map<string, CacheEntry>();
const CACHE_TTL_MS = 15 * 60 * 1000; // 15 minutes
const MAX_CACHE_SIZE = 100;

const getCacheEntry = (key: string): string | null => {
  const entry = cache.get(key);
  if (!entry) return null;
  if (Date.now() - entry.timestamp > CACHE_TTL_MS) {
    cache.delete(key);
    return null;
  }
  return entry.report;
};

export const clearCache = () => {
  cache.clear();
};

const setCacheEntry = (key: string, report: string) => {
  const now = Date.now();
  for (const [k, entry] of cache.entries()) {
    if (now - entry.timestamp > CACHE_TTL_MS) {
      cache.delete(k);
    }
  }
  if (cache.size >= MAX_CACHE_SIZE) {
    cache.clear();
  }
  cache.set(key, { report, timestamp: now });
};

// Simplified WMO Weather Codes interpretation
const weatherCodes: Record<number, string> = {
  0: "Clear sky",
  1: "Mainly clear",
  2: "Partly cloudy",
  3: "Overcast",
  45: "Fog",
  48: "Depositing rime fog",
  51: "Light drizzle",
  53: "Moderate drizzle",
  55: "Dense drizzle",
  61: "Slight rain",
  63: "Moderate rain",
  65: "Heavy rain",
  80: "Slight rain showers",
  81: "Moderate rain showers",
  82: "Violent rain showers",
  95: "Thunderstorm",
};

export const execute = async (args: { location?: string; days?: number }): Promise<string> => {
  try {
    let finalLocation = args.location ? args.location.trim() : "";
    let coords: { lat: number; lon: number } | null = null;
    const forecastDays = Math.max(1, Math.min(args.days || 1, 7));

    // Nếu người dùng không cung cấp địa điểm => Auto Detect qua IP
    if (!finalLocation) {
      logger.info(
        `[Skill: get_weather_forecast] Đang xác định vị trí tự động qua IP...`,
      );
      try {
        const ipRes = await safeFetch("http://ip-api.com/json/", {}, 5000);
        const ipData = await ipRes.json();
        if (ipData?.status === "success" && ipData.lat && ipData.lon) {
          coords = { lat: ipData.lat, lon: ipData.lon };
          finalLocation = ipData.city || ipData.regionName || "Current location";
          logger.info(
            `[Skill: get_weather_forecast] Đã tìm thấy vị trí: ${finalLocation} (${coords.lat}, ${coords.lon})`,
          );
        }
      } catch (ipErr) {
        logger.warn(
          `[Skill: get_weather_forecast] Lỗi định vị bằng IP: ${ipErr}`,
        );
      }
    }

    if (finalLocation) {
      logger.info(
        `[Skill: get_weather_forecast] Đang kiểm tra thời tiết cho khu vực: ${finalLocation}`,
      );
    } else if (!coords) {
      return "Không thể tự động lấy vị trí hiện tại. Hãy phản hồi lại người dùng: 'Bạn muốn xem thời tiết ở tỉnh/thành phố nào ạ?'";
    }

    // Nếu vẫn chưa có độ tọa độ (do có nhập Tên nhưng không có IP detect)
    if (!coords && finalLocation) {
      const locMap: Record<string, { lat: number; lon: number }> = {
        hanoi: { lat: 21.0285, lon: 105.8542 },
        "hà nội": { lat: 21.0285, lon: 105.8542 },
        "ho chi minh": { lat: 10.8231, lon: 106.6297 },
        "hồ chí minh": { lat: 10.8231, lon: 106.6297 },
        hcm: { lat: 10.8231, lon: 106.6297 },
        "da nang": { lat: 16.0544, lon: 108.2022 },
        "đà nẵng": { lat: 16.0544, lon: 108.2022 },
      };

      const query = finalLocation.toLowerCase();

      if (query.includes("mars") || query.includes("sao hỏa")) {
        return "No weather station on Mars. Expected temperature range: -125°C to 20°C.";
      }

      coords = locMap[query];

      if (!coords) {
        // [FIX] LLMs often send "City, Country" format but Open-Meteo only accepts city name
        // Strip country suffix for better geocoding hit rate
        const cityOnly = finalLocation.includes(",") ? finalLocation.split(",")[0].trim() : finalLocation;
        
        // Try geocoding with city name only first, then full string as fallback
        const candidates = [cityOnly, finalLocation].filter((v, i, a) => a.indexOf(v) === i);
        
        for (const candidate of candidates) {
          const geoUrl = `https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(candidate)}&count=1&language=vi&format=json`;
          logger.info(
            `[Skill: get_weather_forecast] Tìm kiếm Geocoding: ${geoUrl}`,
          );
          const geoRes = await safeFetch(geoUrl, {}, 5000);
          const geoData = await geoRes.json();

          if (geoData.results && geoData.results.length > 0) {
            coords = {
              lat: geoData.results[0].latitude,
              lon: geoData.results[0].longitude,
            };
            // Chuẩn hóa lại tên hiển thị cho đẹp
            finalLocation = `${geoData.results[0].name}, ${geoData.results[0].country}`;
            break;
          }
        }
        
        if (!coords) {
          return `Không tìm thấy tọa độ cho địa danh "${finalLocation}". Hãy xin lỗi người dùng và yêu cầu nhập lại tên tỉnh/thành phố chuẩn (ví dụ: Hà Nội, Đà Nẵng).`;
        }
      }
    }

    if (!coords) {
      return "Internal error: Coordinates not resolved. Please specify a city name.";
    }

    // ── Cache Lookup ──
    const cacheKey = `${coords.lat.toFixed(2)}_${coords.lon.toFixed(2)}_${forecastDays}`;
    const cachedReport = getCacheEntry(cacheKey);
    if (cachedReport) {
      logger.info(
        `[Skill: get_weather_forecast] Cache hit for coordinates: ${coords.lat.toFixed(2)}, ${coords.lon.toFixed(2)}`
      );
      return cachedReport;
    }

    const apiKey = process.env.WEATHER_API_KEY;
    let report = "";

    // ── WeatherAPI.com Path ──
    if (apiKey) {
      try {
        logger.info(
          `[Skill: get_weather_forecast] Querying WeatherAPI.com for coordinates: ${coords.lat}, ${coords.lon}`
        );
        const weatherUrl = `http://api.weatherapi.com/v1/forecast.json?key=${apiKey}&q=${coords.lat},${coords.lon}&days=${forecastDays}&aqi=no&alerts=no`;
        const weatherRes = await safeFetch(weatherUrl, {}, 10000);
        const rawJson = await weatherRes.json();
        
        // Validation using Zod
        const parsedData = WeatherApiForecastSchema.parse(rawJson);
        const current = parsedData.current;
        const daily = parsedData.forecast.forecastday;
        const resolvedLocName = `${parsedData.location.name}, ${parsedData.location.country}`;

        if (forecastDays > 1) {
          report = `Weather forecast for [${resolvedLocName}] (${forecastDays} days):\n\n`;
        } else {
          report = `Weather data for [${resolvedLocName}]:\n`;
        }
        report += `📍 Current: ${current.temp_c}°C | Humidity: ${current.humidity}% | ${current.condition.text}\n`;

        if (daily && daily.length > 0) {
          report += `\n`;
          const dayLabels = ["Today", "Tomorrow"];

          for (let i = 0; i < daily.length; i++) {
            const fday = daily[i];
            const dateStr = fday.date;
            const label = i < dayLabels.length ? dayLabels[i] : dateStr;
            const rainChance = fday.day.daily_chance_of_rain;

            report += `📅 ${label} (${dateStr}): ${fday.day.mintemp_c}°C – ${fday.day.maxtemp_c}°C | ${fday.day.condition.text}`;
            if (rainChance !== undefined && rainChance !== null) {
              report += ` | Rain: ${rainChance}%`;
            }
            report += `\n`;
          }
        }

        report = report.trim();
        setCacheEntry(cacheKey, report);
        return report;
      } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.warn(
          `[Skill: get_weather_forecast] WeatherAPI.com failed. Falling back to Open-Meteo. Error: ${errMsg}`
        );
      }
    }

    // ── Open-Meteo Fallback Path ──
    logger.info(
      `[Skill: get_weather_forecast] Querying Open-Meteo fallback for coordinates: ${coords.lat}, ${coords.lon}`
    );
    const weatherUrl = `https://api.open-meteo.com/v1/forecast?latitude=${coords.lat}&longitude=${coords.lon}&current=temperature_2m,relative_humidity_2m,weather_code&daily=temperature_2m_max,temperature_2m_min,weather_code,precipitation_probability_max&forecast_days=${forecastDays}&timezone=auto`;
    const weatherRes = await safeFetch(weatherUrl, {}, 10000);
    const data = await weatherRes.json();
    const current = data.current;
    const daily = data.daily;

    const currentCondition =
      weatherCodes[current.weather_code] ||
      `(Weather code: ${current.weather_code})`;

    if (forecastDays > 1) {
      report = `Weather forecast for [${finalLocation}] (${forecastDays} days):\n\n`;
    } else {
      report = `Weather data for [${finalLocation}]:\n`;
    }
    report += `📍 Current: ${current.temperature_2m}°C | Humidity: ${current.relative_humidity_2m}% | ${currentCondition}\n`;

    if (daily && daily.time) {
      report += `\n`;
      const dayLabels = ["Today", "Tomorrow"];

      for (let i = 0; i < daily.time.length; i++) {
        const dateStr = daily.time[i];
        const label = i < dayLabels.length ? dayLabels[i] : dateStr;
        const dayCondition =
          weatherCodes[daily.weather_code[i]] ||
          `(Weather code: ${daily.weather_code[i]})`;
        const rainChance = daily.precipitation_probability_max?.[i];

        report += `📅 ${label} (${dateStr}): ${daily.temperature_2m_min[i]}°C – ${daily.temperature_2m_max[i]}°C | ${dayCondition}`;
        if (rainChance !== undefined && rainChance !== null) {
          report += ` | Rain: ${rainChance}%`;
        }
        report += `\n`;
      }
    }

    report = report.trim();
    setCacheEntry(cacheKey, report);
    return report;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Weather fetch failed: ${errMsg}`;
  }
};
