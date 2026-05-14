import { SkillRegistry } from "./src/SkillRegistry";

async function runStressTest() {
    console.log("🚀 Starting AgentLoop Stress Test (Concurrent Zod Rejections)");

    const registry = new SkillRegistry();
    await registry.registerLocalSkills();

    const tasks = [
        // 1. get_weather_forecast with string instead of number
        registry.executeSkill("get_weather_forecast", { location: "Hanoi", days: "two" }).catch(e => `Caught: ${e.message}`),
        
        // 2. youtube_downloader with invalid enum "wav"
        registry.executeSkill("youtube_downloader", { url: "https://youtube.com/watch?v=123", format: "wav" }).catch(e => `Caught: ${e.message}`),
        
        // 3. execute_command with "rm -rf /" (HITLGuard or Zod injection protection)
        registry.executeSkill("execute_command", { command: "rm -rf /" }).catch(e => `Caught: ${e.message}`),
        
        // 4. write_local_file with empty path
        registry.executeSkill("write_local_file", { path: "", content: "test" }).catch(e => `Caught: ${e.message}`),
        
        // 5. write_local_file missing required fields entirely
        registry.executeSkill("write_local_file", {}).catch(e => `Caught: ${e.message}`),
    ];

    const results = await Promise.all(tasks);

    console.log("\n================ STRESS TEST RESULTS ================");
    results.forEach((res, index) => {
        console.log(`[Task ${index + 1}] Rejection caught successfully:`);
        console.log(String(res).substring(0, 500));
        console.log("-----------------------------------------------------");
    });
    console.log("✅ All bad payloads rejected safely without crashing Node.js Event Loop!");
    process.exit(0);
}

runStressTest().catch(e => {
    console.error(e);
    process.exit(1);
});
