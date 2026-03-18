import { SkillRegistry } from "./SkillRegistry";

async function runTests() {
    const registry = new SkillRegistry();
    await registry.registerLocalSkills();
    const skills = registry.getAllSkills();

    const results: any[] = [];

    const dummyArgs: Record<string, any> = {
        "get_current_time": {},
        "read_file": { path: "./package.json" },
        "delete_local_file": { path: "./test_auto.txt" },
        "execute_command": { command: "echo Hello" },
        "get_system_info": {},
        "get_weather_forecast": {},
        "list_directory": { path: "./" },
        "open_local_file": { path: "./package.json" },
        "read_emails": { count: 1 },
        "read_local_file": { path: "./package.json" },
        "read_recent_emails": { count: 1 },
        "send_zalo_bot": { message: "Test Auto", topic: "Ghi chú log" },
        "send_zalo_rpa": { message: "Test Auto" },
        "web_search": { query: "OpenAI" },
        "write_local_file": { path: "./test_auto.txt", content: "Test Auto" }
    };

    console.log("Bắt đầu tự động kiểm thử các Tool...\n");

    for (const skill of skills) {
        let status = "❌ FAILED";
        let message = "";
        try {
            const args = dummyArgs[skill.name] || {};
            // Special wait for RPA to not block too long or we just test it
            const res = await registry.executeSkill(skill.name, args);
            status = "✅ PASSED";
            message = String(res).substring(0, 100).replace(/\n/g, " ") + "...";
        } catch (e: any) {
            status = "❌ FAILED";
            message = e.message;
        }
        
        results.push({
            Name: skill.name,
            Status: status,
            Message: message
        });
        
        console.log(`[${status}] ${skill.name}`);
    }

    console.log("\nKẾT QUẢ KIỂM THỬ:");
    console.table(results);
}

runTests().catch(console.error);
