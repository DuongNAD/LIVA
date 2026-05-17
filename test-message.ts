/**
 * LIVA Full Communication Test
 * 
 * Tests the complete LIVA messaging loop:
 * 1. Connect to running LIVA WebSocket
 * 2. Send a test message
 * 3. Wait for AI response
 */

import WebSocket from "ws";
import { logger } from "./openclaw-gateway/src/utils/logger.js";

// Force --dev to bypass token auth
process.argv.push("--dev");

const WS_URL = "ws://127.0.0.1:8082";
const TEST_MESSAGE = "Chào LIVA, đây là tin nhắn test. Bạn có nghe thấy tôi không?";

async function main() {
    logger.info("🚀 LIVA Communication Test Starting...");
    logger.info(`📨 Sending: "${TEST_MESSAGE}"\n`);
    
    return new Promise<void>((resolve) => {
        const ws = new WebSocket(WS_URL);
        let responseCount = 0;
        const maxResponses = 3;
        
        ws.on("open", () => {
            logger.info("✅ Connected to LIVA WebSocket!");
            
            // Send a test message
            ws.send(JSON.stringify({
                type: "user_input",
                data: TEST_MESSAGE
            }));
            logger.info("📤 Message sent, waiting for response...\n");
        });
        
        ws.on("message", (data) => {
            const msg = data.toString();
            
            // Check if it's a response message
            if (msg.includes("llm_response") || msg.includes("tts_partial") || msg.includes("agent_")) {
                responseCount++;
                logger.info(`📥 Response #${responseCount}: ${msg.substring(0, 150)}...`);
            }
            
            // Stop after getting some responses
            if (responseCount >= maxResponses) {
                logger.info("\n✅ Test completed - LIVA is responding!");
                ws.close();
                resolve();
            }
        });
        
        ws.on("error", (err) => {
            logger.error(`❌ WebSocket error: ${err.message}`);
            resolve();
        });
        
        ws.on("close", () => {
            logger.info("WebSocket connection closed");
            if (responseCount === 0) {
                logger.warn("⚠️ No responses received - LIVA might be processing or idle");
            }
            resolve();
        });
        
        // Timeout after 30 seconds
        setTimeout(() => {
            logger.info("\n⏱️ Test timeout");
            ws.close();
            resolve();
        }, 30000);
    });
}

main().catch(console.error);
