import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";

export const metadata = {
  name: "get_weather_forecast",
  search_keywords: ["get_weather_forecast","get weather forecast","thời tiết","weather","trời","mưa","nắng","nhiệt độ","dự báo","forecast","temperature","rain","nóng","lạnh","bão","ngày mai","tomorrow"],
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
      return "Unable to auto-detect location. Please specify a city name (e.g., 'Hanoi').";
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
          return `Unable to geolocate "${finalLocation}". Please try a different name.`;
        }
      }
    }

    if (!coords) {
      return "Internal error: Coordinates not resolved. Please specify a city name.";
    }

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

    // ── Current-only mode (days=1) ──
    if (forecastDays <= 1) {
      const weatherUrl = `https://api.open-meteo.com/v1/forecast?latitude=${coords.lat}&longitude=${coords.lon}&current=temperature_2m,relative_humidity_2m,weather_code&timezone=auto`;
      const weatherRes = await safeFetch(weatherUrl, {}, 10000);
      const data = await weatherRes.json();
      const current = data.current;

      const conditionStr =
        weatherCodes[current.weather_code] ||
        `(Weather code: ${current.weather_code})`;

      return `Weather data for [${finalLocation}]:\n- Temperature: ${current.temperature_2m}°C\n- Humidity: ${current.relative_humidity_2m}%\n- Condition: ${conditionStr}`;
    }

    // ── Multi-day forecast mode (days>1) ──
    const weatherUrl = `https://api.open-meteo.com/v1/forecast?latitude=${coords.lat}&longitude=${coords.lon}&current=temperature_2m,relative_humidity_2m,weather_code&daily=temperature_2m_max,temperature_2m_min,weather_code,precipitation_probability_max&forecast_days=${forecastDays}&timezone=auto`;
    const weatherRes = await safeFetch(weatherUrl, {}, 10000);
    const data = await weatherRes.json();
    const current = data.current;
    const daily = data.daily;

    const currentCondition =
      weatherCodes[current.weather_code] ||
      `(Weather code: ${current.weather_code})`;

    let report = `Weather forecast for [${finalLocation}] (${forecastDays} days):\n\n`;
    report += `📍 Current: ${current.temperature_2m}°C | Humidity: ${current.relative_humidity_2m}% | ${currentCondition}\n\n`;

    // Day names for readability
    const dayLabels = ["Today", "Tomorrow"];

    for (let i = 0; i < daily.time.length; i++) {
      const dateStr = daily.time[i]; // "2026-05-10"
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

    return report.trim();
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Weather fetch failed: ${errMsg}`;
  }
};
