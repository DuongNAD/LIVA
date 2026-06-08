import { describe, it, expect, vi, beforeEach } from "vitest";
import { SemanticRouter } from "../../src/memory/SemanticRouter";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

// We mock the EmbeddingService singleton and its methods
const mockEmbed = vi.fn();
const mockEmbedBatch = vi.fn();

vi.mock("../../src/services/EmbeddingService", () => {
  return {
    EmbeddingNotReadyError: class extends Error {},
    EmbeddingService: {
      getInstance: () => ({
        ensureReady: vi.fn().mockResolvedValue(undefined),
        embed: mockEmbed,
        embedBatch: mockEmbedBatch,
        embedWithTimeout: mockEmbed,
        dimension: 384,
      }),
    },
  };
});

describe("Hybrid Query Routing Tests", () => {
  // Map of query or utterance to their category index
  const routeCategories = ["chitchat", "factual_recall", "deep_reasoning", "system_command", "tool_recall", "news_briefing"];
  const kitCategories = ["OBSIDIAN_KIT", "DATA_KIT", "DEVOPS_KIT", "SOCIAL_KIT", "GENERAL_KIT"];

  const getVectorForCategory = (catIndex: number): number[] => {
    const vec = new Array(384).fill(0.0);
    vec[catIndex] = 1.0;
    return vec;
  };

  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock implementation: return vectors based on categories
    mockEmbedBatch.mockImplementation((texts: string[]) => {
      return texts.map(text => {
        const textLower = text.toLowerCase();
        
        // Check route anchors
        if (textLower.includes("chào") || textLower.includes("hello") || textLower.includes("tạm biệt")) {
          return getVectorForCategory(0);
        }
        if (textLower.includes("ai là") || textLower.includes("cái gì") || textLower.includes("ở đâu")) {
          return getVectorForCategory(1);
        }
        if (textLower.includes("tại sao") || textLower.includes("giải thích") || textLower.includes("phân tích")) {
          return getVectorForCategory(2);
        }
        if (textLower.includes("chụp") || textLower.includes("tắt") || textLower.includes("bật")) {
          return getVectorForCategory(3);
        }
        if (textLower.includes("dùng lại") || textLower.includes("chạy lại")) {
          return getVectorForCategory(4);
        }
        if (textLower.includes("tin tức") || textLower.includes("tin mới")) {
          return getVectorForCategory(5);
        }

        // Check kit anchors
        if (textLower.includes("obsidian") || textLower.includes("note")) {
          return getVectorForCategory(6);
        }
        if (textLower.includes("excel") || textLower.includes("dữ liệu")) {
          return getVectorForCategory(7);
        }
        if (textLower.includes("git") || textLower.includes("docker")) {
          return getVectorForCategory(8);
        }
        if (textLower.includes("linkedin") || textLower.includes("calendar")) {
          return getVectorForCategory(9);
        }
        
        return getVectorForCategory(10); // GENERAL_KIT fallback
      });
    });

    mockEmbed.mockImplementation((text: string) => {
      const textLower = text.toLowerCase();
      if (textLower.includes("hello") || textLower.includes("chào")) return getVectorForCategory(0);
      if (textLower.includes("ai là") || textLower.includes("thời tiết")) return getVectorForCategory(1);
      if (textLower.includes("phân tích") || textLower.includes("tại sao")) return getVectorForCategory(2);
      if (textLower.includes("chụp") || textLower.includes("lệnh")) return getVectorForCategory(3);
      if (textLower.includes("chạy lại")) return getVectorForCategory(4);
      if (textLower.includes("tin tức")) return getVectorForCategory(5);
      
      return getVectorForCategory(10); // GENERAL_KIT fallback
    });
  });

  it("should initialize SemanticRouter and build anchor vectors", async () => {
    const router = new SemanticRouter();
    await router.initialize();
    expect(router.ready).toBe(true);
    expect(mockEmbedBatch).toHaveBeenCalled();
  });

  it("should route simple/social queries to chitchat route", async () => {
    const router = new SemanticRouter();
    const result = await router.route("Hello LIVA, chúc bạn buổi sáng tốt lành");
    
    expect(result.route).toBe("chitchat");
  });

  it("should route complex analytical queries to deep_reasoning route", async () => {
    const router = new SemanticRouter();
    const result = await router.route("Phân tích thuật toán mã hóa AES-256");

    expect(result.route).toBe("deep_reasoning");
  });

  it("should bypass LLM reasoning using exact tool cache (fast-path)", async () => {
    const router = new SemanticRouter();
    
    // Record action in cache
    await router.recordAction("chụp màn hình desktop", "screenshot_capture", { delay: 1 });

    // Query exact cached command
    const result = await router.route("chụp màn hình desktop");

    expect(result.route).toBe("system_command");
    expect(result.cachedAction).toBeDefined();
    expect(result.cachedAction?.toolName).toBe("screenshot_capture");
    expect(result.cachedAction?.toolArgs).toEqual({ delay: 1 });
  });

  it("should evaluate classification accuracy over mock dataset", async () => {
    const router = new SemanticRouter();
    await router.initialize();

    const testSet = [
      { query: "Xin chào bạn nhé", expectedRoute: "chitchat" },
      { query: "Tạm biệt nha", expectedRoute: "chitchat" },
      { query: "Ai là tổng thống Mỹ?", expectedRoute: "factual_recall" },
      { query: "Tại sao lá cây màu xanh?", expectedRoute: "deep_reasoning" },
      { query: "Giải thích cơ chế đồng thuận blockchain", expectedRoute: "deep_reasoning" },
      { query: "Bật nhạc chill đi", expectedRoute: "system_command" },
      { query: "Chạy lại lệnh vừa rồi", expectedRoute: "tool_recall" },
      { query: "Tin tức ngày hôm nay thế nào?", expectedRoute: "news_briefing" },
    ];

    let correctCount = 0;
    for (const item of testSet) {
      const result = await router.route(item.query);
      if (result.route === item.expectedRoute) {
        correctCount++;
      }
    }

    const accuracy = correctCount / testSet.length;
    expect(accuracy).toBeGreaterThanOrEqual(0.75); // Ensure high routing accuracy
  });
});
