import { describe, it, expect, beforeEach } from "vitest";
import { TTSFormatter } from "@utils/TTSFormatter";

describe("TTSFormatter — Semantic Clause Chunking", () => {
    let fmt: TTSFormatter;

    beforeEach(() => {
        fmt = new TTSFormatter();
    });

    // ============================================================
    // Priority 1: Sentence Boundary (.?!\n)
    // ============================================================
    describe("Sentence Boundary (Priority 1)", () => {
        it("should split on period at end of sentence", () => {
            const result = fmt.pushToken("Xin chào bạn. ");
            expect(result).toBe("Xin chào bạn.");
        });

        it("should split on question mark", () => {
            const result = fmt.pushToken("Bạn có khỏe không? ");
            expect(result).toBe("Bạn có khỏe không?");
        });

        it("should split on exclamation mark", () => {
            const result = fmt.pushToken("Tuyệt vời! ");
            expect(result).toBe("Tuyệt vời!");
        });

        it("should split on newline", () => {
            const result = fmt.pushToken("Xin chào\n");
            expect(result).toBe("Xin chào");
        });

        it("should NOT split on decimal numbers (1.5)", () => {
            const result = fmt.pushToken("Giá là 1.5 triệu");
            expect(result).toBeNull();
        });

        it("should NOT split on abbreviations like VD. TP.", () => {
            const result = fmt.pushToken("VD. ");
            expect(result).toBeNull();
        });

        it("should return null when no boundary detected", () => {
            const result = fmt.pushToken("Đang nói dở");
            expect(result).toBeNull();
        });
    });

    // ============================================================
    // Priority 2: Clause Punctuation (, : ; —)
    // ============================================================
    describe("Clause Punctuation (Priority 2)", () => {
        it("should split on comma when buffer exceeds MAX_BUFFER_BEFORE_CLAUSE (60 chars)", () => {
            // Need >60 chars before the comma to trigger clause split (buffer must exceed 60)
            const longText = "Em đã tìm kiếm thông tin trên rất nhiều nguồn dữ liệu khác nhau rồi, tiếp";
            const result = fmt.pushToken(longText);
            expect(result).not.toBeNull();
        });

        it("should NOT split on comma if buffer is short", () => {
            const result = fmt.pushToken("Dạ, ");
            expect(result).toBeNull();
        });

        it("should NOT split on comma inside numbers (1,000)", () => {
            const result = fmt.pushToken("Có 1,000 người tham gia");
            expect(result).toBeNull();
        });
    });

    // ============================================================
    // Priority 3: Vietnamese Conjunction Split
    // ============================================================
    describe("Vietnamese Conjunction Split (Priority 3)", () => {
        it("should split before 'nhưng' in long buffer", () => {
            const text = "Em đã hoàn thành xong tất cả các công việc hôm nay rồi nhưng vẫn còn vài thứ";
            const result = fmt.pushToken(text);
            expect(result).not.toBeNull();
        });

        it("should NOT split conjunctions in short buffer (<40 chars)", () => {
            const result = fmt.pushToken("Ăn cơm nhưng no");
            expect(result).toBeNull();
        });
    });

    // ============================================================
    // Priority 4: Word Count Overflow
    // ============================================================
    describe("Word Count Overflow (Priority 4)", () => {
        it("should force split after 25+ words without any boundary", () => {
            const words = Array(30).fill("từ").join(" ");
            const result = fmt.pushToken(words);
            expect(result).not.toBeNull();
        });
    });

    // ============================================================
    // flush() — End of stream
    // ============================================================
    describe("flush()", () => {
        it("should return remaining buffer content", () => {
            fmt.pushToken("Còn lại đây");
            const flushed = fmt.flush();
            expect(flushed).toBe("Còn lại đây");
        });

        it("should return null for empty buffer", () => {
            expect(fmt.flush()).toBeNull();
        });

        it("should return null for whitespace-only buffer", () => {
            fmt.pushToken("   ");
            expect(fmt.flush()).toBeNull();
        });

        it("should return null for emoji-only buffer", () => {
            fmt.pushToken("😀🎉");
            expect(fmt.flush()).toBeNull();
        });

        it("should clear the buffer after flush", () => {
            fmt.pushToken("Test");
            fmt.flush();
            expect(fmt.flush()).toBeNull();
        });
    });

    // ============================================================
    // reset()
    // ============================================================
    describe("reset()", () => {
        it("should clear the buffer", () => {
            fmt.pushToken("Something pending");
            fmt.reset();
            expect(fmt.flush()).toBeNull();
        });
    });

    // ============================================================
    // Sanitization — tested via pushToken sentence splits
    // The sanitizer runs INSIDE pushToken/flush, so we test by
    // feeding sentences ending with ". " and checking the output.
    // ============================================================
    describe("Sanitization", () => {
        it("should strip inline code backticks", () => {
            const result = fmt.pushToken("Dùng `npm install` nhé. ");
            expect(result).toBeTruthy();
            expect(result).not.toContain("`");
            expect(result).toContain("nhé");
        });

        it("should strip URLs", () => {
            const result = fmt.pushToken("Truy cập https://example.com/path nhé. ");
            expect(result).toBeTruthy();
            expect(result).not.toContain("https://");
        });

        it("should extract text from markdown links", () => {
            const result = fmt.pushToken("[Google](https://google.com) tốt nhất. ");
            expect(result).toBeTruthy();
            expect(result).toContain("Google");
            expect(result).not.toContain("https://google.com");
        });

        it("should strip bold/italic markdown formatting", () => {
            const result = fmt.pushToken("Đây là **bold** và *italic* text. ");
            expect(result).toBeTruthy();
            expect(result).toContain("bold");
            expect(result).toContain("italic");
            expect(result).not.toContain("**");
        });

        it("should strip emoji from output", () => {
            const result = fmt.pushToken("Xin chào 😀 bạn nhé. ");
            expect(result).toBeTruthy();
            expect(result).not.toContain("😀");
            expect(result).toContain("Xin chào");
        });

        it("should replace angle brackets with spaces", () => {
            const result = fmt.pushToken("Tag <tool_call> đây nhé. ");
            expect(result).toBeTruthy();
            expect(result).not.toContain("<");
            expect(result).not.toContain(">");
        });

        it("should collapse multiple whitespace to single space", () => {
            const result = fmt.pushToken("Xin     chào    bạn. ");
            expect(result).toBeTruthy();
            expect(result).not.toMatch(/\s{2,}/);
        });

        it("should strip markdown headers via flush", () => {
            fmt.pushToken("## Heading here");
            const result = fmt.flush();
            expect(result).toBeTruthy();
            expect(result).not.toContain("##");
            expect(result).toContain("Heading here");
        });

        it("should strip blockquote markers via flush", () => {
            fmt.pushToken("> Quoted text");
            const result = fmt.flush();
            expect(result).toBeTruthy();
            expect(result).toContain("Quoted text");
        });

        it("should strip list markers via flush", () => {
            fmt.pushToken("- Item one here");
            const result = fmt.flush();
            expect(result).toBeTruthy();
            expect(result).toContain("Item one here");
            expect(result).not.toMatch(/^- /);
        });

        it("should strip ordered list markers via flush", () => {
            fmt.pushToken("1. First item here");
            const result = fmt.flush();
            expect(result).toBeTruthy();
            expect(result).toContain("First item here");
        });

        it("should strip fenced code blocks", () => {
            // The ``` code block is fully contained, and "Done." triggers sentence boundary
            const result = fmt.pushToken("Here ```code``` done. ");
            expect(result).toBeTruthy();
            expect(result).not.toContain("code");
        });

        it("should strip strikethrough markdown", () => {
            const result = fmt.pushToken("Đã ~~xong~~ rồi nhé. ");
            expect(result).toBeTruthy();
            expect(result).toContain("xong");
            expect(result).not.toContain("~~");
        });
    });

    // ============================================================
    // Multi-token streaming simulation
    // ============================================================
    describe("Multi-token Streaming", () => {
        it("should accumulate tokens until boundary found", () => {
            expect(fmt.pushToken("Xin ")).toBeNull();
            expect(fmt.pushToken("chào ")).toBeNull();
            expect(fmt.pushToken("bạn. ")).toBe("Xin chào bạn.");
        });

        it("should handle consecutive sentences", () => {
            expect(fmt.pushToken("Câu một. ")).toBe("Câu một.");
            expect(fmt.pushToken("Câu hai. ")).toBe("Câu hai.");
        });

        it("should return remaining via flush after streaming", () => {
            fmt.pushToken("Câu một. ");
            const s1 = fmt.pushToken("");
            // sentence boundary already matched on pushToken above
            fmt.pushToken("Chưa kết thúc");
            const remaining = fmt.flush();
            expect(remaining).toBe("Chưa kết thúc");
        });
    });
});
