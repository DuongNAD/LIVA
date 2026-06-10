import { describe, it, expect } from "vitest";
import { DocumentChunker } from "../../src/services/DocumentChunker";

describe("DocumentChunker", () => {
    const chunker = DocumentChunker.getInstance();

    it("should handle empty or whitespace-only inputs", () => {
        expect(chunker.chunkDocument("", "source.txt")).toEqual([]);
        expect(chunker.chunkDocument("   \n   ", "source.txt")).toEqual([]);
    });

    it("should split simple text into chunks based on word count limit", () => {
        const text = "word ".repeat(300); // 300 words
        const chunks = chunker.chunkDocument(text, "test.txt", { maxWords: 100, overlapWords: 20 });

        expect(chunks.length).toBeGreaterThan(1);
        expect(chunks[0].metadata.doc_source).toBe("test.txt");
        expect(chunks[0].metadata.chunk_index).toBe(0);
        expect(chunks[0].metadata.token_count).toBeGreaterThan(0);
        expect(chunks[0].metadata.section_title).toBe("General");
    });

    it("should parse markdown headers and nest section titles correctly", () => {
        const markdown = `# Title 1
This is text in section 1.

## Subtitle 1.1
This is text in subsection 1.1.

# Title 2
This is text in section 2.`;

        const chunks = chunker.chunkDocument(markdown, "test.md", { maxWords: 5, overlapWords: 1 });

        // Validate section titles are correctly assigned
        const titles = chunks.map(c => c.metadata.section_title);
        expect(titles).toContain("Title 1");
        expect(titles).toContain("Title 1 > Subtitle 1.1");
        expect(titles).toContain("Title 2");
    });

    it("should preserve code block boundaries and wrap split code blocks correctly", () => {
        const codeText = "line\n".repeat(80); // a long body of code
        const markdown = `Here is some code:

\`\`\`typescript
${codeText}\`\`\``;

        const chunks = chunker.chunkDocument(markdown, "code.md", { maxWords: 30, overlapWords: 5 });

        expect(chunks.length).toBeGreaterThan(1);
        for (const chunk of chunks) {
            if (chunk.content.includes("line")) {
                expect(chunk.content).toContain("\`\`\`typescript");
                expect(chunk.content.trim().endsWith("\`\`\`")).toBe(true);
            }
        }
    });

    it("should split large paragraphs into sentences and words if necessary", () => {
        const longSentence = "This is an extremely long sentence with many words that will definitely exceed the small chunk limit of ten words. ".repeat(3);
        const chunks = chunker.chunkDocument(longSentence, "long.txt", { maxWords: 15, overlapWords: 3 });

        expect(chunks.length).toBeGreaterThan(1);
        for (const chunk of chunks) {
            expect(chunk.content.split(/\s+/).length).toBeLessThanOrEqual(25); // Allow some leeway
        }
    });

    it("should correctly handle sliding window overlap", () => {
        const text = "one. two. three. four. five. six. seven. eight. nine. ten.";
        const chunks = chunker.chunkDocument(text, "overlap.txt", { maxWords: 5, overlapWords: 2 });

        expect(chunks.length).toBeGreaterThan(1);
        const words0 = chunks[0].content.split(/\s+/);
        const words1 = chunks[1].content.split(/\s+/);

        const overlapWord = words0[words0.length - 1];
        expect(words1.slice(0, 3)).toContain(overlapWord);
    });
});
