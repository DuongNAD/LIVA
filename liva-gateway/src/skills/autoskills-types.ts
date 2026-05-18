import { z } from "zod";

export const TechStackSchema = z.enum([
  "nodejs",
  "react",
  "nextjs",
  "vue",
  "typescript",
  "tailwind",
  "go",
  "rust",
  "python",
  "php",
  "docker",
  "unknown",
]);
export type TechStack = z.infer<typeof TechStackSchema>;

export const ToolKitSchema = z.enum([
  "OBSIDIAN_KIT",
  "DATA_KIT",
  "DEVOPS_KIT",
  "SOCIAL_KIT",
  "GENERAL_KIT",
]);
export type ToolKit = z.infer<typeof ToolKitSchema>;

export const ToolMappingSchema = z.object({
  toolName: z.string().min(1),
  version: z.string().min(1),
  source: z.enum(["local", "remote"]),
  description: z.string(),
  remoteUrl: z.string().url().optional(),
  kit: ToolKitSchema.optional(),
});
export type ToolMapping = z.infer<typeof ToolMappingSchema>;

export const SkillsLockDataSchema = z.object({
  workspaceHash: z.string().min(1),
  detectedStack: z.array(TechStackSchema),
  activeTools: z.array(ToolMappingSchema),
  lastUpdated: z.string(),
  schemaVersion: z.literal("1.0.0"),
});
export type SkillsLockData = z.infer<typeof SkillsLockDataSchema>;
