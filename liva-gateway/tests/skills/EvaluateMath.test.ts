import { describe, it, expect, vi, beforeEach } from "vitest";
import { metadata, execute } from "../../src/skills/core/EvaluateMath";

vi.mock("../../src/utils/logger", () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

describe("EvaluateMath", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should have correct metadata", () => {
    expect(metadata.name).toBe("evaluate_math_expression");
    expect(metadata.category).toBe("core");
    expect(metadata.parameters.required).toContain("expression");
  });

  it("should evaluate basic arithmetic", async () => {
    const res1 = await execute({ expression: "2 + 3 * 4" });
    expect(res1).toBe("14");

    const res2 = await execute({ expression: "10 - 4 / 2" });
    expect(res2).toBe("8");

    const res3 = await execute({ expression: "5 % 2" });
    expect(res3).toBe("1");

    const res4 = await execute({ expression: "2 ^ 3" });
    expect(res4).toBe("8");
  });

  it("should respect parentheses and unary operators", async () => {
    const res1 = await execute({ expression: "(2 + 3) * 4" });
    expect(res1).toBe("20");

    const res2 = await execute({ expression: "-5 + 3" });
    expect(res2).toBe("-2");

    const res3 = await execute({ expression: "--5" });
    expect(res3).toBe("5");
  });

  it("should support constants and functions", async () => {
    const res1 = await execute({ expression: "abs(-10)" });
    expect(res1).toBe("10");

    const res2 = await execute({ expression: "sqrt(16)" });
    expect(res2).toBe("4");

    const res3 = await execute({ expression: "ln(e)" });
    expect(Number(res3)).toBeCloseTo(1);

    const res4 = await execute({ expression: "log(100)" });
    expect(res4).toBe("2");

    const res5 = await execute({ expression: "sin(pi / 2)" });
    expect(Number(res5)).toBeCloseTo(1);

    const res6 = await execute({ expression: "cos(0)" });
    expect(res6).toBe("1");

    const res7 = await execute({ expression: "tan(pi / 4)" });
    expect(Number(res7)).toBeCloseTo(1);

    const res8 = await execute({ expression: "exp(0)" });
    expect(res8).toBe("1");
  });

  it("should fail on forbidden characters", async () => {
    const res = await execute({ expression: "2 + 3; alert(1)" });
    expect(res).toContain("Error");
  });

  it("should fail on unknown identifier", async () => {
    const res = await execute({ expression: "foo(10)" });
    expect(res).toContain("Error");
  });

  it("should fail on division by zero", async () => {
    const res = await execute({ expression: "10 / 0" });
    expect(res).toContain("Error");
  });

  it("should fail on square root of negative number", async () => {
    const res = await execute({ expression: "sqrt(-4)" });
    expect(res).toContain("Error");
  });

  it("should handle empty or invalid inputs", async () => {
    const res1 = await execute({ expression: "" });
    expect(res1).toContain("Error");

    const res2 = await execute({ expression: "   " });
    expect(res2).toContain("Error");
  });

  it("should fail on non-finite values like 0 ^ -1 or log(-10)", async () => {
    const res1 = await execute({ expression: "0 ^ -1" });
    expect(res1).toContain("Error");
    expect(res1).toContain("undefined or infinite");

    const res2 = await execute({ expression: "log(-10)" });
    expect(res2).toContain("Error");
    expect(res2).toContain("undefined or infinite");
  });

  it("should fail on invalid arguments (non-object or missing expression)", async () => {
    const res1 = await execute(null);
    expect(res1).toContain("Error");

    const res2 = await execute("expression");
    expect(res2).toContain("Error");

    const res3 = await execute({});
    expect(res3).toContain("Error");
  });

  it("should fail on expression exceeding 500 characters", async () => {
    const longExpression = "1 + ".repeat(200) + "1"; // 801 chars
    const res = await execute({ expression: longExpression });
    expect(res).toContain("Error");
    expect(res).toContain("exceeds maximum length");
  });

  it("should respect recursion depth limits", async () => {
    // 10 levels of nesting should succeed
    const nestedSucceed = "(".repeat(10) + "2" + ")".repeat(10);
    const resSucceed = await execute({ expression: nestedSucceed });
    expect(resSucceed).toBe("2");

    // 101 levels of nesting should fail with maximum recursion depth exceeded
    const nestedFail = "(".repeat(101) + "2" + ")".repeat(101);
    const resFail = await execute({ expression: nestedFail });
    expect(resFail).toContain("Error");
    expect(resFail).toContain("Maximum recursion depth exceeded");
  });
});
