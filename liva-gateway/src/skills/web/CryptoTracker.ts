import { safeFetch } from "../../utils/HttpClient";
import { logger } from "../../utils/logger";

export const metadata = {
  name: "crypto_tracker",
  search_keywords: ["giá coin", "bitcoin", "crypto", "tiền ảo", "ethereum", "thị trường coin", "giá btc"],
  description:
    "[AUTO_RUN] Theo dõi và báo cáo giá tiền điện tử (Cryptocurrency) theo thời gian thực (ví dụ: BTC, ETH, SOL, BNB). Trả về giá hiện tại theo USD và VND cùng biến động 24h.",
  parameters: {
    type: "object",
    properties: {
      symbols: {
        type: "array",
        items: {
            type: "string"
        },
        description: "Danh sách các mã coin (symbol) cần tra cứu (ví dụ: ['btc', 'eth', 'sol']). Nếu người dùng hỏi chung chung, hãy tự động liệt kê ['btc', 'eth', 'bnb', 'sol'].",
      },
    },
    required: ["symbols"],
  },
};

// Map common symbols to CoinGecko IDs
const COMMON_COINS: Record<string, string> = {
    "btc": "bitcoin",
    "eth": "ethereum",
    "bnb": "binancecoin",
    "sol": "solana",
    "xrp": "ripple",
    "ada": "cardano",
    "doge": "dogecoin",
    "dot": "polkadot",
    "trx": "tron",
    "matic": "matic-network",
    "link": "chainlink",
    "ton": "the-open-network",
    "avax": "avalanche-2",
    "shib": "shiba-inu",
    "bch": "bitcoin-cash",
    "ltc": "litecoin",
    "uni": "uniswap",
    "near": "near",
    "apt": "aptos",
    "sui": "sui"
};

export const execute = async (args: { symbols: string[] }): Promise<string> => {
  try {
    if (!args.symbols || args.symbols.length === 0) {
        args.symbols = ["btc", "eth", "bnb", "sol"];
    }

    logger.info(`[Skill: crypto_tracker] Đang tra cứu giá cho: ${args.symbols.join(", ")}`);

    // Chuyển đổi symbol sang CoinGecko ID
    const idsToFetch: string[] = [];
    const symbolMap = new Map<string, string>(); // geckoId -> symbol

    for (let sym of args.symbols) {
        sym = sym.toLowerCase().trim();
        const geckoId = COMMON_COINS[sym];
        if (geckoId) {
            idsToFetch.push(geckoId);
            symbolMap.set(geckoId, sym.toUpperCase());
        } else {
            // Thử search API nếu không có trong hardcode map
            try {
                const searchRes = await safeFetch(`https://api.coingecko.com/api/v3/search?query=${sym}`, undefined, 5000);
                const searchData = await searchRes.json() as { coins: { id: string, symbol: string }[] };
                if (searchData && searchData.coins && searchData.coins.length > 0) {
                    const topResult = searchData.coins[0];
                    idsToFetch.push(topResult.id);
                    symbolMap.set(topResult.id, topResult.symbol.toUpperCase());
                } else {
                    logger.warn(`[crypto_tracker] Không tìm thấy CoinGecko ID cho symbol: ${sym}`);
                }
            } catch (err: unknown) {
                const warnMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[crypto_tracker] Lỗi tìm kiếm mã coin ${sym}: ${warnMsg}`);
            }
        }
    }

    if (idsToFetch.length === 0) {
        return `[SYSTEM_ERROR] Không thể tìm thấy thông tin cho các mã coin đã cung cấp. Vui lòng thử lại với các mã phổ biến như BTC, ETH.`;
    }

    // Call Simple Price API
    const idsString = idsToFetch.join(",");
    const url = `https://api.coingecko.com/api/v3/simple/price?ids=${idsString}&vs_currencies=usd,vnd&include_24hr_change=true`;
    
    const response = await safeFetch(url, {
        headers: {
            "Accept": "application/json"
        }
    }, 10000);

    const data = await response.json() as Record<string, { usd: number, vnd: number, usd_24h_change: number }>;
    
    let output = `[Crypto Tracker] Tỷ giá tiền điện tử thời gian thực (Nguồn: CoinGecko):\n\n`;

    for (const id of idsToFetch) {
        const coinData = data[id];
        if (coinData) {
            const symbol = symbolMap.get(id) || id.toUpperCase();
            const priceUsd = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(coinData.usd);
            const priceVnd = new Intl.NumberFormat('vi-VN', { style: 'currency', currency: 'VND' }).format(coinData.vnd);
            
            const changeRaw = coinData.usd_24h_change || 0;
            const changeStr = changeRaw > 0 ? `+${changeRaw.toFixed(2)}% 📈` : `${changeRaw.toFixed(2)}% 📉`;
            
            output += `🪙 **${symbol}**\n`;
            output += `   - Giá USD: ${priceUsd}\n`;
            output += `   - Giá VND: ${priceVnd}\n`;
            output += `   - Biến động 24h: ${changeStr}\n\n`;
        }
    }

    output += `(💡 SYSTEM NOTE: Hãy tóm tắt và báo cáo cho người dùng bằng giọng điệu tự nhiên. Nếu có biến động mạnh (>5%), hãy nhấn mạnh.)`;
    return output;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[crypto_tracker] Error: ${errMsg}`);
    return `[SYSTEM_ERROR] Không thể lấy giá coin lúc này do lỗi API (CoinGecko có thể bị rate limit). Lỗi: ${errMsg}`;
  }
};
