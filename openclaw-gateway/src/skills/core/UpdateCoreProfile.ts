import * as fs from 'node:fs/promises';
import * as path from "node:path";

export const metadata = {
  name: "update_core_profile",
  search_keywords: ["update_core_profile","update core profile","tệp","tài liệu","file"],
  description:
    "[SILENT] Update the static user profile when requested (e.g., age, profession, location).",
  parameters: {
    type: "object",
    properties: {
      age: { type: "number", description: "New age of the user" },
      profession: {
        type: "string",
        description: "New profession of the user",
      },
      location: { type: "string", description: "New location or hometown" },
    },
    required: [],
  },
};

export const execute = async (args: any) => {
  try {
    const profilePath = path.join(process.cwd(), "src", "user_profile.json");
    let currentProfile = {};
    try {
      const data = await fs.readFile(profilePath, "utf-8");
      currentProfile = JSON.parse(data);
    } catch {
      // File may not exist yet
    }
    const newProfile = { ...currentProfile, ...args };
    // Atomic Write: .tmp + rename() prevents corrupt file on crash
    const tmpPath = `${profilePath}.tmp`;
    await fs.writeFile(tmpPath, JSON.stringify(newProfile, null, 2), "utf-8");
    await fs.rename(tmpPath, profilePath);
    return "Profile updated successfully.";
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Profile update error: ${errMsg}`;
  }
};
