import { SkillRegistry } from "./SkillRegistry";
import * as path from "path";

async function runTests() {
  console.log("=== BẮT ĐẦU TEST TOÀN DIỆN LIVA SKILLS ===\n");
  const registry = new SkillRegistry();
  await registry.registerLocalSkills();
  console.log(
    "\n[Các Skill đã đăng ký]:",
    registry
      .getAllSkills()
      .map((s) => s.name)
      .join(", "),
  );

  console.log("\n--- TEST 1: Windows OS (Thư mục & Tệp) ---");
  try {
    const testFilePath = path.join(__dirname, "..", "test_liva_file.txt");
    console.log("-> 1.1: Ghi file (write_local_file)");
    const writeRes = await registry.executeSkill("write_local_file", {
      filePath: testFilePath,
      content: "LIVA Test Auto",
    });
    console.log("Result:", writeRes);

    console.log("-> 1.2: Đọc file (read_local_file)");
    const readRes = await registry.executeSkill("read_local_file", {
      filePath: testFilePath,
    });
    console.log("Result:", readRes);

    console.log("-> 1.3: Danh sách thư mục (list_directory)");
    const listRes = await registry.executeSkill("list_directory", {
      targetPath: __dirname,
    });
    console.log("Result:", String(listRes).slice(0, 100) + "...");

    console.log("-> 1.4: Xóa file (delete_local_file)");
    const delRes = await registry.executeSkill("delete_local_file", {
      filePath: testFilePath,
    });
    console.log("Result:", delRes);

    console.log("-> 1.5: Lệnh CMD an toàn (execute_command)");
    const execRes = await registry.executeSkill("execute_command", {
      command: "echo LIVA_IS_ALIVE",
    });
    console.log("Result:", String(execRes).trim());

    console.log("-> 1.6: Lệnh CMD nguy hiểm (execute_command block test)");
    const execBlockRes = await registry.executeSkill("execute_command", {
      command: "rmdir /s /q C:\\Windows",
    });
    console.log("Result:", execBlockRes);
  } catch (e: any) {
    console.error("LỖI TEST WINDOWS OS:", e.message);
  }

  console.log("\n--- TEST 2: Trình duyệt & Mạng ---");
  try {
    console.log("-> 2.1: Lấy thời gian (get_current_time)");
    const timeRes = await registry.executeSkill("get_current_time", {});
    console.log("Result:", timeRes);
  } catch (e: any) {
    console.error("LỖI TEST NETWORK:", e.message);
  }

  console.log("\n--- TEST 3: Google Workspace (Kiểm tra Auth) ---");
  try {
    console.log(
      "-> 3.1: Gọi thử Search Google Drive (Nếu chưa báo auth thì fail gracefully là OK)",
    );
    // Try calling search google drive with an innocent query
    const driveRes = await registry.executeSkill("search_google_drive", {
      query: "name contains 'Test'",
    });
    console.log("Result:", String(driveRes).slice(0, 100));
  } catch (e: any) {
    console.error("LỖI TEST GOOGLE:", e.message);
  }
}

runTests().then(() => console.log("\n=== HOÀN TẤT TEST ==="));
