import { safeFetch } from "../../utils/HttpClient";
import { logger } from "../../utils/logger";

export const metadata = {
  name: "location_search",
  search_keywords: ["bản đồ", "địa điểm", "vị trí", "google map", "địa chỉ", "khoảng cách", "đường đi", "ở đâu", "map", "location", "position", "address", "distance", "route", "where"],
  description:
    "[AUTO_RUN] Look up geographic coordinates, exact map links, and details for a SPECIFIC location. Returns OpenStreetMap data and Google Maps URL.\nCRITICAL RULE: Do NOT use this tool for generic POI queries (e.g., 'fun places', 'cafes'). For generic lists, you MUST call 'web_search' first to get recommendations, then optionally use 'location_search' to get the exact map coordinates of a specific chosen place.",
  parameters: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "[LOCALIZED] The specific location name, street, or address to search (e.g., 'Landmark 81', 'Hoan Kiem Lake'). Provide the query in the user's language.",
      },
    },
    required: ["query"],
  },
};

export const execute = async (args: { query: string }): Promise<string> => {
  try {
    const queryLower = args.query.toLowerCase();
    const genericTerms = ["vui", "chơi", "giải trí", "cafe", "nhà hàng", "quán", "fun places", "places to", "interesting", "top", "list", "đẹp", "ăn"];
    if (genericTerms.some(term => queryLower.includes(term))) {
        throw new Error(`[WRONG_TOOL_ERROR]: You are searching for a generic list of places ('${args.query}'). 'location_search' is ONLY for looking up GPS coordinates of a SPECIFIC location (e.g., 'Landmark 81'). MANDATORY ACTION: Immediately call 'web_search' to find recommendations instead of apologizing to the user!`);
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
        address: any;
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
