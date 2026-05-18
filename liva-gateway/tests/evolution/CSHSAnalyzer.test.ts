import { describe, expect, it } from "vitest";
import { CSHSAnalyzer } from "../../src/evolution/CSHSAnalyzer";

describe("CSHSAnalyzer", () => {
  it("approves valid TypeScript code", () => {
    const analyzer = new CSHSAnalyzer();
    const result = analyzer.analyze("export const value: number = 42;", "valid", 0.25);

    expect(result.score).toBeLessThan(0.1);
    expect(result.pass).toBe(true);
  });

  it("penalizes TODO placeholders", () => {
    const analyzer = new CSHSAnalyzer();
    const result = analyzer.analyze("export const value = 1; // TODO: fill details", "todo", 0.25);

    expect(result.score).toBeGreaterThanOrEqual(0.4);
    expect(result.anchors.some((anchor) => anchor.includes("TODO:"))).toBe(true);
    expect(result.pass).toBe(false);
  });

  it("penalizes undefined names", () => {
    const analyzer = new CSHSAnalyzer();
    const result = analyzer.analyze("export const value = missingSymbol;", "undefined", 0.25);

    expect(result.score).toBeGreaterThanOrEqual(0.3);
    expect(result.anchors.some((anchor) => anchor.includes("TS2304"))).toBe(true);
  });

  it("penalizes unresolved imports", () => {
    const analyzer = new CSHSAnalyzer();
    const result = analyzer.analyze('import x from "not-a-real-module";\nexport { x };', "import", 0.25);

    expect(result.score).toBeGreaterThanOrEqual(0.3);
    expect(result.anchors.some((anchor) => anchor.includes("TS2307"))).toBe(true);
  });

  it("sums compound penalties", () => {
    const analyzer = new CSHSAnalyzer();
    const result = analyzer.analyze(
      'import x from "not-a-real-module";\nexport const value = missingSymbol; // FIXME: later\nexport { x };',
      "compound",
      0.25,
    );

    expect(result.score).toBe(1);
    expect(result.pass).toBe(false);
  });

  it("rejects malformed code", () => {
    const analyzer = new CSHSAnalyzer();
    const result = analyzer.analyze("export const = ;", "malformed", 0.25);

    expect(result.score).toBeGreaterThan(0.25);
    expect(result.pass).toBe(false);
    expect(result.diagnosticCount).toBeGreaterThan(0);
  });
});
