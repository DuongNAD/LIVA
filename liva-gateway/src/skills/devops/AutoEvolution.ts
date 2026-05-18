import { logger } from "../../utils/logger";
import { SkillRegistry } from "../../SkillRegistry";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { exec } from "node:child_process";
import { z } from "zod";

export const metadata = {
  name: "auto_evolution",
  search_keywords: [
    "auto_evolution",
    "tự tiến hóa",
    "viết skill",
    "tạo skill",
    "nâng cấp",
    "học thêm",
    "thêm chức năng",
    "self evolution",
    "self write",
    "generate skill"
  ],
  description:
    "[ASK_FIRST] LIVA's Self-Evolution and dynamic skill builder. Scans existing skills to prevent redundancy, checks for functionally similar tools, drafts new skill specs, writes code to the codebase, compiles/typechecks, and hot-loads it immediately into active memory.",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["analyze_and_propose", "generate_and_install"],
        description:
          "Hành động: 'analyze_and_propose' để rà soát kĩ năng trùng lặp và phác thảo; 'generate_and_install' để viết file, compile, typecheck và kích hoạt nóng."
      },
      prompt: {
        type: "string",
        description:
          "Mô tả chức năng sếp muốn thêm (Yêu cầu cho 'analyze_and_propose'). Ví dụ: 'tra cứu thời tiết thành phố', 'theo dõi giá vàng'."
      },
      skillName: {
        type: "string",
        description:
          "Tên kỹ năng bằng PascalCase (Yêu cầu cho 'generate_and_install'). Ví dụ: 'WeatherTracker', 'GoldTracker'."
      },
      category: {
        type: "string",
        enum: ["web", "personal", "social", "devops", "custom"],
        description:
          "Thư mục danh mục của kĩ năng mới (Yêu cầu cho 'generate_and_install')."
      },
      code: {
        type: "string",
        description:
          "Mã nguồn TypeScript hoàn chỉnh và sạch sẽ của kỹ năng mới (Yêu cầu cho 'generate_and_install'). Mã nguồn phải xuất khẩu 'metadata' và 'execute'."
      }
    },
    required: ["action"]
  }
};

// Jaccard Token-Overlap similarity calculation
function calculateSimilarity(a: string, b: string): number {
  const setA = new Set(a.toLowerCase().split(/\s+/).filter((w) => w.length >= 2));
  const setB = new Set(b.toLowerCase().split(/\s+/).filter((w) => w.length >= 2));
  const intersection = new Set([...setA].filter((x) => setB.has(x)));
  const union = new Set([...setA, ...setB]);
  return union.size === 0 ? 0 : intersection.size / union.size;
}

