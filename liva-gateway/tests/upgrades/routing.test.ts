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

  const getVectorForText = (text: string): number[] => {
    const textLower = text.toLowerCase();
    
    // 1. Chitchat
    if (
      textLower.includes("chào") || textLower.includes("hello") || textLower.includes("tạm biệt") || 
      textLower.includes("hi") || textLower.includes("cảm ơn") || textLower.includes("khỏe") || 
      textLower.includes("thế nào") || textLower.includes("morning") || textLower.includes("thank") || 
      textLower.includes("bye") || textLower.includes("tên gì") || textLower.includes("cười") || 
      textLower.includes("vui") || textLower.includes("nhé") || textLower.includes("nha")
    ) {
      return getVectorForCategory(0);
    }
    
    // 2. Factual recall / KG / Vector recall
    if (
      textLower.includes("ai là") || textLower.includes("cái gì") || textLower.includes("ở đâu") || 
      textLower.includes("bao giờ") || textLower.includes("thông tin") || textLower.includes("tìm kiếm") || 
      textLower.includes("nhớ") || textLower.includes("lịch sử") || textLower.includes("what") || 
      textLower.includes("who") || textLower.includes("when") || textLower.includes("tell") || 
      textLower.includes("thời tiết") || textLower.includes("mỹ") || textLower.includes("quan hệ") ||
      textLower.includes("liên kết") || textLower.includes("kết nối") || textLower.includes("recall") ||
      textLower.includes("retrieve")
    ) {
      return getVectorForCategory(1);
    }
    
    // 3. Deep reasoning
    if (
      textLower.includes("tại sao") || textLower.includes("giải thích") || textLower.includes("phân tích") || 
      textLower.includes("so sánh") || textLower.includes("code") || textLower.includes("kế hoạch") || 
      textLower.includes("lập trình") || textLower.includes("thiết kế") || textLower.includes("đánh giá") || 
      textLower.includes("why") || textLower.includes("explain") || textLower.includes("write") || 
      textLower.includes("analyze") || textLower.includes("create") || textLower.includes("review") || 
      textLower.includes("debug") || textLower.includes("nghiên cứu") || textLower.includes("cơ chế")
    ) {
      return getVectorForCategory(2);
    }
    
    // 4. Tool recall (Check this before system_command because tool_recall anchors have "lại" or "lệnh")
    if (
      textLower.includes("dùng lại") || textLower.includes("chạy lại") || textLower.includes("lần trước") || 
      textLower.includes("again") || textLower.includes("repeat") || textLower.includes("lại")
    ) {
      return getVectorForCategory(4);
    }
    
    // 5. System command
    if (
      textLower.includes("chụp") || textLower.includes("tắt") || textLower.includes("bật") || 
      textLower.includes("xóa") || textLower.includes("mở") || textLower.includes("dọn dẹp") || 
      textLower.includes("dừng") || textLower.includes("thoát") || textLower.includes("lệnh") || 
      textLower.includes("zalo") || textLower.includes("email") || textLower.includes("trình duyệt") || 
      textLower.includes("đọc file") || textLower.includes("ghi file") || textLower.includes("execute") || 
      textLower.includes("screenshot") || textLower.includes("message") || textLower.includes("browser")
    ) {
      return getVectorForCategory(3);
    }
    
    // 6. News briefing
    if (
      textLower.includes("tin") || textLower.includes("news") || 
      textLower.includes("briefing") || textLower.includes("hot")
    ) {
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
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockEmbedBatch.mockImplementation((texts: string[]) => {
      return texts.map(getVectorForText);
    });

    mockEmbed.mockImplementation((text: string) => {
      return getVectorForText(text);
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
