const fs = require("fs");
const path = require("path");
const cp = require("child_process");
const http = require("http");
const https = require("https");

// ── 1. Escape Hatch Check ──
if (process.env.SKIP_AI_HOOK === "1") {
    console.log("🤖 [AI hook] SKIP_AI_HOOK=1 detected. Skipping AI audit.");
    process.exit(0);
}

// ── 2. Get Staged Files ──
let stagedFiles = [];
try {
    const stdout = cp.execSync("git diff --cached --name-only", { encoding: "utf8" }).trim();
    stagedFiles = stdout.split("\n").filter(f => f && (f.endsWith(".ts") || f.endsWith(".vue")));
} catch (e) {
    console.warn("⚠️ [AI Hook] Could not determine staged files. Skipping hook.");
    process.exit(0);
}

if (stagedFiles.length === 0) {
    console.log("🤖 [AI Hook] No staged TS/Vue files. Skipping AI audit.");
    process.exit(0);
}

// ── 3. Parse Local .env Configuration ──
const projectRoot = path.join(__dirname, "..");
const envPath = path.join(projectRoot, ".env");
if (fs.existsSync(envPath)) {
    const envContent = fs.readFileSync(envPath, "utf8");
    envContent.split("\n").forEach(line => {
        const match = line.trim().match(/^([^#=]+)=(.*)$/);
        if (match) {
            const key = match[1].trim();
            let value = match[2].trim();
            // Strip wrapping quotes
            if (value.startsWith('"') && value.endsWith('"')) value = value.slice(1, -1);
            if (value.startsWith("'") && value.endsWith("'")) value = value.slice(1, -1);
            if (!process.env[key]) process.env[key] = value;
        }
    });
}

// Fallback config
const aiBaseUrl = process.env.AI_BASE_URL || "http://127.0.0.1:8000/v1";
const apiKey = process.env.AI_API_KEY || "local-ghost-router";
const modelName = process.env.AI_MODEL || "gemma-4-E4B-it-Q6_K.gguf";

// ── 4. Fail-Open Connection Check (5s timeout) ──
function checkEndpoint(urlStr) {
    return new Promise((resolve) => {
        const url = new URL(urlStr);
        const client = url.protocol === "https:" ? https : http;
        let resolved = false;

        const req = client.get({
            hostname: url.hostname,
            port: url.port,
            path: url.pathname.endsWith("/v1") ? url.pathname + "/models" : url.pathname,
            timeout: 5000,
            headers: { "Authorization": `Bearer ${apiKey}` }
        }, (res) => {
            resolved = true;
            resolve(res.statusCode >= 200 && res.statusCode < 400);
        });

        req.on("error", () => {
            if (!resolved) {
                resolved = true;
                resolve(false);
            }
        });

        req.on("timeout", () => {
            req.destroy();
            if (!resolved) {
                resolved = true;
                resolve(false);
            }
        });
    });
}

// ── 5. Main Auditing Execution ──
async function run() {
    console.log("🤖 [AI Hook] Pinging LLM endpoint...");
    const isAlive = await checkEndpoint(aiBaseUrl);
    if (!isAlive) {
        console.warn("⚠️ [AI Hook] LLM Endpoint is offline or connection timed out (5s limit). Fail-Open: Skipping AI pre-commit audit.");
        process.exit(0);
    }

    console.log("🤖 [AI Hook] LLM Endpoint is alive. Reading review prompt...");
    const promptPath = path.join(projectRoot, "docs", "prompts", "code-review-prompt.md");
    if (!fs.existsSync(promptPath)) {
        console.warn("⚠️ [AI Hook] code-review-prompt.md not found. Skipping hook.");
        process.exit(0);
    }
    const systemPrompt = fs.readFileSync(promptPath, "utf8");

    // Gather staged diffs
    let diffs = "";
    stagedFiles.forEach(file => {
        try {
            const diff = cp.execSync(`git diff --cached ${file}`, { encoding: "utf8" });
            diffs += `\n\n--- FILE: ${file} ---\n${diff}`;
        } catch (e) {
            // Ignore single file diff errors
        }
    });

    if (!diffs.trim()) {
        console.log("🤖 [AI Hook] Staged diffs are empty. Skipping hook.");
        process.exit(0);
    }

    console.log("🤖 [AI Hook] Submitting staged changes for code review audit...");
    try {
        const result = await callLLM(systemPrompt, `Review the following Git diffs for any critical violations or excessive cognitive complexity:\n${diffs}`);
        
        console.log("\n=================== AI CODE REVIEW REPORT ===================");
        console.log(result);
        console.log("=============================================================\n");

        // Parse audit result XML block
        const match = result.match(/<audit_result>([\s\S]*?)<\/audit_result>/);
        if (match && match[1]) {
            try {
                const auditData = JSON.parse(match[1].trim());
                if (auditData.block === true) {
                    console.error(`❌ [AI Hook] Commit blocked! Reason: ${auditData.reason}`);
                    process.exit(1);
                }
            } catch (jsonErr) {
                // Could not parse JSON, fallback to substring check
            }
        }

        if (result.includes("BLOCK_COMMIT") || result.includes("CRITICAL VIOLATION")) {
            console.error("❌ [AI Hook] Commit blocked! Critical violations found in code review.");
            process.exit(1);
        }

        console.log("✅ [AI Hook] AI audit passed. Proceeding with commit.");
        process.exit(0);
    } catch (err) {
        console.warn(`⚠️ [AI Hook] Audit failed due to LLM error: ${err.message}. Fail-Open: Proceeding with commit.`);
        process.exit(0);
    }
}

function callLLM(systemText, userText) {
    return new Promise((resolve, reject) => {
        const url = new URL(aiBaseUrl + "/chat/completions");
        const client = url.protocol === "https:" ? https : http;

        const postData = JSON.stringify({
            model: modelName,
            messages: [
                { role: "system", content: systemText },
                { role: "user", content: userText }
            ],
            temperature: 0.1,
            max_tokens: 1500
        });

        const req = client.request({
            hostname: url.hostname,
            port: url.port,
            path: url.pathname,
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "Content-Length": Buffer.byteLength(postData),
                "Authorization": `Bearer ${apiKey}`
            },
            timeout: 25000 // 25s timeout for AI completion
        }, (res) => {
            let data = "";
            res.on("data", chunk => data += chunk);
            res.on("end", () => {
                if (res.statusCode >= 200 && res.statusCode < 300) {
                    try {
                        const parsed = JSON.parse(data);
                        resolve(parsed.choices[0]?.message?.content || "");
                    } catch (e) {
                        reject(new Error("Failed to parse LLM JSON response"));
                    }
                } else {
                    reject(new Error(`LLM returned status code ${res.statusCode}`));
                }
            });
        });

        req.on("error", reject);
        req.on("timeout", () => {
            req.destroy();
            reject(new Error("LLM request timed out (25s limit)"));
        });

        req.write(postData);
        req.end();
    });
}

run();
