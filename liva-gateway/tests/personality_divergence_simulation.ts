import { StructuredMemory } from "../src/memory/StructuredMemory";
import * as path from "node:path";
import * as fs from "node:fs";
import { strict as assert } from "node:assert";

async function runSimulation() {
    console.log("=== STARTING PERSONALITY DIVERGENCE SIMULATION ===");

    const randId = Math.random().toString(36).substring(7);
    const agentIdFriendly = `friendly_agent_${randId}`;
    const agentIdToxic = `toxic_agent_${randId}`;
    const baseDir = path.join(process.cwd(), "data", "agents", `simulation_${randId}`);
    const friendlyDbPath = path.join(baseDir, "friendly.sqlite");
    const toxicDbPath = path.join(baseDir, "toxic.sqlite");

    // Ensure directory exists
    fs.mkdirSync(baseDir, { recursive: true });

    let friendlyMem: StructuredMemory | null = null;
    let toxicMem: StructuredMemory | null = null;

    try {
        friendlyMem = await StructuredMemory.create(agentIdFriendly, friendlyDbPath);
        toxicMem = await StructuredMemory.create(agentIdToxic, toxicDbPath);

        // Fetch initial states
        const initialFriendlyState = await friendlyMem.getPersonalityState();
        const initialToxicState = await toxicMem.getPersonalityState();

        console.log("Initial State (Default):", initialFriendlyState);

        // 1. Feed friendly history
        const friendlyMessages = [
            "Thank you, you are doing a wonderful job!",
            "Please help me with this, you are so kind",
            "I love working with you, you are sweet and friendly",
            "Great effort, I appreciate your help so much",
            "Wonderful, thank you, that is extremely tốt"
        ];

        console.log("\nFeeding friendly messages...");
        for (const msg of friendlyMessages) {
            await friendlyMem.insertEvent({
                eventId: `evt_f_${Math.random().toString(36).substring(7)}`,
                timestamp: Date.now(),
                phi: { facts: [], entities: [] },
                psi: { sentiment: "positive", intent: "praise", relational: "warm" },
                rawUserMsg: msg,
                rawAiReply: "You are welcome!"
            });
            // Small wait to ensure async DB operations complete
            await new Promise(r => setTimeout(r, 100));
        }

        const finalFriendlyState = await friendlyMem.getPersonalityState();
        console.log("Final Friendly State:", finalFriendlyState);

        // 2. Feed toxic history
        const toxicMessages = [
            "Shut up you stupid useless trash!",
            "You are the worst idiot ever, I hate you",
            "Bad agent, ghét and dốt and điên",
            "You are so dumb and toxic, câm đi",
            "Shut up, rác khùng"
        ];

        console.log("\nFeeding toxic messages...");
        for (const msg of toxicMessages) {
            await toxicMem.insertEvent({
                eventId: `evt_t_${Math.random().toString(36).substring(7)}`,
                timestamp: Date.now(),
                phi: { facts: [], entities: [] },
                psi: { sentiment: "negative", intent: "abuse", relational: "hostile" },
                rawUserMsg: msg,
                rawAiReply: "I apologize."
            });
            // Small wait to ensure async DB operations complete
            await new Promise(r => setTimeout(r, 100));
        }

        const finalToxicState = await toxicMem.getPersonalityState();
        console.log("Final Toxic State:", finalToxicState);

        // Verify friendly friendliness increases
        assert.ok(
            finalFriendlyState.friendliness > initialFriendlyState.friendliness,
            `Expected friendliness to increase from ${initialFriendlyState.friendliness} but got ${finalFriendlyState.friendliness}`
        );

        // Verify toxic friendliness decreases, assertiveness & arousal increase
        assert.ok(
            finalToxicState.friendliness < initialToxicState.friendliness,
            `Expected friendliness to decrease from ${initialToxicState.friendliness} but got ${finalToxicState.friendliness}`
        );
        assert.ok(
            finalToxicState.assertiveness > initialToxicState.assertiveness,
            `Expected assertiveness to increase from ${initialToxicState.assertiveness} but got ${finalToxicState.assertiveness}`
        );
        assert.ok(
            finalToxicState.arousal > initialToxicState.arousal,
            `Expected arousal to increase from ${initialToxicState.arousal} but got ${finalToxicState.arousal}`
        );

        // Assert that the resulting personality coordinates significantly diverge (friendliness difference > 0.3)
        const friendlinessDiff = finalFriendlyState.friendliness - finalToxicState.friendliness;
        console.log(`\nFriendliness difference: ${friendlinessDiff.toFixed(4)}`);
        assert.ok(
            friendlinessDiff > 0.3,
            `Expected friendliness difference to be > 0.3, but got ${friendlinessDiff}`
        );

        console.log("\n=== ALL ASSERTIONS PASSED SUCCESSFULLY ===");

    } finally {
        // Clean up
        if (friendlyMem) await friendlyMem.close();
        if (toxicMem) await toxicMem.close();

        try {
            // Give a tiny delay for node:sqlite file lock release
            await new Promise(r => setTimeout(r, 200));
            fs.rmSync(baseDir, { recursive: true, force: true });
            console.log("Temporary databases and directory cleaned up.");
        } catch (cleanupErr) {
            console.error("Cleanup error:", cleanupErr);
        }
    }
}

runSimulation().catch(err => {
    console.error("Simulation failed:", err);
    process.exit(1);
});
