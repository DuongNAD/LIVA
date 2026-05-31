import { promises as fsp } from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";
import { RPAGuardrails } from "../../security/RPAGuardrails";

export const metadata: SkillMetadata = {
  name: "auto_backup",
  category: "personal",
  short_desc: "Backup system or user files.",
  semantic_tags: ["#backup", "#copy", "#save", "#file", "#saoluu"],
  search_keywords: ["backup", "sao lưu", "zip", "nén", "archive"],
  description: "Create a backup copy of specified folders or files to a designated location with timestamp naming.",
  requires_hitl: true,
  parameters: {
    type: "object",
    properties: {
      source_paths: { type: "array", items: { type: "string" }, description: "List of file/folder paths to backup (required)." },
      destination: { type: "string", description: "Where to save the backup (default: ~/LIVA_Backups/)." },
      name: { type: "string", description: "Custom backup name (default: auto with timestamp)." },
    },
    required: ["source_paths"],
  },
};

// Helper removed: using fsp.cp directly

export const execute = async (args: {
  source_paths: string[];
  destination?: string;
  name?: string;
}): Promise<string> => {
  if (!args.source_paths?.length) return "Error: No source paths provided.";

  const ts = new Date().toISOString().replaceAll(/[:.]/g, "-").substring(0, 19);
  const backupName = args.name?.trim() || `LIVA_Backup_${ts}`;
  const destDir = path.join(args.destination?.trim() || path.join(os.homedir(), "LIVA_Backups"), backupName);

  logger.info(`[Skill: auto_backup] Starting backup → ${destDir}`);
  try {
    let totalFiles = 0;
    let totalSize = 0;
    const validSources: string[] = [];

    for (const src of args.source_paths) {
      const resolved = path.resolve(src);
      try { await fsp.access(resolved); validSources.push(resolved); }
      catch { logger.warn(`[auto_backup] Skipping missing: ${resolved}`); }
    }
    if (!validSources.length) return "Error: None of the source paths exist.";

    await fsp.mkdir(destDir, { recursive: true });

    for (const src of validSources) {
      if (!RPAGuardrails.isPathSafe(src)) {
        throw new Error(`Path not allowed for backup: ${src}`);
      }
      const destPath = path.join(destDir, path.basename(src));
      await fsp.cp(src, destPath, { recursive: true });
      totalFiles++; // Note: fsp.cp handles entire trees, we just count the root items copied here for simplicity
    }

    // Calculate size
    const calcSize = async (dir: string): Promise<number> => {
      let size = 0;
      const entries = await fsp.readdir(dir, { withFileTypes: true });
      for (const e of entries) {
        const fp = path.join(dir, e.name);
        if (e.isFile()) { size += (await fsp.stat(fp)).size; }
        else if (e.isDirectory()) { size += await calcSize(fp); }
      }
      return size;
    };
    totalSize = await calcSize(destDir);

    return `✅ Backup completed!\n📦 Name: ${backupName}\n📁 Location: ${destDir}\n📄 Files: ${totalFiles}\n💾 Size: ${(totalSize / 1048576).toFixed(2)} MB`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Backup error: ${errMsg}`;
  }
};
