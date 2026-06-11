import { describe, it, expect } from "vitest";
import { ChitchatFastPath } from "../../src/core/ChitchatFastPath";

describe("ChitchatFastPath", () => {
    it("should return null for empty or long queries", () => {
        expect(ChitchatFastPath.matchAndRespond("")).toBeNull();
        expect(ChitchatFastPath.matchAndRespond("   ")).toBeNull();
        expect(ChitchatFastPath.matchAndRespond("a".repeat(65))).toBeNull();
    });

    it("should match Vietnamese greeting queries", () => {
        const response = ChitchatFastPath.matchAndRespond("chào sếp");
        expect(response).toBeDefined();
        expect(typeof response).toBe("string");
        expect(response).not.toBeNull();
    });

    it("should match English greeting queries", () => {
        const response = ChitchatFastPath.matchAndRespond("hello");
        expect(response).toBeDefined();
        expect(typeof response).toBe("string");
        expect(response).not.toBeNull();
    });

    it("should match identity queries", () => {
        const responseVi = ChitchatFastPath.matchAndRespond("bạn là ai");
        expect(responseVi).not.toBeNull();
        
        const responseEn = ChitchatFastPath.matchAndRespond("who are you");
        expect(responseEn).not.toBeNull();
    });

    it("should match goodbye queries", () => {
        const responseVi = ChitchatFastPath.matchAndRespond("tạm biệt");
        expect(responseVi).not.toBeNull();

        const responseEn = ChitchatFastPath.matchAndRespond("bye bye");
        expect(responseEn).not.toBeNull();
    });

    it("should match thanks queries", () => {
        const responseVi = ChitchatFastPath.matchAndRespond("cảm ơn em");
        expect(responseVi).not.toBeNull();

        const responseEn = ChitchatFastPath.matchAndRespond("thank you");
        expect(responseEn).not.toBeNull();
    });

    it("should match health/pleasantries queries", () => {
        const responseVi = ChitchatFastPath.matchAndRespond("khỏe không");
        expect(responseVi).not.toBeNull();

        const responseEn = ChitchatFastPath.matchAndRespond("how's it going");
        expect(responseEn).not.toBeNull();
    });

    it("should return null for non-chitchat queries", () => {
        expect(ChitchatFastPath.matchAndRespond("Run git sync command")).toBeNull();
        expect(ChitchatFastPath.matchAndRespond("gửi tin nhắn cho Vợ yêu")).toBeNull();
    });
});
