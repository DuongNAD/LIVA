import { safeFetch } from "../../utils/HttpClient";
import { logger } from "../../utils/logger";

export const metadata = {
  name: "location_search",
  search_keywords: ["bản đồ", "địa điểm", "vị trí", "google map", "địa chỉ", "khoảng cách", "đường đi", "ở đâu", "map", "location", "position", "address", "distance", "route", "where", "gần đây", "chỗ chơi"],
  description:
    "[AUTO_RUN] Tra cứu tọa độ, link bản đồ chính xác cho một địa điểm CỤ THỂ HOẶC tìm kiếm các địa điểm chung chung (ví dụ: 'chỗ chơi gần đây', 'quán cafe'). Trả về link Google Maps trực tiếp để hướng dẫn người dùng.",
  parameters: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "[LOCALIZED] The specific location name, street, or address to search (e.g., 'Landmark 81', 'quán cafe gần đây'). Provide the query in the user's language.",
      },
    },
    required: ["query"],
  },
};

export const execute = async (args: { query: string }): Promise<string> => {
  try {
    const queryLower = args.query.toLowerCase();
    const genericTerms = ["vui", "chơi", "giải trí", "cafe", "nhà hàng", "quán", "fun places", "places to", "interesting", "top", "list", "đẹp", "ăn", "gần đây"];
    const isGeneric = genericTerms.some(term => queryLower.includes(term));

    if (isGeneric) {
        logger.info(`[Skill: location_search] Xử lý truy vấn POI chung chung: "${args.query}"`);
        const googleMapsSearchLink = `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(args.query)}`;
        let output = `[Location Search Results] Kết quả tra cứu cho "${args.query}":\n\n`;
        output += `📍 Đây là câu truy vấn tìm kiếm địa điểm chung chung (Generic POI). Dưới đây là link tìm kiếm trực tiếp trên Google Maps:\n`;
        output += `- Google Maps Link: ${googleMapsSearchLink}\n\n`;
        output += `(💡 SYSTEM NOTE: Hãy cung cấp link Google Maps này cho người dùng để họ tiện xem trên bản đồ. Đồng thời, nếu muốn gợi ý danh sách cụ thể, BẠN CÓ THỂ GỌI THÊM KỸ NĂNG 'web_search' ngay trong lượt này để đọc các bài review!)`;
        return output;
    }

    logger.info(`[Skill: location_search] Đang tra cứu địa điểm: "${args.query}"`);

    // Dùng API Nominatim của OpenStreetMap (miễn phí, không cần API Key)
    const url = `https://nominatim.openstreetmap.org/search?q=${encodeURIComponent(args.query)}&format=json&addressdetails=1&limit=3`;
    
    const response = await safeFetch(url, {
      headers: {
        // Nominatim requires a valid User-Agent
        "User-Agent": "LIVA-AI-Agent/1.0",
        "Accept-Language": "vi-VN,vi;q=0.9,en;q=0.8"
      },
    }, 10000);

    const data = await response.json() as Array<{
        place_id: number;
        lat: string;
        lon: string;
        display_name: string;
        type: string;
        address: Record<string, string>;
    }>;

    if (!data || data.length === 0) {
      return `[SYSTEM_ERROR]: No geographic information found for "${args.query}". Try a shorter keyword or an exact street name.`;
    }

    let output = `[Location Search Results] Coordinates and details for "${args.query}":\n\n`;

    data.forEach((place, index) => {
        const lat = parseFloat(place.lat).toFixed(5);
        const lon = parseFloat(place.lon).toFixed(5);
        const googleMapsLink = `https://www.google.com/maps/search/?api=1&query=${lat},${lon}`;
        
        output += `📍 **Location ${index + 1}:**\n`;
        output += `- Full Name/Address: ${place.display_name}\n`;
        output += `- Category/Type: ${place.type}\n`;
        output += `- Coordinates (Lat, Lon): ${lat}, ${lon}\n`;
        output += `- Google Maps Link: ${googleMapsLink}\n\n`;
    });

    output += `(💡 SYSTEM NOTE: Use the exact coordinates or the Google Maps link to guide the user. You can present this in the user's preferred language.)`;

    return output;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[location_search] Search error: ${errMsg}`);
    return `[SYSTEM_ERROR] Location search failed: ${errMsg}`;
  }
};
