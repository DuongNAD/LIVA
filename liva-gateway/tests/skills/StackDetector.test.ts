import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { promises as fsp } from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { StackDetector } from "../../src/skills/StackDetector";

vi.mock("../../src/utils/logger", () => ({
  logger: {
    warn: vi.fn(),
  },
}));

describe("StackDetector", () => {
  let workspaceDir: string;

  beforeEach(async () => {
    workspaceDir = await fsp.mkdtemp(path.join(os.tmpdir(), "liva-stack-"));
  });

  afterEach(async () => {
    await fsp.rm(workspaceDir, { recursive: true, force: true });
    vi.restoreAllMocks();
  });

  it("should detect node ecosystem dependencies from package.json", async () => {
    await fsp.writeFile(path.join(workspaceDir, "package.json"), JSON.stringify({
      dependencies: {
        react: "^18.0.0",
        next: "^14.0.0",
        tailwindcss: "^3.0.0",
      },
      devDependencies: {
        typescript: "^5.0.0",
      },
    }), "utf-8");

    const stack = await new StackDetector().detect(workspaceDir);

    expect(stack).toEqual(expect.arrayContaining(["nodejs", "react", "nextjs", "typescript", "tailwind"]));
  });

  it("should detect non-node project markers", async () => {
    await fsp.writeFile(path.join(workspaceDir, "go.mod"), "module demo", "utf-8");
    await fsp.writeFile(path.join(workspaceDir, "Cargo.toml"), "[package]", "utf-8");
    await fsp.writeFile(path.join(workspaceDir, "pyproject.toml"), "[project]", "utf-8");
    await fsp.writeFile(path.join(workspaceDir, "composer.json"), "{}", "utf-8");
    await fsp.writeFile(path.join(workspaceDir, "Dockerfile"), "FROM node:20", "utf-8");

    const stack = await new StackDetector().detect(workspaceDir);

    expect(stack).toEqual(expect.arrayContaining(["go", "rust", "python", "php", "docker"]));
  });

  it("should return unknown when workspace cannot be read", async () => {
    await fsp.rm(workspaceDir, { recursive: true, force: true });

    const stack = await new StackDetector().detect(workspaceDir);

    expect(stack).toEqual(["unknown"]);
  });

  it("should avoid reading oversized package.json", async () => {
    const largePackage = `{"dependencies":{"react":"^18.0.0"},"padding":"${"x".repeat(513 * 1024)}"}`;
    await fsp.writeFile(path.join(workspaceDir, "package.json"), largePackage, "utf-8");

    const readSpy = vi.spyOn(fsp, "readFile");
    const stack = await new StackDetector().detect(workspaceDir);

    expect(stack).toEqual(["nodejs"]);
    expect(readSpy).not.toHaveBeenCalled();
  });

  it("should keep nodejs and ignore malformed package.json details", async () => {
    await fsp.writeFile(path.join(workspaceDir, "package.json"), "{bad json", "utf-8");

    const stack = await new StackDetector().detect(workspaceDir);

    expect(stack).toEqual(["nodejs"]);
  });

  it("should perform O(1) disk scan by avoiding redundant stat calls", async () => {
    await fsp.writeFile(path.join(workspaceDir, "go.mod"), "module test", "utf-8");
    await fsp.writeFile(path.join(workspaceDir, "Cargo.toml"), "[package]", "utf-8");
    await fsp.writeFile(path.join(workspaceDir, "package.json"), "{}", "utf-8");

    const statSpy = vi.spyOn(fsp, "stat");
    
    await new StackDetector().detect(workspaceDir);

    // Ensure O(1) IO footprint: stat should ONLY be called once specifically for package.json OOM protection check
    expect(statSpy).toHaveBeenCalledTimes(1);
    expect(statSpy.mock.calls[0][0]).toContain("package.json");
  });
});