export const execute = async (args: {
  action: "analyze_and_propose" | "generate_and_install";
  prompt?: string;
  skillName?: string;
  category?: "web" | "personal" | "social" | "devops" | "custom";
  code?: string;
}): Promise<string> => {
  const skillsDir = path.resolve(process.cwd(), "src", "skills");

  // =========================================================================
  // PHASE 1: DISCOVER AND SCAN EXISTING SKILLS
  // =========================================================================
  const existingSkills: Array<{
    name: string;
    filePath: string;
    description: string;
    search_keywords: string[];
    category: string;
  }> = [];

  try {
    const files = await fs.readdir(skillsDir, { recursive: true });
    for (const file of files) {
      if (
        (file.endsWith(".ts") || file.endsWith(".js")) &&
        !file.endsWith("index.ts") &&
        !file.endsWith("index.js") &&
        !file.includes("AutoSkillOrchestrator") &&
        !file.includes("autoskills-types") &&
        !file.includes("SkillMetadata") &&
        !file.includes("StackDetector")
      ) {
        const fullPath = path.join(skillsDir, file);
        const content = await fs.readFile(fullPath, "utf-8");

        // Parse metadata using regex for extreme speed and robustness
        const nameMatch = content.match(/name:\s*["']([^"']+)["']/);
        const descMatch = content.match(/description:\s*["']([^"']+)["']/);
        const kwMatch = content.match(/search_keywords:\s*\[([\s\S]*?)\]/);

        const name = nameMatch ? nameMatch[1] : path.basename(file, path.extname(file));
        const description = descMatch ? descMatch[1] : "";
        let search_keywords: string[] = [];

        if (kwMatch) {
          search_keywords = kwMatch[1]
            .split(",")
            .map((k) => k.replace(/["'\s]/g, ""))
            .filter((k) => k.length > 0);
        }

        const category = path.dirname(file);
        existingSkills.push({
          name,
          filePath: fullPath,
          description,
          search_keywords,
          category
        });
      }
    }
  } catch (err: any) {
    logger.error(`[AutoEvolution] Error scanning skills folder: ${err.message}`);
  }

  // =========================================================================
  // ACTION: ANALYZE AND PROPOSE
  // =========================================================================
  if (args.action === "analyze_and_propose") {
    if (!args.prompt || args.prompt.trim().length === 0) {
      return "Lỗi: Vui lòng cung cấp mô tả chức năng muốn thêm trong trường 'prompt' / Error: Please specify target prompt.";
    }

    const targetPrompt = args.prompt.trim();
    logger.info(`[AutoEvolution] Rà soát và phân tích kĩ năng trùng lặp cho: "${targetPrompt}"`);

    // Similarity checking against all existing skills
    const similarSkills: Array<{
      name: string;
      category: string;
      reason: string;
      similarityScore: number;
    }> = [];

    const promptWords = targetPrompt.toLowerCase().split(/\s+/).filter((w) => w.length >= 2);

    for (const skill of existingSkills) {
      let score = 0;
      let reason = "";

      // 1. Check exact search keyword overlap
      const matchedKws = skill.search_keywords.filter((kw) =>
        targetPrompt.toLowerCase().includes(kw.toLowerCase())
      );
      if (matchedKws.length > 0) {
        score += 0.4 + matchedKws.length * 0.1;
        reason = `Trùng khớp từ khóa tìm kiếm: [${matchedKws.join(", ")}]`;
      }

      // 2. Check token similarity on name
      const nameSim = calculateSimilarity(skill.name, targetPrompt);
      if (nameSim > 0.25) {
        score = Math.max(score, nameSim);
        reason = reason ? `${reason} & Tên kĩ năng khá giống` : "Tên kĩ năng có cấu trúc từ vựng tương đồng";
      }

      // 3. Check token similarity on description
      const descSim = calculateSimilarity(skill.description, targetPrompt);
      if (descSim > 0.20) {
        score = Math.max(score, descSim);
        reason = reason ? `${reason} & Mô tả tương tự` : "Mô tả chức năng có ngữ nghĩa giống nhau";
      }

      if (score >= 0.3) {
        similarSkills.push({
          name: skill.name,
          category: skill.category,
          reason,
          similarityScore: Math.min(1.0, score)
        });
      }
    }

    // Sort matching skills by similarity
    similarSkills.sort((a, b) => b.similarityScore - a.similarityScore);

    // Build the proposal report
    let report = `## 📊 Báo Cáo Phân Tích & Phác Thảo Kỹ Năng Mới / Skills Analysis & Proposal\n\n`;
    report += `* **Yêu cầu của sếp**: "${targetPrompt}"\n`;
    report += `* **Số lượng kỹ năng hiện có trong hệ thống**: ${existingSkills.length} kỹ năng\n\n`;

    if (similarSkills.length > 0) {
      report += `> [!WARNING]\n`;
      report += `> **CẢNH BÁO TRÙNG LẶP / SIMILARITY ALERT**:\n`;
      report += `> LIVA phát hiện đã có **${similarSkills.length}** kỹ năng có chức năng gần tương tự hoặc liên quan trong hệ thống:\n\n`;

      for (const sim of similarSkills) {
        report += `*   🔑 **\`${sim.name}\`** (Danh mục: \`${sim.category}\`) - Độ trùng khớp: **${(
          sim.similarityScore * 100
        ).toFixed(0)}%**\n`;
        report += `    *   *Lý do*: ${sim.reason}\n`;
      }
      report += `\n*Nếu sếp vẫn muốn tạo mới kĩ năng riêng, LIVA khuyên sếp nên đặt tên khác biệt hoặc cập nhật đè lên kĩ năng trùng lặp ở trên để giữ codebase sạch sẽ.*`;
    } else {
      report += `> [!NOTE]\n`;
      report += `> **✅ CODEBASE AN TOÀN**: Không tìm thấy kỹ năng nào trùng lặp. Sếp có thể tự tin tạo mới hoàn toàn kỹ năng này!\n`;
    }

    // Propose blueprint details
    const cleanSkillName = targetPrompt
      .replace(/[^a-zA-Z0-9\s]/g, "")
      .split(/\s+/)
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join("");

    report += `\n\n### 📐 Phác Thảo Thiết Kế Kỹ Năng / Recommended Blueprint\n`;
    report += `*   **Tên Kỹ Năng Đề Xuất (PascalCase)**: \`${cleanSkillName}\` (Tên file: \`${cleanSkillName}.ts\`)\n`;
    report += `*   **Danh Mục (Category)**: \`web\` (nếu gọi API / lấy tin), \`personal\` (nếu xử lý tệp tin cục bộ), hoặc \`custom\` (dự phòng).\n`;
    report += `*   **Mẫu Metadata Đề Xuất**:\n`;
    report += `\`\`\`typescript\n`;
    report += `export const metadata = {\n`;
    report += `  name: "${cleanSkillName.replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase()}",\n`;
    report += `  search_keywords: ["${cleanSkillName.toLowerCase()}", "tra cuu", "tim kiem"],\n`;
    report += `  description: "[AUTO_RUN] Tra cứu thông tin cho ${targetPrompt} ...",\n`;
    report += `  parameters: {\n`;
    report += `    type: "object",\n`;
    report += `    properties: {\n`;
    report += `      // Thêm các tham số đầu vào tại đây\n`;
    report += `    },\n`;
    report += `    required: []\n`;
    report += `  }\n`;
    report += `};\n`;
    report += `\`\`\`\n\n`;
    report += `*Sếp hãy bảo LIVA tiến hành tạo mã nguồn hoàn chỉnh và chạy cài đặt nóng (generate_and_install) kỹ năng này nhé!*`;

    return report;
  }

  // =========================================================================
  // ACTION: GENERATE AND INSTALL
  // =========================================================================
  if (args.action === "generate_and_install") {
    if (!args.skillName || !args.category || !args.code) {
      return "Lỗi: Thiếu tham số bắt buộc. Cần cung cấp 'skillName', 'category', và 'code' để cài đặt nóng kỹ năng mới. / Missing required params.";
    }

    const cleanSkillName = args.skillName.replace(/[^a-zA-Z0-9]/g, "");
    const destDir = path.join(skillsDir, args.category);
    const destPath = path.join(destDir, `${cleanSkillName}.ts`);

    logger.info(`[AutoEvolution] Đang thực hiện cài đặt nóng kỹ năng mới: ${cleanSkillName} vào ${destPath}`);

    try {
      // 1. Tạo thư mục nếu chưa tồn tại
      await fs.mkdir(destDir, { recursive: true });

      // 2. Viết mã nguồn vào file
      await fs.writeFile(destPath, args.code, "utf-8");
      logger.info(`[AutoEvolution] Đã ghi mã nguồn kỹ năng tại: ${destPath}`);

      // 3. Thực thi Typecheck biên dịch ngầm để đảm bảo code an toàn
      const workspaceRoot = path.resolve(process.cwd());
      logger.info(`[AutoEvolution] Chạy TypeScript typecheck để kiểm định mã nguồn...`);

      const typecheckPromise = new Promise<{ success: boolean; errorMsg: string }>((resolve) => {
        exec("npm run typecheck", { cwd: workspaceRoot, timeout: 30000 }, (error, stdout, stderr) => {
          if (error) {
            const out = stdout ? stdout.toString() : "";
            const err = stderr ? stderr.toString() : "";
            const fullOutput = out + "\n" + err;
            if (error.killed) {
              resolve({ success: false, errorMsg: "TypeScript typecheck timed out after 30 seconds to prevent resource locks." });
            } else {
              resolve({ success: false, errorMsg: fullOutput });
            }
          } else {
            resolve({ success: true, errorMsg: "" });
          }
        });
      });

      const buildResult = await typecheckPromise;

      if (!buildResult.success) {
        // Tự động rollback xóa file lỗi để đảm bảo hệ thống không bị crash!
        await fs.unlink(destPath).catch(() => {});
        logger.error(`[AutoEvolution] Biên dịch thất bại. Đã rollback gỡ file: ${cleanSkillName}`);
        
        let failReport = `> [!CAUTION]\n`;
        failReport += `> **❌ LỖI BIÊN DỊCH / COMPILATION ERROR**:\n`;
        failReport += `> Kỹ năng mới \`${cleanSkillName}\` không vượt qua được kiểm tra lỗi cú pháp TypeScript. LIVA đã tự động gỡ bỏ file lỗi để bảo vệ hệ thống.\n\n`;
        failReport += `### Chi tiết lỗi biên dịch từ hệ thống / Compiler output:\n`;
        failReport += `\`\`\`bash\n${buildResult.errorMsg.substring(0, 1500)}\n\`\`\`\n\n`;
        failReport += `*Sếp hãy yêu cầu LIVA xem lại lỗi và viết lại mã nguồn sửa đổi lỗi trên nhé!*`;
        
        return failReport;
      }

      // 4. Hot-reload: Đăng ký nóng vào bộ nhớ SkillRegistry!
      logger.info(`[AutoEvolution] Biên dịch thành công! Đang kích hoạt nóng kĩ năng mới vào SkillRegistry...`);
      const registry = SkillRegistry.getInstance();
      if (registry) {
        await registry.registerLocalSkills();
        logger.info(`[AutoEvolution] ✅ Hot-reload kĩ năng mới thành công!`);
      } else {
        logger.warn(`[AutoEvolution] Không tìm thấy SkillRegistry instance để hot-reload.`);
      }

      let successReport = `> [!TIP]\n`;
      successReport += `> **🎉 KÍCH HOẠT NÓNG THÀNH CÔNG / SKILL DYNAMICALLY INSTALLED**:\n`;
      successReport += `> Kỹ năng mới **\`${cleanSkillName}\`** đã vượt qua tất cả bài test biên dịch và đã được LIVA tự học + kích hoạt nóng thành công vào bộ não!\n\n`;
      successReport += `*   **Đường dẫn file**: [${cleanSkillName}.ts](file:///${destPath.replace(/\\/g, "/")})\n`;
      successReport += `*   **Danh mục**: \`${args.category}\` (Đã được load vào MCP Local Server)\n`;
      successReport += `*   **Trạng thái hoạt động**: **Sẵn sàng thực thi tức thì!**\n\n`;
      successReport += `*Sếp có thể ngay lập tức sử dụng kỹ năng mới này bằng cách ra lệnh cho LIVA kiểm tra nhé!*`;

      return successReport;

    } catch (err: any) {
      const errMsg = err.message || String(err);
      logger.error(`[AutoEvolution] Lỗi nghiêm trọng khi cài đặt kỹ năng: ${errMsg}`);
      return `❌ Lỗi hệ thống khi cài đặt kỹ năng: ${errMsg}`;
    }
  }

  return "Lỗi: Hành động không hợp lệ. Chỉ chấp nhận 'analyze_and_propose' hoặc 'generate_and_install'.";
};
