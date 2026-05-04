import { describe, it, expect, vi } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        child: vi.fn().mockReturnValue({ info: vi.fn() })
    }
}));

import { evoLogger } from "../../src/evolution/EvolutionLogger";

describe("EvolutionLogger", () => {
    it("should export an instance of logger", () => {
        expect(evoLogger).toBeDefined();
    });
});
