import * as readline from 'node:readline';
import { SkillRegistry } from "./SkillRegistry";

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
});

const askQuestion = (query: string): Promise<string> => {
  return new Promise((resolve) => rl.question(query, resolve));
};

async function main() {
  console.log("=========================================");
  console.log("   LIVA SKILLS INTERACTIVE TEST CLI");
  console.log("=========================================");
  console.log(
    "Môi trường này cho phép chạy và kiểm thử trực tiếp các chức năng",
  );
  console.log("MÀ KHÔNG CẦN THÔNG QUA AI (LLM). Output sẽ được in nguyên bản.");
  console.log("=========================================\n");

  const registry = new SkillRegistry();
  await registry.registerLocalSkills();

  const skills = registry.getAllSkills();

  while (true) {
    console.log("\n--- DANH SÁCH CÔNG CỤ (SKILLS) HIỆN CÓ ---");
    skills.forEach((skill: any, index: any) => {
      console.log(`[${index}] ${skill.name} - ${skill.description}`);
    });
    console.log(`[${skills.length}] Thoát chương trình (Exit)`);

    const choiceStr = await askQuestion(
      `\nChọn công cụ theo số thứ tự (0-${skills.length}): `,
    );
    const choice = parseInt(choiceStr.trim(), 10);

    if (isNaN(choice) || choice < 0 || choice > skills.length) {
      console.log("❌ Lựa chọn không hợp lệ. Vui lòng nhập số hợp lệ.");
      continue;
    }

    if (choice === skills.length) {
      console.log("Đang thoát...");
      rl.close();
      break;
    }

    const selectedSkill = skills[choice];
    console.log(`\n>> BẠN ĐANG KIỂM THỬ SKILL: [${selectedSkill.name}]`);
    console.log(`>> Tham số đầu vào bắt buộc (JSON Schema):`);
    console.log(JSON.stringify(selectedSkill.parameters, null, 2));

    const argsStr = await askQuestion(
      `\nNhập tham số (Arguments) dưới dạng chuỗi JSON (Ví dụ nếu có: {"key": "value"})\nHoặc ấn Enter để bỏ trống (nếu tool không đòi hỏi tham số): `,
    );

    let args: any = {};
    if (argsStr.trim() !== "") {
      try {
        args = JSON.parse(argsStr);
      } catch (e: any) {
        console.log(
          `⚠️ Lỗi nhập liệu JSON: ${e.message}. Hãy gõ JSON chuẩn (nháy kép ở key và biến string).`,
        );
        continue;
      }
    } else {
      // Force location to be empty string so it triggers Auto-IP
      console.log(
        ">> Tham số bị bỏ trống. Ép hệ thống dùng giá trị mặc định / tự động lấy Vị trí.",
      );
      args = {};
    }

    console.log(`\n⏳ Đang thực thi ${selectedSkill.name} với tham số:`, args);
    try {
      const startTime = Date.now();
      const result = await registry.executeSkill(selectedSkill.name, args);
      const duration = Date.now() - startTime;

      console.log(
        `\n✅ KẾT QUẢ ĐẦU RA TỰ NHIÊN (Chưa qua màng lọc LLM) - Mất ${duration}ms:`,
      );
      console.log("--------------------------------------------------");
      console.log(
        typeof result === "string" ? result : JSON.stringify(result, null, 2),
      );
      console.log("--------------------------------------------------");
    } catch (error: any) {
      console.log(`\n❌ LỖI TRONG QUÁ TRÌNH THỰC THI (ERROR):`);
      console.error(error);
    }

    await askQuestion(`\n[Nhấn Enter để quay lại danh sách công cụ]`);
  }
}

main().catch((error) => {
  console.error("Lỗi hệ thống:", error);
  process.exit(1);
});
