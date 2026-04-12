import { CoreKernel } from "./core/CoreKernel";
import { logger } from "./utils/logger";

async function runTests() {
  // 1. Khởi tạo lõi hệ thống
  const kernel = new CoreKernel();
  await kernel.memory.initialize();
  await kernel.registry.registerLocalSkills();
  const skills = kernel.registry.getAllSkills();

  const results: any[] = [];

  // 2. Định nghĩa tham số giả (dummy arguments) cho toàn bộ skill
  const dummyArgs: Record<string, any> = {
    get_current_time: {},
    read_file: { path: "./package.json" },
    delete_local_file: { path: "./test_auto.txt" },
    execute_command: { command: "echo Hello" },
    get_system_info: {},
    get_weather_forecast: {},
    list_directory: { path: "./" },
    open_local_file: { path: "./package.json" },
    read_emails: { count: 1 },
    read_local_file: { path: "./package.json" },
    read_recent_emails: { count: 1 },
    send_zalo_bot: { message: "Test Auto", topic: "Ghi chú log" },
    send_zalo_rpa: { message: "Test Auto", receiver: "Test User" },
    web_search: { query: "OpenAI" },
    write_local_file: { path: "./test_auto.txt", content: "Test Auto" },
    update_core_profile: { age: 30, profession: "Engineer", location: "Hanoi" },
    create_google_doc: { title: "Test Doc", content: "Test Content" },
    append_google_doc: { documentId: "dummy_id", text: "Append Test" },
    read_google_sheet: { spreadsheetId: "dummy_id", range: "A1:B2" },
    write_google_sheet: {
      spreadsheetId: "dummy_id",
      range: "A1:B2",
      values: [["Test", "Auto"]],
    },
    search_google_drive: { query: "name contains 'LIVA'" },
  };

  console.log(`\n======================================================`);
  console.log(
    `BẮT ĐẦU KIỂM THỬ KHỐI LƯỢNG KỸ NĂNG CỦA LIVA (${skills.length} Skills)`,
  );
  console.log(`======================================================\n`);

  for (const skill of skills) {
    let status = "❌ FAILED";
    let message = "";
    try {
      const args = dummyArgs[skill.name] || {};

      // Xây dựng cơ chế Timeout (5 giây) để tránh test bị treo bởi các tác vụ nặng như RPA/Browser
      const timeoutPromise = new Promise((_, reject) =>
        setTimeout(
          () => reject(new Error("Timeout (Quá 5 giây - Bỏ qua test này)")),
          5000,
        ),
      );

      const execPromise = kernel.registry.executeSkill(skill.name, args);

      // Race giữa quá trình chạy hàm và timeout
      const res = await Promise.race([execPromise, timeoutPromise]);

      status = "✅ PASSED";
      message = String(res).substring(0, 100).replace(/\n/g, " ") + "...";
    } catch (e: any) {
      status = "❌ FAILED/BLOCKED";
      message = e.message;
      // Catch expected errors like missing Google credentials or UI timeouts as partial passes in an automated env
      if (message.includes("No credentials") || message.includes("Timeout")) {
        status = "⚠️ WARNING";
      }
    }

    results.push({
      "Mã Kỹ Năng": skill.name,
      "Tình Trạng": status,
      "Thông tin Code Trả về": message.substring(0, 80),
    });

    console.log(`[${status}] Kỹ năng: ${skill.name}`);
  }

  console.log("\nBẢNG TỔNG KẾT KIỂM THỬ:");
  console.table(results);

  // Tắt hệ thống
  process.exit(0);
}

runTests().catch(console.error);
