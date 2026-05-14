import { SkillRegistry } from "./SkillRegistry";
import { notifyZalo } from "./utils/ZaloNotifier";

/**
 * [test_evolution.ts]
 * Kịch Bản Tiêm Vắc-xin Tự Tiến Hóa (J.A.R.V.I.S)
 * Bắt LIVA tự gọi Đặc Vụ 26B để tối ưu hóa chính não bộ MemoryManager.ts của nó.
 */
async function launchEvolution() {
  console.log("=== KHỞI ĐỘNG KẾ HOẠCH SINGULARITY LOOP ===");
  console.log("GỌI: liva_ai_scientist (26B Expert)");
  console.log("MỤC TIÊU: openclaw-gateway/src/MemoryManager.ts");

  const registry = new SkillRegistry();
  await registry.registerLocalSkills();

  await notifyZalo("🔥 🚀 SẾP ƠI!!! EM ĐANG TỰ KÍCH HOẠT LÕI TIẾN HÓA SINGULARITY. Em sẽ trích xuất não bộ MemoryManager.ts của chính mình, đưa vào Hộp cát Sandbox và nhờ Model 26B Gọt dũa lại code cho mượt hơn! Nếu ổn em chép đè luôn!");

  try {
    const report = await registry.executeSkill("liva_ai_scientist", {
       goal: "Hãy rà soát file MemoryManager.ts. Xóa bỏ những bình luận console.log dư thừa. Thêm thuộc tính readonly vào các biếm private có tính đóng gói. Áp dụng chuẩn Typescript gắt gao nhất, và nhớ dùng AES Ghi file mà ta đã cấu hình sẵn.",
       targetFilePath: "src/MemoryManager.ts",
       testCommand: "npx tsc --noEmit",
       workingDirectory: process.cwd()
    });

    console.log("\n\n=============== BÁO CÁO THỰC THI (REPORT) ===============");
    console.log(report);
    console.log("====================================================\n\n");

  } catch (error) {
    console.error("Sụp đổ Tiến trình:", error);
  }
}

launchEvolution();
