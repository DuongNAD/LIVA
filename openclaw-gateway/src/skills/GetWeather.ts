import { safeFetch } from "../utils/HttpClient";
import { logger } from "../utils/logger";

export const metadata = {
  name: "get_weather_forecast",
  search_keywords: ["get_weather_forecast","get weather forecast"],
  description: "Công cụ lấy dự báo thời tiết tại một địa điểm cụ thể.",
  parameters: {
    type: "object",
    properties: {
      location: {
        type: "string",
        description:
          "Tên thành phố hoặc Vị trí địa lý. (Nếu để trống, AI sẽ tự động định vị người dùng qua Internet).",
      },
    },
    required: [],
  },
};

export const execute = async (args: { location?: string }): Promise<string> => {
  try {
    let finalLocation = args.location ? args.location.trim() : "";
    let coords: { lat: number; lon: number } | null = null;

    // Nếu người dùng không cung cấp địa điểm => Auto Detect qua IP
    if (!finalLocation) {
      logger.info(
        `[Skill: get_weather_forecast] Đang xác định vị trí tự động qua IP...`,
      );
      try {
        const ipRes = await safeFetch("http://ip-api.com/json/", {}, 5000);
        const ipData = await ipRes.json();
        if (ipData && ipData.status === "success" && ipData.lat && ipData.lon) {
          coords = { lat: ipData.lat, lon: ipData.lon };
          finalLocation = ipData.city || ipData.regionName || "Vị trí hiện tại";
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
      return "Không thể tự động xác định vị trí. Vui lòng cung cấp tên thành phố cụ thể (Ví dụ: 'Hà Nội').";
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
        mars: null as any,
        "sao hỏa": null as any,
      };

      const query = finalLocation.toLowerCase();

      if (query.includes("mars") || query.includes("sao hỏa")) {
        return "Không có trạm khí tượng nào trên Sao Hỏa. Nhiệt độ dự kiến dao động từ -125°C đến 20°C.";
      }

      coords = locMap[query];

      if (!coords) {
        // Attempt a blind geocoding via Open-Meteo's geocoding API if not hardcoded
        const geoUrl = `https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(finalLocation)}&count=1&language=vi&format=json`;
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
        } else {
          return `Hệ thống không thể định vị được địa danh "${finalLocation}". Hãy thử một tính năng khác.`;
        }
      }
    }

    if (!coords) {
      return "Lỗi nội bộ: Không xác định được tọa độ. Vui lòng nhập rõ Tên Thành Phố.";
    }

    // 2. Fetch real weather data from Open-Meteo
    const weatherUrl = `https://api.open-meteo.com/v1/forecast?latitude=${coords.lat}&longitude=${coords.lon}&current=temperature_2m,relative_humidity_2m,weather_code&timezone=auto`;
    const weatherRes = await safeFetch(weatherUrl, {}, 10000);
    // safeFetch() already throws on non-2xx — no need to check .ok

    const data = await weatherRes.json();
    const current = data.current;

    // Simplified WMO Weather Codes interpretation
    const weatherCodes: Record<number, string> = {
      0: "Trời quang đãng (Clear sky)",
      1: "Chủ yếu là nắng (Mainly clear)",
      2: "Có mây rải rác (Partly cloudy)",
      3: "Nhiều mây (Overcast)",
      45: "Có sương mù (Fog)",
      48: "Có sương mù dày đặc (Depositing rime fog)",
      51: "Mưa phùn nhẹ (Light drizzle)",
      53: "Mưa phùn (Moderate drizzle)",
      55: "Mưa phùn dày đặc (Dense drizzle)",
      61: "Mưa nhỏ (Slight rain)",
      63: "Mưa vừa (Moderate rain)",
      65: "Mưa to (Heavy rain)",
      80: "Mưa rào nhẹ (Slight rain showers)",
      81: "Mưa rào (Moderate rain showers)",
      82: "Mưa rào xối xả (Violent rain showers)",
      95: "Khả năng có dông bão (Thunderstorm)",
    };

    const conditionStr =
      weatherCodes[current.weather_code] ||
      `(Mã thời tiết: ${current.weather_code})`;

    return `Dữ liệu khí tượng cập nhật cho [${finalLocation}]:\n- Nhiệt độ: ${current.temperature_2m}°C\n- Độ ẩm: ${current.relative_humidity_2m}%\n- Tình trạng: ${conditionStr}`;
  } catch (error: any) {
    return `Truy xuất thời tiết thất bại: ${error.message}`;
  }
};
