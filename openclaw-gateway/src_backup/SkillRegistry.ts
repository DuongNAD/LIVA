import * as fs from "fs";
import { promises as fsp } from "fs";
import * as path from "path";
import { logger } from "./utils/logger";

export interface AgentSkill {
  name: string;
  description: string;
  parameters: any; // JSON Schema cho tham số
  search_keywords?: string[];
  isCoreSkill?: boolean;
  execute: (args: any) => Promise<any>;
}

export class SkillRegistry {
  private skills: Map<string, AgentSkill> = new Map();

  constructor() {
    this.registerBuiltInSkills();
  }

  public async registerLocalSkills() {
    const skillsDir = path.join(process.cwd(), "src", "skills");
    
    try {
      await fsp.access(skillsDir);
    } catch {
      logger.warn(`[SkillRegistry] Thư mục kỹ năng không tồn tại: ${skillsDir}`);
      return;
    }

    const files = await fsp.readdir(skillsDir);
    const importPromises = files.map(async (file) => {
      if (file.endsWith(".ts") || file.endsWith(".js")) {
        const skillPath = path.join(skillsDir, file);
        try {
          // V14 Dynamic Hot-Reloading: Tiêm ?v= Timestamp để phá vỡ vách ngăn Cache bảo thủ của Node.js
          const module = await import(
            `file://${skillPath.replace(/\\/g, "/")}?v=${Date.now()}`
          );
          if (module.metadata && module.execute) {
            this.registerSkill({
              name: module.metadata.name,
              description: module.metadata.description,
              parameters: module.metadata.parameters,
              search_keywords: module.metadata.search_keywords,
              isCoreSkill: module.metadata.isCoreSkill || false,
              execute: module.execute,
            });
          }
        } catch (error) {
          // Fallback to require
          try {
            // V14 Xóa Cache thủ công cho Require
            const resolvedPath = require.resolve(skillPath);
            if (require.cache[resolvedPath]) {
                delete require.cache[resolvedPath];
            }
            const module = require(skillPath);
            if (module.metadata && module.execute) {
              this.registerSkill({
                name: module.metadata.name,
                description: module.metadata.description,
                parameters: module.metadata.parameters,
                search_keywords: module.metadata.search_keywords,
                isCoreSkill: module.metadata.isCoreSkill || false,
                execute: module.execute,
              });
            }
          } catch (err) {
            logger.error(`[SkillRegistry] Lỗi tải kỹ năng từ ${file}:`, err);
          }
        }
      }
    });

    await Promise.all(importPromises);

    logger.info(
      `[SkillRegistry] Đã quét và nạp xong các kỹ năng trong thư mục local.`
    );
  }

  public registerSkill(skill: AgentSkill) {
    this.skills.set(skill.name, skill);
    logger.info(`[SkillRegistry] Đã đăng ký kỹ năng: ${skill.name}`);
  }

  public getSkill(name: string): AgentSkill | undefined {
    return this.skills.get(name);
  }

  public getAllSkills(): AgentSkill[] {
    return Array.from(this.skills.values());
  }

  public async executeSkill(name: string, args: any): Promise<any> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Kỹ năng '${name}' không tồn tại.`);
    }
    logger.info(
      `[SkillRegistry] Đang thực thi kỹ năng: ${name} với tham số:`,
      args,
    );
    return await skill.execute(args);
  }

  private registerBuiltInSkills() {
    // Kỹ năng 1: Xem giờ hệ thống
    this.registerSkill({
      name: "get_current_time",
      description: "Lấy thời gian hiện tại của hệ thống.",
      isCoreSkill: true,
      search_keywords: ["giờ", "đồng hồ", "thời gian", "ngày", "tháng", "năm", "hôm nay", "từ nay"],
      parameters: {
        type: "object",
        properties: {
          timezone: {
            type: "string",
            description: "Múi giờ (vd: Asia/Ho_Chi_Minh). Tùy chọn.",
          },
        },
      },
      execute: async (args: any) => {
        const date = new Date();
        if (args.timezone) {
          return date.toLocaleString("vi-VN", { timeZone: args.timezone });
        }

        // Tự động lấy Múi giờ chuẩn của thiết bị đang chạy (VD: Asia/Ho_Chi_Minh hoặc khu vực khác)
        const localTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
        return date.toLocaleString("vi-VN", { timeZone: localTimeZone });
      },
    });

    // Kỹ năng 2: Đọc nội dung tệp tin
    this.registerSkill({
      name: "read_file",
      description: "Đọc nội dung của một tệp tin trên hệ thống (Local).",
      isCoreSkill: false,
      search_keywords: ["đọc", "mở", "nội dung file", "folder", "mã nguồn", "source code", "xem trước"],
      parameters: {
        type: "object",
        properties: {
          path: {
            type: "string",
            description: "Đường dẫn tuyệt đối hoặc tương đối tới tệp tin.",
          },
        },
        required: ["path"],
      },
      execute: async (args: any) => {
        try {
          const content = fs.readFileSync(args.path, "utf8");
          return content;
        } catch (error: any) {
          return `Lỗi khi đọc tệp: ${(error as Error).message}`;
        }
      },
    });
  }
}
