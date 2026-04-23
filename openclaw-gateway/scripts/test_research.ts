import { execute } from "../src/skills/ResearchIdeation.js";

async function main() {
    console.log("Starting research test for 'oppo A77s'...");
    try {
        const result = await execute({
            topic: "Đánh giá chuyên sâu về điện thoại Oppo A77s",
            fileLocation: "E:/Project/openclaw_remake/scratch_workspace"
        });
        console.log("Research Result:", result);
    } catch (e) {
        console.error("Test failed:", e);
    }
}

main();
