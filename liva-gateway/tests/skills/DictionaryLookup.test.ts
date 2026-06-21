import { describe, it, expect, vi, beforeEach } from "vitest";
import { metadata, execute } from "../../src/skills/core/DictionaryLookup";
import { GeminiAPI } from "../../src/tools/GeminiAPI";

vi.mock("../../src/utils/logger", () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/tools/GeminiAPI", () => ({
  GeminiAPI: {
    generateStructured: vi.fn(),
  },
}));

describe("DictionaryLookup", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should have correct metadata", () => {
    expect(metadata.name).toBe("dictionary_lookup");
    expect(metadata.category).toBe("core");
    expect(metadata.parameters.required).toContain("word");
  });

  it("should fail when no word is provided", async () => {
    const res = await execute({ word: "" });
    expect(res).toContain("Error");
  });

  it("should look up a word successfully", async () => {
    const mockResult = {
      word: "benevolent",
      phonetics: "/bəˈnevələnt/",
      partsOfSpeech: [
        {
          pos: "adjective",
          definitions: ["Well meaning and kindly."],
          examples: ["a benevolent smile"]
        }
      ],
      synonyms: ["kind", "kindly", "caring"],
      antonyms: ["malevolent", "spiteful"],
      translation: "nhân từ, rộng lượng"
    };

    (GeminiAPI.generateStructured as any).mockResolvedValueOnce(mockResult);

    const res = await execute({ word: "benevolent" });

    expect(res).toContain("# Dictionary Lookup: **benevolent**");
    expect(res).toContain("Phonetics:* `/bəˈnevələnt/`");
    expect(res).toContain("Translation:* **nhân từ, rộng lượng**");
    expect(res).toContain("### *adjective*");
    expect(res).toContain("1. Well meaning and kindly.");
    expect(res).toContain('*Example:* "a benevolent smile"');
    expect(res).toContain("**Synonyms:** kind, kindly, caring");
    expect(res).toContain("**Antonyms:** malevolent, spiteful");
  });

  it("should handle empty API response", async () => {
    (GeminiAPI.generateStructured as any).mockResolvedValueOnce({});

    const res = await execute({ word: "xyz" });
    expect(res).toContain("Failed to look up word");
  });

  it("should handle API failure", async () => {
    (GeminiAPI.generateStructured as any).mockRejectedValueOnce(new Error("API Connection Failed"));

    const res = await execute({ word: "hello" });
    expect(res).toContain("Error: API Connection Failed");
  });

  it("should fail on invalid arguments (non-object or missing word)", async () => {
    // @ts-expect-error - Testing runtime invalid args
    const res1 = await execute(null);
    expect(res1).toContain("Error");

    // @ts-expect-error - Testing runtime invalid args
    const res2 = await execute("hello");
    expect(res2).toContain("Error");

    // @ts-expect-error - Testing runtime invalid args
    const res3 = await execute({});
    expect(res3).toContain("Error");
    expect(res3).toContain("No word provided");
  });

  it("should fail when word parameter exceeds 100 characters", async () => {
    const longWord = "a".repeat(101);
    const res = await execute({ word: longWord });
    expect(res).toContain("Error");
    expect(res).toContain("exceeds maximum length");
  });

  it("should defensive-check against null items, missing arrays, and index mismatches in partsOfSpeech", async () => {
    const mockResult = {
      word: "anomaly",
      phonetics: "/əˈnɒm.ə.li/",
      partsOfSpeech: [
        null,
        {
          pos: "noun",
          definitions: null as any,
          examples: ["example 1"]
        },
        {
          pos: "noun",
          definitions: ["Something that deviates from what is standard."],
          examples: undefined as any
        },
        {
          pos: "noun",
          definitions: ["Another definition."],
          examples: []
        }
      ],
      synonyms: null as any,
      antonyms: undefined as any,
      translation: "sự dị thường"
    };

    (GeminiAPI.generateStructured as any).mockResolvedValueOnce(mockResult);

    const res = await execute({ word: "anomaly" });

    expect(res).toContain("# Dictionary Lookup: **anomaly**");
    expect(res).toContain("### *noun*");
    expect(res).toContain("Something that deviates from what is standard.");
    expect(res).toContain("Another definition.");
    expect(res).not.toContain("**Synonyms:**");
    expect(res).not.toContain("**Antonyms:**");
  });
});
