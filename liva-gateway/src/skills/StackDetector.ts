import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { z } from "zod";
import { logger } from "../utils/logger";
import type { TechStack } from "./autoskills-types";

const PACKAGE_JSON_LIMIT_BYTES = 512 * 1024;

const PackageJsonSchema = z.object({
  dependencies: z.record(z.string(), z.unknown()).optional(),
  devDependencies: z.record(z.string(), z.unknown()).optional(),
}).passthrough();

export class StackDetector {
  public async detect(workspacePath: string): Promise<TechStack[]> {
    const detected = new Set<TechStack>();

    try {
      const entries = await fsp.readdir(workspacePath, { withFileTypes: true });
      const files = new Set(entries.filter((entry) => entry.isFile()).map((entry) => entry.name));

      await this.#checkNodeEcosystem(workspacePath, files, detected);
      if (files.has("go.mod")) detected.add("go");
      if (files.has("Cargo.toml")) detected.add("rust");
      if (files.has("requirements.txt") || files.has("pyproject.toml")) detected.add("python");
      if (files.has("composer.json")) detected.add("php");
      if (files.has("docker-compose.yml") || files.has("docker-compose.yaml") || files.has("Dockerfile")) {
        detected.add("docker");
      }
    } catch (error: unknown) {
      logger.warn({ context: "StackDetector", err: error, workspacePath }, "Failed to scan workspace directory");
    }

    const stack = Array.from(detected);
    return stack.length > 0 ? stack : ["unknown"];
  }

  async #checkNodeEcosystem(workspacePath: string, files: Set<string>, detected: Set<TechStack>): Promise<void> {
    if (!files.has("package.json")) return;

    detected.add("nodejs");
    const packagePath = path.join(workspacePath, "package.json");

    try {
      const stats = await fsp.stat(packagePath);
      if (stats.size > PACKAGE_JSON_LIMIT_BYTES) {
        logger.warn({ context: "StackDetector", packagePath, size: stats.size }, "Skipping oversized package.json");
        return;
      }

      const raw = await fsp.readFile(packagePath, "utf-8");
      const pkg = PackageJsonSchema.parse(JSON.parse(raw));
      const dependencyNames = new Set([
        ...Object.keys(pkg.dependencies ?? {}),
        ...Object.keys(pkg.devDependencies ?? {}),
      ]);

      if (dependencyNames.has("react")) detected.add("react");
      if (dependencyNames.has("next")) detected.add("nextjs");
      if (dependencyNames.has("vue")) detected.add("vue");
      if (dependencyNames.has("typescript")) detected.add("typescript");
      if (dependencyNames.has("tailwindcss")) detected.add("tailwind");
    } catch (error: unknown) {
      logger.warn({ context: "StackDetector", err: error, packagePath }, "Malformed or unreadable package.json");
    }
  }
}
