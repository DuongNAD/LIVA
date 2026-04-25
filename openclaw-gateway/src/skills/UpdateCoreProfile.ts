import * as fs from "fs/promises";
import * as path from "node:path";

export const metadata = {
  name: "update_core_profile",
  search_keywords: ["update_core_profile","update core profile","tệp","tài liệu","file"],
  description:
    "Cập nhật hồ sơ tĩnh của người dùng khi có yêu cầu thay đổi (ví dụ: tuổi, nghề nghiệp, quê quán).",
  parameters: {
    type: "object",
    properties: {
      age: { type: "number", description: "Tuổi mới của người dùng" },
      profession: {
        type: "string",
        description: "Nghề nghiệp mới của người dùng",
      },
      location: { type: "string", description: "Quê quán / Nơi ở mới" },
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
    } catch (e) {
      // File may not exist yet
    }
    const newProfile = { ...currentProfile, ...args };
    // Atomic Write: .tmp + rename() prevents corrupt file on crash
    const tmpPath = `${profilePath}.tmp`;
    await fs.writeFile(tmpPath, JSON.stringify(newProfile, null, 2), "utf-8");
    await fs.rename(tmpPath, profilePath);
    return "Đã cập nhật thành công (Successfully updated)";
  } catch (error: any) {
    return `Lỗi cập nhật profile: ${error.message}`;
  }
};
